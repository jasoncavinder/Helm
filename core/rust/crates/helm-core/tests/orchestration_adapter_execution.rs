use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    BlocksUntilReleased {
        started: Arc<AtomicBool>,
        released: Arc<AtomicBool>,
        response: AdapterResponse,
    },
    SignalsThenSucceeds(Arc<AtomicBool>, AdapterResponse),
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
            AdapterBehavior::BlocksUntilReleased {
                started,
                released,
                response,
            } => {
                started.store(true, Ordering::SeqCst);
                while !released.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Ok(response.clone())
            }
            AdapterBehavior::SignalsThenSucceeds(started, response) => {
                started.store(true, Ordering::SeqCst);
                Ok(response.clone())
            }
            AdapterBehavior::Panics => panic!("simulated adapter panic"),
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

#[tokio::test]
async fn cancelled_homebrew_task_holds_execution_lock_until_adapter_exits() {
    let formula_runtime = AdapterExecutionRuntime::new();
    let cask_runtime = AdapterExecutionRuntime::new();
    let formula_started = Arc::new(AtomicBool::new(false));
    let release_formula = Arc::new(AtomicBool::new(false));
    let cask_started = Arc::new(AtomicBool::new(false));

    let formula_task = formula_runtime
        .submit(
            Arc::new(TestAdapter::new(
                ManagerId::HomebrewFormula,
                AdapterBehavior::BlocksUntilReleased {
                    started: formula_started.clone(),
                    released: release_formula.clone(),
                    response: AdapterResponse::Refreshed,
                },
            )),
            AdapterRequest::Refresh(RefreshRequest),
        )
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(1), async {
        while !formula_started.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("formula adapter should start");

    formula_runtime
        .cancel(formula_task, CancellationMode::Immediate)
        .await
        .unwrap();

    let cask_task = cask_runtime
        .submit(
            Arc::new(TestAdapter::new(
                ManagerId::HomebrewCask,
                AdapterBehavior::SignalsThenSucceeds(
                    cask_started.clone(),
                    AdapterResponse::Refreshed,
                ),
            )),
            AdapterRequest::Refresh(RefreshRequest),
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(!cask_started.load(Ordering::SeqCst));
    assert_eq!(
        cask_runtime.status(cask_task).await.unwrap(),
        TaskStatus::Queued
    );

    release_formula.store(true, Ordering::SeqCst);
    let cask_snapshot = cask_runtime
        .wait_for_terminal(cask_task, Some(Duration::from_secs(1)))
        .await
        .expect("cask task should complete after formula adapter exits");
    assert!(cask_started.load(Ordering::SeqCst));
    assert_eq!(cask_snapshot.runtime.status, TaskStatus::Completed);
    assert_eq!(
        formula_runtime.status(formula_task).await.unwrap(),
        TaskStatus::Cancelled
    );
}

#[tokio::test]
async fn graceful_cancel_allows_near_complete_adapter_execution() {
    let runtime = AdapterExecutionRuntime::new();
    let adapter = Arc::new(TestAdapter::new(
        ManagerId::Pnpm,
        AdapterBehavior::SleepsThenSucceeds(Duration::from_millis(40), AdapterResponse::Refreshed),
    ));

    let task_id = runtime
        .submit(adapter, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;
    runtime
        .cancel(
            task_id,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(250),
            },
        )
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
async fn graceful_cancel_times_out_and_cancels_adapter_execution() {
    let runtime = AdapterExecutionRuntime::new();
    let adapter = Arc::new(TestAdapter::new(
        ManagerId::Yarn,
        AdapterBehavior::SleepsThenSucceeds(Duration::from_millis(400), AdapterResponse::Refreshed),
    ));

    let task_id = runtime
        .submit(adapter, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;
    runtime
        .cancel(
            task_id,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(20),
            },
        )
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

#[tokio::test]
async fn adapter_panic_is_reported_as_failed_terminal_state() {
    let runtime = AdapterExecutionRuntime::new();
    let adapter = Arc::new(TestAdapter::new(ManagerId::Pip, AdapterBehavior::Panics));

    let task_id = runtime
        .submit(adapter, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Failed);
    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Failed(error)) => {
            assert_eq!(error.manager, Some(ManagerId::Pip));
            assert_eq!(error.task, Some(TaskType::Refresh));
            assert_eq!(error.action, Some(ManagerAction::Refresh));
            assert_eq!(error.kind, CoreErrorKind::Internal);
        }
        other => panic!("expected failed terminal state, got {other:?}"),
    }
}
