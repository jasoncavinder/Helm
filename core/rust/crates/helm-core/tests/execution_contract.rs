use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use helm_core::execution::{
    CommandSpec, ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput,
    ProcessSpawnRequest, ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
    spawn_validated,
};
use helm_core::models::{CoreErrorKind, ManagerAction, ManagerId, TaskType};

#[derive(Clone)]
struct FakeExecutor {
    captured: Arc<Mutex<Option<ProcessSpawnRequest>>>,
    output: ProcessOutput,
    terminate_calls: Arc<Mutex<Vec<ProcessTerminationMode>>>,
}

impl FakeExecutor {
    fn new(output: ProcessOutput) -> Self {
        Self {
            captured: Arc::new(Mutex::new(None)),
            output,
            terminate_calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn captured_request(&self) -> Option<ProcessSpawnRequest> {
        self.captured.lock().ok()?.clone()
    }

    fn terminate_count(&self) -> usize {
        self.terminate_calls
            .lock()
            .map(|calls| calls.len())
            .unwrap_or(0)
    }
}

struct FakeProcess {
    pid: Option<u32>,
    output: ProcessOutput,
    terminate_calls: Arc<Mutex<Vec<ProcessTerminationMode>>>,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        self.pid
    }

    fn terminate(&self, mode: ProcessTerminationMode) -> ExecutionResult<()> {
        let mut calls = self
            .terminate_calls
            .lock()
            .map_err(|_| helm_core::models::CoreError {
                manager: None,
                task: None,
                action: None,
                kind: CoreErrorKind::Internal,
                message: "terminate lock poisoned".to_string(),
            })?;
        calls.push(mode);
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output.clone();
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for FakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let mut captured = self
            .captured
            .lock()
            .map_err(|_| helm_core::models::CoreError {
                manager: None,
                task: None,
                action: None,
                kind: CoreErrorKind::Internal,
                message: "capture lock poisoned".to_string(),
            })?;
        *captured = Some(request);

        Ok(Box::new(FakeProcess {
            pid: Some(4242),
            output: self.output.clone(),
            terminate_calls: self.terminate_calls.clone(),
        }))
    }
}

#[test]
fn request_validation_rejects_empty_program() {
    let request = ProcessSpawnRequest::new(
        ManagerId::HomebrewFormula,
        TaskType::Refresh,
        ManagerAction::Refresh,
        CommandSpec::new(""),
    );

    let error = request.validate().expect_err("expected validation failure");
    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::HomebrewFormula));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
}

#[test]
fn request_validation_rejects_zero_timeout() {
    let request = ProcessSpawnRequest::new(
        ManagerId::Npm,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new("npm").args(["search", "ripgrep"]),
    )
    .timeout(Duration::ZERO);

    let error = request
        .validate()
        .expect_err("expected timeout validation failure");
    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::Npm));
    assert_eq!(error.task, Some(TaskType::Search));
    assert_eq!(error.action, Some(ManagerAction::Search));
}

#[tokio::test]
async fn validated_spawn_uses_structured_args_and_supports_termination() {
    let now = SystemTime::now();
    let output = ProcessOutput {
        status: ProcessExitStatus::ExitCode(0),
        stdout: b"ok".to_vec(),
        stderr: Vec::new(),
        started_at: now,
        finished_at: now,
    };
    let executor = FakeExecutor::new(output.clone());

    let request = ProcessSpawnRequest::new(
        ManagerId::Pip,
        TaskType::Refresh,
        ManagerAction::Refresh,
        CommandSpec::new("python3")
            .args(["-m", "pip", "list", "--outdated"])
            .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
            .working_dir(PathBuf::from("/tmp")),
    )
    .requires_elevation(false)
    .timeout(Duration::from_secs(10));

    let handle = spawn_validated(&executor, request).expect("validated spawn should succeed");
    assert_eq!(handle.pid(), Some(4242));

    handle
        .terminate(ProcessTerminationMode::Graceful {
            grace_period: Duration::from_secs(2),
        })
        .expect("terminate should succeed");

    let result = handle.wait().await.expect("wait should succeed");
    assert_eq!(result, output);

    let captured = executor
        .captured_request()
        .expect("executor should capture spawn request");
    assert_eq!(captured.command.program, PathBuf::from("python3"));
    assert_eq!(
        captured.command.args,
        vec!["-m", "pip", "list", "--outdated"]
    );
    assert_eq!(
        captured.command.env.get("PIP_DISABLE_PIP_VERSION_CHECK"),
        Some(&"1".to_string())
    );
    assert_eq!(captured.command.working_dir, Some(PathBuf::from("/tmp")));
    assert_eq!(executor.terminate_count(), 1);
}
