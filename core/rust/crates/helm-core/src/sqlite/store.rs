use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

use crate::models::{
    CachedSearchResult, CoreError, CoreErrorKind, DetectionInfo, HomebrewKegPolicy,
    InstalledPackage, ManagerId, OutdatedPackage, PackageCandidate, PackageKegPolicy, PackageRef,
    PinKind, PinRecord, TaskId, TaskRecord, TaskStatus, TaskType,
};
use crate::persistence::{
    DetectionStore, MigrationStore, PackageStore, PersistenceResult, PinStore, SearchCacheStore,
    TaskStore,
};
use crate::sqlite::migrations::{SqliteMigration, current_schema_version, migration, migrations};

const MIGRATIONS_TABLE: &str = "helm_schema_migrations";

pub struct SqliteStore {
    database_path: PathBuf,
}

impl SqliteStore {
    pub fn new(database_path: impl Into<PathBuf>) -> Self {
        Self {
            database_path: database_path.into(),
        }
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn planned_migrations(&self, from_version: i64) -> Vec<&'static SqliteMigration> {
        migrations()
            .iter()
            .filter(|entry| entry.version > from_version)
            .collect()
    }

    pub fn migrate_to_latest(&self) -> PersistenceResult<()> {
        self.apply_migration(current_schema_version())
    }

    fn with_connection<T>(
        &self,
        operation_name: &str,
        operation: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> PersistenceResult<T> {
        let mut connection = open_connection(&self.database_path)
            .map_err(|error| storage_error(operation_name, error))?;
        operation(&mut connection).map_err(|error| storage_error(operation_name, error))
    }
}

impl MigrationStore for SqliteStore {
    fn current_version(&self) -> PersistenceResult<i64> {
        self.with_connection("current_version", |connection| {
            ensure_migrations_table(connection)?;
            read_current_version(connection)
        })
    }

