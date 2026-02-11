use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use helm_core::models::CoreErrorKind;
use helm_core::persistence::{MigrationStore, PackageStore};
use helm_core::sqlite::{SqliteStore, current_schema_version};

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-{test_name}-{nanos}.sqlite3"))
}

#[test]
fn planned_migrations_include_versions_after_requested_version() {
    let path = test_db_path("planned-migrations");
    let store = SqliteStore::new(&path);
    let planned = store.planned_migrations(0);

    assert!(!planned.is_empty());
    assert_eq!(planned[0].version, 1);

    let _ = std::fs::remove_file(path);
}

#[test]
fn applying_defined_migration_updates_persisted_schema_version() {
    let path = test_db_path("apply-and-reopen");
    let store = SqliteStore::new(&path);
    assert_eq!(store.current_version().unwrap(), 0);

    store.apply_migration(current_schema_version()).unwrap();
    assert_eq!(store.current_version().unwrap(), current_schema_version());

    let reopened = SqliteStore::new(&path);
    assert_eq!(
        reopened.current_version().unwrap(),
        current_schema_version()
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn applying_undefined_migration_fails_with_storage_error() {
    let path = test_db_path("undefined-target");
    let store = SqliteStore::new(&path);
    let error = store
        .apply_migration(current_schema_version() + 1)
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::StorageFailure);

    let _ = std::fs::remove_file(path);
}

#[test]
fn rolling_back_migration_resets_schema_version() {
    let path = test_db_path("rollback");
    let store = SqliteStore::new(&path);

    store.apply_migration(current_schema_version()).unwrap();
    assert_eq!(store.current_version().unwrap(), current_schema_version());

    store.apply_migration(0).unwrap();
    assert_eq!(store.current_version().unwrap(), 0);

    let _ = std::fs::remove_file(path);
}

#[test]
fn non_migration_store_operations_are_explicitly_not_implemented() {
    let path = test_db_path("not-implemented");
    let store = SqliteStore::new(&path);
    let error = store.list_installed().unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::StorageFailure);
    assert!(error.message.contains("not implemented"));

    let _ = std::fs::remove_file(path);
}
