use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, DatabaseName, OptionalExtension, params};

use crate::models::{
    AutomationLevel, CachedSearchResult, CoreError, CoreErrorKind, DetectionInfo,
    HomebrewKegPolicy, InstallInstanceIdentityKind, InstallProvenance, InstalledPackage, ManagerId,
    ManagerInstallInstance, NewTaskLogRecord, OutdatedPackage, PackageCandidate, PackageKegPolicy,
    PackageRef, PinKind, PinRecord, StrategyKind, TaskId, TaskLogLevel, TaskLogRecord, TaskRecord,
    TaskStatus, TaskType,
};
use crate::persistence::doctor_persistence::{
    DoctorScanScope, DoctorStore, EffectiveKnowledge, KnowledgeTrustLevel, PersistedDoctorFinding,
    RepairHistoryRecord,
};
use crate::persistence::repair_knowledge::{
    KnowledgeEntry, KnowledgeEnvelope, KnowledgeSelector, sha256_hex,
};
use crate::persistence::{
    DetectionStore, ManagerPreference, MigrationStore, PackageManagerPreference, PackageStore,
    PersistenceResult, PinStore, SearchCacheStore, TaskStore,
};
use crate::security_advisory::{AdvisoryCacheRecord, AdvisoryCacheStore};
use crate::sqlite::migrations::{
    SqliteMigration, current_schema_version, migration, migration_definition_checksum_for_version,
    migrations, validate_migration_manifest,
};
use crate::versioning::normalize_package_family_key;