    fn apply_migration(&self, target_version: i64) -> PersistenceResult<()> {
        if target_version < 0 || target_version > current_schema_version() {
            return Err(storage_error_text(
                "apply_migration",
                format!("invalid migration target version '{target_version}'"),
            ));
        }

        if target_version > 0 && migration(target_version).is_none() {
            return Err(storage_error_text(
                "apply_migration",
                format!("migration version '{target_version}' is not defined"),
            ));
        }

        self.with_connection("apply_migration", |connection| {
            ensure_migrations_table(connection)?;
            let current_version = read_current_version(connection)?;

            if target_version == current_version {
                // Re-apply all DDL to handle corrupted state where migration
                // version was recorded but tables are missing. All DDL uses
                // CREATE TABLE/INDEX IF NOT EXISTS, so this is idempotent.
                // ALTER TABLE ADD COLUMN is NOT idempotent in SQLite, so we
                // tolerate "duplicate column name" errors.
                for version in 1..=target_version {
                    let m = migration(version).expect("validated migration version must exist");
                    execute_batch_tolerant(connection, m.up_sql)?;
                }
                return Ok(());
            }

            if target_version > current_version {
                for version in (current_version + 1)..=target_version {
                    let migration =
                        migration(version).expect("validated migration version must exist");
                    apply_up_migration(connection, migration)?;
                }
            } else {
                for version in ((target_version + 1)..=current_version).rev() {
                    let migration =
                        migration(version).expect("validated migration version must exist");
                    apply_down_migration(connection, migration)?;
                }
            }

            Ok(())
        })
    }
}

impl PackageStore for SqliteStore {
    fn upsert_installed(&self, packages: &[InstalledPackage]) -> PersistenceResult<()> {
        self.with_connection("upsert_installed", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            {
                let mut statement = transaction.prepare(
                    "
INSERT INTO installed_packages (
    manager_id, package_name, installed_version, pinned, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, strftime('%s', 'now'))
ON CONFLICT(manager_id, package_name) DO UPDATE SET
    installed_version = excluded.installed_version,
    pinned = excluded.pinned,
    updated_at_unix = excluded.updated_at_unix
",
                )?;

                for package in packages {
                    statement.execute((
                        package.package.manager.as_str(),
                        package.package.name.as_str(),
                        package.installed_version.as_deref(),
                        bool_to_sqlite(package.pinned),
                    ))?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
    }

    fn upsert_outdated(&self, packages: &[OutdatedPackage]) -> PersistenceResult<()> {
        self.with_connection("upsert_outdated", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            {
                let mut statement = transaction.prepare(
                    "
INSERT INTO outdated_packages (
    manager_id, package_name, installed_version, candidate_version, pinned, restart_required, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s', 'now'))
ON CONFLICT(manager_id, package_name) DO UPDATE SET
    installed_version = excluded.installed_version,
    candidate_version = excluded.candidate_version,
    pinned = excluded.pinned,
    restart_required = excluded.restart_required,
    updated_at_unix = excluded.updated_at_unix
",
                )?;

                for package in packages {
                    statement.execute((
                        package.package.manager.as_str(),
                        package.package.name.as_str(),
                        package.installed_version.as_deref(),
                        package.candidate_version.as_str(),
                        bool_to_sqlite(package.pinned),
                        bool_to_sqlite(package.restart_required),
                    ))?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
    }

    fn replace_outdated_snapshot(
        &self,
        manager: ManagerId,
        packages: &[OutdatedPackage],
    ) -> PersistenceResult<()> {
        self.with_connection("replace_outdated_snapshot", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;

            transaction.execute(
                "DELETE FROM outdated_packages WHERE manager_id = ?1",
                [manager.as_str()],
            )?;

            {
                let mut statement = transaction.prepare(
                    "
INSERT INTO outdated_packages (
    manager_id, package_name, installed_version, candidate_version, pinned, restart_required, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%s', 'now'))
",
                )?;

                for package in packages {
                    statement.execute((
                        package.package.manager.as_str(),
                        package.package.name.as_str(),
                        package.installed_version.as_deref(),
                        package.candidate_version.as_str(),
                        bool_to_sqlite(package.pinned),
                        bool_to_sqlite(package.restart_required),
                    ))?;
                }
            }

            transaction.commit()?;
            Ok(())
        })
    }

    fn list_installed(&self) -> PersistenceResult<Vec<InstalledPackage>> {
        self.with_connection("list_installed", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT
    ip.manager_id,
    ip.package_name,
    ip.installed_version,
    CASE
        WHEN pr.manager_id IS NOT NULL THEN 1
        ELSE ip.pinned
    END AS pinned
FROM installed_packages ip
LEFT JOIN pin_records pr
    ON pr.manager_id = ip.manager_id
   AND pr.package_name = ip.package_name
ORDER BY ip.manager_id, ip.package_name
",
            )?;

            let rows = statement.query_map([], |row| {
                let manager_id: String = row.get(0)?;
                let package_name: String = row.get(1)?;
                let installed_version: Option<String> = row.get(2)?;
                let pinned_int: i64 = row.get(3)?;

                let manager = parse_manager_id(&manager_id)?;
                Ok(InstalledPackage {
                    package: PackageRef {
                        manager,
                        name: package_name,
                    },
                    installed_version,
                    pinned: sqlite_to_bool(pinned_int),
                })
            })?;

            rows.collect()
        })
    }

    fn list_outdated(&self) -> PersistenceResult<Vec<OutdatedPackage>> {
        self.with_connection("list_outdated", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT
    op.manager_id,
    op.package_name,
    op.installed_version,
    op.candidate_version,
    CASE
        WHEN pr.manager_id IS NOT NULL THEN 1
        ELSE op.pinned
    END AS pinned,
    op.restart_required
FROM outdated_packages op
LEFT JOIN pin_records pr
    ON pr.manager_id = op.manager_id
   AND pr.package_name = op.package_name
ORDER BY op.manager_id, op.package_name
",
            )?;

            let rows = statement.query_map([], |row| {
                let manager_id: String = row.get(0)?;
                let package_name: String = row.get(1)?;
                let installed_version: Option<String> = row.get(2)?;
                let candidate_version: String = row.get(3)?;
                let pinned_int: i64 = row.get(4)?;
                let restart_required_int: i64 = row.get(5)?;

                let manager = parse_manager_id(&manager_id)?;
                Ok(OutdatedPackage {
                    package: PackageRef {
                        manager,
                        name: package_name,
                    },
                    installed_version,
                    candidate_version,
                    pinned: sqlite_to_bool(pinned_int),
                    restart_required: sqlite_to_bool(restart_required_int),
                })
            })?;

            rows.collect()
        })
    }

    fn set_snapshot_pinned(&self, package: &PackageRef, pinned: bool) -> PersistenceResult<()> {
        self.with_connection("set_snapshot_pinned", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;

            transaction.execute(
                "
UPDATE installed_packages
SET pinned = ?3, updated_at_unix = strftime('%s', 'now')
WHERE manager_id = ?1 AND package_name = ?2
",
                params![
                    package.manager.as_str(),
                    package.name.as_str(),
                    bool_to_sqlite(pinned),
                ],
            )?;

            transaction.execute(
                "
UPDATE outdated_packages
SET pinned = ?3, updated_at_unix = strftime('%s', 'now')
WHERE manager_id = ?1 AND package_name = ?2
",
                params![
                    package.manager.as_str(),
                    package.name.as_str(),
                    bool_to_sqlite(pinned),
                ],
            )?;

            transaction.commit()?;
            Ok(())
        })
    }
}

impl PinStore for SqliteStore {
    fn upsert_pin(&self, pin: &PinRecord) -> PersistenceResult<()> {
        self.with_connection("upsert_pin", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO pin_records (
    manager_id, package_name, pin_kind, pinned_version, created_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(manager_id, package_name) DO UPDATE SET
    pin_kind = excluded.pin_kind,
    pinned_version = excluded.pinned_version,
    created_at_unix = excluded.created_at_unix
",
                params![
                    pin.package.manager.as_str(),
                    pin.package.name.as_str(),
                    pin_kind_to_str(pin.kind),
                    pin.pinned_version.as_deref(),
                    to_unix_seconds(pin.created_at)?,
                ],
            )?;
            Ok(())
        })
    }

    fn remove_pin(&self, package_key: &str) -> PersistenceResult<()> {
        self.with_connection("remove_pin", |connection| {
            ensure_schema_ready(connection)?;
            let (manager, package_name) = parse_package_key(package_key)?;
            connection.execute(
                "DELETE FROM pin_records WHERE manager_id = ?1 AND package_name = ?2",
                params![manager.as_str(), package_name],
            )?;
            Ok(())
        })
    }

    fn list_pins(&self) -> PersistenceResult<Vec<PinRecord>> {
        self.with_connection("list_pins", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT manager_id, package_name, pin_kind, pinned_version, created_at_unix
FROM pin_records
ORDER BY manager_id, package_name
",
            )?;
            let rows = statement.query_map([], |row| {
                let manager_raw: String = row.get(0)?;
                let package_name: String = row.get(1)?;
                let pin_kind_raw: String = row.get(2)?;
                let pinned_version: Option<String> = row.get(3)?;
                let created_at_unix: i64 = row.get(4)?;

                Ok(PinRecord {
                    package: PackageRef {
                        manager: parse_manager_id(&manager_raw)?,
                        name: package_name,
                    },
                    kind: parse_pin_kind(&pin_kind_raw)?,
                    pinned_version,
                    created_at: from_unix_seconds(created_at_unix)?,
                })
            })?;

            rows.collect()
        })
    }
}

