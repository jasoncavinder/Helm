use super::*;
use helm_core::adapters::AdapterResult;
use helm_core::models::{
    ActionSafety, ManagerAction, ManagerDescriptor, NewTaskLogRecord, TaskRecord,
};
use helm_core::persistence::PersistenceResult;
use std::sync::{Mutex, mpsc};

struct SnapshotAdapter;

impl ManagerAdapter for SnapshotAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        registry::manager(ManagerId::Uv).unwrap()
    }

    fn action_safety(&self, _: ManagerAction) -> ActionSafety {
        ActionSafety::ReadOnly
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        if matches!(request, AdapterRequest::Detect(_)) {
            return Ok(AdapterResponse::Detection(DetectionInfo {
                installed: true,
                version: Some("0.12.18".into()),
                executable_path: None,
            }));
        }
        assert!(matches!(request, AdapterRequest::ListOutdated(_)));
        Ok(AdapterResponse::OutdatedPackages(vec![OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Uv,
                name: "persisted-tool".into(),
            },
            package_identifier: None,
            installed_version: Some("1.0".into()),
            candidate_version: "1.1".into(),
            pinned: false,
            restart_required: false,
            runtime_state: Default::default(),
        }]))
    }
}

struct GatedTaskStore {
    store: Arc<SqliteStore>,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl TaskStore for GatedTaskStore {
    fn create_task(&self, task: &TaskRecord) -> PersistenceResult<()> {
        self.store.create_task(task)
    }
    fn update_task(&self, task: &TaskRecord) -> PersistenceResult<()> {
        self.store.update_task(task)
    }
    fn update_task_with_log(
        &self,
        task: &TaskRecord,
        log: &NewTaskLogRecord,
    ) -> PersistenceResult<()> {
        if task.status == TaskStatus::Completed {
            self.entered.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
        }
        self.store.update_task_with_log(task, log)
    }
    fn list_recent_tasks(&self, limit: usize) -> PersistenceResult<Vec<TaskRecord>> {
        self.store.list_recent_tasks(limit)
    }
    fn next_task_id(&self) -> PersistenceResult<u64> {
        self.store.next_task_id()
    }
    fn prune_completed_tasks(&self, seconds: i64) -> PersistenceResult<usize> {
        self.store.prune_completed_tasks(seconds)
    }
    fn delete_all_tasks(&self) -> PersistenceResult<()> {
        self.store.delete_all_tasks()
    }
    fn append_task_log(&self, log: &NewTaskLogRecord) -> PersistenceResult<()> {
        self.store.append_task_log(log)
    }
}

fn check_waits_for_persistence(coordinator: bool) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let store = Arc::new(SqliteStore::new(
        env::temp_dir().join(format!("helm-cli-completion-{nonce}.db")),
    ));
    store.migrate_to_latest().unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let task_store = Arc::new(GatedTaskStore {
        store: store.clone(),
        entered: entered_tx,
        release: Mutex::new(release_rx),
    });
    let runtime = AdapterRuntime::with_all_stores(
        [Arc::new(SnapshotAdapter) as Arc<dyn ManagerAdapter>],
        task_store,
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let worker_store = store.clone();
    let worker = thread::spawn(move || {
        let success = if coordinator {
            handle_coordinator_request(
                &runtime,
                &worker_store,
                CoordinatorRequest::Submit {
                    manager_id: "uv".into(),
                    request: CoordinatorSubmitRequest::Detect,
                    wait: true,
                },
            )
            .ok
        } else {
            cli_tokio_runtime()
                .unwrap()
                .block_on(submit_request_wait(
                    &runtime,
                    ManagerId::Uv,
                    AdapterRequest::ListOutdated(ListOutdatedRequest),
                ))
                .is_ok()
        };
        done_tx.send(success).unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // The adapter is terminal, but both its task row and domain write are still pending.
    let premature = done_rx.recv_timeout(Duration::from_millis(100));
    assert!(store.list_outdated().unwrap().is_empty());
    release_tx.send(()).unwrap();
    let success = match premature {
        Ok(success) => success,
        Err(_) => done_rx.recv_timeout(Duration::from_secs(10)).unwrap(),
    };
    worker.join().unwrap();
    assert!(
        premature.is_err(),
        "CLI returned before response persistence completed"
    );
    assert!(success);
    if coordinator {
        assert_eq!(
            store.list_detections().unwrap()[0].1.version.as_deref(),
            Some("0.12.18")
        );
    } else {
        assert_eq!(store.list_outdated().unwrap()[0].candidate_version, "1.1");
    }
    assert_eq!(
        store.list_recent_tasks(1).unwrap()[0].status,
        TaskStatus::Completed
    );
}

#[test]
fn cli_wait_helper_waits_for_terminal_and_domain_persistence() {
    check_waits_for_persistence(false);
}

#[test]
fn cli_coordinator_waits_for_terminal_and_domain_persistence() {
    let _guard = helm_core::execution::manager_execution_preferences_test_guard();
    check_waits_for_persistence(true);
}
