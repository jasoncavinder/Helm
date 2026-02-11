use helm_core::models::CoreErrorKind;
use helm_core::persistence::{MigrationStore, PackageStore};
use helm_core::sqlite::{SqliteStoreSkeleton, current_schema_version};

#[test]
fn planned_migrations_include_versions_after_requested_version() {
    let store = SqliteStoreSkeleton::new("/tmp/helm.db");
    let planned = store.planned_migrations(0);

    assert!(!planned.is_empty());
    assert_eq!(planned[0].version, 1);
}

#[test]
fn applying_defined_migration_updates_virtual_schema_version() {
    let store = SqliteStoreSkeleton::new("/tmp/helm.db");
    assert_eq!(store.current_version().unwrap(), 0);

    store.apply_migration(current_schema_version()).unwrap();
    assert_eq!(store.current_version().unwrap(), current_schema_version());
}

#[test]
fn applying_undefined_migration_fails_with_storage_error() {
    let store = SqliteStoreSkeleton::new("/tmp/helm.db");
    let error = store
        .apply_migration(current_schema_version() + 1)
        .unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::StorageFailure);
}

#[test]
fn non_migration_store_operations_are_explicitly_not_implemented() {
    let store = SqliteStoreSkeleton::new("/tmp/helm.db");
    let error = store.list_installed().unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::StorageFailure);
    assert!(error.message.contains("not implemented"));
}
