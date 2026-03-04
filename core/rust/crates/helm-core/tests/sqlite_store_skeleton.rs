use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::models::{
    AutomationLevel, CachedSearchResult, CoreErrorKind, HomebrewKegPolicy,
    InstallInstanceIdentityKind, InstallProvenance, InstalledPackage, ManagerId,
    ManagerInstallInstance, NewTaskLogRecord, OutdatedPackage, PackageCandidate, PackageRef,
    PinKind, PinRecord, StrategyKind, TaskId, TaskLogLevel, TaskRecord, TaskStatus, TaskType,
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
fn auto_check_settings_roundtrip() {
    let path = test_db_path("auto-check-settings-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    assert!(!store.auto_check_for_updates().unwrap());
    assert_eq!(store.auto_check_frequency_minutes().unwrap(), 1_440);
    assert_eq!(store.auto_check_last_checked_unix().unwrap(), None);

    store.set_auto_check_for_updates(true).unwrap();
    store.set_auto_check_frequency_minutes(60).unwrap();
    store.set_auto_check_last_checked_unix(12345).unwrap();

    assert!(store.auto_check_for_updates().unwrap());
    assert_eq!(store.auto_check_frequency_minutes().unwrap(), 60);
    assert_eq!(store.auto_check_last_checked_unix().unwrap(), Some(12345));

    store.set_auto_check_for_updates(false).unwrap();
    store.set_auto_check_frequency_minutes(1_440).unwrap();
    store.set_auto_check_last_checked_unix(0).unwrap();

    assert!(!store.auto_check_for_updates().unwrap());
    assert_eq!(store.auto_check_frequency_minutes().unwrap(), 1_440);
    assert_eq!(store.auto_check_last_checked_unix().unwrap(), Some(0));

    let _ = std::fs::remove_file(path);
}

#[test]
fn cli_onboarding_settings_roundtrip() {
    let path = test_db_path("cli-onboarding-settings-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    assert!(!store.cli_onboarding_completed().unwrap());
    assert_eq!(store.cli_accepted_license_terms_version().unwrap(), None);

    store.set_cli_onboarding_completed(true).unwrap();
    store
        .set_cli_accepted_license_terms_version(Some("helm-source-available-license-v1.0-pre1.0"))
        .unwrap();

    assert!(store.cli_onboarding_completed().unwrap());
    assert_eq!(
        store.cli_accepted_license_terms_version().unwrap(),
        Some("helm-source-available-license-v1.0-pre1.0".to_string())
    );

    store.set_cli_onboarding_completed(false).unwrap();
    store.set_cli_accepted_license_terms_version(None).unwrap();

    assert!(!store.cli_onboarding_completed().unwrap());
    assert_eq!(store.cli_accepted_license_terms_version().unwrap(), None);

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
fn apply_install_result_updates_installed_snapshot_and_clears_outdated_entry() {
    let path = test_db_path("apply-install-result");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let package = PackageRef {
        manager: ManagerId::Npm,
        name: "eslint".to_string(),
    };

    store
        .upsert_outdated(&[OutdatedPackage {
            package: package.clone(),
            installed_version: Some("9.24.0".to_string()),
            candidate_version: "9.25.0".to_string(),
            pinned: false,
            restart_required: false,
        }])
        .unwrap();

    store
        .apply_install_result(&package, Some("9.25.0"))
        .unwrap();

    let installed = store.list_installed().unwrap();
    let outdated = store.list_outdated().unwrap();
    let installed_entry = installed
        .iter()
        .find(|entry| entry.package == package)
        .expect("installed snapshot should contain package after install result");

    assert_eq!(installed_entry.installed_version.as_deref(), Some("9.25.0"));
    assert!(outdated.iter().all(|entry| entry.package != package));

    let _ = std::fs::remove_file(path);
}

#[test]
fn apply_uninstall_result_removes_package_from_cached_snapshots() {
    let path = test_db_path("apply-uninstall-result");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let package = PackageRef {
        manager: ManagerId::Pnpm,
        name: "typescript".to_string(),
    };

    store
        .upsert_installed(&[InstalledPackage {
            package: package.clone(),
            installed_version: Some("5.8.3".to_string()),
            pinned: false,
        }])
        .unwrap();
    store
        .upsert_outdated(&[OutdatedPackage {
            package: package.clone(),
            installed_version: Some("5.8.3".to_string()),
            candidate_version: "5.9.0".to_string(),
            pinned: false,
            restart_required: false,
        }])
        .unwrap();

    store.apply_uninstall_result(&package).unwrap();

    let installed = store.list_installed().unwrap();
    let outdated = store.list_outdated().unwrap();
    assert!(installed.iter().all(|entry| entry.package != package));
    assert!(outdated.iter().all(|entry| entry.package != package));

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
fn upsert_detection_treats_empty_version_as_missing() {
    let path = test_db_path("detection-empty-version");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    store
        .upsert_detection(
            ManagerId::HomebrewFormula,
            &helm_core::models::DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from("/usr/local/bin/brew")),
                version: Some(String::new()),
            },
        )
        .unwrap();

    let detections = store.list_detections().unwrap();
    let homebrew = detections
        .into_iter()
        .find(|(manager, _)| *manager == ManagerId::HomebrewFormula)
        .expect("homebrew detection should exist");
    assert_eq!(homebrew.1.version, None);

    let _ = std::fs::remove_file(path);
}

#[test]
fn replace_install_instances_roundtrip_and_filtering() {
    let path = test_db_path("install-instances-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let rustup_instance = ManagerInstallInstance {
        manager: ManagerId::Rustup,
        instance_id: "rustup-a".to_string(),
        identity_kind: InstallInstanceIdentityKind::DevInode,
        identity_value: "16777230:42".to_string(),
        display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
        canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
        alias_paths: vec![
            PathBuf::from("/Users/test/.cargo/bin/rustup"),
            PathBuf::from("/usr/local/bin/rustup"),
        ],
        is_active: true,
        version: Some("1.28.2".to_string()),
        provenance: InstallProvenance::RustupInit,
        confidence: 0.91,
        decision_margin: Some(0.49),
        automation_level: AutomationLevel::Automatic,
        uninstall_strategy: StrategyKind::RustupSelf,
        update_strategy: StrategyKind::RustupSelf,
        remediation_strategy: StrategyKind::ManualRemediation,
        explanation_primary: Some("path is in ~/.cargo/bin".to_string()),
        explanation_secondary: Some("no Homebrew cellar evidence".to_string()),
        competing_provenance: Some(InstallProvenance::Homebrew),
        competing_confidence: Some(0.42),
    };
    let brew_instance = ManagerInstallInstance {
        manager: ManagerId::HomebrewFormula,
        instance_id: "brew-a".to_string(),
        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
        identity_value: "/opt/homebrew/bin/brew".to_string(),
        display_path: PathBuf::from("/opt/homebrew/bin/brew"),
        canonical_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
        alias_paths: vec![PathBuf::from("/opt/homebrew/bin/brew")],
        is_active: true,
        version: Some("4.6.0".to_string()),
        provenance: InstallProvenance::Homebrew,
        confidence: 1.0,
        decision_margin: Some(1.0),
        automation_level: AutomationLevel::Automatic,
        uninstall_strategy: StrategyKind::HomebrewFormula,
        update_strategy: StrategyKind::HomebrewFormula,
        remediation_strategy: StrategyKind::ManualRemediation,
        explanation_primary: Some("brew canonical path detected".to_string()),
        explanation_secondary: None,
        competing_provenance: None,
        competing_confidence: None,
    };

    store
        .replace_install_instances(ManagerId::Rustup, std::slice::from_ref(&rustup_instance))
        .unwrap();
    store
        .replace_install_instances(
            ManagerId::HomebrewFormula,
            std::slice::from_ref(&brew_instance),
        )
        .unwrap();

    let all = store.list_install_instances(None).unwrap();
    assert_eq!(all.len(), 2);

    let rustup_only = store
        .list_install_instances(Some(ManagerId::Rustup))
        .unwrap();
    assert_eq!(rustup_only.len(), 1);
    assert_eq!(rustup_only[0].manager, ManagerId::Rustup);
    assert_eq!(rustup_only[0].provenance, InstallProvenance::RustupInit);
    assert_eq!(rustup_only[0].automation_level, AutomationLevel::Automatic);
    assert_eq!(rustup_only[0].alias_paths.len(), 2);
    assert_eq!(rustup_only[0].decision_margin, Some(0.49));

    let _ = std::fs::remove_file(path);
}

#[test]
fn replace_install_instances_replaces_previous_rows_for_manager() {
    let path = test_db_path("install-instances-replace");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let first = ManagerInstallInstance {
        manager: ManagerId::Rustup,
        instance_id: "rustup-first".to_string(),
        identity_kind: InstallInstanceIdentityKind::DevInode,
        identity_value: "1:1".to_string(),
        display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
        canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
        alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
        is_active: true,
        version: Some("1.28.2".to_string()),
        provenance: InstallProvenance::RustupInit,
        confidence: 0.9,
        decision_margin: Some(0.4),
        automation_level: AutomationLevel::Automatic,
        uninstall_strategy: StrategyKind::RustupSelf,
        update_strategy: StrategyKind::RustupSelf,
        remediation_strategy: StrategyKind::ManualRemediation,
        explanation_primary: None,
        explanation_secondary: None,
        competing_provenance: None,
        competing_confidence: None,
    };
    let second = ManagerInstallInstance {
        manager: ManagerId::Rustup,
        instance_id: "rustup-second".to_string(),
        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
        identity_value: "/opt/homebrew/bin/rustup".to_string(),
        display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
        canonical_path: Some(PathBuf::from(
            "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
        )),
        alias_paths: vec![
            PathBuf::from("/opt/homebrew/bin/rustup"),
            PathBuf::from("/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup"),
        ],
        is_active: true,
        version: Some("1.28.2".to_string()),
        provenance: InstallProvenance::Homebrew,
        confidence: 0.94,
        decision_margin: Some(0.7),
        automation_level: AutomationLevel::Automatic,
        uninstall_strategy: StrategyKind::HomebrewFormula,
        update_strategy: StrategyKind::HomebrewFormula,
        remediation_strategy: StrategyKind::ManualRemediation,
        explanation_primary: Some("cellar target".to_string()),
        explanation_secondary: None,
        competing_provenance: None,
        competing_confidence: None,
    };

    store
        .replace_install_instances(ManagerId::Rustup, std::slice::from_ref(&first))
        .unwrap();
    assert_eq!(
        store
            .list_install_instances(Some(ManagerId::Rustup))
            .unwrap()
            .len(),
        1
    );

    store
        .replace_install_instances(ManagerId::Rustup, std::slice::from_ref(&second))
        .unwrap();
    let rows = store
        .list_install_instances(Some(ManagerId::Rustup))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].instance_id, "rustup-second");
    assert_eq!(rows[0].provenance, InstallProvenance::Homebrew);

    let _ = std::fs::remove_file(path);
}

#[test]
fn manager_multi_instance_ack_fingerprint_roundtrip() {
    let path = test_db_path("manager-multi-instance-ack-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    assert_eq!(
        store
            .manager_multi_instance_ack_fingerprint(ManagerId::Rustup)
            .unwrap(),
        None
    );

    store
        .set_manager_multi_instance_ack_fingerprint(ManagerId::Rustup, Some("abc123"))
        .unwrap();
    assert_eq!(
        store
            .manager_multi_instance_ack_fingerprint(ManagerId::Rustup)
            .unwrap()
            .as_deref(),
        Some("abc123")
    );

    store
        .set_manager_multi_instance_ack_fingerprint(ManagerId::Rustup, None)
        .unwrap();
    assert_eq!(
        store
            .manager_multi_instance_ack_fingerprint(ManagerId::Rustup)
            .unwrap(),
        None
    );

    let _ = std::fs::remove_file(path);
}

#[test]
fn manager_preferences_selection_fields_roundtrip() {
    let path = test_db_path("manager-preferences-selection-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    store.set_manager_enabled(ManagerId::Pip, false).unwrap();
    store
        .set_manager_selected_executable_path(ManagerId::Pip, Some("/tmp/custom-python3"))
        .unwrap();
    store
        .set_manager_selected_install_method(ManagerId::Pip, Some("homebrew"))
        .unwrap();
    store
        .set_manager_timeout_hard_seconds(ManagerId::Pip, Some(600))
        .unwrap();
    store
        .set_manager_timeout_idle_seconds(ManagerId::Pip, Some(180))
        .unwrap();

    let prefs = store.list_manager_preferences().unwrap();
    let pip_pref = prefs
        .into_iter()
        .find(|pref| pref.manager == ManagerId::Pip)
        .expect("pip manager preference should exist");
    assert!(!pip_pref.enabled);
    assert_eq!(
        pip_pref.selected_executable_path.as_deref(),
        Some("/tmp/custom-python3")
    );
    assert_eq!(
        pip_pref.selected_install_method.as_deref(),
        Some("homebrew")
    );
    assert_eq!(pip_pref.timeout_hard_seconds, Some(600));
    assert_eq!(pip_pref.timeout_idle_seconds, Some(180));

    store
        .set_manager_selected_executable_path(ManagerId::Pip, None)
        .unwrap();
    store
        .set_manager_selected_install_method(ManagerId::Pip, None)
        .unwrap();
    store
        .set_manager_timeout_hard_seconds(ManagerId::Pip, None)
        .unwrap();
    store
        .set_manager_timeout_idle_seconds(ManagerId::Pip, None)
        .unwrap();

    let prefs = store.list_manager_preferences().unwrap();
    let pip_pref = prefs
        .into_iter()
        .find(|pref| pref.manager == ManagerId::Pip)
        .expect("pip manager preference should exist");
    assert_eq!(pip_pref.selected_executable_path, None);
    assert_eq!(pip_pref.selected_install_method, None);
    assert_eq!(pip_pref.timeout_hard_seconds, None);
    assert_eq!(pip_pref.timeout_idle_seconds, None);
    assert!(!pip_pref.enabled);

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
fn search_cache_keeps_single_package_entry_and_preserves_summary() {
    let path = test_db_path("search-preserve-summary");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let now = SystemTime::now();
    let first = CachedSearchResult {
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
    };
    store.upsert_search_results(&[first]).unwrap();

    let second = CachedSearchResult {
        result: PackageCandidate {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: "ripgrep".to_string(),
            },
            version: None,
            summary: None,
        },
        source_manager: ManagerId::HomebrewFormula,
        originating_query: "rg".to_string(),
        cached_at: now + Duration::from_secs(5),
    };
    store.upsert_search_results(&[second]).unwrap();

    let all = store.query_local("", 10).unwrap();
    assert_eq!(
        all.len(),
        1,
        "package cache should deduplicate by manager/name"
    );
    assert_eq!(all[0].result.package.name, "ripgrep");
    assert_eq!(all[0].result.version.as_deref(), Some("14.1.0"));
    assert_eq!(
        all[0].result.summary.as_deref(),
        Some("line-oriented search tool"),
        "summary should be preserved when newer response omits it"
    );
    assert_eq!(all[0].originating_query, "rg");

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

#[test]
fn prune_completed_tasks_removes_cancelled_and_keeps_running_records() {
    let path = test_db_path("tasks-prune-filter");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let old_created_at = UNIX_EPOCH + Duration::from_secs(5);
    let records = [
        TaskRecord {
            id: TaskId(1),
            manager: ManagerId::HomebrewFormula,
            task_type: TaskType::Refresh,
            status: TaskStatus::Completed,
            created_at: old_created_at,
        },
        TaskRecord {
            id: TaskId(2),
            manager: ManagerId::HomebrewFormula,
            task_type: TaskType::Refresh,
            status: TaskStatus::Failed,
            created_at: old_created_at,
        },
        TaskRecord {
            id: TaskId(3),
            manager: ManagerId::HomebrewFormula,
            task_type: TaskType::Refresh,
            status: TaskStatus::Cancelled,
            created_at: old_created_at,
        },
        TaskRecord {
            id: TaskId(4),
            manager: ManagerId::HomebrewFormula,
            task_type: TaskType::Refresh,
            status: TaskStatus::Running,
            created_at: old_created_at,
        },
    ];

    for record in &records {
        store.create_task(record).unwrap();
    }

    let deleted = store.prune_completed_tasks(1).unwrap();
    assert_eq!(deleted, 2);

    let remaining = store.list_recent_tasks(10).unwrap();
    assert_eq!(remaining.len(), 2);
    let statuses = remaining
        .into_iter()
        .map(|task| task.status)
        .collect::<Vec<_>>();
    assert!(statuses.contains(&TaskStatus::Failed));
    assert!(statuses.contains(&TaskStatus::Running));

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_task_removes_record_and_logs() {
    let path = test_db_path("delete-task-with-logs");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let task = TaskRecord {
        id: TaskId(333),
        manager: ManagerId::Npm,
        task_type: TaskType::Upgrade,
        status: TaskStatus::Failed,
        created_at: UNIX_EPOCH + Duration::from_secs(1_000),
    };
    store.create_task(&task).unwrap();
    store
        .append_task_log(&NewTaskLogRecord {
            task_id: task.id,
            manager: task.manager,
            task_type: task.task_type,
            status: Some(TaskStatus::Failed),
            level: TaskLogLevel::Error,
            message: "task failed".to_string(),
            created_at: UNIX_EPOCH + Duration::from_secs(1_001),
        })
        .unwrap();

    store.delete_task(task.id).unwrap();
    assert!(store.list_recent_tasks(10).unwrap().is_empty());
    assert!(store.list_task_logs(task.id, 10).unwrap().is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn delete_tasks_for_manager_removes_only_matching_manager_rows() {
    let path = test_db_path("delete-tasks-for-manager");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let homebrew_task = TaskRecord {
        id: TaskId(400),
        manager: ManagerId::HomebrewFormula,
        task_type: TaskType::Upgrade,
        status: TaskStatus::Failed,
        created_at: UNIX_EPOCH + Duration::from_secs(1_100),
    };
    let npm_task = TaskRecord {
        id: TaskId(401),
        manager: ManagerId::Npm,
        task_type: TaskType::Upgrade,
        status: TaskStatus::Failed,
        created_at: UNIX_EPOCH + Duration::from_secs(1_101),
    };
    store.create_task(&homebrew_task).unwrap();
    store.create_task(&npm_task).unwrap();

    store
        .append_task_log(&NewTaskLogRecord {
            task_id: homebrew_task.id,
            manager: homebrew_task.manager,
            task_type: homebrew_task.task_type,
            status: Some(TaskStatus::Failed),
            level: TaskLogLevel::Error,
            message: "homebrew failed".to_string(),
            created_at: UNIX_EPOCH + Duration::from_secs(1_102),
        })
        .unwrap();
    store
        .append_task_log(&NewTaskLogRecord {
            task_id: npm_task.id,
            manager: npm_task.manager,
            task_type: npm_task.task_type,
            status: Some(TaskStatus::Failed),
            level: TaskLogLevel::Error,
            message: "npm failed".to_string(),
            created_at: UNIX_EPOCH + Duration::from_secs(1_103),
        })
        .unwrap();

    store
        .delete_tasks_for_manager(ManagerId::HomebrewFormula)
        .unwrap();

    let remaining = store.list_recent_tasks(10).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].manager, ManagerId::Npm);
    assert!(
        store
            .list_task_logs(homebrew_task.id, 10)
            .unwrap()
            .is_empty()
    );
    assert_eq!(store.list_task_logs(npm_task.id, 10).unwrap().len(), 1);

    let _ = std::fs::remove_file(path);
}

#[test]
fn append_and_list_task_logs_roundtrip() {
    let path = test_db_path("task-logs-roundtrip");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let task = TaskRecord {
        id: TaskId(77),
        manager: ManagerId::Npm,
        task_type: TaskType::Refresh,
        status: TaskStatus::Queued,
        created_at: UNIX_EPOCH + Duration::from_secs(900),
    };
    store.create_task(&task).unwrap();

    let first = NewTaskLogRecord {
        task_id: task.id,
        manager: task.manager,
        task_type: task.task_type,
        status: Some(TaskStatus::Queued),
        level: TaskLogLevel::Info,
        message: "task queued".to_string(),
        created_at: UNIX_EPOCH + Duration::from_secs(901),
    };
    let second = NewTaskLogRecord {
        task_id: task.id,
        manager: task.manager,
        task_type: task.task_type,
        status: Some(TaskStatus::Failed),
        level: TaskLogLevel::Error,
        message: "task failed: simulated error".to_string(),
        created_at: UNIX_EPOCH + Duration::from_secs(902),
    };

    store.append_task_log(&first).unwrap();
    store.append_task_log(&second).unwrap();

    let logs = store.list_task_logs(task.id, 10).unwrap();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].status, Some(TaskStatus::Failed));
    assert_eq!(logs[0].level, TaskLogLevel::Error);
    assert!(logs[0].message.contains("simulated error"));
    assert_eq!(logs[1].status, Some(TaskStatus::Queued));
    assert_eq!(logs[1].level, TaskLogLevel::Info);

    let _ = std::fs::remove_file(path);
}

#[test]
fn prune_completed_tasks_removes_associated_task_logs() {
    let path = test_db_path("task-logs-prune-with-task");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();

    let old_created_at = UNIX_EPOCH + Duration::from_secs(5);
    let completed_task = TaskRecord {
        id: TaskId(101),
        manager: ManagerId::HomebrewFormula,
        task_type: TaskType::Refresh,
        status: TaskStatus::Completed,
        created_at: old_created_at,
    };

    store.create_task(&completed_task).unwrap();
    store
        .append_task_log(&NewTaskLogRecord {
            task_id: completed_task.id,
            manager: completed_task.manager,
            task_type: completed_task.task_type,
            status: Some(TaskStatus::Completed),
            level: TaskLogLevel::Info,
            message: "task completed".to_string(),
            created_at: old_created_at,
        })
        .unwrap();

    let deleted = store.prune_completed_tasks(1).unwrap();
    assert_eq!(deleted, 1);
    assert!(store.list_recent_tasks(10).unwrap().is_empty());
    assert!(
        store
            .list_task_logs(completed_task.id, 10)
            .unwrap()
            .is_empty()
    );

    let _ = std::fs::remove_file(path);
}