impl SearchCacheStore for SqliteStore {
    fn upsert_search_results(&self, results: &[CachedSearchResult]) -> PersistenceResult<()> {
        self.with_connection("upsert_search_results", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            {
                let mut delete_statement = transaction.prepare(
                    "
DELETE FROM search_cache
WHERE manager_id = ?1
  AND package_name = ?2
  AND COALESCE(version, '') = COALESCE(?3, '')
  AND originating_query = ?4
",
                )?;
                let mut insert_statement = transaction.prepare(
                    "
INSERT INTO search_cache (
    manager_id, package_name, version, summary, originating_query, cached_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
",
                )?;

                for result in results {
                    delete_statement.execute(params![
                        result.source_manager.as_str(),
                        result.result.package.name.as_str(),
                        result.result.version.as_deref(),
                        result.originating_query.as_str(),
                    ])?;

                    insert_statement.execute(params![
                        result.source_manager.as_str(),
                        result.result.package.name.as_str(),
                        result.result.version.as_deref(),
                        result.result.summary.as_deref(),
                        result.originating_query.as_str(),
                        to_unix_seconds(result.cached_at)?,
                    ])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
    }

    fn query_local(&self, query: &str, limit: usize) -> PersistenceResult<Vec<CachedSearchResult>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        self.with_connection("query_local", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT manager_id, package_name, version, summary, originating_query, cached_at_unix
FROM search_cache
WHERE (?1 = '' OR package_name LIKE ?2 OR COALESCE(summary, '') LIKE ?2)
ORDER BY cached_at_unix DESC, package_name ASC
LIMIT ?3
",
            )?;

            let pattern = format!("%{}%", query.trim());
            let rows =
                statement.query_map(params![query.trim(), pattern, to_i64(limit)?], |row| {
                    let manager_raw: String = row.get(0)?;
                    let package_name: String = row.get(1)?;
                    let version: Option<String> = row.get(2)?;
                    let summary: Option<String> = row.get(3)?;
                    let originating_query: String = row.get(4)?;
                    let cached_at_unix: i64 = row.get(5)?;

                    let manager = parse_manager_id(&manager_raw)?;
                    Ok(CachedSearchResult {
                        result: PackageCandidate {
                            package: PackageRef {
                                manager,
                                name: package_name,
                            },
                            version,
                            summary,
                        },
                        source_manager: manager,
                        originating_query,
                        cached_at: from_unix_seconds(cached_at_unix)?,
                    })
                })?;

            rows.collect()
        })
    }
}

impl TaskStore for SqliteStore {
    fn create_task(&self, task: &TaskRecord) -> PersistenceResult<()> {
        self.with_connection("create_task", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO task_records (task_id, manager_id, task_type, status, created_at_unix)
VALUES (?1, ?2, ?3, ?4, ?5)
",
                params![
                    task_id_to_i64(task.id)?,
                    task.manager.as_str(),
                    task_type_to_str(task.task_type),
                    task_status_to_str(task.status),
                    to_unix_seconds(task.created_at)?,
                ],
            )?;
            Ok(())
        })
    }

    fn update_task(&self, task: &TaskRecord) -> PersistenceResult<()> {
        self.with_connection("update_task", |connection| {
            ensure_schema_ready(connection)?;
            let updated = connection.execute(
                "
UPDATE task_records
SET manager_id = ?2, task_type = ?3, status = ?4, created_at_unix = ?5
WHERE task_id = ?1
",
                params![
                    task_id_to_i64(task.id)?,
                    task.manager.as_str(),
                    task_type_to_str(task.task_type),
                    task_status_to_str(task.status),
                    to_unix_seconds(task.created_at)?,
                ],
            )?;

            if updated == 0 {
                return Err(storage_error_sqlite("task id was not found for update"));
            }
            Ok(())
        })
    }

    fn list_recent_tasks(&self, limit: usize) -> PersistenceResult<Vec<TaskRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        self.with_connection("list_recent_tasks", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT task_id, manager_id, task_type, status, created_at_unix
FROM task_records
ORDER BY created_at_unix DESC, task_id DESC
LIMIT ?1
",
            )?;
            let rows = statement.query_map(params![to_i64(limit)?], |row| {
                let task_id_raw: i64 = row.get(0)?;
                let manager_raw: String = row.get(1)?;
                let task_type_raw: String = row.get(2)?;
                let status_raw: String = row.get(3)?;
                let created_at_unix: i64 = row.get(4)?;

                Ok(TaskRecord {
                    id: TaskId(i64_to_u64(task_id_raw)?),
                    manager: parse_manager_id(&manager_raw)?,
                    task_type: parse_task_type(&task_type_raw)?,
                    status: parse_task_status(&status_raw)?,
                    created_at: from_unix_seconds(created_at_unix)?,
                })
            })?;

            rows.collect()
        })
    }

    fn next_task_id(&self) -> PersistenceResult<u64> {
        self.with_connection("next_task_id", |connection| {
            ensure_schema_ready(connection)?;
            let max_id: Option<i64> =
                connection.query_row("SELECT MAX(task_id) FROM task_records", [], |row| {
                    row.get(0)
                })?;
            match max_id {
                Some(id) => Ok(i64_to_u64(id)?.saturating_add(1)),
                None => Ok(0),
            }
        })
    }

    fn prune_completed_tasks(&self, max_age_secs: i64) -> PersistenceResult<usize> {
        self.with_connection("prune_completed_tasks", |connection| {
            ensure_schema_ready(connection)?;
            let cutoff = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs() as i64
                - max_age_secs;
            let deleted = connection.execute(
                "
DELETE FROM task_records
WHERE status IN ('completed', 'failed', 'cancelled')
  AND created_at_unix < ?1
",
                params![cutoff],
            )?;
            Ok(deleted)
        })
    }

    fn delete_all_tasks(&self) -> PersistenceResult<()> {
        self.with_connection("delete_all_tasks", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute("DELETE FROM task_records", [])?;
            Ok(())
        })
    }
}

