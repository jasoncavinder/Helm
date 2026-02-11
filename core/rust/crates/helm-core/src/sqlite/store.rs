use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::models::{
    CachedSearchResult, CoreError, CoreErrorKind, InstalledPackage, ManagerId, OutdatedPackage,
    PackageRef, PinRecord, TaskRecord,
};
use crate::persistence::{
    MigrationStore, PackageStore, PersistenceResult, PinStore, SearchCacheStore, TaskStore,
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
    manager_id, package_name, installed_version, candidate_version, pinned, updated_at_unix
) VALUES (?1, ?2, ?3, ?4, ?5, strftime('%s', 'now'))
ON CONFLICT(manager_id, package_name) DO UPDATE SET
    installed_version = excluded.installed_version,
    candidate_version = excluded.candidate_version,
    pinned = excluded.pinned,
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
SELECT manager_id, package_name, installed_version, pinned
FROM installed_packages
ORDER BY manager_id, package_name
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
SELECT manager_id, package_name, installed_version, candidate_version, pinned
FROM outdated_packages
ORDER BY manager_id, package_name
",
            )?;

            let rows = statement.query_map([], |row| {
                let manager_id: String = row.get(0)?;
                let package_name: String = row.get(1)?;
                let installed_version: Option<String> = row.get(2)?;
                let candidate_version: String = row.get(3)?;
                let pinned_int: i64 = row.get(4)?;

                let manager = parse_manager_id(&manager_id)?;
                Ok(OutdatedPackage {
                    package: PackageRef {
                        manager,
                        name: package_name,
                    },
                    installed_version,
                    candidate_version,
                    pinned: sqlite_to_bool(pinned_int),
                })
            })?;

            rows.collect()
        })
    }
}

impl PinStore for SqliteStore {
    fn upsert_pin(&self, _pin: &PinRecord) -> PersistenceResult<()> {
        Err(not_implemented("upsert_pin"))
    }

    fn remove_pin(&self, _package_key: &str) -> PersistenceResult<()> {
        Err(not_implemented("remove_pin"))
    }

    fn list_pins(&self) -> PersistenceResult<Vec<PinRecord>> {
        Err(not_implemented("list_pins"))
    }
}

impl SearchCacheStore for SqliteStore {
    fn upsert_search_results(&self, _results: &[CachedSearchResult]) -> PersistenceResult<()> {
        Err(not_implemented("upsert_search_results"))
    }

    fn query_local(
        &self,
        _query: &str,
        _limit: usize,
    ) -> PersistenceResult<Vec<CachedSearchResult>> {
        Err(not_implemented("query_local"))
    }
}

impl TaskStore for SqliteStore {
    fn create_task(&self, _task: &TaskRecord) -> PersistenceResult<()> {
        Err(not_implemented("create_task"))
    }

    fn update_task(&self, _task: &TaskRecord) -> PersistenceResult<()> {
        Err(not_implemented("update_task"))
    }

    fn list_recent_tasks(&self, _limit: usize) -> PersistenceResult<Vec<TaskRecord>> {
        Err(not_implemented("list_recent_tasks"))
    }
}

fn not_implemented(operation: &str) -> CoreError {
    storage_error_text(
        operation,
        format!("sqlite store operation '{operation}' is not implemented"),
    )
}

fn open_connection(database_path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = database_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
        }
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
    transaction.execute_batch(migration.up_sql)?;
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
    ManagerId::from_str(raw).ok_or_else(|| {
        storage_error_sqlite(&format!(
            "unknown manager id '{raw}' found in persisted sqlite record"
        ))
    })
}

fn bool_to_sqlite(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn sqlite_to_bool(value: i64) -> bool {
    value != 0
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
