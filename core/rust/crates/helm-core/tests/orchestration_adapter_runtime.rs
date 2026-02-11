use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter, RefreshRequest, SearchRequest,
};
use helm_core::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, ManagerAction, ManagerAuthority,
    ManagerCategory, ManagerDescriptor, ManagerId, SearchQuery, TaskStatus, TaskType,
};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

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