impl DetectionStore for SqliteStore {
    fn upsert_detection(&self, manager: ManagerId, info: &DetectionInfo) -> PersistenceResult<()> {
        self.with_connection("upsert_detection", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO manager_detection (manager_id, detected, executable_path, version, detected_at_unix)
VALUES (?1, ?2, ?3, ?4, strftime('%s', 'now'))
ON CONFLICT(manager_id) DO UPDATE SET
    detected = excluded.detected,
    executable_path = CASE
        WHEN excluded.detected = 1 THEN COALESCE(excluded.executable_path, manager_detection.executable_path)
        ELSE excluded.executable_path
    END,
    version = CASE
        WHEN excluded.detected = 1 THEN COALESCE(excluded.version, manager_detection.version)
        ELSE excluded.version
    END,
    detected_at_unix = excluded.detected_at_unix
",
                params![
                    manager.as_str(),
                    bool_to_sqlite(info.installed),
                    info.executable_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string()),
                    info.version.as_deref(),
                ],
            )?;
            Ok(())
        })
    }

    fn list_detections(&self) -> PersistenceResult<Vec<(ManagerId, DetectionInfo)>> {
        self.with_connection("list_detections", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT manager_id, detected, executable_path, version
FROM manager_detection
ORDER BY manager_id
",
            )?;

            let rows = statement.query_map([], |row| {
                let manager_raw: String = row.get(0)?;
                let detected_int: i64 = row.get(1)?;
                let executable_path: Option<String> = row.get(2)?;
                let version: Option<String> = row.get(3)?;

                let manager = parse_manager_id(&manager_raw)?;
                Ok((
                    manager,
                    DetectionInfo {
                        installed: sqlite_to_bool(detected_int),
                        executable_path: executable_path.map(std::path::PathBuf::from),
                        version,
                    },
                ))
            })?;

            rows.collect()
        })
    }

    fn set_manager_enabled(&self, manager: ManagerId, enabled: bool) -> PersistenceResult<()> {
        self.with_connection("set_manager_enabled", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO manager_preferences (manager_id, enabled)
VALUES (?1, ?2)
ON CONFLICT(manager_id) DO UPDATE SET
    enabled = excluded.enabled
",
                params![manager.as_str(), bool_to_sqlite(enabled)],
            )?;
            Ok(())
        })
    }

    fn list_manager_preferences(&self) -> PersistenceResult<Vec<(ManagerId, bool)>> {
        self.with_connection("list_manager_preferences", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT manager_id, enabled
FROM manager_preferences
ORDER BY manager_id
",
            )?;

            let rows = statement.query_map([], |row| {
                let manager_raw: String = row.get(0)?;
                let enabled_int: i64 = row.get(1)?;

                let manager = parse_manager_id(&manager_raw)?;
                Ok((manager, sqlite_to_bool(enabled_int)))
            })?;

            rows.collect()
        })
    }

    fn set_safe_mode(&self, enabled: bool) -> PersistenceResult<()> {
        self.with_connection("set_safe_mode", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO app_settings (key, value)
VALUES ('safe_mode', ?1)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value
",
                params![if enabled { "1" } else { "0" }],
            )?;
            Ok(())
        })
    }

    fn safe_mode(&self) -> PersistenceResult<bool> {
        self.with_connection("safe_mode", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement =
                connection.prepare("SELECT value FROM app_settings WHERE key = 'safe_mode'")?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                return Ok(false);
            };
            let value: String = row.get(0)?;
            Ok(value.trim() == "1")
        })
    }

    fn set_homebrew_keg_policy(&self, policy: HomebrewKegPolicy) -> PersistenceResult<()> {
        self.with_connection("set_homebrew_keg_policy", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO app_settings (key, value)
VALUES ('homebrew_keg_policy', ?1)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value
",
                params![policy.as_str()],
            )?;
            Ok(())
        })
    }

    fn homebrew_keg_policy(&self) -> PersistenceResult<HomebrewKegPolicy> {
        self.with_connection("homebrew_keg_policy", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection
                .prepare("SELECT value FROM app_settings WHERE key = 'homebrew_keg_policy'")?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                return Ok(HomebrewKegPolicy::Keep);
            };
            let value: String = row.get(0)?;
            Ok(value
                .trim()
                .parse::<HomebrewKegPolicy>()
                .unwrap_or(HomebrewKegPolicy::Keep))
        })
    }

    fn set_package_keg_policy(
        &self,
        package: &PackageRef,
        policy: Option<HomebrewKegPolicy>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_package_keg_policy", |connection| {
            ensure_schema_ready(connection)?;

            match policy {
                Some(policy) => {
                    connection.execute(
                        "
INSERT INTO package_keg_policies (manager_id, package_name, policy, updated_at_unix)
VALUES (?1, ?2, ?3, strftime('%s', 'now'))
ON CONFLICT(manager_id, package_name) DO UPDATE SET
    policy = excluded.policy,
    updated_at_unix = excluded.updated_at_unix
",
                        params![package.manager.as_str(), package.name.as_str(), policy.as_str()],
                    )?;
                }
                None => {
                    connection.execute(
                        "DELETE FROM package_keg_policies WHERE manager_id = ?1 AND package_name = ?2",
                        params![package.manager.as_str(), package.name.as_str()],
                    )?;
                }
            }

            Ok(())
        })
    }

    fn package_keg_policy(
        &self,
        package: &PackageRef,
    ) -> PersistenceResult<Option<HomebrewKegPolicy>> {
        self.with_connection("package_keg_policy", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT policy
FROM package_keg_policies
WHERE manager_id = ?1 AND package_name = ?2
",
            )?;
            let mut rows =
                statement.query(params![package.manager.as_str(), package.name.as_str()])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            let value: String = row.get(0)?;
            Ok(value.trim().parse::<HomebrewKegPolicy>().ok())
        })
    }

    fn list_package_keg_policies(&self) -> PersistenceResult<Vec<PackageKegPolicy>> {
        self.with_connection("list_package_keg_policies", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT manager_id, package_name, policy
FROM package_keg_policies
ORDER BY manager_id, package_name
",
            )?;
            let rows = statement.query_map([], |row| {
                let manager_raw: String = row.get(0)?;
                let package_name: String = row.get(1)?;
                let policy_raw: String = row.get(2)?;

                let manager = parse_manager_id(&manager_raw)?;
                let policy = policy_raw
                    .parse::<HomebrewKegPolicy>()
                    .map_err(|_| storage_error_sqlite("invalid keg policy value"))?;

                Ok(PackageKegPolicy {
                    package: PackageRef {
                        manager,
                        name: package_name,
                    },
                    policy,
                })
            })?;

            rows.collect()
        })
    }
}

