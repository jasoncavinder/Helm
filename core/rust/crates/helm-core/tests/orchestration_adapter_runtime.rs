use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter, RefreshRequest, SearchRequest,
};
use helm_core::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, ManagerAction, ManagerAuthority,
    ManagerCategory, ManagerDescriptor, ManagerId, SearchQuery, TaskId, TaskRecord, TaskStatus,
    TaskType,
};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};
use helm_core::persistence::{PersistenceResult, TaskStore};

const TEST_CAPABILITIES: &[Capability] = &[Capability::Refresh, Capability::Search];

#[derive(Clone)]
enum AdapterBehavior {
    Succeeds(AdapterResponse),
    Fails(CoreError),
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
                display_name: "test-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities: TEST_CAPABILITIES,
            },
            behavior,
        }
    }
}

#[derive(Default)]
struct RecordingTaskStore {
    records: Mutex<HashMap<TaskId, TaskRecord>>,
    fail_create: bool,
}

impl RecordingTaskStore {
    fn failing_create() -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            fail_create: true,
        }
    }

    fn get(&self, task_id: TaskId) -> Option<TaskRecord> {
        self.records.lock().ok()?.get(&task_id).cloned()
    }
}

impl TaskStore for RecordingTaskStore {
    fn create_task(&self, task: &TaskRecord) -> PersistenceResult<()> {
        if self.fail_create {
            return Err(CoreError {
                manager: None,
                task: None,
                action: None,
                kind: CoreErrorKind::StorageFailure,
                message: "create_task forced failure".to_string(),
            });
        }

        let mut records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        records.insert(task.id, task.clone());
        Ok(())
    }

    fn update_task(&self, task: &TaskRecord) -> PersistenceResult<()> {
        let mut records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        if !records.contains_key(&task.id) {
            return Err(CoreError {
                manager: None,
                task: None,
                action: None,
                kind: CoreErrorKind::StorageFailure,
                message: "task record not found".to_string(),
            });
        }
        records.insert(task.id, task.clone());
        Ok(())
    }

    fn list_recent_tasks(&self, limit: usize) -> PersistenceResult<Vec<TaskRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        let mut values = records.values().cloned().collect::<Vec<_>>();
        values.sort_by_key(|record| std::cmp::Reverse(record.id.0));
        values.truncate(limit);
        Ok(values)
    }

    fn next_task_id(&self) -> PersistenceResult<u64> {
        let records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        Ok(records.keys().map(|id| id.0).max().map_or(0, |m| m + 1))
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
        match &self.behavior {
            AdapterBehavior::Succeeds(response) => Ok(response.clone()),
            AdapterBehavior::Fails(error) => Err(error.clone()),
        }
    }
}

#[tokio::test]
async fn submit_routes_to_registered_adapter() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();

    let task_id = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();
    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);
    assert_eq!(
        snapshot.terminal_state,
        Some(AdapterTaskTerminalState::Succeeded(
            AdapterResponse::Refreshed
        ))
    );
}

#[tokio::test]
async fn submit_returns_structured_error_for_unregistered_manager() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();
    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "fd".to_string(),
            issued_at: SystemTime::now(),
        },
    });

    let error = runtime
        .submit(ManagerId::Pip, request)
        .await
        .expect_err("expected unregistered manager error");

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::Pip));
    assert_eq!(error.task, Some(TaskType::Search));
    assert_eq!(error.action, Some(ManagerAction::Search));
}

#[tokio::test]
async fn duplicate_adapter_registration_is_rejected() {
    let first: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let second: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Fails(CoreError {
            manager: Some(ManagerId::Npm),
            task: Some(TaskType::Refresh),
            action: Some(ManagerAction::Refresh),
            kind: CoreErrorKind::Internal,
            message: "should not be used".to_string(),
        }),
    ));

    let error = AdapterRuntime::new([first, second])
        .err()
        .expect("expected duplicate manager error");
    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::Npm));
}

#[tokio::test]
async fn submit_with_task_store_persists_queued_then_terminal_status() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let task_store = Arc::new(RecordingTaskStore::default());
    let runtime = AdapterRuntime::with_task_store([adapter], task_store.clone()).unwrap();

    let task_id = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();
    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    let mut persisted = None;
    for _ in 0..20 {
        if let Some(record) = task_store.get(task_id)
            && record.status == TaskStatus::Completed
        {
            persisted = Some(record);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let record = persisted.expect("expected completed persisted task record");
    assert_eq!(record.id, task_id);
    assert_eq!(record.manager, ManagerId::Npm);
    assert_eq!(record.task_type, TaskType::Refresh);
    assert_eq!(record.status, TaskStatus::Completed);
}

#[tokio::test]
async fn submit_returns_error_when_initial_task_persistence_fails() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let task_store = Arc::new(RecordingTaskStore::failing_create());
    let runtime = AdapterRuntime::with_task_store([adapter], task_store).unwrap();

    let error = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect_err("expected task store failure");

    assert_eq!(error.kind, CoreErrorKind::StorageFailure);
    assert_eq!(error.manager, Some(ManagerId::Npm));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
}