const MIGRATIONS_TABLE: &str = "helm_schema_migrations";
const MIGRATION_CHECKSUM_SCHEMA_VERSION: i64 = 20;
pub const BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY: &str = "bundled:helm";

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
        self.apply_migration(current_schema_version())?;
        self.ensure_bundled_repair_knowledge()
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

    pub fn latest_search_cached_at_unix(
        &self,
        manager: ManagerId,
    ) -> PersistenceResult<Option<i64>> {
        self.with_connection("latest_search_cached_at_unix", |connection| {
            ensure_schema_ready(connection)?;
            connection.query_row(
                "SELECT MAX(cached_at_unix) FROM search_cache WHERE manager_id = ?1 AND COALESCE(originating_query, '') = ''",
                [manager.as_str()],
                |row| row.get::<_, Option<i64>>(0),
            )
        })
    }

    pub fn ensure_bundled_repair_knowledge(&self) -> PersistenceResult<()> {
        let envelope = KnowledgeEnvelope::parse_and_validate(include_str!(
            "../../resources/bundled_knowledge.json"
        ))
        .map_err(|error| storage_error_text("parse_bundled_repair_knowledge", error.to_string()))?;
        self.import_knowledge(
            BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY,
            &envelope,
            KnowledgeTrustLevel::Bundled,
        )
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
        validate_migration_manifest()
            .map_err(|error| storage_error_text("apply_migration", error))?;

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
            let reconciliation_required = replaced_migration_reconciliation_required(
                connection,
                current_version,
                target_version,
            )?;
            if current_version > 0 && (target_version > current_version || reconciliation_required)
            {
                let backup_path =
                    create_pre_migration_backup(connection, &self.database_path, current_version)?;
                tracing::info!(
                    path = %backup_path.display(),
                    from_version = current_version,
                    target_version,
                    "created verified pre-migration SQLite backup"
                );
            }
            reconcile_replaced_migrations(connection, current_version, target_version)?;
            validate_applied_migration_identities(connection, current_version)?;

            if target_version == current_version {
                if target_version >= MIGRATION_CHECKSUM_SCHEMA_VERSION {
                    backfill_migration_checksums(connection, target_version)?;
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

            if target_version >= MIGRATION_CHECKSUM_SCHEMA_VERSION {
                backfill_migration_checksums(connection, target_version)?;
            }

            Ok(())
        })?;

        if target_version == 0 {
            remove_pre_migration_backups(&self.database_path).map_err(|error| {
                storage_error_text("remove_migration_backups", error.to_string())
            })?;
        }
        Ok(())
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
INSERT INTO installed_package_versions (
    manager_id, package_name, package_identifier, installed_version, pinned, is_active, is_default, has_override, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, strftime('%s', 'now'))
ON CONFLICT(manager_id, package_name, package_identifier, installed_version) DO UPDATE SET
    installed_version = excluded.installed_version,
    pinned = excluded.pinned,
    is_active = excluded.is_active,
    is_default = excluded.is_default,
    has_override = excluded.has_override,
    updated_at_unix = excluded.updated_at_unix
",
                )?;

                for package in packages {
                    let installed_version =
                        to_installed_version_token(package.installed_version.as_deref());
                    let package_identifier =
                        package.package_identifier.as_deref().unwrap_or_default();
                    statement.execute((
                        package.package.manager.as_str(),
                        package.package.name.as_str(),
                        package_identifier,
                        installed_version.as_str(),
                        bool_to_sqlite(package.pinned),
                        bool_to_sqlite(package.runtime_state.is_active),
                        bool_to_sqlite(package.runtime_state.is_default),
                        bool_to_sqlite(package.runtime_state.has_override),
                    ))?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
    }

    fn replace_installed_snapshot(
        &self,
        manager: ManagerId,
        packages: &[InstalledPackage],
    ) -> PersistenceResult<()> {
        self.with_connection("replace_installed_snapshot", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;

            transaction.execute(
                "DELETE FROM installed_package_versions WHERE manager_id = ?1",
                [manager.as_str()],
            )?;

            {
                let mut statement = transaction.prepare(
                    "
INSERT INTO installed_package_versions (
    manager_id, package_name, package_identifier, installed_version, pinned, is_active, is_default, has_override, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, strftime('%s', 'now'))
",
                )?;

                for package in packages {
                    let installed_version =
                        to_installed_version_token(package.installed_version.as_deref());
                    let package_identifier =
                        package.package_identifier.as_deref().unwrap_or_default();
                    statement.execute((
                        package.package.manager.as_str(),
                        package.package.name.as_str(),
                        package_identifier,
                        installed_version.as_str(),
                        bool_to_sqlite(package.pinned),
                        bool_to_sqlite(package.runtime_state.is_active),
                        bool_to_sqlite(package.runtime_state.is_default),
                        bool_to_sqlite(package.runtime_state.has_override),
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
    manager_id, package_name, package_identifier, installed_version, candidate_version, pinned, restart_required, is_active, is_default, has_override, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, strftime('%s', 'now'))
ON CONFLICT(manager_id, package_name, package_identifier) DO UPDATE SET
    installed_version = excluded.installed_version,
    candidate_version = excluded.candidate_version,
    pinned = excluded.pinned,
    restart_required = excluded.restart_required,
    is_active = excluded.is_active,
    is_default = excluded.is_default,
    has_override = excluded.has_override,
    updated_at_unix = excluded.updated_at_unix
",
                )?;

                for package in packages {
                    let package_identifier =
                        package.package_identifier.as_deref().unwrap_or_default();
                    statement.execute((
                        package.package.manager.as_str(),
                        package.package.name.as_str(),
                        package_identifier,
                        package.installed_version.as_deref(),
                        package.candidate_version.as_str(),
                        bool_to_sqlite(package.pinned),
                        bool_to_sqlite(package.restart_required),
                        bool_to_sqlite(package.runtime_state.is_active),
                        bool_to_sqlite(package.runtime_state.is_default),
                        bool_to_sqlite(package.runtime_state.has_override),
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
    manager_id, package_name, package_identifier, installed_version, candidate_version, pinned, restart_required, is_active, is_default, has_override, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, strftime('%s', 'now'))
",
                )?;

                for package in packages {
                    let package_identifier =
                        package.package_identifier.as_deref().unwrap_or_default();
                    statement.execute((
                        package.package.manager.as_str(),
                        package.package.name.as_str(),
                        package_identifier,
                        package.installed_version.as_deref(),
                        package.candidate_version.as_str(),
                        bool_to_sqlite(package.pinned),
                        bool_to_sqlite(package.restart_required),
                        bool_to_sqlite(package.runtime_state.is_active),
                        bool_to_sqlite(package.runtime_state.is_default),
                        bool_to_sqlite(package.runtime_state.has_override),
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
    ipv.manager_id,
    ipv.package_name,
    ipv.package_identifier,
    ipv.installed_version,
    CASE
        WHEN EXISTS (
            SELECT 1
            FROM pin_records pr
            WHERE pr.manager_id = ipv.manager_id
              AND pr.package_name = ipv.package_name
              AND (pr.pinned_version = '' OR pr.pinned_version = ipv.installed_version)
        ) THEN 1
        ELSE ipv.pinned
    END AS pinned,
    ipv.is_active,
    ipv.is_default,
    ipv.has_override
FROM installed_package_versions ipv
ORDER BY ipv.manager_id, ipv.package_name, ipv.package_identifier, ipv.installed_version
",
            )?;

            let rows = statement.query_map([], |row| {
                let manager_id: String = row.get(0)?;
                let package_name: String = row.get(1)?;
                let package_identifier_raw: String = row.get(2)?;
                let installed_version_raw: String = row.get(3)?;
                let pinned_int: i64 = row.get(4)?;
                let is_active_int: i64 = row.get(5)?;
                let is_default_int: i64 = row.get(6)?;
                let has_override_int: i64 = row.get(7)?;

                let manager = parse_manager_id(&manager_id)?;
                Ok(InstalledPackage {
                    package: PackageRef {
                        manager,
                        name: package_name,
                    },
                    package_identifier: from_installed_version_token(package_identifier_raw),
                    installed_version: from_installed_version_token(installed_version_raw),
                    pinned: sqlite_to_bool(pinned_int),
                    runtime_state: crate::models::PackageRuntimeState {
                        is_active: sqlite_to_bool(is_active_int),
                        is_default: sqlite_to_bool(is_default_int),
                        has_override: sqlite_to_bool(has_override_int),
                    },
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
    op.package_identifier,
    op.installed_version,
    op.candidate_version,
    CASE
        WHEN EXISTS (
            SELECT 1
            FROM pin_records pr
            WHERE pr.manager_id = op.manager_id
              AND pr.package_name = op.package_name
              AND (
                    pr.pinned_version = ''
                    OR pr.pinned_version = COALESCE(op.installed_version, '')
              )
        ) THEN 1
        ELSE op.pinned
    END AS pinned,
    op.restart_required,
    op.is_active,
    op.is_default,
    op.has_override
FROM outdated_packages op
ORDER BY op.manager_id, op.package_name, op.package_identifier
",
            )?;

            let rows = statement.query_map([], |row| {
                let manager_id: String = row.get(0)?;
                let package_name: String = row.get(1)?;
                let package_identifier_raw: String = row.get(2)?;
                let installed_version: Option<String> = row.get(3)?;
                let candidate_version: String = row.get(4)?;
                let pinned_int: i64 = row.get(5)?;
                let restart_required_int: i64 = row.get(6)?;
                let is_active_int: i64 = row.get(7)?;
                let is_default_int: i64 = row.get(8)?;
                let has_override_int: i64 = row.get(9)?;

                let manager = parse_manager_id(&manager_id)?;
                Ok(OutdatedPackage {
                    package: PackageRef {
                        manager,
                        name: package_name,
                    },
                    package_identifier: from_installed_version_token(package_identifier_raw),
                    installed_version,
                    candidate_version,
                    pinned: sqlite_to_bool(pinned_int),
                    restart_required: sqlite_to_bool(restart_required_int),
                    runtime_state: crate::models::PackageRuntimeState {
                        is_active: sqlite_to_bool(is_active_int),
                        is_default: sqlite_to_bool(is_default_int),
                        has_override: sqlite_to_bool(has_override_int),
                    },
                })
            })?;

            rows.collect()
        })
    }

    fn set_snapshot_pinned(
        &self,
        package: &PackageRef,
        version: Option<&str>,
        pinned: bool,
    ) -> PersistenceResult<()> {
        self.with_connection("set_snapshot_pinned", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            let version_token = to_installed_version_token(version);

            transaction.execute(
                "
UPDATE installed_package_versions
SET pinned = ?3, updated_at_unix = strftime('%s', 'now')
WHERE manager_id = ?1
  AND package_name = ?2
  AND (?4 = '' OR installed_version = ?4)
",
                params![
                    package.manager.as_str(),
                    package.name.as_str(),
                    bool_to_sqlite(pinned),
                    version_token.as_str(),
                ],
            )?;

            transaction.execute(
                "
UPDATE outdated_packages
SET pinned = ?3, updated_at_unix = strftime('%s', 'now')
WHERE manager_id = ?1
  AND package_name = ?2
  AND (?4 = '' OR COALESCE(installed_version, '') = ?4)
",
                params![
                    package.manager.as_str(),
                    package.name.as_str(),
                    bool_to_sqlite(pinned),
                    version_token.as_str(),
                ],
            )?;

            transaction.commit()?;
            Ok(())
        })
    }

    fn apply_install_result(
        &self,
        package: &PackageRef,
        package_identifier: Option<&str>,
        installed_version: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("apply_install_result", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;

            let installed_version_token = to_installed_version_token(installed_version);
            let package_identifier_token = package_identifier.unwrap_or_default();
            transaction.execute(
                "
INSERT INTO installed_package_versions (
    manager_id, package_name, package_identifier, installed_version, pinned, is_active, is_default, has_override, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, 0, 0, 0, 0, strftime('%s', 'now'))
ON CONFLICT(manager_id, package_name, package_identifier, installed_version) DO UPDATE SET
    installed_version = excluded.installed_version,
    is_active = excluded.is_active,
    is_default = excluded.is_default,
    has_override = excluded.has_override,
    updated_at_unix = excluded.updated_at_unix
",
                params![
                    package.manager.as_str(),
                    package.name.as_str(),
                    package_identifier_token,
                    installed_version_token.as_str()
                ],
            )?;

            if package.manager != ManagerId::Asdf {
                transaction.execute(
                    "
DELETE FROM outdated_packages
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
",
                    params![
                        package.manager.as_str(),
                        package.name.as_str(),
                        package_identifier_token,
                    ],
                )?;
            }

            transaction.commit()?;
            Ok(())
        })
    }

    fn apply_uninstall_result(
        &self,
        package: &PackageRef,
        package_identifier: Option<&str>,
        removed_version: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("apply_uninstall_result", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            let package_identifier_token = package_identifier.unwrap_or_default();

            if let Some(removed_version) = removed_version {
                let removed_version_token = to_installed_version_token(Some(removed_version));
                let removed_rows = transaction.execute(
                    "
DELETE FROM installed_package_versions
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
  AND installed_version = ?4
",
                    params![
                        package.manager.as_str(),
                        package.name.as_str(),
                        package_identifier_token,
                        removed_version_token.as_str(),
                    ],
                )?;
                if removed_rows == 0 && single_version_snapshot_manager(package.manager) {
                    transaction.execute(
                        "
DELETE FROM installed_package_versions
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
",
                        params![
                            package.manager.as_str(),
                            package.name.as_str(),
                            package_identifier_token,
                        ],
                    )?;
                }
            } else {
                transaction.execute(
                    "
DELETE FROM installed_package_versions
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
",
                    params![
                        package.manager.as_str(),
                        package.name.as_str(),
                        package_identifier_token,
                    ],
                )?;
            }

            transaction.execute(
                "
DELETE FROM outdated_packages
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
",
                params![
                    package.manager.as_str(),
                    package.name.as_str(),
                    package_identifier_token,
                ],
            )?;

            transaction.commit()?;
            Ok(())
        })
    }

    fn apply_upgrade_result(
        &self,
        package: &PackageRef,
        package_identifier: Option<&str>,
        before_version: Option<&str>,
        after_version: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("apply_upgrade_result", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            let package_identifier_token = package_identifier.unwrap_or_default();

            let outdated_entry: Option<(Option<String>, String, i64, i64, i64, i64)> = transaction
                .query_row(
                    "
SELECT installed_version, candidate_version, pinned, is_active, is_default, has_override
FROM outdated_packages
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
",
                    params![
                        package.manager.as_str(),
                        package.name.as_str(),
                        package_identifier_token,
                    ],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )
                .optional()?;

            let mut clear_outdated = package.manager != ManagerId::Asdf;
            if let Some((
                installed_version,
                candidate_version,
                pinned,
                is_active,
                is_default,
                has_override,
            )) = outdated_entry
            {
                let promoted_version = after_version.unwrap_or(candidate_version.as_str());
                let promoted_version_token =
                    to_installed_version_token(Some(promoted_version));
                transaction.execute(
                    "
INSERT INTO installed_package_versions (
    manager_id, package_name, package_identifier, installed_version, pinned, is_active, is_default, has_override, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, strftime('%s', 'now'))
ON CONFLICT(manager_id, package_name, package_identifier, installed_version) DO UPDATE SET
    pinned = excluded.pinned,
    is_active = excluded.is_active,
    is_default = excluded.is_default,
    has_override = excluded.has_override,
    updated_at_unix = excluded.updated_at_unix
",
                    params![
                        package.manager.as_str(),
                        package.name.as_str(),
                        package_identifier_token,
                        promoted_version_token.as_str(),
                        pinned,
                        is_active,
                        is_default,
                        has_override,
                    ],
                )?;

                let prior_version_token = to_installed_version_token(
                    before_version.or(installed_version.as_deref()),
                );
                if prior_version_token != promoted_version_token {
                    transaction.execute(
                        "
DELETE FROM installed_package_versions
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
  AND installed_version = ?4
",
                        params![
                            package.manager.as_str(),
                            package.name.as_str(),
                            package_identifier_token,
                            prior_version_token.as_str(),
                        ],
                    )?;
                }
                if package.manager == ManagerId::Asdf {
                    clear_outdated = is_active != 0 && is_default != 0 && has_override == 0;
                }
            }

            if clear_outdated {
                transaction.execute(
                    "
DELETE FROM outdated_packages
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
",
                    params![
                        package.manager.as_str(),
                        package.name.as_str(),
                        package_identifier_token,
                    ],
                )?;
            }

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
ON CONFLICT(manager_id, package_name, pinned_version) DO UPDATE SET
    pin_kind = excluded.pin_kind,
    created_at_unix = excluded.created_at_unix
",
                params![
                    pin.package.manager.as_str(),
                    pin.package.name.as_str(),
                    pin_kind_to_str(pin.kind),
                    to_installed_version_token(pin.pinned_version.as_deref()),
                    to_unix_seconds(pin.created_at)?,
                ],
            )?;
            Ok(())
        })
    }

    fn remove_pin(
        &self,
        package: &PackageRef,
        pinned_version: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("remove_pin", |connection| {
            ensure_schema_ready(connection)?;
            let version_token = to_installed_version_token(pinned_version);
            connection.execute(
                "
DELETE FROM pin_records
WHERE manager_id = ?1
  AND package_name = ?2
  AND pinned_version = ?3
",
                params![
                    package.manager.as_str(),
                    package.name.as_str(),
                    version_token.as_str(),
                ],
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
ORDER BY manager_id, package_name, pinned_version
",
            )?;
            let rows = statement.query_map([], |row| {
                let manager_raw: String = row.get(0)?;
                let package_name: String = row.get(1)?;
                let pin_kind_raw: String = row.get(2)?;
                let pinned_version_raw: String = row.get(3)?;
                let created_at_unix: i64 = row.get(4)?;

                Ok(PinRecord {
                    package: PackageRef {
                        manager: parse_manager_id(&manager_raw)?,
                        name: package_name,
                    },
                    kind: parse_pin_kind(&pin_kind_raw)?,
                    pinned_version: from_installed_version_token(pinned_version_raw),
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
                let mut select_statement = transaction.prepare(
                    "
SELECT version, summary
FROM search_cache
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
  AND COALESCE(version, '') = COALESCE(?4, '')
ORDER BY cached_at_unix DESC
LIMIT 1
",
                )?;
                let mut delete_statement = transaction.prepare(
                    "
DELETE FROM search_cache
WHERE manager_id = ?1
  AND package_name = ?2
  AND package_identifier = ?3
  AND COALESCE(version, '') = COALESCE(?4, '')
",
                )?;
                let mut insert_statement = transaction.prepare(
                    "
INSERT INTO search_cache (
    manager_id, package_name, package_identifier, version, summary, originating_query, cached_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
",
                )?;

                for result in results {
                    let incoming_version = normalize_optional_text(result.result.version.clone());
                    let package_identifier = result
                        .result
                        .package_identifier
                        .as_deref()
                        .unwrap_or_default();
                    let existing_entry: Option<(Option<String>, Option<String>)> = select_statement
                        .query_row(
                            params![
                                result.source_manager.as_str(),
                                result.result.package.name.as_str(),
                                package_identifier,
                                incoming_version.as_deref(),
                            ],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()?;
                    let (existing_version, existing_summary) =
                        existing_entry.unwrap_or((None, None));
                    let merged_version = incoming_version
                        .clone()
                        .or_else(|| normalize_optional_text(existing_version));
                    let merged_summary = normalize_optional_text(result.result.summary.clone())
                        .or_else(|| normalize_optional_text(existing_summary));

                    delete_statement.execute(params![
                        result.source_manager.as_str(),
                        result.result.package.name.as_str(),
                        package_identifier,
                        merged_version.as_deref(),
                    ])?;

                    insert_statement.execute(params![
                        result.source_manager.as_str(),
                        result.result.package.name.as_str(),
                        package_identifier,
                        merged_version.as_deref(),
                        merged_summary.as_deref(),
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
SELECT manager_id, package_name, package_identifier, version, summary, originating_query, cached_at_unix
FROM search_cache
WHERE (?1 = '' OR package_name LIKE ?2 OR package_identifier LIKE ?2 OR COALESCE(summary, '') LIKE ?2)
ORDER BY cached_at_unix DESC, package_name ASC
LIMIT ?3
",
            )?;

            let pattern = format!("%{}%", query.trim());
            let rows =
                statement.query_map(params![query.trim(), pattern, to_i64(limit)?], |row| {
                    let manager_raw: String = row.get(0)?;
                    let package_name: String = row.get(1)?;
                    let package_identifier_raw: String = row.get(2)?;
                    let version: Option<String> = row.get(3)?;
                    let summary: Option<String> = row.get(4)?;
                    let originating_query: String = row.get(5)?;
                    let cached_at_unix: i64 = row.get(6)?;

                    let manager = parse_manager_id(&manager_raw)?;
                    Ok(CachedSearchResult {
                        result: PackageCandidate {
                            package: PackageRef {
                                manager,
                                name: package_name,
                            },
                            package_identifier: from_installed_version_token(
                                package_identifier_raw,
                            ),
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

    fn update_task_with_log(
        &self,
        task: &TaskRecord,
        entry: &NewTaskLogRecord,
    ) -> PersistenceResult<()> {
        self.with_connection("update_task_with_log", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            let updated = transaction.execute(
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

            transaction.execute(
                "
INSERT INTO task_log_records (
    task_id, manager_id, task_type, status, level, message, created_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
",
                params![
                    task_id_to_i64(entry.task_id)?,
                    entry.manager.as_str(),
                    task_type_to_str(entry.task_type),
                    entry.status.map(task_status_to_str),
                    task_log_level_to_str(entry.level),
                    entry.message.as_str(),
                    to_unix_seconds(entry.created_at)?,
                ],
            )?;

            transaction.commit()?;
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
            let transaction = connection.transaction()?;
            transaction.execute(
                "
DELETE FROM task_log_records
WHERE task_id IN (
    SELECT task_id
    FROM task_records
    WHERE status IN ('completed', 'cancelled')
      AND created_at_unix < ?1
)
",
                params![cutoff],
            )?;
            let deleted = transaction.execute(
                "
DELETE FROM task_records
WHERE status IN ('completed', 'cancelled')
  AND created_at_unix < ?1
",
                params![cutoff],
            )?;
            transaction.commit()?;
            Ok(deleted)
        })
    }

    fn delete_task(&self, task_id: TaskId) -> PersistenceResult<()> {
        self.with_connection("delete_task", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            transaction.execute(
                "DELETE FROM task_log_records WHERE task_id = ?1",
                params![task_id_to_i64(task_id)?],
            )?;
            transaction.execute(
                "DELETE FROM task_records WHERE task_id = ?1",
                params![task_id_to_i64(task_id)?],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    fn delete_tasks_for_manager(&self, manager: ManagerId) -> PersistenceResult<()> {
        self.with_connection("delete_tasks_for_manager", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            transaction.execute(
                "
DELETE FROM task_log_records
WHERE task_id IN (
    SELECT task_id
    FROM task_records
    WHERE manager_id = ?1
)
",
                params![manager.as_str()],
            )?;
            transaction.execute(
                "DELETE FROM task_records WHERE manager_id = ?1",
                params![manager.as_str()],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    fn delete_all_tasks(&self) -> PersistenceResult<()> {
        self.with_connection("delete_all_tasks", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            transaction.execute("DELETE FROM task_log_records", [])?;
            transaction.execute("DELETE FROM task_records", [])?;
            transaction.commit()?;
            Ok(())
        })
    }

    fn append_task_log(&self, entry: &NewTaskLogRecord) -> PersistenceResult<()> {
        self.with_connection("append_task_log", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO task_log_records (
    task_id, manager_id, task_type, status, level, message, created_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
",
                params![
                    task_id_to_i64(entry.task_id)?,
                    entry.manager.as_str(),
                    task_type_to_str(entry.task_type),
                    entry.status.map(task_status_to_str),
                    task_log_level_to_str(entry.level),
                    entry.message.as_str(),
                    to_unix_seconds(entry.created_at)?,
                ],
            )?;
            Ok(())
        })
    }

    fn list_task_logs(
        &self,
        task_id: TaskId,
        limit: usize,
    ) -> PersistenceResult<Vec<TaskLogRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        self.with_connection("list_task_logs", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT
    log_id,
    task_id,
    manager_id,
    task_type,
    status,
    level,
    message,
    created_at_unix
FROM task_log_records
WHERE task_id = ?1
ORDER BY created_at_unix DESC, log_id DESC
LIMIT ?2
",
            )?;
            let rows =
                statement.query_map(params![task_id_to_i64(task_id)?, to_i64(limit)?], |row| {
                    let log_id_raw: i64 = row.get(0)?;
                    let task_id_raw: i64 = row.get(1)?;
                    let manager_raw: String = row.get(2)?;
                    let task_type_raw: String = row.get(3)?;
                    let status_raw: Option<String> = row.get(4)?;
                    let level_raw: String = row.get(5)?;
                    let message: String = row.get(6)?;
                    let created_at_unix: i64 = row.get(7)?;

                    Ok(TaskLogRecord {
                        id: i64_to_u64(log_id_raw)?,
                        task_id: TaskId(i64_to_u64(task_id_raw)?),
                        manager: parse_manager_id(&manager_raw)?,
                        task_type: parse_task_type(&task_type_raw)?,
                        status: status_raw.as_deref().map(parse_task_status).transpose()?,
                        level: parse_task_log_level(&level_raw)?,
                        message,
                        created_at: from_unix_seconds(created_at_unix)?,
                    })
                })?;

            rows.collect()
        })
    }

    fn list_recent_failure_diagnostic_logs(
        &self,
        cutoff: SystemTime,
        issue_key: &str,
        limit: usize,
    ) -> PersistenceResult<Vec<TaskLogRecord>> {
        if limit == 0 || issue_key.trim().is_empty() {
            return Ok(Vec::new());
        }

        self.with_connection("list_recent_failure_diagnostic_logs", |connection| {
            ensure_schema_ready(connection)?;
            let issue_pattern = format!("%\"issueKey\":\"{}\"%", issue_key.trim());
            let mut statement = connection.prepare(
                "
SELECT
    logs.log_id,
    logs.task_id,
    logs.manager_id,
    logs.task_type,
    logs.status,
    logs.level,
    logs.message,
    logs.created_at_unix
FROM task_log_records AS logs
INNER JOIN task_records AS tasks ON tasks.task_id = logs.task_id
WHERE tasks.status = 'failed'
  AND logs.created_at_unix >= ?1
  AND logs.message LIKE '[diagnostic.v1] %'
  AND logs.message LIKE ?2
ORDER BY logs.created_at_unix DESC, logs.log_id DESC
LIMIT ?3
",
            )?;
            let rows = statement.query_map(
                params![to_unix_seconds(cutoff)?, issue_pattern, to_i64(limit)?],
                |row| {
                    let log_id_raw: i64 = row.get(0)?;
                    let task_id_raw: i64 = row.get(1)?;
                    let manager_raw: String = row.get(2)?;
                    let task_type_raw: String = row.get(3)?;
                    let status_raw: Option<String> = row.get(4)?;
                    let level_raw: String = row.get(5)?;
                    let message: String = row.get(6)?;
                    let created_at_unix: i64 = row.get(7)?;

                    Ok(TaskLogRecord {
                        id: i64_to_u64(log_id_raw)?,
                        task_id: TaskId(i64_to_u64(task_id_raw)?),
                        manager: parse_manager_id(&manager_raw)?,
                        task_type: parse_task_type(&task_type_raw)?,
                        status: status_raw.as_deref().map(parse_task_status).transpose()?,
                        level: parse_task_log_level(&level_raw)?,
                        message,
                        created_at: from_unix_seconds(created_at_unix)?,
                    })
                },
            )?;

            rows.collect()
        })
    }

    fn prune_task_logs(&self, max_age_secs: i64) -> PersistenceResult<usize> {
        self.with_connection("prune_task_logs", |connection| {
            ensure_schema_ready(connection)?;
            let cutoff = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs() as i64
                - max_age_secs;
            let deleted = connection.execute(
                "
DELETE FROM task_log_records
WHERE created_at_unix < ?1
",
                params![cutoff],
            )?;
            Ok(deleted)
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
VALUES (?1, ?2, NULLIF(?3, ''), NULLIF(?4, ''), strftime('%s', 'now'))
ON CONFLICT(manager_id) DO UPDATE SET
    detected = excluded.detected,
    executable_path = CASE
        WHEN excluded.detected = 1 THEN COALESCE(
            NULLIF(excluded.executable_path, ''),
            NULLIF(manager_detection.executable_path, '')
        )
        ELSE excluded.executable_path
    END,
    version = CASE
        WHEN excluded.detected = 1 THEN COALESCE(
            NULLIF(excluded.version, ''),
            NULLIF(manager_detection.version, '')
        )
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

    fn replace_install_instances(
        &self,
        manager: ManagerId,
        instances: &[ManagerInstallInstance],
    ) -> PersistenceResult<()> {
        self.with_connection("replace_install_instances", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;

            transaction.execute(
                "DELETE FROM manager_install_instances WHERE manager_id = ?1",
                params![manager.as_str()],
            )?;

            if !instances.is_empty() {
                let mut statement = transaction.prepare(
                    "
INSERT INTO manager_install_instances (
    manager_id,
    instance_id,
    identity_kind,
    identity_value,
    display_path,
    canonical_path,
    alias_paths_json,
    is_active,
    version,
    provenance,
    confidence,
    automation_level,
    uninstall_strategy,
    update_strategy,
    remediation_strategy,
    explanation_primary,
    explanation_secondary,
    competing_provenance,
    competing_confidence,
    decision_margin,
    detected_at_unix
)
VALUES (
    ?1, ?2, ?3, ?4, ?5, NULLIF(?6, ''), ?7, ?8, NULLIF(?9, ''), ?10, ?11, ?12, ?13, ?14, ?15, NULLIF(?16, ''), NULLIF(?17, ''), NULLIF(?18, ''), ?19, ?20, strftime('%s', 'now')
)
",
                )?;

                for instance in instances {
                    let alias_paths: Vec<String> = instance
                        .alias_paths
                        .iter()
                        .map(|path| path.to_string_lossy().to_string())
                        .collect();
                    let alias_paths_json = serde_json::to_string(&alias_paths).unwrap_or_else(|_| {
                        "[]".to_string()
                    });
                    statement.execute(params![
                        manager.as_str(),
                        instance.instance_id.as_str(),
                        instance.identity_kind.as_str(),
                        instance.identity_value.as_str(),
                        instance.display_path.to_string_lossy().to_string(),
                        instance
                            .canonical_path
                            .as_ref()
                            .map(|path| path.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        alias_paths_json,
                        bool_to_sqlite(instance.is_active),
                        instance.version.as_deref().unwrap_or_default(),
                        instance.provenance.as_str(),
                        instance.confidence,
                        instance.automation_level.as_str(),
                        instance.uninstall_strategy.as_str(),
                        instance.update_strategy.as_str(),
                        instance.remediation_strategy.as_str(),
                        instance.explanation_primary.as_deref().unwrap_or_default(),
                        instance.explanation_secondary.as_deref().unwrap_or_default(),
                        instance
                            .competing_provenance
                            .map(|value| value.as_str())
                            .unwrap_or_default(),
                        instance.competing_confidence,
                        instance.decision_margin,
                    ])?;
                }
            }

            transaction.commit()?;
            Ok(())
        })
    }

    fn list_install_instances(
        &self,
        manager: Option<ManagerId>,
    ) -> PersistenceResult<Vec<ManagerInstallInstance>> {
        self.with_connection("list_install_instances", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT manager_id,
       instance_id,
       identity_kind,
       identity_value,
       display_path,
       canonical_path,
       alias_paths_json,
       is_active,
       version,
       provenance,
       confidence,
       automation_level,
       uninstall_strategy,
       update_strategy,
       remediation_strategy,
       explanation_primary,
       explanation_secondary,
       competing_provenance,
       competing_confidence,
       decision_margin
FROM manager_install_instances
WHERE (?1 IS NULL OR manager_id = ?1)
ORDER BY manager_id, is_active DESC, instance_id
",
            )?;
            let manager_filter = manager.map(|value| value.as_str().to_string());
            let rows = statement.query_map(params![manager_filter], |row| {
                let manager_raw: String = row.get(0)?;
                let instance_id: String = row.get(1)?;
                let identity_kind_raw: String = row.get(2)?;
                let identity_value: String = row.get(3)?;
                let display_path_raw: String = row.get(4)?;
                let canonical_path_raw: Option<String> = row.get(5)?;
                let alias_paths_json: String = row.get(6)?;
                let is_active_raw: i64 = row.get(7)?;
                let version_raw: Option<String> = row.get(8)?;
                let provenance_raw: String = row.get(9)?;
                let confidence: f64 = row.get(10)?;
                let automation_level_raw: String = row.get(11)?;
                let uninstall_strategy_raw: String = row.get(12)?;
                let update_strategy_raw: String = row.get(13)?;
                let remediation_strategy_raw: String = row.get(14)?;
                let explanation_primary_raw: Option<String> = row.get(15)?;
                let explanation_secondary_raw: Option<String> = row.get(16)?;
                let competing_provenance_raw: Option<String> = row.get(17)?;
                let competing_confidence: Option<f64> = row.get(18)?;
                let decision_margin: Option<f64> = row.get(19)?;

                let manager = parse_manager_id(&manager_raw)?;
                let identity_kind = parse_install_instance_identity_kind(&identity_kind_raw)?;
                let provenance = parse_install_provenance(&provenance_raw)?;
                let automation_level = parse_automation_level(&automation_level_raw)?;
                let uninstall_strategy = parse_strategy_kind(&uninstall_strategy_raw)?;
                let update_strategy = parse_strategy_kind(&update_strategy_raw)?;
                let remediation_strategy = parse_strategy_kind(&remediation_strategy_raw)?;
                let alias_paths_raw: Vec<String> =
                    serde_json::from_str(&alias_paths_json).unwrap_or_default();
                let alias_paths = alias_paths_raw
                    .into_iter()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>();

                Ok(ManagerInstallInstance {
                    manager,
                    instance_id,
                    identity_kind,
                    identity_value,
                    display_path: PathBuf::from(display_path_raw),
                    canonical_path: normalize_optional_text(canonical_path_raw).map(PathBuf::from),
                    alias_paths,
                    is_active: sqlite_to_bool(is_active_raw),
                    version: normalize_optional_text(version_raw),
                    provenance,
                    confidence,
                    decision_margin,
                    automation_level,
                    uninstall_strategy,
                    update_strategy,
                    remediation_strategy,
                    explanation_primary: normalize_optional_text(explanation_primary_raw),
                    explanation_secondary: normalize_optional_text(explanation_secondary_raw),
                    competing_provenance: normalize_optional_text(competing_provenance_raw)
                        .and_then(|value: String| value.parse::<InstallProvenance>().ok()),
                    competing_confidence,
                })
            })?;

            rows.collect()
        })
    }

    fn set_manager_multi_instance_ack_fingerprint(
        &self,
        manager: ManagerId,
        fingerprint: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_manager_multi_instance_ack_fingerprint", |connection| {
            ensure_schema_ready(connection)?;
            if let Some(value) = fingerprint.map(str::trim).filter(|entry| !entry.is_empty()) {
                connection.execute(
                    "
INSERT INTO manager_multi_instance_ack (
    manager_id,
    instances_fingerprint,
    acknowledged_at_unix
)
VALUES (?1, ?2, strftime('%s', 'now'))
ON CONFLICT(manager_id) DO UPDATE SET
    instances_fingerprint = excluded.instances_fingerprint,
    acknowledged_at_unix = excluded.acknowledged_at_unix
",
                    params![manager.as_str(), value],
                )?;
            } else {
                connection.execute(
                    "DELETE FROM manager_multi_instance_ack WHERE manager_id = ?1",
                    params![manager.as_str()],
                )?;
            }
            Ok(())
        })
    }

    fn manager_multi_instance_ack_fingerprint(
        &self,
        manager: ManagerId,
    ) -> PersistenceResult<Option<String>> {
        self.with_connection("manager_multi_instance_ack_fingerprint", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "SELECT instances_fingerprint
                 FROM manager_multi_instance_ack
                 WHERE manager_id = ?1",
            )?;
            let value = statement
                .query_row(params![manager.as_str()], |row| row.get::<_, String>(0))
                .optional()?;
            Ok(value.and_then(|entry| normalize_optional_text(Some(entry))))
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

    fn set_manager_selected_executable_path(
        &self,
        manager: ManagerId,
        path: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_manager_selected_executable_path", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO manager_preferences (manager_id, enabled, selected_executable_path)
VALUES (
    ?1,
    COALESCE((SELECT enabled FROM manager_preferences WHERE manager_id = ?1), 1),
    NULLIF(?2, '')
)
ON CONFLICT(manager_id) DO UPDATE SET
    selected_executable_path = NULLIF(excluded.selected_executable_path, '')
",
                params![manager.as_str(), path],
            )?;
            Ok(())
        })
    }

    fn set_manager_selected_install_method(
        &self,
        manager: ManagerId,
        method: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_manager_selected_install_method", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO manager_preferences (manager_id, enabled, selected_install_method)
VALUES (
    ?1,
    COALESCE((SELECT enabled FROM manager_preferences WHERE manager_id = ?1), 1),
    NULLIF(?2, '')
)
ON CONFLICT(manager_id) DO UPDATE SET
    selected_install_method = NULLIF(excluded.selected_install_method, '')
",
                params![manager.as_str(), method],
            )?;
            Ok(())
        })
    }

    fn set_manager_timeout_hard_seconds(
        &self,
        manager: ManagerId,
        seconds: Option<u64>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_manager_timeout_hard_seconds", |connection| {
            ensure_schema_ready(connection)?;
            let seconds = seconds.and_then(|value| i64::try_from(value).ok());
            connection.execute(
                "
INSERT INTO manager_preferences (manager_id, enabled, timeout_hard_seconds)
VALUES (
    ?1,
    COALESCE((SELECT enabled FROM manager_preferences WHERE manager_id = ?1), 1),
    ?2
)
ON CONFLICT(manager_id) DO UPDATE SET
    timeout_hard_seconds = excluded.timeout_hard_seconds
",
                params![manager.as_str(), seconds],
            )?;
            Ok(())
        })
    }

    fn set_manager_timeout_idle_seconds(
        &self,
        manager: ManagerId,
        seconds: Option<u64>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_manager_timeout_idle_seconds", |connection| {
            ensure_schema_ready(connection)?;
            let seconds = seconds.and_then(|value| i64::try_from(value).ok());
            connection.execute(
                "
INSERT INTO manager_preferences (manager_id, enabled, timeout_idle_seconds)
VALUES (
    ?1,
    COALESCE((SELECT enabled FROM manager_preferences WHERE manager_id = ?1), 1),
    ?2
)
ON CONFLICT(manager_id) DO UPDATE SET
    timeout_idle_seconds = excluded.timeout_idle_seconds
",
                params![manager.as_str(), seconds],
            )?;
            Ok(())
        })
    }

    fn list_manager_preferences(&self) -> PersistenceResult<Vec<ManagerPreference>> {
        self.with_connection("list_manager_preferences", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT manager_id,
       enabled,
       selected_executable_path,
       selected_install_method,
       timeout_hard_seconds,
       timeout_idle_seconds
FROM manager_preferences
ORDER BY manager_id
",
            )?;

            let rows = statement.query_map([], |row| {
                let manager_raw: String = row.get(0)?;
                let enabled_int: i64 = row.get(1)?;
                let selected_executable_path: Option<String> = row.get(2)?;
                let selected_install_method: Option<String> = row.get(3)?;
                let timeout_hard_seconds_raw: Option<i64> = row.get(4)?;
                let timeout_idle_seconds_raw: Option<i64> = row.get(5)?;

                let manager = parse_manager_id(&manager_raw)?;
                Ok(ManagerPreference {
                    manager,
                    enabled: sqlite_to_bool(enabled_int),
                    selected_executable_path,
                    selected_install_method,
                    timeout_hard_seconds: timeout_hard_seconds_raw
                        .and_then(|value| u64::try_from(value).ok())
                        .filter(|value| *value > 0),
                    timeout_idle_seconds: timeout_idle_seconds_raw
                        .and_then(|value| u64::try_from(value).ok())
                        .filter(|value| *value > 0),
                })
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

    fn set_auto_check_for_updates(&self, enabled: bool) -> PersistenceResult<()> {
        self.with_connection("set_auto_check_for_updates", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO app_settings (key, value)
VALUES ('auto_check_for_updates', ?1)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value
",
                params![if enabled { "1" } else { "0" }],
            )?;
            Ok(())
        })
    }

    fn auto_check_for_updates(&self) -> PersistenceResult<bool> {
        self.with_connection("auto_check_for_updates", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection
                .prepare("SELECT value FROM app_settings WHERE key = 'auto_check_for_updates'")?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                return Ok(false);
            };
            let value: String = row.get(0)?;
            Ok(value.trim() == "1")
        })
    }

    fn set_auto_check_frequency_minutes(&self, minutes: u32) -> PersistenceResult<()> {
        self.with_connection("set_auto_check_frequency_minutes", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO app_settings (key, value)
VALUES ('auto_check_frequency_minutes', ?1)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value
",
                params![minutes.to_string()],
            )?;
            Ok(())
        })
    }

    fn auto_check_frequency_minutes(&self) -> PersistenceResult<u32> {
        self.with_connection("auto_check_frequency_minutes", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "SELECT value FROM app_settings WHERE key = 'auto_check_frequency_minutes'",
            )?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                return Ok(1_440);
            };
            let value: String = row.get(0)?;
            let parsed = value.trim().parse::<u32>().unwrap_or(1_440);
            if parsed == 0 { Ok(1_440) } else { Ok(parsed) }
        })
    }

    fn set_auto_check_last_checked_unix(&self, value: i64) -> PersistenceResult<()> {
        self.with_connection("set_auto_check_last_checked_unix", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO app_settings (key, value)
VALUES ('auto_check_last_checked_unix', ?1)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value
",
                params![value.to_string()],
            )?;
            Ok(())
        })
    }

    fn auto_check_last_checked_unix(&self) -> PersistenceResult<Option<i64>> {
        self.with_connection("auto_check_last_checked_unix", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "SELECT value FROM app_settings WHERE key = 'auto_check_last_checked_unix'",
            )?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            let value: String = row.get(0)?;
            let parsed = value.trim().parse::<i64>().ok();
            Ok(parsed)
        })
    }

    fn set_cli_onboarding_completed(&self, completed: bool) -> PersistenceResult<()> {
        self.with_connection("set_cli_onboarding_completed", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "
INSERT INTO app_settings (key, value)
VALUES ('cli_onboarding_completed', ?1)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value
",
                params![if completed { "1" } else { "0" }],
            )?;
            Ok(())
        })
    }

    fn cli_onboarding_completed(&self) -> PersistenceResult<bool> {
        self.with_connection("cli_onboarding_completed", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection
                .prepare("SELECT value FROM app_settings WHERE key = 'cli_onboarding_completed'")?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                return Ok(false);
            };
            let value: String = row.get(0)?;
            Ok(value.trim() == "1")
        })
    }

    fn set_cli_accepted_license_terms_version(
        &self,
        version: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_cli_accepted_license_terms_version", |connection| {
            ensure_schema_ready(connection)?;
            match version {
                Some(value) => {
                    connection.execute(
                        "
INSERT INTO app_settings (key, value)
VALUES ('cli_accepted_license_terms_version', ?1)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value
",
                        params![value],
                    )?;
                }
                None => {
                    connection.execute(
                        "DELETE FROM app_settings WHERE key = 'cli_accepted_license_terms_version'",
                        [],
                    )?;
                }
            }
            Ok(())
        })
    }

    fn cli_accepted_license_terms_version(&self) -> PersistenceResult<Option<String>> {
        self.with_connection("cli_accepted_license_terms_version", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "SELECT value FROM app_settings WHERE key = 'cli_accepted_license_terms_version'",
            )?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            let value: String = row.get(0)?;
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            Ok(Some(trimmed.to_string()))
        })
    }

    fn set_manager_priority_overrides_json(
        &self,
        overrides_json: Option<&str>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_manager_priority_overrides_json", |connection| {
            ensure_schema_ready(connection)?;
            match overrides_json {
                Some(json) => {
                    connection.execute(
                        "
INSERT INTO app_settings (key, value)
VALUES ('manager_priority_overrides', ?1)
ON CONFLICT(key) DO UPDATE SET
    value = excluded.value
",
                        params![json],
                    )?;
                }
                None => {
                    connection.execute(
                        "DELETE FROM app_settings WHERE key = 'manager_priority_overrides'",
                        [],
                    )?;
                }
            }
            Ok(())
        })
    }

    fn manager_priority_overrides_json(&self) -> PersistenceResult<Option<String>> {
        self.with_connection("manager_priority_overrides_json", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "SELECT value FROM app_settings WHERE key = 'manager_priority_overrides'",
            )?;
            let mut rows = statement.query([])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            let value: String = row.get(0)?;
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed))
            }
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

    fn set_package_manager_preference(
        &self,
        package_family_key: &str,
        manager: Option<ManagerId>,
    ) -> PersistenceResult<()> {
        self.with_connection("set_package_manager_preference", |connection| {
            ensure_schema_ready(connection)?;
            let Some(normalized_package_family_key) =
                normalize_package_family_key(package_family_key)
            else {
                return Err(storage_error_sqlite("package_family_key cannot be empty"));
            };

            match manager {
                Some(manager) => {
                    connection.execute(
                        "
INSERT INTO package_manager_preferences (package_name, manager_id, updated_at_unix)
VALUES (?1, ?2, strftime('%s', 'now'))
ON CONFLICT(package_name) DO UPDATE SET
    manager_id = excluded.manager_id,
    updated_at_unix = excluded.updated_at_unix
",
                        params![normalized_package_family_key, manager.as_str()],
                    )?;
                }
                None => {
                    connection.execute(
                        "DELETE FROM package_manager_preferences WHERE package_name = ?1",
                        params![normalized_package_family_key],
                    )?;
                }
            }

            Ok(())
        })
    }

    fn package_manager_preference(
        &self,
        package_family_key: &str,
    ) -> PersistenceResult<Option<ManagerId>> {
        self.with_connection("package_manager_preference", |connection| {
            ensure_schema_ready(connection)?;
            let Some(normalized_package_family_key) =
                normalize_package_family_key(package_family_key)
            else {
                return Ok(None);
            };

            let mut statement = connection.prepare(
                "
SELECT manager_id
FROM package_manager_preferences
WHERE package_name = ?1
",
            )?;
            let mut rows = statement.query(params![normalized_package_family_key])?;
            let Some(row) = rows.next()? else {
                return Ok(None);
            };
            let manager_raw: String = row.get(0)?;
            Ok(Some(parse_manager_id(manager_raw.as_str())?))
        })
    }

    fn list_package_manager_preferences(&self) -> PersistenceResult<Vec<PackageManagerPreference>> {
        self.with_connection("list_package_manager_preferences", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "
SELECT package_name, manager_id
FROM package_manager_preferences
ORDER BY package_name
",
            )?;
            let rows = statement.query_map([], |row| {
                let package_family_key: String = row.get(0)?;
                let manager_raw: String = row.get(1)?;
                let manager = parse_manager_id(manager_raw.as_str())?;
                Ok(PackageManagerPreference {
                    package_family_key,
                    manager,
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
    let connection = Connection::open(database_path)?;
    connection.execute_batch(
        "
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA busy_timeout = 5000;
",
    )?;
    Ok(connection)
}

fn ensure_migrations_table(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        "
CREATE TABLE IF NOT EXISTS helm_schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at_unix INTEGER NOT NULL,
    definition_checksum TEXT
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

fn replaced_migration_reconciliation_required(
    connection: &Connection,
    current_version: i64,
    target_version: i64,
) -> rusqlite::Result<bool> {
    const REPLACED_MIGRATION_VERSION: i64 = 17;
    if current_version < REPLACED_MIGRATION_VERSION || target_version < REPLACED_MIGRATION_VERSION {
        return Ok(false);
    }
    let recorded_name: Option<String> = connection
        .query_row(
            &format!("SELECT name FROM {MIGRATIONS_TABLE} WHERE version = ?1"),
            [REPLACED_MIGRATION_VERSION],
            |row| row.get(0),
        )
        .optional()?;
    Ok(recorded_name.as_deref() == Some("add_advisory_cache"))
}

fn create_pre_migration_backup(
    connection: &Connection,
    database_path: &Path,
    current_version: i64,
) -> rusqlite::Result<PathBuf> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?
        .as_nanos();
    let file_name = database_path
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("helm.db"))
        .to_string_lossy();
    let parent = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let backup_path = parent.join(format!(
        "{file_name}.pre-migration-v{current_version}-{timestamp}.backup"
    ));
    let partial_path = backup_path.with_extension("backup.partial");

    let result = (|| {
        create_private_file(&partial_path)?;
        connection.backup(DatabaseName::Main, &partial_path, None)?;
        let backup = Connection::open(&partial_path)?;
        let integrity: String = backup.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(storage_error_sqlite(&format!(
                "pre-migration backup integrity check failed: {integrity}"
            )));
        }
        drop(backup);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&partial_path, fs::Permissions::from_mode(0o600))
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
        fs::rename(&partial_path, &backup_path)
            .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        if let Err(error) = prune_pre_migration_backups(database_path, 3) {
            tracing::warn!(
                error = %error,
                "failed to prune old pre-migration SQLite backups"
            );
        }
        Ok(backup_path.clone())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&partial_path);
    }
    result
}

fn create_private_file(path: &Path) -> rusqlite::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map(drop)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

fn pre_migration_backup_paths(database_path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let parent = database_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let prefix = format!(
        "{}.pre-migration-",
        database_path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("helm.db"))
            .to_string_lossy()
    );
    let mut backups = fs::read_dir(parent)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with(&prefix)
                    && (name.ends_with(".backup") || name.ends_with(".backup.partial"))
            })
        })
        .collect::<Vec<_>>();
    backups.sort_by(|left, right| {
        let left_modified = fs::metadata(left)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        let right_modified = fs::metadata(right)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(UNIX_EPOCH);
        left_modified
            .cmp(&right_modified)
            .then_with(|| left.cmp(right))
    });
    Ok(backups)
}