fn open_connection(database_path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = database_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    }
    Connection::open(database_path)
}

fn ensure_migrations_table(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
CREATE TABLE IF NOT EXISTS helm_schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at_unix INTEGER NOT NULL
);
",
    )?;
    Ok(())
}

fn ensure_schema_ready(connection: &Connection) -> rusqlite::Result<()> {
    ensure_migrations_table(connection)?;
    let version = read_current_version(connection)?;
    if version <= 0 {
        return Err(storage_error_sqlite(
            "database schema is not initialized; apply migrations before package operations",
        ));
    }
    Ok(())
}

fn read_current_version(connection: &Connection) -> rusqlite::Result<i64> {
    connection.query_row(
        &format!("SELECT COALESCE(MAX(version), 0) FROM {MIGRATIONS_TABLE}"),
        [],
        |row| row.get(0),
    )
}

fn apply_up_migration(
    connection: &mut Connection,
    migration: &SqliteMigration,
) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    execute_batch_tolerant(&transaction, migration.up_sql)?;
    transaction.execute(
        &format!(
            "INSERT INTO {MIGRATIONS_TABLE} (version, name, applied_at_unix)
             VALUES (?1, ?2, strftime('%s', 'now'))"
        ),
        (migration.version, migration.name),
    )?;
    transaction.commit()?;
    Ok(())
}

