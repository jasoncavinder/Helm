use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use helm_core::adapters::cargo::{CargoAdapter, CargoDetectOutput, CargoSource};
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, InstallRequest, ManagerAdapter,
    UninstallRequest, UpgradeRequest,
};
use helm_core::models::{
    CoreError, CoreErrorKind, InstalledPackage, ManagerAction, ManagerId, PackageRef,
};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};
use helm_core::persistence::{PackageStore, TaskStore};
use helm_core::sqlite::SqliteStore;

const OLD: &str = "bat v0.24.0:\n    bat\n";
const NEW: &str = "bat v0.25.0:\n    bat\n";
const OUTDATED: &str =
    r#"[{"name":"bat","installed_version":"0.24.0","candidate_version":"0.25.0"}]"#;

type MutationCall = (ManagerAction, String, Option<String>);

#[derive(Clone)]
struct Source {
    inventories: Arc<Mutex<VecDeque<AdapterResult<String>>>>,
    outdated: String,
    outdated_calls: Arc<AtomicUsize>,
    mutations: Arc<Mutex<Vec<MutationCall>>>,
    mutation_result: AdapterResult<String>,
}

impl Source {
    fn new(inventories: &[&str]) -> Self {
        Self {
            inventories: Arc::new(Mutex::new(
                inventories.iter().map(|s| Ok((*s).into())).collect(),
            )),
            outdated: OUTDATED.into(),
            outdated_calls: Arc::new(AtomicUsize::new(0)),
            mutations: Default::default(),
            mutation_result: Ok(String::new()),
        }
    }
    fn mutate(
        &self,
        action: ManagerAction,
        name: &str,
        version: Option<&str>,
    ) -> AdapterResult<String> {
        self.mutations
            .lock()
            .unwrap()
            .push((action, name.into(), version.map(str::to_owned)));
        self.mutation_result.clone()
    }
}

impl CargoSource for Source {
    fn detect(&self) -> AdapterResult<CargoDetectOutput> {
        panic!("unexpected detect")
    }
    fn search(&self, _: &str) -> AdapterResult<String> {
        panic!("unexpected search")
    }
    fn list_installed(&self) -> AdapterResult<String> {
        self.inventories
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected inventory read")
    }
    fn list_outdated(&self) -> AdapterResult<String> {
        assert_eq!(
            self.outdated_calls.fetch_add(1, Ordering::SeqCst),
            0,
            "no remote verification or candidate re-resolution"
        );
        Ok(self.outdated.clone())
    }
    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        self.mutate(ManagerAction::Install, name, version)
    }
    fn uninstall(&self, name: &str) -> AdapterResult<String> {
        self.mutate(ManagerAction::Uninstall, name, None)
    }
    fn upgrade(&self, name: &str, version: &str) -> AdapterResult<String> {
        self.mutate(ManagerAction::Upgrade, name, Some(version))
    }
}

fn package() -> PackageRef {
    PackageRef {
        manager: ManagerId::Cargo,
        name: "bat".into(),
    }
}
fn install(version: Option<&str>) -> AdapterRequest {
    AdapterRequest::Install(InstallRequest {
        package: package(),
        target_name: None,
        version: version.map(str::to_owned),
    })
}
fn upgrade(version: Option<&str>) -> AdapterRequest {
    AdapterRequest::Upgrade(UpgradeRequest {
        package: Some(package()),
        target_name: None,
        version: version.map(str::to_owned),
    })
}
fn bulk() -> AdapterRequest {
    AdapterRequest::Upgrade(UpgradeRequest {
        package: None,
        target_name: None,
        version: None,
    })
}

#[test]
fn explicit_and_resolved_upgrades_bind_one_candidate_and_report_observed_versions() {
    for version in [Some("0.25.0"), None] {
        let mut source = Source::new(&[OLD, OLD, NEW]);
        if version.is_some() {
            source.outdated = "a newer remote candidate must never be queried".into();
        }
        let result = CargoAdapter::new(source.clone())
            .execute(upgrade(version))
            .unwrap();
        let AdapterResponse::Mutation(result) = result else {
            panic!("expected mutation")
        };
        assert_eq!(result.before_version.as_deref(), Some("0.24.0"));
        assert_eq!(result.after_version.as_deref(), Some("0.25.0"));
        assert_eq!(
            *source.mutations.lock().unwrap(),
            vec![(ManagerAction::Upgrade, "bat".into(), Some("0.25.0".into()))]
        );
        assert_eq!(
            source.outdated_calls.load(Ordering::SeqCst),
            usize::from(version.is_none())
        );
    }
}

