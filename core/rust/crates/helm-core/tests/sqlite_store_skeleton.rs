use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use helm_core::models::{CoreErrorKind, InstalledPackage, ManagerId, OutdatedPackage, PackageRef};
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
fn package_operations_require_migrations() {
    let path = test_db_path("requires-migration");
    let store = SqliteStore::new(&path);
    let error = store.list_installed().unwrap_err();

    assert_eq!(error.kind, CoreErrorKind::StorageFailure);
    assert!(error.message.contains("apply migrations"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn upsert_and_list_installed_roundtrip() {
    let path = test_db_path("installed-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let packages = vec![
        InstalledPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: "git".to_string(),
            },
            installed_version: Some("2.45.1".to_string()),
            pinned: false,
        },
        InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Pnpm,
                name: "typescript".to_string(),
            },
            installed_version: Some("5.5.2".to_string()),
            pinned: true,
        },
    ];

    store.upsert_installed(&packages).unwrap();
    let mut persisted = store.list_installed().unwrap();
    persisted.sort_by(|left, right| left.package.name.cmp(&right.package.name));

    assert_eq!(persisted.len(), 2);
    assert_eq!(persisted[0].package.name, "git");
    assert_eq!(persisted[1].package.name, "typescript");
    assert!(persisted[1].pinned);

    let _ = std::fs::remove_file(path);
}

#[test]
fn upsert_and_list_outdated_roundtrip() {
    let path = test_db_path("outdated-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let packages = vec![OutdatedPackage {
        package: PackageRef {
            manager: ManagerId::HomebrewFormula,
            name: "openssl@3".to_string(),
        },
        installed_version: Some("3.3.1".to_string()),
        candidate_version: "3.3.2".to_string(),
        pinned: false,
    }];

    store.upsert_outdated(&packages).unwrap();
    let persisted = store.list_outdated().unwrap();

    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].package.name, "openssl@3");
    assert_eq!(persisted[0].candidate_version, "3.3.2");

    let _ = std::fs::remove_file(path);
}
