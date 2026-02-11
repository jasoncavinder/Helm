use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter, RefreshRequest, SearchRequest,
};
use helm_core::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, ManagerAction, ManagerAuthority,
    ManagerCategory, ManagerDescriptor, ManagerId, SearchQuery, TaskStatus, TaskType,
};
use helm_core::orchestration::{
    AdapterExecutionRuntime, AdapterTaskTerminalState, CancellationMode,
};

const TEST_CAPABILITIES: &[Capability] = &[Capability::Refresh, Capability::Search];

#[derive(Clone)]
enum AdapterBehavior {
    Succeeds(AdapterResponse),
    Fails(CoreError),
    SleepsThenSucceeds(Duration, AdapterResponse),
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
            AdapterBehavior::SleepsThenSucceeds(delay, response) => {
                std::thread::sleep(*delay);
                Ok(response.clone())
            }
        }
    }
}

#[tokio::test]
async fn successful_execution_returns_completed_terminal_state() {
    let runtime = AdapterExecutionRuntime::new();
    let adapter = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));

    let task_id = runtime
        .submit(adapter, AdapterRequest::Refresh(RefreshRequest))
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
async fn failed_execution_is_attributed_with_manager_task_and_action() {
    let runtime = AdapterExecutionRuntime::new();
    let adapter = Arc::new(TestAdapter::new(
        ManagerId::Pip,
        AdapterBehavior::Fails(CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::ParseFailure,
            message: "parse failure".to_string(),
        }),
    ));
    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "ruff".to_string(),
            issued_at: SystemTime::now(),
        },
    });

    let task_id = runtime.submit(adapter, request).await.unwrap();
    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Failed);

    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Failed(error)) => {
            assert_eq!(error.manager, Some(ManagerId::Pip));
            assert_eq!(error.task, Some(TaskType::Search));
            assert_eq!(error.action, Some(ManagerAction::Search));
            assert_eq!(error.kind, CoreErrorKind::ParseFailure);
        }
        other => panic!("expected failed terminal state, got {other:?}"),
    }
}

#[tokio::test]
async fn immediate_cancel_reports_cancelled_terminal_state() {
    let runtime = AdapterExecutionRuntime::new();
    let adapter = Arc::new(TestAdapter::new(
        ManagerId::HomebrewFormula,
        AdapterBehavior::SleepsThenSucceeds(Duration::from_millis(300), AdapterResponse::Refreshed),
    ));

    let task_id = runtime
        .submit(adapter, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(20)).await;
    runtime
        .cancel(task_id, CancellationMode::Immediate)
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Cancelled);
    assert!(matches!(
        snapshot.terminal_state,
        Some(AdapterTaskTerminalState::Cancelled(_))
    ));
}
