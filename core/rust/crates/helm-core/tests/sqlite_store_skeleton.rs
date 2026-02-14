use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::models::{
    CachedSearchResult, CoreErrorKind, HomebrewKegPolicy, InstalledPackage, ManagerId,
    OutdatedPackage, PackageCandidate, PackageRef, PinKind, PinRecord, TaskId, TaskRecord,
    TaskStatus, TaskType,
};
use helm_core::persistence::{
    DetectionStore, MigrationStore, PackageStore, PinStore, SearchCacheStore, TaskStore,
};
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
        restart_required: false,
    }];

    store.upsert_outdated(&packages).unwrap();
    let persisted = store.list_outdated().unwrap();

    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].package.name, "openssl@3");
    assert_eq!(persisted[0].candidate_version, "3.3.2");

    let _ = std::fs::remove_file(path);
}

#[test]
fn replace_outdated_snapshot_clears_stale_rows_for_manager() {
    let path = test_db_path("outdated-replace-snapshot");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    store
        .upsert_outdated(&[OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: "sevenzip".to_string(),
            },
            installed_version: Some("25.01".to_string()),
            candidate_version: "26.00".to_string(),
            pinned: false,
            restart_required: false,
        }])
        .unwrap();

    store
        .replace_outdated_snapshot(ManagerId::HomebrewFormula, &[])
        .unwrap();

    let persisted = store.list_outdated().unwrap();
    assert!(persisted.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn upsert_and_remove_pins_roundtrip() {
    let path = test_db_path("pins-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let pin = PinRecord {
        package: PackageRef {
            manager: ManagerId::HomebrewFormula,
            name: "git".to_string(),
        },
        kind: PinKind::Native,
        pinned_version: Some("2.45.1".to_string()),
        created_at: UNIX_EPOCH + Duration::from_secs(123),
    };

    store.upsert_pin(&pin).unwrap();
    let listed = store.list_pins().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].package.name, "git");
    assert_eq!(listed[0].kind, PinKind::Native);

    store.remove_pin("homebrew_formula:git").unwrap();
    assert!(store.list_pins().unwrap().is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn remove_pin_requires_structured_package_key() {
    let path = test_db_path("pin-key-format");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let error = store.remove_pin("git").unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::StorageFailure);
    assert!(error.message.contains("package_key"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn safe_mode_defaults_false_and_roundtrips() {
    let path = test_db_path("safe-mode-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    assert!(!store.safe_mode().unwrap());
    store.set_safe_mode(true).unwrap();
    assert!(store.safe_mode().unwrap());
    store.set_safe_mode(false).unwrap();
    assert!(!store.safe_mode().unwrap());

    let _ = std::fs::remove_file(path);
}

#[test]
fn homebrew_keg_policy_defaults_keep_and_roundtrips() {
    let path = test_db_path("keg-policy-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    assert_eq!(
        store.homebrew_keg_policy().unwrap(),
        HomebrewKegPolicy::Keep
    );
    store
        .set_homebrew_keg_policy(HomebrewKegPolicy::Cleanup)
        .unwrap();
    assert_eq!(
        store.homebrew_keg_policy().unwrap(),
        HomebrewKegPolicy::Cleanup
    );
    store
        .set_homebrew_keg_policy(HomebrewKegPolicy::Keep)
        .unwrap();
    assert_eq!(
        store.homebrew_keg_policy().unwrap(),
        HomebrewKegPolicy::Keep
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn package_keg_policy_roundtrip_and_clear() {
    let path = test_db_path("package-keg-policy-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let package = PackageRef {
        manager: ManagerId::HomebrewFormula,
        name: "sevenzip".to_string(),
    };

    assert!(store.package_keg_policy(&package).unwrap().is_none());

    store
        .set_package_keg_policy(&package, Some(HomebrewKegPolicy::Cleanup))
        .unwrap();
    assert_eq!(
        store.package_keg_policy(&package).unwrap(),
        Some(HomebrewKegPolicy::Cleanup)
    );

    let listed = store.list_package_keg_policies().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].package.name, "sevenzip");
    assert_eq!(listed[0].policy, HomebrewKegPolicy::Cleanup);

    store.set_package_keg_policy(&package, None).unwrap();
    assert!(store.package_keg_policy(&package).unwrap().is_none());
    assert!(store.list_package_keg_policies().unwrap().is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_installed_marks_package_pinned_when_pin_record_exists() {
    let path = test_db_path("installed-pin-overlay");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    store
        .upsert_installed(&[InstalledPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: "git".to_string(),
            },
            installed_version: Some("2.45.1".to_string()),
            pinned: false,
        }])
        .unwrap();

    store
        .upsert_pin(&PinRecord {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: "git".to_string(),
            },
            kind: PinKind::Virtual,
            pinned_version: None,
            created_at: UNIX_EPOCH + Duration::from_secs(500),
        })
        .unwrap();

    let installed = store.list_installed().unwrap();
    assert_eq!(installed.len(), 1);
    assert!(installed[0].pinned);

    let _ = std::fs::remove_file(path);
}

#[test]
fn list_outdated_marks_package_pinned_when_pin_record_exists() {
    let path = test_db_path("outdated-pin-overlay");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    store
        .upsert_outdated(&[OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Mas,
                name: "Xcode".to_string(),
            },
            installed_version: Some("16.1".to_string()),
            candidate_version: "16.2".to_string(),
            pinned: false,
            restart_required: false,
        }])
        .unwrap();

    store
        .upsert_pin(&PinRecord {
            package: PackageRef {
                manager: ManagerId::Mas,
                name: "Xcode".to_string(),
            },
            kind: PinKind::Virtual,
            pinned_version: Some("16.1".to_string()),
            created_at: UNIX_EPOCH + Duration::from_secs(501),
        })
        .unwrap();

    let outdated = store.list_outdated().unwrap();
    assert_eq!(outdated.len(), 1);
    assert!(outdated[0].pinned);

    let _ = std::fs::remove_file(path);
}

#[test]
fn set_snapshot_pinned_updates_cached_rows_immediately() {
    let path = test_db_path("set-snapshot-pinned");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let package = PackageRef {
        manager: ManagerId::HomebrewFormula,
        name: "libzip".to_string(),
    };

    store
        .upsert_installed(&[InstalledPackage {
            package: package.clone(),
            installed_version: Some("1.11.4".to_string()),
            pinned: true,
        }])
        .unwrap();
    store
        .upsert_outdated(&[OutdatedPackage {
            package: package.clone(),
            installed_version: Some("1.11.4".to_string()),
            candidate_version: "1.11.4_1".to_string(),
            pinned: true,
            restart_required: false,
        }])
        .unwrap();

    store.set_snapshot_pinned(&package, false).unwrap();

    let installed = store.list_installed().unwrap();
    let outdated = store.list_outdated().unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(outdated.len(), 1);
    assert!(!installed[0].pinned);
    assert!(!outdated[0].pinned);

    let _ = std::fs::remove_file(path);
}

#[test]
fn apply_upgrade_result_promotes_package_to_installed_snapshot() {
    let path = test_db_path("apply-upgrade-result");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let package = PackageRef {
        manager: ManagerId::HomebrewFormula,
        name: "abseil".to_string(),
    };

    store
        .upsert_installed(&[InstalledPackage {
            package: package.clone(),
            installed_version: Some("20250127.0".to_string()),
            pinned: false,
        }])
        .unwrap();
    store
        .upsert_outdated(&[OutdatedPackage {
            package: package.clone(),
            installed_version: Some("20250127.0".to_string()),
            candidate_version: "20250814.0".to_string(),
            pinned: false,
            restart_required: false,
        }])
        .unwrap();

    store.apply_upgrade_result(&package).unwrap();

    let installed = store.list_installed().unwrap();
    let outdated = store.list_outdated().unwrap();

    let upgraded = installed
        .iter()
        .find(|entry| entry.package == package)
        .expect("upgraded package should remain installed");
    assert_eq!(upgraded.installed_version.as_deref(), Some("20250814.0"));
    assert!(outdated.iter().all(|entry| entry.package != package));

    let _ = std::fs::remove_file(path);
}

#[test]
fn upsert_detection_preserves_previous_version_when_new_version_is_missing() {
    let path = test_db_path("detection-preserve-version");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    store
        .upsert_detection(
            ManagerId::HomebrewFormula,
            &helm_core::models::DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                version: Some("4.6.0".to_string()),
            },
        )
        .unwrap();

    store
        .upsert_detection(
            ManagerId::HomebrewFormula,
            &helm_core::models::DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                version: None,
            },
        )
        .unwrap();

    let detections = store.list_detections().unwrap();
    let homebrew = detections
        .into_iter()
        .find(|(manager, _)| *manager == ManagerId::HomebrewFormula)
        .expect("homebrew detection should exist");
    assert_eq!(homebrew.1.version.as_deref(), Some("4.6.0"));

    let _ = std::fs::remove_file(path);
}

