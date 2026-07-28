use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter, RefreshRequest, SearchRequest,
};
use helm_core::execution::{
    CommandSpec, ExecutionResult, ProcessCancellation, ProcessExecutor, ProcessExitStatus,
    ProcessOutput, ProcessSpawnRequest, ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
    spawn_validated,
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

struct ProcessAdapter {
    descriptor: ManagerDescriptor,
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessAdapter {
    fn new(manager: ManagerId, executor: Arc<dyn ProcessExecutor>) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id: manager,
                display_name: "process-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities: TEST_CAPABILITIES,
            },
            executor,
        }
    }
}

impl ManagerAdapter for ProcessAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, _request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        let request = ProcessSpawnRequest::new(
            self.descriptor.id,
            TaskType::Refresh,
            ManagerAction::Refresh,
            CommandSpec::new("process-adapter-test"),
        );
        let process = spawn_validated(self.executor.as_ref(), request)?;
        let output = tokio::runtime::Handle::current().block_on(process.wait())?;
        match output.status {
            ProcessExitStatus::ExitCode(0) => Ok(AdapterResponse::Refreshed),
            ProcessExitStatus::ExitCode(code) => Err(CoreError {
                manager: Some(self.descriptor.id),
                task: Some(TaskType::Refresh),
                action: Some(ManagerAction::Refresh),
                kind: CoreErrorKind::ProcessFailure,
                message: format!("process exited with code {code}"),
            }),
            ProcessExitStatus::Terminated => Err(CoreError {
                manager: Some(self.descriptor.id),
                task: Some(TaskType::Refresh),
                action: Some(ManagerAction::Refresh),
                kind: CoreErrorKind::ProcessFailure,
                message: "process terminated by cancellation".to_string(),
            }),
        }
    }
}

#[derive(Default)]
struct TrackingProcessControl {
    terminated: AtomicBool,
    reaped: AtomicBool,
}

impl ProcessCancellation for TrackingProcessControl {
    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        self.terminated.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct TrackingProcess {
    control: Arc<TrackingProcessControl>,
}

impl RunningProcess for TrackingProcess {
    fn pid(&self) -> Option<u32> {
        Some(42)
    }

    fn terminate(&self, mode: ProcessTerminationMode) -> ExecutionResult<()> {
        self.control.terminate(mode)
    }

    fn cancellation_handle(&self) -> Option<Arc<dyn ProcessCancellation>> {
        Some(self.control.clone())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let control = self.control;
        Box::pin(async move {
            while !control.terminated.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            control.reaped.store(true, Ordering::SeqCst);
            let now = SystemTime::now();
            Ok(ProcessOutput {
                status: ProcessExitStatus::Terminated,
                stdout: Vec::new(),
                stderr: Vec::new(),
                started_at: now,
                finished_at: now,
            })
        })
    }
}

struct TrackingExecutor {
    control: Arc<TrackingProcessControl>,
    spawned: Arc<AtomicBool>,
}

impl ProcessExecutor for TrackingExecutor {
    fn spawn(&self, _request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        self.spawned.store(true, Ordering::SeqCst);
        Ok(Box::new(TrackingProcess {
            control: self.control.clone(),
        }))
    }
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
        ManagerId::Setapp,
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
        ManagerId::Npm,
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
async fn immediate_cancel_terminates_and_reaps_adapter_process_before_cancellation() {
    let runtime = AdapterExecutionRuntime::new();
    let control = Arc::new(TrackingProcessControl::default());
    let spawned = Arc::new(AtomicBool::new(false));
    let executor = Arc::new(TrackingExecutor {
        control: control.clone(),
        spawned: spawned.clone(),
    });
    let task_id = runtime
        .submit(
            Arc::new(ProcessAdapter::new(ManagerId::DockerDesktop, executor)),
            AdapterRequest::Refresh(RefreshRequest),
        )
        .await
        .expect("task should start");

    tokio::time::timeout(Duration::from_secs(1), async {
        while !spawned.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("adapter process should spawn");
    runtime
        .cancel(task_id, CancellationMode::Immediate)
        .await
        .expect("cancellation should signal the process");
    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .expect("process termination should reach a terminal task state");

    assert_eq!(snapshot.runtime.status, TaskStatus::Cancelled);
    assert!(matches!(
        snapshot.terminal_state,
        Some(AdapterTaskTerminalState::Cancelled(_))
    ));
    assert!(control.terminated.load(Ordering::SeqCst));
    assert!(control.reaped.load(Ordering::SeqCst));
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