fn prune_pre_migration_backups(database_path: &Path, retain: usize) -> std::io::Result<()> {
    let backups = pre_migration_backup_paths(database_path)?;
    let (partial, completed): (Vec<_>, Vec<_>) = backups
        .into_iter()
        .partition(|path| path.to_string_lossy().ends_with(".backup.partial"));
    for backup in partial {
        fs::remove_file(backup)?;
    }
    let remove_count = completed.len().saturating_sub(retain);
    for backup in completed.into_iter().take(remove_count) {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn remove_pre_migration_backups(database_path: &Path) -> std::io::Result<()> {
    for backup in pre_migration_backup_paths(database_path)? {
        fs::remove_file(backup)?;
    }
    Ok(())
}

fn migration_checksum_column_exists(connection: &Connection) -> rusqlite::Result<bool> {
    let mut statement = connection.prepare("PRAGMA table_info(helm_schema_migrations)")?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    for column in columns {
        if column? == "definition_checksum" {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_applied_migration_identities(
    connection: &Connection,
    current_version: i64,
) -> rusqlite::Result<()> {
    if current_version < 0 || current_version > current_schema_version() {
        return Err(storage_error_sqlite(&format!(
            "database migration version {current_version} is outside the supported range"
        )));
    }

    let has_checksums = migration_checksum_column_exists(connection)?;
    let records = if has_checksums {
        let mut statement = connection.prepare(&format!(
            "SELECT version, name, definition_checksum FROM {MIGRATIONS_TABLE} ORDER BY version"
        ))?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        let mut statement = connection.prepare(&format!(
            "SELECT version, name FROM {MIGRATIONS_TABLE} ORDER BY version"
        ))?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, None))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    if i64::try_from(records.len()).ok() != Some(current_version) {
        return Err(storage_error_sqlite(
            "database migration ledger is not contiguous",
        ));
    }

    for (index, (version, name, checksum)) in records.iter().enumerate() {
        let expected_version = i64::try_from(index + 1)
            .map_err(|_| storage_error_sqlite("database migration ledger is too large"))?;
        if *version != expected_version {
            return Err(storage_error_sqlite(&format!(
                "database migration ledger is missing version {expected_version}"
            )));
        }
        let expected = migration(*version).ok_or_else(|| {
            storage_error_sqlite(&format!(
                "database contains unknown migration version {version}"
            ))
        })?;
        if name != expected.name {
            return Err(storage_error_sqlite(&format!(
                "unexpected migration identity for version {version}: '{name}'"
            )));
        }
        if has_checksums
            && current_version >= MIGRATION_CHECKSUM_SCHEMA_VERSION
            && checksum.as_deref().is_none_or(str::is_empty)
        {
            return Err(storage_error_sqlite(&format!(
                "missing migration checksum for version {version}"
            )));
        }
        if let Some(checksum) = checksum.as_deref().filter(|value| !value.is_empty()) {
            let expected_checksum = migration_definition_checksum_for_version(*version)
                .map_err(|error| storage_error_sqlite(&error))?;
            if checksum != expected_checksum {
                return Err(storage_error_sqlite(&format!(
                    "unexpected migration checksum for version {version}"
                )));
            }
        }
    }
    Ok(())
}

fn backfill_migration_checksums(
    connection: &mut Connection,
    current_version: i64,
) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    backfill_migration_checksums_in_transaction(&transaction, current_version)?;
    transaction.commit()?;
    validate_applied_migration_identities(connection, current_version)
}

fn backfill_migration_checksums_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    current_version: i64,
) -> rusqlite::Result<()> {
    for version in 1..=current_version {
        let checksum = migration_definition_checksum_for_version(version)
            .map_err(|error| storage_error_sqlite(&error))?;
        transaction.execute(
            &format!(
                "UPDATE {MIGRATIONS_TABLE}
                 SET definition_checksum = ?2
                 WHERE version = ?1
                   AND (definition_checksum IS NULL OR definition_checksum = '')"
            ),
            (version, checksum),
        )?;
    }
    Ok(())
}

fn reconcile_replaced_migrations(
    connection: &mut Connection,
    current_version: i64,
    target_version: i64,
) -> rusqlite::Result<()> {
    // A development build used version 17 for add_advisory_cache before the
    // released schema assigned it to doctor/repair persistence. Repair that
    // known collision before later migrations depend on the released tables.
    const REPLACED_MIGRATION_VERSION: i64 = 17;
    const REPLACED_MIGRATION_NAME: &str = "add_advisory_cache";
    if current_version < REPLACED_MIGRATION_VERSION || target_version < REPLACED_MIGRATION_VERSION {
        return Ok(());
    }

    let expected = migration(REPLACED_MIGRATION_VERSION)
        .expect("replaced migration version must remain defined");
    let recorded_name: Option<String> = connection
        .query_row(
            &format!("SELECT name FROM {MIGRATIONS_TABLE} WHERE version = ?1"),
            [REPLACED_MIGRATION_VERSION],
            |row| row.get(0),
        )
        .optional()?;
    match recorded_name.as_deref() {
        None => return Ok(()),
        Some(name) if name == expected.name => return Ok(()),
        Some(REPLACED_MIGRATION_NAME) => {}
        Some(name) => {
            return Err(storage_error_sqlite(&format!(
                "unexpected migration identity for version {REPLACED_MIGRATION_VERSION}: '{name}'"
            )));
        }
    }

    let transaction = connection.transaction()?;
    execute_batch_tolerant(&transaction, expected.up_sql)?;
    transaction.execute("DROP TABLE IF EXISTS advisory_cache", [])?;
    transaction.execute(
        &format!(
            "UPDATE {MIGRATIONS_TABLE}
             SET name = ?2, applied_at_unix = strftime('%s', 'now')
             WHERE version = ?1"
        ),
        (REPLACED_MIGRATION_VERSION, expected.name),
    )?;
    transaction.commit()?;
    Ok(())
}

fn apply_up_migration(
    connection: &mut Connection,
    migration: &SqliteMigration,
) -> rusqlite::Result<()> {
    let transaction = connection.transaction()?;
    execute_batch_tolerant(&transaction, migration.up_sql)?;
    if migration_checksum_column_exists(&transaction)? {
        let checksum = migration_definition_checksum_for_version(migration.version)
            .map_err(|error| storage_error_sqlite(&error))?;
        transaction.execute(
            &format!(
                "INSERT INTO {MIGRATIONS_TABLE}
                    (version, name, applied_at_unix, definition_checksum)
                 VALUES (?1, ?2, strftime('%s', 'now'), ?3)"
            ),
            (migration.version, migration.name, checksum),
        )?;
    } else {
        transaction.execute(
            &format!(
                "INSERT INTO {MIGRATIONS_TABLE} (version, name, applied_at_unix)
                 VALUES (?1, ?2, strftime('%s', 'now'))"
            ),
            (migration.version, migration.name),
        )?;
    }
    if migration.version == MIGRATION_CHECKSUM_SCHEMA_VERSION {
        backfill_migration_checksums_in_transaction(&transaction, migration.version)?;
    }
    transaction.commit()?;
    Ok(())
}

/// Execute a SQL batch, tolerating "duplicate column name" errors from
/// `ALTER TABLE ADD COLUMN` which is not idempotent in SQLite.
///
/// # Design rationale
///
/// SQLite's `ALTER TABLE ADD COLUMN` does not support `IF NOT EXISTS`. When a
/// forward migration encounters a column already restored by a targeted
/// schema repair, the statement fails with "duplicate column name: \<column\>".
///
/// This function deliberately swallows that specific error class. The scope
/// of error tolerance is narrow:
///
/// - **Tolerated**: errors whose message contains "duplicate column name"
/// - **Propagated**: all other `rusqlite::Error` variants (syntax errors,
///   constraint violations, I/O failures, etc.)
///
/// Called from normal forward migrations and targeted migration reconciliation.
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

fn parse_install_instance_identity_kind(
    raw: &str,
) -> rusqlite::Result<InstallInstanceIdentityKind> {
    raw.parse::<InstallInstanceIdentityKind>().map_err(|_| {
        storage_error_sqlite(&format!(
            "unknown install identity kind '{raw}' in sqlite record"
        ))
    })
}

fn parse_install_provenance(raw: &str) -> rusqlite::Result<InstallProvenance> {
    raw.parse::<InstallProvenance>().map_err(|_| {
        storage_error_sqlite(&format!(
            "unknown install provenance '{raw}' in sqlite record"
        ))
    })
}

fn parse_automation_level(raw: &str) -> rusqlite::Result<AutomationLevel> {
    raw.parse::<AutomationLevel>().map_err(|_| {
        storage_error_sqlite(&format!(
            "unknown automation level '{raw}' in sqlite record"
        ))
    })
}

fn parse_strategy_kind(raw: &str) -> rusqlite::Result<StrategyKind> {
    raw.parse::<StrategyKind>().map_err(|_| {
        storage_error_sqlite(&format!("unknown strategy kind '{raw}' in sqlite record"))
    })
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
        TaskType::CatalogSync => "catalog_sync",
        TaskType::Install => "install",
        TaskType::Uninstall => "uninstall",
        TaskType::Upgrade => "upgrade",
        TaskType::Configure => "configure",
        TaskType::Pin => "pin",
        TaskType::Unpin => "unpin",
    }
}

fn parse_task_type(raw: &str) -> rusqlite::Result<TaskType> {
    match raw {
        "detection" => Ok(TaskType::Detection),
        "refresh" => Ok(TaskType::Refresh),
        "search" => Ok(TaskType::Search),
        "catalog_sync" => Ok(TaskType::CatalogSync),
        "install" => Ok(TaskType::Install),
        "uninstall" => Ok(TaskType::Uninstall),
        "upgrade" => Ok(TaskType::Upgrade),
        "configure" => Ok(TaskType::Configure),
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

fn task_log_level_to_str(value: TaskLogLevel) -> &'static str {
    match value {
        TaskLogLevel::Info => "info",
        TaskLogLevel::Warn => "warn",
        TaskLogLevel::Error => "error",
    }
}

fn parse_task_log_level(raw: &str) -> rusqlite::Result<TaskLogLevel> {
    match raw {
        "info" => Ok(TaskLogLevel::Info),
        "warn" => Ok(TaskLogLevel::Warn),
        "error" => Ok(TaskLogLevel::Error),
        _ => Err(storage_error_sqlite(&format!(
            "unknown task log level '{raw}' in sqlite record"
        ))),
    }
}

fn bool_to_sqlite(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn sqlite_to_bool(value: i64) -> bool {
    value != 0
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn to_installed_version_token(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn from_installed_version_token(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn single_version_snapshot_manager(manager: ManagerId) -> bool {
    !matches!(
        manager,
        ManagerId::Asdf
            | ManagerId::Mise
            | ManagerId::Rustup
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::MacPorts
    )
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

impl AdvisoryCacheStore for SqliteStore {
    fn upsert_advisories(&self, records: &[AdvisoryCacheRecord]) -> Result<usize, String> {
        self.with_connection("upsert_advisories", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            let mut changed = 0usize;
            {
                let mut statement = transaction.prepare(
                    "INSERT INTO security_advisories (
                        cache_key, advisory_id, ecosystem, scope, package_name,
                        affected_range_json, severity, cvss_score, summary, description,
                        fixed_version, source_provider, source_feed, fetched_at_epoch_ms,
                        expires_at_epoch_ms
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
                    )
                    ON CONFLICT(cache_key) DO UPDATE SET
                        advisory_id = excluded.advisory_id,
                        ecosystem = excluded.ecosystem,
                        scope = excluded.scope,
                        package_name = excluded.package_name,
                        affected_range_json = excluded.affected_range_json,
                        severity = excluded.severity,
                        cvss_score = excluded.cvss_score,
                        summary = excluded.summary,
                        description = excluded.description,
                        fixed_version = excluded.fixed_version,
                        source_provider = excluded.source_provider,
                        source_feed = excluded.source_feed,
                        fetched_at_epoch_ms = excluded.fetched_at_epoch_ms,
                        expires_at_epoch_ms = excluded.expires_at_epoch_ms
                    WHERE excluded.fetched_at_epoch_ms >= security_advisories.fetched_at_epoch_ms",
                )?;

                for record in records {
                    validate_advisory_cache_record(record)?;
                    changed += statement.execute(params![
                        record.cache_key,
                        record.advisory_id,
                        record.ecosystem,
                        record.scope,
                        record.package_name,
                        record.affected_range_json,
                        record.severity,
                        record.cvss_score,
                        record.summary,
                        record.description,
                        record.fixed_version,
                        record.source_provider,
                        record.source_feed,
                        record.fetched_at_epoch_ms,
                        record.expires_at_epoch_ms,
                    ])?;
                }
            }
            transaction.commit()?;
            Ok(changed)
        })
        .map_err(|error| error.to_string())
    }

    fn get_advisories_for_package(
        &self,
        ecosystem: &str,
        package_name: &str,
    ) -> Result<Vec<AdvisoryCacheRecord>, String> {
        let ecosystem_norm = crate::security_advisory::normalize_ecosystem(ecosystem);
        let package_norm = crate::security_advisory::normalize_package_name(package_name);
        self.query_advisory_records(
            "get_advisories_for_package",
            "WHERE ecosystem = ?1 AND package_name = ?2 ORDER BY cache_key ASC",
            params![ecosystem_norm, package_norm],
        )
    }

    fn get_advisories_by_source(
        &self,
        source_provider: &str,
    ) -> Result<Vec<AdvisoryCacheRecord>, String> {
        let provider_norm = crate::security_advisory::normalize_source_provider(source_provider);
        self.query_advisory_records(
            "get_advisories_by_source",
            "WHERE source_provider = ?1 ORDER BY cache_key ASC",
            params![provider_norm],
        )
    }

    fn prune_expired(&self, before_epoch_ms: i64) -> Result<usize, String> {
        self.with_connection("prune_expired_advisories", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute(
                "DELETE FROM security_advisories WHERE expires_at_epoch_ms <= ?1",
                [before_epoch_ms],
            )
        })
        .map_err(|error| error.to_string())
    }

    fn clear_all(&self) -> Result<(), String> {
        self.with_connection("clear_all_advisories", |connection| {
            ensure_schema_ready(connection)?;
            connection.execute("DELETE FROM security_advisories", [])?;
            Ok(())
        })
        .map_err(|error| error.to_string())
    }

    fn count(&self) -> Result<usize, String> {
        self.with_connection("count_advisories", |connection| {
            ensure_schema_ready(connection)?;
            connection.query_row("SELECT COUNT(*) FROM security_advisories", [], |row| {
                row.get(0)
            })
        })
        .map_err(|error| error.to_string())
    }
}

impl SqliteStore {
    fn query_advisory_records<P>(
        &self,
        operation: &str,
        clause: &str,
        parameters: P,
    ) -> Result<Vec<AdvisoryCacheRecord>, String>
    where
        P: rusqlite::Params,
    {
        self.with_connection(operation, |connection| {
            ensure_schema_ready(connection)?;
            let sql = format!(
                "SELECT cache_key, advisory_id, ecosystem, scope, package_name,
                    affected_range_json, severity, cvss_score, summary, description,
                    fixed_version, source_provider, source_feed, fetched_at_epoch_ms,
                    expires_at_epoch_ms FROM security_advisories {clause}"
            );
            let mut statement = connection.prepare(sql.as_str())?;
            let rows = statement.query_map(parameters, advisory_cache_record_from_row)?;
            rows.collect()
        })
        .map_err(|error| error.to_string())
    }
}

fn advisory_cache_record_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<AdvisoryCacheRecord> {
    Ok(AdvisoryCacheRecord {
        cache_key: row.get(0)?,
        advisory_id: row.get(1)?,
        ecosystem: row.get(2)?,
        scope: row.get(3)?,
        package_name: row.get(4)?,
        affected_range_json: row.get(5)?,
        severity: row.get(6)?,
        cvss_score: row.get(7)?,
        summary: row.get(8)?,
        description: row.get(9)?,
        fixed_version: row.get(10)?,
        source_provider: row.get(11)?,
        source_feed: row.get(12)?,
        fetched_at_epoch_ms: row.get(13)?,
        expires_at_epoch_ms: row.get(14)?,
    })
}

fn validate_advisory_cache_record(record: &AdvisoryCacheRecord) -> rusqlite::Result<()> {
    let has_required_text = [
        record.cache_key.as_str(),
        record.advisory_id.as_str(),
        record.ecosystem.as_str(),
        record.package_name.as_str(),
        record.summary.as_str(),
        record.source_provider.as_str(),
    ]
    .into_iter()
    .all(|value| !value.trim().is_empty());

    let has_identifier_control_chars = [
        record.cache_key.as_str(),
        record.advisory_id.as_str(),
        record.ecosystem.as_str(),
        record.package_name.as_str(),
        record.source_provider.as_str(),
    ]
    .into_iter()
    .any(crate::security_advisory::contains_control_chars)
        || record
            .scope
            .as_deref()
            .is_some_and(crate::security_advisory::contains_control_chars)
        || record
            .source_feed
            .as_deref()
            .is_some_and(crate::security_advisory::contains_control_chars)
        || record
            .fixed_version
            .as_deref()
            .is_some_and(crate::security_advisory::contains_control_chars);
    let has_prose_control_chars =
        crate::security_advisory::contains_forbidden_prose_control_chars(record.summary.as_str())
            || record
                .description
                .as_deref()
                .is_some_and(crate::security_advisory::contains_forbidden_prose_control_chars);

    let valid_severity = matches!(
        record.severity.as_str(),
        "low" | "medium" | "high" | "critical"
    );
    let valid_range = serde_json::from_str::<crate::security_advisory::AffectedRange>(
        record.affected_range_json.as_str(),
    )
    .is_ok_and(|range| crate::security_advisory::affected_range_is_canonical(&range));
    let valid_timestamps =
        record.fetched_at_epoch_ms > 0 && record.expires_at_epoch_ms > record.fetched_at_epoch_ms;
    let valid_score = record
        .cvss_score
        .is_none_or(|score| score.is_finite() && (0.0..=10.0).contains(&score));

    let is_canonical = record.advisory_id
        == crate::security_advisory::normalize_advisory_id(&record.advisory_id)
        && record.ecosystem == crate::security_advisory::normalize_ecosystem(&record.ecosystem)
        && record.package_name
            == crate::security_advisory::normalize_package_name(&record.package_name)
        && record.source_provider
            == crate::security_advisory::normalize_source_provider(&record.source_provider)
        && record.scope.as_ref().is_none_or(|s| {
            !s.is_empty() && s == &crate::security_advisory::normalize_package_name(s)
        })
        && record.source_feed.as_ref().is_none_or(|f| {
            !f.is_empty() && f == &crate::security_advisory::normalize_source_provider(f)
        })
        && record.fixed_version.as_ref().is_none_or(|version| {
            !version.is_empty()
                && version == &crate::security_advisory::AdvisoryFixedVersion::new(version).version
        });

    let expected_source_key = if let Some(feed) = &record.source_feed {
        format!("{}:{}", record.source_provider, feed)
    } else {
        record.source_provider.clone()
    };
    let expected_cache_key =
        format!("advisory:{}:{}", expected_source_key, record.advisory_id).to_lowercase();
    let cache_key_matches = record.cache_key == expected_cache_key;

    if has_required_text
        && !has_identifier_control_chars
        && !has_prose_control_chars
        && valid_severity
        && valid_range
        && valid_timestamps
        && valid_score
        && is_canonical
        && cache_key_matches
    {
        Ok(())
    } else {
        Err(storage_error_sqlite("invalid advisory cache record"))
    }
}

impl SqliteStore {
    fn record_failed_knowledge_import(
        &self,
        source_key: &str,
        envelope: &KnowledgeEnvelope,
        trust_level: KnowledgeTrustLevel,
        imported_at_unix: i64,
        diagnostic: &str,
    ) -> PersistenceResult<()> {
        self.with_connection("record_failed_knowledge_import", |connection| {
            ensure_schema_ready(connection)?;
            insert_knowledge_import_audit(
                connection,
                source_key,
                envelope,
                trust_level,
                imported_at_unix,
                "rejected",
                Some(diagnostic),
            )
        })
    }
}

fn current_unix_seconds() -> PersistenceResult<i64> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| storage_error_text("system_time", "clock is before Unix epoch"))?
        .as_secs();
    i64::try_from(seconds)
        .map_err(|_| storage_error_text("system_time", "Unix timestamp is too large"))
}

fn validate_persisted_doctor_finding(finding: &PersistedDoctorFinding) -> rusqlite::Result<()> {
    let valid_fingerprint = finding
        .fingerprint
        .strip_prefix("helm-doctor:v2:sha256:")
        .is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        });
    let valid_text = [
        finding.finding_code.as_str(),
        finding.issue_code.as_str(),
        finding.subject_kind.as_str(),
        finding.subject_value.as_str(),
        finding.severity.as_str(),
        finding.detector_id.as_str(),
        finding.detector_version.as_str(),
    ]
    .into_iter()
    .all(|value| !value.trim().is_empty());
    let valid_evidence =
        serde_json::from_str::<serde_json::Value>(finding.evidence_json.as_str()).is_ok();
    if valid_fingerprint
        && valid_text
        && valid_evidence
        && finding.first_seen_unix >= 0
        && finding.last_seen_unix >= finding.first_seen_unix
    {
        Ok(())
    } else {
        Err(storage_error_sqlite("invalid persisted doctor finding"))
    }
}