/// Execute a SQL batch, tolerating "duplicate column name" errors from
/// `ALTER TABLE ADD COLUMN` which is not idempotent in SQLite.
fn execute_batch_tolerant(connection: &Connection, sql: &str) -> rusqlite::Result<()> {
    match connection.execute_batch(sql) {
        Ok(()) => Ok(()),
        Err(e) if e.to_string().contains("duplicate column name") => Ok(()),
        Err(e) => Err(e),
    }
}

fn apply_down_migration(
    connection: &mut Connection,
    migration: &SqliteMigration,
) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(migration.down_sql)?;
    transaction.execute(
        &format!("DELETE FROM {MIGRATIONS_TABLE} WHERE version = ?1"),
        [migration.version],
    )?;
    transaction.commit()?;
    Ok(())
}

fn storage_error(operation: &str, error: rusqlite::Error) -> CoreError {
    storage_error_text(operation, error.to_string())
}

fn storage_error_sqlite(message: &str) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(message.to_string())))
}

fn parse_manager_id(raw: &str) -> rusqlite::Result<ManagerId> {
    raw.parse::<ManagerId>().map_err(|_| {
        storage_error_sqlite(&format!(
            "unknown manager id '{raw}' found in persisted sqlite record"
        ))
    })
}

fn parse_package_key(package_key: &str) -> rusqlite::Result<(ManagerId, &str)> {
    let (manager_raw, package_name) = package_key.split_once(':').ok_or_else(|| {
        storage_error_sqlite("package_key must use '<manager_id>:<package_name>' format")
    })?;
    if package_name.trim().is_empty() {
        return Err(storage_error_sqlite(
            "package_key must include a non-empty package_name",
        ));
    }
    Ok((parse_manager_id(manager_raw)?, package_name))
}