#[test]
fn upsert_and_query_search_cache_roundtrip() {
    let path = test_db_path("search-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let now = SystemTime::now();
    let results = vec![
        CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "ripgrep".to_string(),
                },
                version: Some("14.1.0".to_string()),
                summary: Some("line-oriented search tool".to_string()),
            },
            source_manager: ManagerId::HomebrewFormula,
            originating_query: "rip".to_string(),
            cached_at: now,
        },
        CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Pnpm,
                    name: "typescript".to_string(),
                },
                version: Some("5.5.2".to_string()),
                summary: Some("language for application-scale JS".to_string()),
            },
            source_manager: ManagerId::Pnpm,
            originating_query: "type".to_string(),
            cached_at: now + Duration::from_secs(1),
        },
    ];

    store.upsert_search_results(&results).unwrap();

    let by_name = store.query_local("ripgrep", 10).unwrap();
    assert_eq!(by_name.len(), 1);
    assert_eq!(by_name[0].result.package.name, "ripgrep");

    let all = store.query_local("", 10).unwrap();
    assert_eq!(all.len(), 2);

    let _ = std::fs::remove_file(path);
}

#[test]
fn create_update_and_list_recent_tasks_roundtrip() {
    let path = test_db_path("tasks-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let mut task = TaskRecord {
        id: TaskId(42),
        manager: ManagerId::HomebrewFormula,
        task_type: TaskType::Refresh,
        status: TaskStatus::Queued,
        created_at: UNIX_EPOCH + Duration::from_secs(777),
    };

    store.create_task(&task).unwrap();
    let listed = store.list_recent_tasks(10).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].status, TaskStatus::Queued);

    task.status = TaskStatus::Running;
    store.update_task(&task).unwrap();

    let listed = store.list_recent_tasks(10).unwrap();
    assert_eq!(listed[0].status, TaskStatus::Running);

    let _ = std::fs::remove_file(path);
}