fn persisted_doctor_finding_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<PersistedDoctorFinding> {
    let manager_id = parse_stored_manager(row.get::<_, String>(3)?.as_str(), 3)?;
    let source_manager_id = row
        .get::<_, Option<String>>(4)?
        .map(|value| parse_stored_manager(value.as_str(), 4))
        .transpose()?;
    Ok(PersistedDoctorFinding {
        fingerprint: row.get(0)?,
        finding_code: row.get(1)?,
        issue_code: row.get(2)?,
        manager_id,
        source_manager_id,
        subject_kind: row.get(5)?,
        subject_value: row.get(6)?,
        severity: row.get(7)?,
        evidence_json: row.get(8)?,
        detector_id: row.get(9)?,
        detector_version: row.get(10)?,
        first_seen_unix: row.get(11)?,
        last_seen_unix: row.get(12)?,
        latest_observation_generation: row.get(13)?,
        resolution_state: row.get(14)?,
    })
}

fn parse_stored_manager(value: &str, column: usize) -> rusqlite::Result<ManagerId> {
    value.parse::<ManagerId>().map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown stored manager id '{value}'"),
            )),
        )
    })
}

fn validate_knowledge_source_binding(
    source_key: &str,
    envelope: &KnowledgeEnvelope,
    trust_level: KnowledgeTrustLevel,
) -> Result<(), &'static str> {
    let valid_source_key = !source_key.is_empty()
        && source_key.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b':' | b'.' | b'_' | b'-')
        });
    if !valid_source_key {
        return Err("source_key is malformed");
    }
    let protected_claim = envelope.declared_source_id.starts_with("helm_")
        || envelope.declared_source_id.starts_with("helm.")
        || envelope.declared_source_id.starts_with("helm-");
    match trust_level {
        KnowledgeTrustLevel::Bundled
            if source_key == BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY
                && envelope.declared_source_id == "helm_bundled_knowledge"
                && envelope.integrity.signature.is_none() =>
        {
            Ok(())
        }
        KnowledgeTrustLevel::Bundled => Err("bundled source identity is invalid"),
        KnowledgeTrustLevel::UserImported
            if source_key.starts_with("user:") && !protected_claim =>
        {
            Ok(())
        }
        KnowledgeTrustLevel::UserImported => {
            Err("user import cannot claim a protected source namespace")
        }
        KnowledgeTrustLevel::VerifiedSigned => {
            Err("verified signed imports require a configured local verifier")
        }
    }
}