fn pin_kind_to_str(kind: PinKind) -> &'static str {
    match kind {
        PinKind::Native => "native",
        PinKind::Virtual => "virtual",
    }
}

fn parse_pin_kind(raw: &str) -> rusqlite::Result<PinKind> {
    match raw {
        "native" => Ok(PinKind::Native),
        "virtual" => Ok(PinKind::Virtual),
        _ => Err(storage_error_sqlite(&format!(
            "unknown pin kind '{raw}' in sqlite record"
        ))),
    }
}

fn task_type_to_str(value: TaskType) -> &'static str {
    match value {
        TaskType::Detection => "detection",
        TaskType::Refresh => "refresh",
        TaskType::Search => "search",
        TaskType::Install => "install",
        TaskType::Uninstall => "uninstall",
        TaskType::Upgrade => "upgrade",
        TaskType::Pin => "pin",
        TaskType::Unpin => "unpin",
    }
}

fn parse_task_type(raw: &str) -> rusqlite::Result<TaskType> {
    match raw {
        "detection" => Ok(TaskType::Detection),
        "refresh" => Ok(TaskType::Refresh),
        "search" => Ok(TaskType::Search),
        "install" => Ok(TaskType::Install),
        "uninstall" => Ok(TaskType::Uninstall),
        "upgrade" => Ok(TaskType::Upgrade),
        "pin" => Ok(TaskType::Pin),
        "unpin" => Ok(TaskType::Unpin),
        _ => Err(storage_error_sqlite(&format!(
            "unknown task type '{raw}' in sqlite record"
        ))),
    }
}

