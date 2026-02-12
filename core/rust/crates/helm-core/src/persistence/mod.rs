use crate::models::{
    CachedSearchResult, CoreError, InstalledPackage, OutdatedPackage, PinRecord, TaskRecord,
};

pub type PersistenceResult<T> = Result<T, CoreError>;

pub trait MigrationStore: Send + Sync {
    fn current_version(&self) -> PersistenceResult<i64>;

    fn apply_migration(&self, target_version: i64) -> PersistenceResult<()>;
}

pub trait PackageStore: Send + Sync {
    fn upsert_installed(&self, packages: &[InstalledPackage]) -> PersistenceResult<()>;

    fn upsert_outdated(&self, packages: &[OutdatedPackage]) -> PersistenceResult<()>;

    fn list_installed(&self) -> PersistenceResult<Vec<InstalledPackage>>;

    fn list_outdated(&self) -> PersistenceResult<Vec<OutdatedPackage>>;
}

pub trait PinStore: Send + Sync {
    fn upsert_pin(&self, pin: &PinRecord) -> PersistenceResult<()>;

    fn remove_pin(&self, package_key: &str) -> PersistenceResult<()>;

    fn list_pins(&self) -> PersistenceResult<Vec<PinRecord>>;
}

pub trait SearchCacheStore: Send + Sync {
    fn upsert_search_results(&self, results: &[CachedSearchResult]) -> PersistenceResult<()>;

    fn query_local(&self, query: &str, limit: usize) -> PersistenceResult<Vec<CachedSearchResult>>;
}

pub trait TaskStore: Send + Sync {
    fn create_task(&self, task: &TaskRecord) -> PersistenceResult<()>;

    fn update_task(&self, task: &TaskRecord) -> PersistenceResult<()>;

    fn list_recent_tasks(&self, limit: usize) -> PersistenceResult<Vec<TaskRecord>>;

    fn next_task_id(&self) -> PersistenceResult<u64>;
}