fn persist_knowledge_entry(
    transaction: &rusqlite::Transaction<'_>,
    source_key: &str,
    entry: &KnowledgeEntry,
) -> rusqlite::Result<()> {
    let revision = i64::try_from(entry.revision)
        .map_err(|_| storage_error_sqlite("knowledge entry revision is too large"))?;
    let canonical = serde_jcs::to_vec(entry)
        .map_err(|_| storage_error_sqlite("failed to canonicalize knowledge entry"))?;
    let entry_checksum = sha256_hex(canonical.as_slice());
    let existing_checksum: Option<String> = transaction
        .query_row(
            "SELECT entry_checksum FROM repair_knowledge_entries
             WHERE source_key = ?1 AND knowledge_entry_id = ?2 AND revision = ?3",
            params![source_key, entry.knowledge_entry_id, revision],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing_checksum) = existing_checksum {
        if existing_checksum == entry_checksum {
            return Ok(());
        }
        return Err(storage_error_sqlite(
            "knowledge entry equivocation detected",
        ));
    }
    let latest_revision: Option<i64> = transaction.query_row(
        "SELECT MAX(revision) FROM repair_knowledge_entries
         WHERE source_key = ?1 AND knowledge_entry_id = ?2",
        params![source_key, entry.knowledge_entry_id],
        |row| row.get(0),
    )?;
    if latest_revision.is_some_and(|latest| revision < latest) {
        return Err(storage_error_sqlite("knowledge entry downgrade rejected"));
    }
    let selector_json = serialize_stored_json(&entry.selector, "selector")?;
    let policy_json = entry
        .policy
        .as_ref()
        .map(|value| serialize_stored_json(value, "policy"))
        .transpose()?;
    let bindings_json = entry
        .parameter_bindings
        .as_ref()
        .map(|value| serialize_stored_json(value, "parameter bindings"))
        .transpose()?;
    let content_json = entry
        .content_keys
        .as_ref()
        .map(|value| serialize_stored_json(value, "content keys"))
        .transpose()?;
    transaction.execute(
        "INSERT INTO repair_knowledge_entries (
            source_key, knowledge_entry_id, revision, state, selector_json,
            option_id, action_id, recommendation_rank, policy_json,
            parameter_bindings_json, content_keys_json, entry_checksum
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            source_key,
            entry.knowledge_entry_id,
            revision,
            entry.state,
            selector_json,
            entry.option_id,
            entry.action_id,
            entry.recommendation_rank,
            policy_json,
            bindings_json,
            content_json,
            entry_checksum,
        ],
    )?;
    Ok(())
}

