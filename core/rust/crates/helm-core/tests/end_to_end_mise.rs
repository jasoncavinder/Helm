use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::mise::MiseAdapter;
use helm_core::adapters::mise_process::ProcessMiseSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, ListInstalledRequest, ListOutdatedRequest,
    ManagerAdapter,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{CoreErrorKind, ManagerId, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const INSTALLED_FIXTURE: &str = include_str!("fixtures/mise/ls_json.txt");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/mise/outdated_json.txt");

struct MiseFakeExecutor {
    fail_all: bool,
}

impl MiseFakeExecutor {
    fn normal() -> Self {
        Self { fail_all: false }
    }

    fn failing() -> Self {
        Self { fail_all: true }
    }
}

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9999)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for MiseFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();

        if self.fail_all {
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(127),
                    stdout: Vec::new(),
                    stderr: b"mise: command not found".to_vec(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }

        let program = request.command.program.to_string_lossy().to_string();
        let args = &request.command.args;

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/Users/dev/.local/bin/mise".to_vec()
        } else if program == "mise" || program.ends_with("/mise") {
            match args.first().map(String::as_str) {
                Some("--version") => b"mise 2026.2.6 macos-x64\n".to_vec(),
                Some("ls") => INSTALLED_FIXTURE.as_bytes().to_vec(),
                Some("outdated") => OUTDATED_FIXTURE.as_bytes().to_vec(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };

        Ok(Box::new(FakeProcess {
            output: ProcessOutput {
                status: ProcessExitStatus::ExitCode(0),
                stdout,
                stderr: Vec::new(),
                started_at: now,
                finished_at: now,
            },
        }))
    }
}

fn build_runtime(executor: Arc<dyn ProcessExecutor>) -> AdapterRuntime {
    let source = ProcessMiseSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(MiseAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn detect_mise_through_full_orchestration_path() {
    let executor = Arc::new(MiseFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(ManagerId::Mise, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version, Some("2026.2.6".to_string()));
        }
        other => panic!("expected Detection response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_installed_through_full_orchestration_path() {
    let executor = Arc::new(MiseFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Mise,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert_eq!(packages.len(), 4); // go, node, python x2
            assert!(
                packages
                    .iter()
                    .all(|p| p.package.manager == ManagerId::Mise)
            );
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_outdated_through_full_orchestration_path() {
    let executor = Arc::new(MiseFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Mise,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 2); // node, python
            assert!(
                packages
                    .iter()
                    .all(|p| p.package.manager == ManagerId::Mise)
            );
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn mise_not_installed_propagates_as_structured_error() {
    let executor = Arc::new(MiseFakeExecutor::failing());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Mise,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Failed);

    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Failed(error)) => {
            assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
            assert_eq!(error.manager, Some(ManagerId::Mise));
        }
        other => panic!("expected Failed terminal state, got {other:?}"),
    }
}
