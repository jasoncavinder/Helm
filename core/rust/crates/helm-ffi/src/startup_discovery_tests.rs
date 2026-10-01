use super::*;
use helm_core::adapters::{AdapterResponse, ManagerAdapter, RefreshRequest};
use helm_core::models::{ActionSafety, ManagerCategory, ManagerDescriptor};

const CHILD_MODE: &str = "HELM_TEST_STARTUP_DISCOVERY_MODE";
const CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

struct RecordingAdapter {
    descriptor: ManagerDescriptor,
    events: Arc<Mutex<Vec<(ManagerId, ManagerAction)>>>,
    release_refresh: Arc<AtomicBool>,
    release_detection: Arc<AtomicBool>,
}

impl ManagerAdapter for RecordingAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }
    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }
    fn execute(
        &self,
        request: AdapterRequest,
    ) -> helm_core::adapters::AdapterResult<AdapterResponse> {
        self.events
            .lock()
            .unwrap()
            .push((self.descriptor.id, request.action()));
        match request {
            AdapterRequest::Detect(_) => {
                wait_until(|| self.release_detection.load(Ordering::Acquire));
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed: true,
                    executable_path: Some("/usr/bin/true".into()),
                    version: Some("1.2.3".into()),
                }))
            }
            AdapterRequest::Refresh(_) => {
                wait_until(|| self.release_refresh.load(Ordering::Acquire));
                Ok(AdapterResponse::Refreshed)
            }
            AdapterRequest::ListInstalled(_) => {
                Ok(AdapterResponse::InstalledPackages(vec![InstalledPackage {
                    package: PackageRef {
                        manager: self.descriptor.id,
                        name: "startup-fixture".into(),
                    },
                    package_identifier: None,
                    installed_version: Some("1.0".into()),
                    pinned: false,
                    runtime_state: Default::default(),
                }]))
            }
            AdapterRequest::ListOutdated(_) => Ok(AdapterResponse::OutdatedPackages(Vec::new())),
            _ => panic!("startup must not execute a mutation or unrelated action"),
        }
    }
}