fn insert_knowledge_import_audit(
    connection: &Connection,
    source_key: &str,
    envelope: &KnowledgeEnvelope,
    trust_level: KnowledgeTrustLevel,
    imported_at_unix: i64,
    result: &str,
    diagnostic: Option<&str>,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO repair_knowledge_imports (
            source_key, source_revision, envelope_checksum, trust_level,
            imported_at_unix, result, diagnostic
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            source_key,
            i64::try_from(envelope.source_revision)
                .map_err(|_| storage_error_sqlite("source revision is too large"))?,
            envelope.integrity.value,
            trust_level.as_str(),
            imported_at_unix,
            result,
            diagnostic,
        ],
    )?;
    Ok(())
}

fn serialize_stored_json<T: serde::Serialize>(value: &T, field: &str) -> rusqlite::Result<String> {
    serde_json::to_string(value)
        .map_err(|_| storage_error_sqlite(format!("failed to serialize {field}").as_str()))
}

fn parse_stored_json<T: serde::de::DeserializeOwned>(
    value: &str,
    field: &str,
) -> rusqlite::Result<T> {
    serde_json::from_str(value)
        .map_err(|_| storage_error_sqlite(format!("stored {field} is malformed").as_str()))
}

fn knowledge_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeEntry> {
    let revision = row.get::<_, i64>(1)?;
    Ok(KnowledgeEntry {
        knowledge_entry_id: row.get(0)?,
        revision: u64::try_from(revision)
            .map_err(|_| storage_error_sqlite("stored entry revision is invalid"))?,
        state: row.get(2)?,
        selector: parse_stored_json(row.get::<_, String>(3)?.as_str(), "selector")?,
        option_id: row.get(4)?,
        action_id: row.get(5)?,
        recommendation_rank: row.get(6)?,
        policy: row
            .get::<_, Option<String>>(7)?
            .map(|json| parse_stored_json(json.as_str(), "policy"))
            .transpose()?,
        parameter_bindings: row
            .get::<_, Option<String>>(8)?
            .map(|json| parse_stored_json(json.as_str(), "parameter bindings"))
            .transpose()?,
        content_keys: row
            .get::<_, Option<String>>(9)?
            .map(|json| parse_stored_json(json.as_str(), "content keys"))
            .transpose()?,
    })
}