#[test]
fn missing_wrong_ambiguous_malformed_and_source_annotated_results_fail_verification() {
    for observed in [
        "",
        OLD,
        "garbage",
        "    orphaned binary\n",
        "bat v0.25.0:\n",
        "bat v0.25:\n    bat\n",
        "bat v0.25.0:\n    bat\nbat v0.25.0:\n    bat\n",
        "bat v0.25.0 (/tmp/other):\n    bat\n",
    ] {
        for request in [install(Some("0.25.0")), upgrade(Some("0.25.0"))] {
            let source = if request.action() == ManagerAction::Install {
                Source::new(&[OLD, observed])
            } else {
                Source::new(&[OLD, OLD, observed])
            };
            assert!(
                CargoAdapter::new(source.clone()).execute(request).is_err(),
                "unverified inventory: {observed}"
            );
            assert_eq!(source.mutations.lock().unwrap().len(), 1);
        }
    }
}

#[test]
fn observed_unversioned_install_and_exact_prerelease_install_succeed() {
    for (requested, observed, expected) in [
        (None, NEW, "0.25.0"),
        (
            Some(" 0.25.0-rc.1 "),
            "bat v0.25.0-rc.1:\n    bat\n",
            "0.25.0-rc.1",
        ),
    ] {
        let source = Source::new(&["", observed]);
        let AdapterResponse::Mutation(result) = CargoAdapter::new(source)
            .execute(install(requested))
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(result.after_version.as_deref(), Some(expected));
    }
}

#[test]
fn invalid_versions_and_ambiguous_preconditions_do_not_mutate() {
    for version in [
        "",
        " ",
        "^0.25",
        "*",
        ">=0.25.0",
        "0.25",
        "--git",
        "0.25.0\n--force",
    ] {
        for request in [install(Some(version)), upgrade(Some(version))] {
            let source = Source::new(&[]);
            assert_eq!(
                CargoAdapter::new(source.clone())
                    .execute(request)
                    .unwrap_err()
                    .kind,
                CoreErrorKind::InvalidInput
            );
            assert!(source.mutations.lock().unwrap().is_empty());
        }
    }
    for inventory in [
        "garbage",
        "bat v0.24.0 (/tmp/local):\n    bat\n",
        "bat v0.24.0 (https://github.com/sharkdp/bat#abc):\n    bat\n",
        "bat v0.24.0:\n    bat\nbat v0.24.0:\n    bat\n",
    ] {
        let source = Source::new(&[inventory]);
        assert!(
            CargoAdapter::new(source.clone())
                .execute(upgrade(Some("0.25.0")))
                .is_err()
        );
        assert!(source.mutations.lock().unwrap().is_empty());
    }
}

#[test]
fn failed_post_read_and_mutation_cancellation_propagate_without_retry() {
    for kind in [
        CoreErrorKind::Cancelled,
        CoreErrorKind::Timeout,
        CoreErrorKind::ProcessFailure,
    ] {
        let error = CoreError {
            manager: Some(ManagerId::Cargo),
            action: Some(ManagerAction::Upgrade),
            task: None,
            kind,
            message: "injected failure".into(),
        };
        let source = Source::new(&[OLD, OLD]);
        source
            .inventories
            .lock()
            .unwrap()
            .push_back(Err(error.clone()));
        assert_eq!(
            CargoAdapter::new(source.clone())
                .execute(upgrade(Some("0.25.0")))
                .unwrap_err(),
            error
        );
        assert_eq!(source.mutations.lock().unwrap().len(), 1);
        let mut source = Source::new(&[OLD, OLD]);
        source.mutation_result = Err(error.clone());
        assert_eq!(
            CargoAdapter::new(source.clone())
                .execute(upgrade(Some("0.25.0")))
                .unwrap_err(),
            error
        );
        assert_eq!(source.mutations.lock().unwrap().len(), 1);
        assert!(source.inventories.lock().unwrap().is_empty());
    }
}