fn wait_until(predicate: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !predicate() {
        assert!(Instant::now() < deadline, "startup fixture timed out");
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn acknowledged_startup_discovers_once_then_refreshes_and_preserves_policy() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        for mode in ["fresh", "sparse", "offline", "overlap", "restart"] {
            let root = std::env::temp_dir().join(format!(
                "helm-startup-discovery-{}-{mode}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&root).unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "startup_discovery_tests::acknowledged_startup_discovers_once_then_refreshes_and_preserves_policy", "--nocapture"])
                .env(CHILD_MODE, mode).env("HELM_TEST_STARTUP_ROOT", &root)
                .env("HOME", &root).env("PATH", "/usr/bin:/bin")
                .env(LEGACY_FILE_COORDINATOR_IPC_ENV, "0")
                .output().unwrap();
            assert!(
                output.status.success(),
                "{mode}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    let root = PathBuf::from(std::env::var_os("HELM_TEST_STARTUP_ROOT").unwrap());
    let store = Arc::new(SqliteStore::new(root.join("helm.db")));
    store.migrate_to_latest().unwrap();
    store.set_safe_mode(true).unwrap();
    store.set_manager_enabled(ManagerId::Yarn, false).unwrap();
    store
        .set_manager_selected_executable_path(ManagerId::Yarn, Some("/preserved/yarn"))
        .unwrap();
    store
        .set_cli_accepted_license_terms_version(Some("helm-source-available-license-v1.0-pre1.0"))
        .unwrap();
    if mode != "fresh" {
        store.set_cli_onboarding_completed(true).unwrap();
        store
            .upsert_detection(
                ManagerId::Yarn,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some("/preserved/yarn".into()),
                    version: Some("old".into()),
                },
            )
            .unwrap();
    }
    let path = CString::new(store.database_path().to_str().unwrap()).unwrap();
    let prepared = unsafe { helm_prepare_startup(path.as_ptr(), true) };
    assert!(!prepared.is_null());
    unsafe { helm_free_string(prepared) };
    assert!(!helm_start_runtime_with_discovery(true));
    assert!(lock_or_recover(&STATE, "state").is_none());
    assert!(store.list_recent_tasks(100).unwrap().is_empty());
    store
        .acknowledge_first_run_experience(FirstRunExperience::CURRENT)
        .unwrap();
    let preferences = store.list_manager_preferences().unwrap();
    let disabled_detection = store.list_detections().unwrap();
    let events = Arc::new(Mutex::new(Vec::new()));
    let runs = if mode == "restart" { 2 } else { 1 };
    for _ in 0..runs {
        let release_refresh = Arc::new(AtomicBool::new(mode != "overlap"));
        let release_detection = Arc::new(AtomicBool::new(false));
        let adapters: Vec<Arc<dyn ManagerAdapter>> = [ManagerId::Pnpm, ManagerId::Yarn]
            .into_iter()
            .map(|manager| {
                Arc::new(RecordingAdapter {
                    descriptor: ManagerDescriptor {
                        id: manager,
                        display_name: "Startup fixture",
                        category: ManagerCategory::Language,
                        authority: ManagerAuthority::Standard,
                        capabilities: CAPABILITIES,
                    },
                    events: events.clone(),
                    release_refresh: release_refresh.clone(),
                    release_detection: release_detection.clone(),
                }) as Arc<dyn ManagerAdapter>
            })
            .collect();
        let runtime = Arc::new(
            AdapterRuntime::with_all_stores(
                adapters,
                store.clone(),
                store.clone(),
                store.clone(),
                store.clone(),
            )
            .unwrap(),
        );
        let tokio = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let discovery: Arc<Mutex<StartupDiscovery>> = Default::default();
        if mode == "overlap" {
            tokio
                .block_on(runtime.submit(ManagerId::Pnpm, AdapterRequest::Refresh(RefreshRequest)))
                .unwrap();
            wait_until(|| {
                events
                    .lock()
                    .unwrap()
                    .contains(&(ManagerId::Pnpm, ManagerAction::Refresh))
            });
        }
        *lock_or_recover(&STATE, "state") = Some(HelmState {
            store: store.clone(),
            runtime: runtime.clone(),
            rt_handle: tokio.handle().clone(),
            startup_discovery: discovery.clone(),
            _tokio_rt: tokio,
        });
        if mode == "overlap" {
            assert!(
                helm_trigger_detection(),
                "manual trigger coalesces with the running refresh"
            );
            assert!(
                !events
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(_, action)| *action == ManagerAction::Detect)
            );
        }
        let online = mode != "offline";
        assert!(helm_start_runtime_with_discovery(online));
        // Duplicate activation cannot launch another discovery or overwrite a
        // newer network handoff with an old snapshot from a reconnecting client.
        let callers: Vec<_> = (0..4)
            .map(|_| thread::spawn(|| helm_start_runtime_with_discovery(false)))
            .collect();
        for caller in callers {
            assert!(caller.join().unwrap());
        }
        assert_eq!(runtime.network_work_allowed(), online);
        release_refresh.store(true, Ordering::Release);
        release_detection.store(true, Ordering::Release);
        wait_until(|| !discovery.lock().unwrap().is_running());
        assert!(
            store
                .list_detections()
                .unwrap()
                .iter()
                .any(|(id, info)| *id == ManagerId::Pnpm
                    && info.installed
                    && info.version.as_deref() == Some("1.2.3"))
        );
        assert!(
            store
                .list_installed()
                .unwrap()
                .iter()
                .any(|p| p.package.name == "startup-fixture")
        );
        if !online {
            assert!(
                !events
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|(_, action)| *action == ManagerAction::ListOutdated)
            );
            assert!(helm_set_network_available(true));
            assert!(helm_set_network_available(true));
            wait_until(|| !discovery.lock().unwrap().is_running());
        }
        let prepared = unsafe { helm_prepare_startup(path.as_ptr(), true) };
        assert!(!prepared.is_null());
        unsafe { helm_free_string(prepared) };
        assert!(helm_start_runtime_with_discovery(true));
        assert!(!discovery.lock().unwrap().is_running());
        for preference in &preferences {
            assert!(
                store
                    .list_manager_preferences()
                    .unwrap()
                    .contains(preference)
            );
        }
        assert!(store.safe_mode().unwrap());
        assert_eq!(store.cli_onboarding_completed().unwrap(), mode != "fresh");
        for (manager, info) in &disabled_detection {
            assert!(
                store
                    .list_detections()
                    .unwrap()
                    .contains(&(*manager, info.clone()))
            );
        }
        // Drop outside the STATE lock; the test simulates a new service process
        // while retaining the same durable profile, not a new acknowledgment.
        let old = lock_or_recover(&STATE, "state").take();
        drop(old);
    }
    let events = events.lock().unwrap();
    assert!(
        events
            .iter()
            .all(|(manager, _)| *manager == ManagerId::Pnpm)
    );
    assert_eq!(
        events
            .iter()
            .filter(|(_, action)| *action == ManagerAction::Detect)
            .count(),
        runs
    );
    assert_eq!(
        events
            .iter()
            .filter(|(_, action)| *action == ManagerAction::ListOutdated)
            .count(),
        runs
    );
    let tasks = store.list_recent_tasks(100).unwrap();
    assert!(
        tasks
            .iter()
            .all(|task| task.status == TaskStatus::Completed)
    );
    assert_eq!(
        tasks
            .iter()
            .filter(|task| task.task_type == TaskType::Detection)
            .count(),
        runs
    );
}