fn knowledge_selector_matches(
    selector: &KnowledgeSelector,
    finding: &crate::doctor::DoctorFinding,
) -> bool {
    if let Some(fingerprint) = selector.fingerprint.as_deref() {
        return fingerprint == finding.fingerprint;
    }
    let source_manager = finding.source_manager_id.as_deref();
    let (subject_kind, subject_value) = doctor_finding_subject(finding);
    selector
        .finding_code
        .as_deref()
        .is_none_or(|value| value == finding.finding_code)
        && selector
            .issue_code
            .as_deref()
            .is_none_or(|value| value == finding.issue_code)
        && selector
            .manager_id
            .as_deref()
            .is_none_or(|value| value == finding.manager_id)
        && selector
            .source_manager_id
            .as_deref()
            .is_none_or(|value| Some(value) == source_manager)
        && selector
            .package_name
            .as_deref()
            .is_none_or(|value| finding.package_name.as_deref() == Some(value))
        && selector
            .subject_kind
            .as_deref()
            .is_none_or(|value| value == subject_kind)
        && selector
            .subject_value
            .as_deref()
            .is_none_or(|value| value == subject_value)
}

fn doctor_finding_subject(finding: &crate::doctor::DoctorFinding) -> (&'static str, &str) {
    if let Some(package_name) = finding.package_name.as_deref() {
        ("package", package_name)
    } else {
        ("manager", finding.manager_id.as_str())
    }
}

fn resolve_effective_knowledge(mut candidates: Vec<EffectiveKnowledge>) -> Vec<EffectiveKnowledge> {
    candidates.sort_by(|left, right| {
        right
            .trust_level
            .authority_rank()
            .cmp(&left.trust_level.authority_rank())
            .then_with(|| right.revision.cmp(&left.revision))
            .then_with(|| left.source_key.cmp(&right.source_key))
            .then_with(|| left.knowledge_entry_id.cmp(&right.knowledge_entry_id))
    });
    let mut by_option: std::collections::BTreeMap<String, Vec<EffectiveKnowledge>> =
        std::collections::BTreeMap::new();
    for candidate in candidates {
        by_option
            .entry(candidate.option_id.clone())
            .or_default()
            .push(candidate);
    }

    let mut resolved = Vec::new();
    for (_, group) in by_option {
        let Some(mut selected) = group.first().cloned() else {
            continue;
        };
        let highest_authority = selected.trust_level.authority_rank();
        let conflicting_high_authority = group.iter().any(|candidate| {
            candidate.trust_level.authority_rank() == highest_authority
                && candidate.action_id != selected.action_id
        });
        if conflicting_high_authority {
            continue;
        }
        let matching = group
            .iter()
            .filter(|candidate| candidate.action_id == selected.action_id)
            .collect::<Vec<_>>();
        selected.policy.requires_confirmation = matching
            .iter()
            .any(|candidate| candidate.policy.requires_confirmation);
        selected.policy.enabled = Some(
            matching
                .iter()
                .all(|candidate| candidate.policy.enabled.unwrap_or(true)),
        );
        selected.policy.automation_level = matching
            .iter()
            .filter_map(|candidate| candidate.policy.automation_level())
            .max_by_key(|level| repair_automation_rank(*level))
            .map(|level| level.as_str().to_string())
            .unwrap_or_else(|| "read_only".to_string());
        if selected.policy.enabled == Some(true) {
            resolved.push(selected);
        }
    }
    resolved
}

fn repair_automation_rank(level: crate::repair::RepairAutomationLevel) -> u8 {
    match level {
        crate::repair::RepairAutomationLevel::Automatic => 0,
        crate::repair::RepairAutomationLevel::NeedsConfirmation => 1,
        crate::repair::RepairAutomationLevel::ReadOnly => 2,
    }
}

impl SqliteStore {
    fn finish_incomplete_doctor_scan(
        &self,
        scan_id: &str,
        completed_at_unix: i64,
        terminal_state: &'static str,
    ) -> PersistenceResult<()> {
        let operation = match terminal_state {
            "failed" => "fail_scan",
            "cancelled" => "cancel_scan",
            _ => return Err(storage_error_text("finish_scan", "invalid terminal state")),
        };
        self.with_connection(operation, |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            transaction.execute(
                "UPDATE doctor_scan_scopes SET completion_state = ?2
                 WHERE scan_id = ?1 AND completion_state = 'pending'",
                params![scan_id, terminal_state],
            )?;
            let changed = transaction.execute(
                "UPDATE doctor_scans SET completed_at_unix = ?2, completion_state = ?3
                 WHERE scan_id = ?1 AND completion_state = 'running'",
                params![scan_id, completed_at_unix, terminal_state],
            )?;
            if changed != 1 {
                return Err(storage_error_sqlite(
                    "doctor scan is missing or already terminal",
                ));
            }
            transaction.commit()?;
            Ok(())
        })
    }
}

