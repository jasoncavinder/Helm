use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::models::{
    CachedSearchResult, CoreError, CoreErrorKind, InstalledPackage, OutdatedPackage, PinRecord,
    TaskRecord,
};
use crate::persistence::{
    MigrationStore, PackageStore, PersistenceResult, PinStore, SearchCacheStore, TaskStore,
};
use crate::sqlite::migrations::{SqliteMigration, current_schema_version, migration, migrations};

pub struct SqliteStoreSkeleton {
    database_path: PathBuf,
    schema_version: Mutex<i64>,
}

impl SqliteStoreSkeleton {
    pub fn new(database_path: impl Into<PathBuf>) -> Self {
        Self {
            database_path: database_path.into(),
            schema_version: Mutex::new(0),
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
}

impl MigrationStore for SqliteStoreSkeleton {
    fn current_version(&self) -> PersistenceResult<i64> {
        self.schema_version
            .lock()
            .map(|version| *version)
            .map_err(|_| storage_error("sqlite store schema version lock was poisoned".to_string()))
    }

    fn apply_migration(&self, target_version: i64) -> PersistenceResult<()> {
        if target_version < 0 || target_version > current_schema_version() {
            return Err(storage_error(format!(
                "invalid migration target version '{target_version}'"
            )));
        }

        if target_version > 0 && migration(target_version).is_none() {
            return Err(storage_error(format!(
                "migration version '{target_version}' is not defined"
            )));
        }

        let mut version = self.schema_version.lock().map_err(|_| {
            storage_error("sqlite store schema version lock was poisoned".to_string())
        })?;
        *version = target_version;
        Ok(())
    }
}

impl PackageStore for SqliteStoreSkeleton {
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

impl PinStore for SqliteStoreSkeleton {
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

impl SearchCacheStore for SqliteStoreSkeleton {
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

impl TaskStore for SqliteStoreSkeleton {
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
    storage_error(format!(
        "sqlite store skeleton operation '{operation}' is not implemented"
    ))
}

fn storage_error(message: String) -> CoreError {
    CoreError {
        manager: None,
        task: None,
        action: None,
        kind: CoreErrorKind::StorageFailure,
        message,
    }
}