#[test]
fn already_current_target_is_observed_without_forced_reinstall() {
    for requested in [Some("0.25.0"), None] {
        let mut source = Source::new(&[NEW, NEW, NEW]);
        source.outdated = "[]".into();
        let AdapterResponse::Mutation(result) = CargoAdapter::new(source.clone())
            .execute(upgrade(requested))
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(result.before_version, result.after_version);
        assert_eq!(result.after_version.as_deref(), Some("0.25.0"));
        assert!(source.mutations.lock().unwrap().is_empty());
    }
}

#[test]
fn bulk_candidates_are_validated_up_front_and_verified_individually() {
    let before = "bat v0.24.0:\n    bat\nzellij v0.42.0:\n    zellij\n";
    let intermediate = before.replace("0.24.0", "0.25.0");
    let after = intermediate.replace("0.42.0", "0.42.1");
    let candidates = r#"[{"name":"bat","installed_version":"0.24.0","candidate_version":"0.25.0"},{"name":"zellij","installed_version":"0.42.0","candidate_version":"0.42.1"}]"#;
    let mut source = Source::new(&[before, before, &intermediate, &intermediate, &after]);
    source.outdated = candidates.into();
    let AdapterResponse::Mutation(result) =
        CargoAdapter::new(source.clone()).execute(bulk()).unwrap()
    else {
        panic!()
    };
    assert_eq!(result.package.name, "__all__");
    assert_eq!(
        result.after_version, None,
        "a batch must not claim the last package's version"
    );
    assert_eq!(
        *source.mutations.lock().unwrap(),
        vec![
            (ManagerAction::Upgrade, "bat".into(), Some("0.25.0".into())),
            (
                ManagerAction::Upgrade,
                "zellij".into(),
                Some("0.42.1".into())
            )
        ]
    );

    for candidates in [
        candidates.replace("0.42.1", "^0.42"),
        candidates.replace("0.42.1", ""),
        candidates.replace("zellij", "bat"),
    ] {
        let mut source = Source::new(&[before]);
        source.outdated = candidates;
        assert!(CargoAdapter::new(source.clone()).execute(bulk()).is_err());
        assert!(source.mutations.lock().unwrap().is_empty());
    }
}

#[test]
fn inventory_drift_before_execution_stops_instead_of_recreating_removed_package() {
    for changed in ["", "bat v0.23.0:\n    bat\n"] {
        let source = Source::new(&[OLD, changed]);
        assert!(
            CargoAdapter::new(source.clone())
                .execute(upgrade(Some("0.25.0")))
                .is_err()
        );
        assert!(source.mutations.lock().unwrap().is_empty());
    }
}

#[test]
fn uninstall_must_observe_absence() {
    for after in ["", OLD] {
        let source = Source::new(&[OLD, after]);
        let result =
            CargoAdapter::new(source).execute(AdapterRequest::Uninstall(UninstallRequest {
                package: package(),
                target_name: None,
                version: None,
            }));
        assert_eq!(result.is_ok(), after.is_empty());
    }
}

#[tokio::test]
async fn unverified_upgrade_never_persists_the_candidate_as_installed() {
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteStore::new(root.path().join("contract.db")));
    store.migrate_to_latest().unwrap();
    let old = InstalledPackage {
        package: package(),
        package_identifier: None,
        installed_version: Some("0.24.0".into()),
        pinned: false,
        runtime_state: Default::default(),
    };
    store.upsert_installed(std::slice::from_ref(&old)).unwrap();
    let source = Source::new(&[OLD, OLD, OLD]);
    let runtime = AdapterRuntime::with_all_stores(
        [Arc::new(CargoAdapter::new(source)) as Arc<dyn ManagerAdapter>],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();
    let (task, receipt) = runtime
        .submit_with_persistence(ManagerId::Cargo, upgrade(Some("0.25.0")))
        .await
        .unwrap();
    let terminal = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    assert!(matches!(
        terminal.terminal_state,
        Some(AdapterTaskTerminalState::Failed(_))
    ));
    tokio::time::timeout(Duration::from_secs(5), receipt.wait_for_completion())
        .await
        .unwrap();
    assert_eq!(store.list_installed().unwrap(), vec![old]);
    assert!(
        store
            .list_task_logs(task, 250)
            .unwrap()
            .iter()
            .any(|log| log.message.contains("unverified"))
    );
}