impl DoctorStore for SqliteStore {
    fn start_scan(
        &self,
        scan_id: &str,
        started_at_unix: i64,
        scopes: &[DoctorScanScope],
    ) -> PersistenceResult<i64> {
        if scan_id.trim().is_empty() || started_at_unix < 0 || scopes.is_empty() {
            return Err(storage_error_text("start_scan", "invalid scan metadata"));
        }
        self.with_connection("start_scan", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            let generation = transaction.query_row(
                "SELECT COALESCE(MAX(generation), 0) + 1 FROM doctor_scans",
                [],
                |row| row.get::<_, i64>(0),
            )?;
            transaction.execute(
                "INSERT INTO doctor_scans (
                    scan_id, generation, started_at_unix, completion_state
                 ) VALUES (?1, ?2, ?3, 'running')",
                params![scan_id, generation, started_at_unix],
            )?;
            let mut seen = std::collections::HashSet::new();
            for scope in scopes {
                if scope.detector_id.trim().is_empty()
                    || !seen.insert((scope.detector_id.as_str(), scope.manager_id))
                {
                    return Err(storage_error_sqlite(
                        "invalid or duplicate doctor scan scope",
                    ));
                }
                transaction.execute(
                    "INSERT INTO doctor_scan_scopes (
                        scan_id, detector_id, manager_id, completion_state
                     ) VALUES (?1, ?2, ?3, 'pending')",
                    params![scan_id, scope.detector_id, scope.manager_id.as_str()],
                )?;
            }
            transaction.commit()?;
            Ok(generation)
        })
    }

    fn complete_scan(
        &self,
        scan_id: &str,
        generation: i64,
        findings: &[PersistedDoctorFinding],
        successful_scopes: &[DoctorScanScope],
        completed_at_unix: i64,
    ) -> PersistenceResult<()> {
        self.with_connection("complete_scan", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            let scan_state: Option<String> = transaction
                .query_row(
                    "SELECT completion_state FROM doctor_scans
                     WHERE scan_id = ?1 AND generation = ?2",
                    params![scan_id, generation],
                    |row| row.get(0),
                )
                .optional()?;
            if scan_state.as_deref() != Some("running") {
                return Err(storage_error_sqlite("doctor scan is missing or already terminal"));
            }

            let mut successful_scope_keys = std::collections::HashSet::new();
            for scope in successful_scopes {
                if !successful_scope_keys.insert((scope.detector_id.as_str(), scope.manager_id)) {
                    return Err(storage_error_sqlite("duplicate successful doctor scope"));
                }
            }

            for finding in findings {
                validate_persisted_doctor_finding(finding)?;
                if !successful_scope_keys
                    .contains(&(finding.detector_id.as_str(), finding.manager_id))
                {
                    return Err(storage_error_sqlite(
                        "doctor finding belongs to a scope that did not succeed",
                    ));
                }
                transaction.execute(
                    "INSERT INTO doctor_findings (
                        fingerprint, finding_code, issue_code, manager_id, source_manager_id, subject_kind,
                        subject_value, severity, evidence_json, detector_id, detector_version,
                        first_seen_unix, last_seen_unix, latest_observation_generation, resolution_state
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, 'active')
                    ON CONFLICT(fingerprint) DO UPDATE SET
                        severity = excluded.severity,
                        evidence_json = excluded.evidence_json,
                        detector_version = excluded.detector_version,
                        last_seen_unix = excluded.last_seen_unix,
                        latest_observation_generation = excluded.latest_observation_generation,
                        resolution_state = 'active'
                    WHERE excluded.latest_observation_generation > doctor_findings.latest_observation_generation",
                    params![
                        finding.fingerprint,
                        finding.finding_code,
                        finding.issue_code,
                        finding.manager_id.as_str(),
                        finding.source_manager_id.map(|id| id.as_str().to_string()),
                        finding.subject_kind,
                        finding.subject_value,
                        finding.severity,
                        finding.evidence_json,
                        finding.detector_id,
                        finding.detector_version,
                        finding.first_seen_unix,
                        finding.last_seen_unix,
                        generation
                    ],
                )?;
            }

            for scope in successful_scopes {
                let changed = transaction.execute(
                    "UPDATE doctor_scan_scopes SET completion_state = 'succeeded'
                     WHERE scan_id = ?1 AND detector_id = ?2 AND manager_id = ?3
                       AND completion_state = 'pending'",
                    params![scan_id, scope.detector_id, scope.manager_id.as_str()],
                )?;
                if changed != 1 {
                    return Err(storage_error_sqlite("successful scope was not declared by scan"));
                }
                transaction.execute(
                    "UPDATE doctor_findings
                     SET resolution_state = 'resolved'
                     WHERE detector_id = ?1 AND manager_id = ?2 AND latest_observation_generation < ?3",
                    params![scope.detector_id, scope.manager_id.as_str(), generation],
                )?;
            }

            transaction.execute(
                "UPDATE doctor_scan_scopes SET completion_state = 'skipped'
                 WHERE scan_id = ?1 AND completion_state = 'pending'",
                [scan_id],
            )?;
            transaction.execute(
                "UPDATE doctor_scans
                 SET completed_at_unix = ?2, completion_state = ?3
                 WHERE scan_id = ?1",
                params![
                    scan_id,
                    completed_at_unix,
                    if successful_scopes.is_empty() {
                        "failed"
                    } else {
                        "completed"
                    }
                ],
            )?;
            transaction.commit()?;
            Ok(())
        })
    }

    fn fail_scan(&self, scan_id: &str, completed_at_unix: i64) -> PersistenceResult<()> {
        self.finish_incomplete_doctor_scan(scan_id, completed_at_unix, "failed")
    }

    fn cancel_scan(&self, scan_id: &str, completed_at_unix: i64) -> PersistenceResult<()> {
        self.finish_incomplete_doctor_scan(scan_id, completed_at_unix, "cancelled")
    }

    fn get_active_findings(&self) -> PersistenceResult<Vec<PersistedDoctorFinding>> {
        self.with_connection("get_active_findings", |connection| {
            ensure_schema_ready(connection)?;
            let mut stmt = connection.prepare(
                "SELECT fingerprint, finding_code, issue_code, manager_id, source_manager_id,
                 subject_kind, subject_value, severity, evidence_json, detector_id, detector_version,
                 first_seen_unix, last_seen_unix, latest_observation_generation, resolution_state
                 FROM doctor_findings
                 WHERE resolution_state = 'active'
                 ORDER BY fingerprint ASC",
            )?;
            let findings = stmt.query_map([], persisted_doctor_finding_from_row)?;
            findings.collect()
        })
    }

    fn import_knowledge(
        &self,
        source_key: &str,
        envelope: &KnowledgeEnvelope,
        trust_level: KnowledgeTrustLevel,
    ) -> PersistenceResult<()> {
        envelope
            .validate()
            .map_err(|error| storage_error_text("import_knowledge", error.to_string()))?;
        validate_knowledge_source_binding(source_key, envelope, trust_level)
            .map_err(|message| storage_error_text("import_knowledge", message))?;
        let now = current_unix_seconds()?;
        let result = self.with_connection("import_knowledge", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction()?;
            let existing: Option<(i64, String, String, String)> = transaction
                .query_row(
                    "SELECT latest_revision, envelope_checksum, trust_level, declared_source_id
                     FROM repair_knowledge_sources WHERE source_key = ?1",
                    [source_key],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            if let Some((revision, checksum, stored_trust, declared_source)) = existing {
                if stored_trust != trust_level.as_str()
                    || declared_source != envelope.declared_source_id
                {
                    return Err(storage_error_sqlite(
                        "source identity or trust cannot change",
                    ));
                }
                let revision = u64::try_from(revision)
                    .map_err(|_| storage_error_sqlite("stored source revision is invalid"))?;
                if envelope.source_revision < revision {
                    return Err(storage_error_sqlite("knowledge source downgrade rejected"));
                }
                if envelope.source_revision == revision {
                    if checksum != envelope.integrity.value {
                        return Err(storage_error_sqlite(
                            "knowledge source equivocation detected",
                        ));
                    }
                    transaction.commit()?;
                    return Ok(());
                }
            }

            for entry in &envelope.entries {
                persist_knowledge_entry(&transaction, source_key, entry)?;
            }
            let signature_json = envelope
                .integrity
                .signature
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|_| storage_error_sqlite("failed to serialize signature metadata"))?;
            transaction.execute(
                "INSERT INTO repair_knowledge_sources (
                    source_key, declared_source_id, latest_revision, trust_level,
                    imported_at_unix, envelope_checksum, generated_at_unix, signature_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(source_key) DO UPDATE SET
                    latest_revision = excluded.latest_revision,
                    imported_at_unix = excluded.imported_at_unix,
                    envelope_checksum = excluded.envelope_checksum,
                    generated_at_unix = excluded.generated_at_unix,
                    signature_json = excluded.signature_json",
                params![
                    source_key,
                    envelope.declared_source_id,
                    i64::try_from(envelope.source_revision)
                        .map_err(|_| storage_error_sqlite("source revision is too large"))?,
                    trust_level.as_str(),
                    now,
                    envelope.integrity.value,
                    i64::try_from(envelope.generated_at_unix)
                        .map_err(|_| storage_error_sqlite("generated timestamp is too large"))?,
                    signature_json,
                ],
            )?;
            insert_knowledge_import_audit(
                &transaction,
                source_key,
                envelope,
                trust_level,
                now,
                "imported",
                None,
            )?;
            transaction.commit()?;
            Ok(())
        });
        if let Err(error) = result.as_ref() {
            let _ = self.record_failed_knowledge_import(
                source_key,
                envelope,
                trust_level,
                now,
                error.message.as_str(),
            );
        }
        result
    }

    fn export_knowledge(&self, source_key: &str) -> PersistenceResult<KnowledgeEnvelope> {
        self.with_connection("export_knowledge", |connection| {
            ensure_schema_ready(connection)?;

            let (declared_source_id, latest_revision, generated_at_unix, signature_json): (
                String,
                i64,
                i64,
                Option<String>,
            ) = connection.query_row(
                "SELECT declared_source_id, latest_revision, generated_at_unix, signature_json
                 FROM repair_knowledge_sources WHERE source_key = ?1",
                params![source_key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;

            let mut stmt = connection.prepare(
                "SELECT e.knowledge_entry_id, e.revision, e.state, e.selector_json,
                        e.option_id, e.action_id, e.recommendation_rank, e.policy_json,
                        e.parameter_bindings_json, e.content_keys_json
                 FROM repair_knowledge_entries e
                 JOIN (
                    SELECT source_key, knowledge_entry_id, MAX(revision) AS revision
                    FROM repair_knowledge_entries
                    WHERE source_key = ?1
                    GROUP BY source_key, knowledge_entry_id
                 ) latest ON latest.source_key = e.source_key
                    AND latest.knowledge_entry_id = e.knowledge_entry_id
                    AND latest.revision = e.revision
                 WHERE e.source_key = ?1
                 ORDER BY e.knowledge_entry_id ASC",
            )?;
            let entries = stmt
                .query_map(params![source_key], knowledge_entry_from_row)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let signature = signature_json
                .map(|json| serde_json::from_str(json.as_str()))
                .transpose()
                .map_err(|_| storage_error_sqlite("stored signature metadata is malformed"))?;
            let mut envelope = KnowledgeEnvelope {
                schema_version: 1,
                declared_source_id,
                source_revision: u64::try_from(latest_revision)
                    .map_err(|_| storage_error_sqlite("stored source revision is invalid"))?,
                generated_at_unix: u64::try_from(generated_at_unix)
                    .map_err(|_| storage_error_sqlite("stored generated timestamp is invalid"))?,
                entries,
                integrity: crate::persistence::repair_knowledge::KnowledgeIntegrity {
                    algorithm: "sha256".to_string(),
                    value: String::new(),
                    signature,
                },
            };
            let canonical = envelope
                .to_canonical_jcs_without_integrity()
                .map_err(|error| storage_error_sqlite(error.to_string().as_str()))?;
            envelope.integrity.value = sha256_hex(canonical.as_bytes());
            Ok(envelope)
        })
    }

    fn get_effective_knowledge(
        &self,
        finding: &crate::doctor::DoctorFinding,
    ) -> PersistenceResult<Vec<EffectiveKnowledge>> {
        self.with_connection("get_effective_knowledge", |connection| {
            ensure_schema_ready(connection)?;
            let mut stmt = connection.prepare(
                "SELECT e.source_key, s.trust_level, e.knowledge_entry_id, e.revision,
                        e.state, e.selector_json, e.option_id, e.action_id,
                        e.recommendation_rank, e.policy_json, e.content_keys_json,
                        COALESCE(o.disabled, 0)
                 FROM repair_knowledge_entries e
                 JOIN repair_knowledge_sources s ON s.source_key = e.source_key
                 JOIN (
                    SELECT source_key, knowledge_entry_id, MAX(revision) AS revision
                    FROM repair_knowledge_entries
                    GROUP BY source_key, knowledge_entry_id
                 ) latest ON latest.source_key = e.source_key
                    AND latest.knowledge_entry_id = e.knowledge_entry_id
                    AND latest.revision = e.revision
                 LEFT JOIN repair_knowledge_overrides o ON o.option_id = e.option_id
                 ORDER BY e.source_key ASC, e.knowledge_entry_id ASC",
            )?;
            let mut candidates = Vec::new();
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<u32>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            })?;
            for row in rows {
                let (
                    source_key,
                    trust,
                    entry_id,
                    revision,
                    state,
                    selector_json,
                    option_id,
                    action_id,
                    recommendation_rank,
                    policy_json,
                    content_json,
                    disabled,
                ) = row?;
                if state != "active" || disabled != 0 {
                    continue;
                }
                let selector: KnowledgeSelector = parse_stored_json(&selector_json, "selector")?;
                if !knowledge_selector_matches(&selector, finding) {
                    continue;
                }
                let Some(option_id) = option_id else { continue };
                let Some(action_id) = action_id else { continue };
                let Some(policy_json) = policy_json else {
                    continue;
                };
                let Some(content_json) = content_json else {
                    continue;
                };
                let trust_level = trust
                    .parse::<KnowledgeTrustLevel>()
                    .map_err(|_| storage_error_sqlite("stored trust level is invalid"))?;
                candidates.push(EffectiveKnowledge {
                    source_key,
                    trust_level,
                    knowledge_entry_id: entry_id,
                    revision: u64::try_from(revision)
                        .map_err(|_| storage_error_sqlite("stored entry revision is invalid"))?,
                    option_id,
                    action_id,
                    recommendation_rank,
                    policy: parse_stored_json(&policy_json, "policy")?,
                    content_keys: parse_stored_json(&content_json, "content keys")?,
                });
            }
            Ok(resolve_effective_knowledge(candidates))
        })
    }

    fn record_repair_history(&self, record: &RepairHistoryRecord) -> PersistenceResult<()> {
        self.with_connection("record_repair_history", |connection| {
            ensure_schema_ready(connection)?;
            if record.fingerprint.trim().is_empty()
                || record.option_id.trim().is_empty()
                || record.action_id.trim().is_empty()
                || record.result.trim().is_empty()
                || record.executed_at_unix < 0
                || crate::repair::validate_knowledge_binding(
                    record.option_id.as_str(),
                    record.action_id.as_str(),
                )
                .is_err()
            {
                return Err(storage_error_sqlite("invalid repair history record"));
            }
            let task_id = record
                .task_id
                .map(|task| i64::try_from(task.0))
                .transpose()
                .map_err(|_| storage_error_sqlite("task id is too large"))?;
            connection.execute(
                "INSERT INTO repair_history (fingerprint, option_id, action_id, task_id, result, verified_outcome, executed_at_unix)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    record.fingerprint,
                    record.option_id,
                    record.action_id,
                    task_id,
                    record.result,
                    record.verified_outcome,
                    record.executed_at_unix
                ]
            )?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::SqliteStore;
    use crate::models::{
        ManagerId, NewTaskLogRecord, TaskId, TaskLogLevel, TaskRecord, TaskStatus, TaskType,
    };
    use crate::persistence::TaskStore;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_store(test_name: &str) -> SqliteStore {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("helm-sqlite-store-{test_name}-{nanos}.db"));
        SqliteStore::new(path)
    }

    #[test]
    fn update_task_with_log_is_atomic_when_task_row_is_missing() {
        let store = temp_store("task-update-with-log-atomic");
        store
            .migrate_to_latest()
            .expect("sqlite migrations should apply");

        let task = TaskRecord {
            id: TaskId(42),
            manager: ManagerId::Rustup,
            task_type: TaskType::Refresh,
            status: TaskStatus::Cancelled,
            created_at: SystemTime::now(),
        };
        let log = NewTaskLogRecord {
            task_id: task.id,
            manager: task.manager,
            task_type: task.task_type,
            status: Some(task.status),
            level: TaskLogLevel::Warn,
            message: "terminal transition".to_string(),
            created_at: SystemTime::now(),
        };

        let result = store.update_task_with_log(&task, &log);
        assert!(result.is_err(), "missing task row should fail atomically");

        let logs = store
            .list_task_logs(task.id, 10)
            .expect("task log listing should succeed");
        assert!(
            logs.is_empty(),
            "task log should not be written when the task update fails"
        );

        let tasks = store
            .list_recent_tasks(10)
            .expect("task listing should succeed");
        assert!(
            tasks.is_empty(),
            "missing task update should not create a phantom task row"
        );

        let _ = fs::remove_file(store.database_path());
    }
}
