use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::models::{
    CachedSearchResult, CoreError, CoreErrorKind, InstalledPackage, OutdatedPackage, PinRecord,
    TaskRecord,
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
    fn upsert_installed(&self, _packages: &[InstalledPackage]) -> PersistenceResult<()> {
        Err(not_implemented("upsert_installed"))
    }

    fn upsert_outdated(&self, _packages: &[OutdatedPackage]) -> PersistenceResult<()> {
        Err(not_implemented("upsert_outdated"))
    }

    fn list_installed(&self) -> PersistenceResult<Vec<InstalledPackage>> {
        Err(not_implemented("list_installed"))
    }

    fn list_outdated(&self) -> PersistenceResult<Vec<OutdatedPackage>> {
        Err(not_implemented("list_outdated"))
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

fn storage_error_text(operation: &str, message: impl AsRef<str>) -> CoreError {
    CoreError {
        manager: None,
        task: None,
        action: None,
        kind: CoreErrorKind::StorageFailure,
        message: format!("sqlite store '{operation}' failed: {}", message.as_ref()),
    }
}
