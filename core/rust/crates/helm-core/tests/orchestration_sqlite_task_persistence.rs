use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter, RefreshRequest,
};
use helm_core::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, ManagerAction, ManagerAuthority,
    ManagerCategory, ManagerDescriptor, ManagerId, TaskStatus, TaskType,
};
use helm_core::orchestration::AdapterRuntime;
use helm_core::persistence::TaskStore;
use helm_core::sqlite::SqliteStore;

const TEST_CAPABILITIES: &[Capability] = &[Capability::Refresh];

#[derive(Clone)]
enum AdapterBehavior {
    Succeeds,
    Fails,
    Panics,
}

struct TestAdapter {
    descriptor: ManagerDescriptor,
    behavior: AdapterBehavior,
}

impl TestAdapter {
    fn new(manager: ManagerId, behavior: AdapterBehavior) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id: manager,
                display_name: "sqlite-task-test-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities: TEST_CAPABILITIES,
            },
            behavior,
        }
    }
}

impl ManagerAdapter for TestAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, _request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        match self.behavior {
            AdapterBehavior::Succeeds => Ok(AdapterResponse::Refreshed),
            AdapterBehavior::Fails => Err(CoreError {
                manager: Some(self.descriptor.id),
                task: Some(TaskType::Refresh),
                action: Some(ManagerAction::Refresh),
                kind: CoreErrorKind::ProcessFailure,
                message: "simulated refresh failure".to_string(),
            }),
            AdapterBehavior::Panics => panic!("simulated adapter panic"),
        }
    }
}

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-{test_name}-{nanos}.sqlite3"))
}

async fn wait_for_persisted_status(
    store: &SqliteStore,
    task_id: u64,
    expected: TaskStatus,
) -> Option<TaskStatus> {
    for _ in 0..30 {
        let tasks = store
            .list_recent_tasks(20)
            .expect("list_recent_tasks should succeed");
        if let Some(task) = tasks.into_iter().find(|task| task.id.0 == task_id)
            && task.status == expected
        {
            return Some(task.status);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    None
}

#[tokio::test]
async fn completed_task_is_persisted_to_sqlite() {
    let path = test_db_path("orchestration-sqlite-completed");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    let adapter: Arc<dyn ManagerAdapter> =
        Arc::new(TestAdapter::new(ManagerId::Npm, AdapterBehavior::Succeeds));
    let runtime = AdapterRuntime::with_task_store([adapter], store.clone()).unwrap();

    let task_id = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();
    let terminal = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(terminal.runtime.status, TaskStatus::Completed);

    let persisted = wait_for_persisted_status(store.as_ref(), task_id.0, TaskStatus::Completed)
        .await
        .expect("expected completed status to persist");
    assert_eq!(persisted, TaskStatus::Completed);

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn failed_task_is_persisted_to_sqlite() {
    let path = test_db_path("orchestration-sqlite-failed");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    let adapter: Arc<dyn ManagerAdapter> =
        Arc::new(TestAdapter::new(ManagerId::Pip, AdapterBehavior::Fails));
    let runtime = AdapterRuntime::with_task_store([adapter], store.clone()).unwrap();

    let task_id = runtime
        .submit(ManagerId::Pip, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();
    let terminal = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(terminal.runtime.status, TaskStatus::Failed);

    let persisted = wait_for_persisted_status(store.as_ref(), task_id.0, TaskStatus::Failed)
        .await
        .expect("expected failed status to persist");
    assert_eq!(persisted, TaskStatus::Failed);

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn panicking_task_is_persisted_as_failed_to_sqlite() {
    let path = test_db_path("orchestration-sqlite-panicked");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    let adapter: Arc<dyn ManagerAdapter> =
        Arc::new(TestAdapter::new(ManagerId::Pip, AdapterBehavior::Panics));
    let runtime = AdapterRuntime::with_task_store([adapter], store.clone()).unwrap();

    let task_id = runtime
        .submit(ManagerId::Pip, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();
    let terminal = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(terminal.runtime.status, TaskStatus::Failed);

    let persisted = wait_for_persisted_status(store.as_ref(), task_id.0, TaskStatus::Failed)
        .await
        .expect("expected failed status to persist for panicking adapter");
    assert_eq!(persisted, TaskStatus::Failed);

    let _ = std::fs::remove_file(path);
}