fn task_status_to_str(value: TaskStatus) -> &'static str {
    match value {
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Failed => "failed",
    }
}

fn parse_task_status(raw: &str) -> rusqlite::Result<TaskStatus> {
    match raw {
        "queued" => Ok(TaskStatus::Queued),
        "running" => Ok(TaskStatus::Running),
        "completed" => Ok(TaskStatus::Completed),
        "cancelled" => Ok(TaskStatus::Cancelled),
        "failed" => Ok(TaskStatus::Failed),
        _ => Err(storage_error_sqlite(&format!(
            "unknown task status '{raw}' in sqlite record"
        ))),
    }
}

fn bool_to_sqlite(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn sqlite_to_bool(value: i64) -> bool {
    value != 0
}

fn to_unix_seconds(value: SystemTime) -> rusqlite::Result<i64> {
    let duration = value.duration_since(UNIX_EPOCH).map_err(|error| {
        storage_error_sqlite(&format!("time before unix epoch is not supported: {error}"))
    })?;
    let seconds = i64::try_from(duration.as_secs())
        .map_err(|_| storage_error_sqlite("unix timestamp seconds exceed i64 range"))?;
    Ok(seconds)
}

fn from_unix_seconds(value: i64) -> rusqlite::Result<SystemTime> {
    if value < 0 {
        return Err(storage_error_sqlite(
            "negative unix timestamps are not supported",
        ));
    }
    let seconds = u64::try_from(value)
        .map_err(|_| storage_error_sqlite("failed to convert unix timestamp to u64"))?;
    Ok(UNIX_EPOCH + Duration::from_secs(seconds))
}

fn task_id_to_i64(value: TaskId) -> rusqlite::Result<i64> {
    i64::try_from(value.0).map_err(|_| storage_error_sqlite("task id exceeds i64 range"))
}

fn i64_to_u64(value: i64) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| storage_error_sqlite("negative task id in sqlite record"))
}

fn to_i64(value: usize) -> rusqlite::Result<i64> {
    i64::try_from(value).map_err(|_| storage_error_sqlite("value exceeds i64 range"))
}

fn storage_error_text(operation: &str, message: impl AsRef<str>) -> CoreError {
    CoreError {
        manager: None,
        task: None,
        action: None,
        kind: CoreErrorKind::StorageFailure,
        message: format!("sqlite store '{operation}' failed: {}", message.as_ref()),
    }
}
