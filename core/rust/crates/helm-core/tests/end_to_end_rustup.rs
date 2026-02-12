use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::rustup::RustupAdapter;
use helm_core::adapters::rustup_process::ProcessRustupSource;
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

const TOOLCHAIN_LIST_FIXTURE: &str = include_str!("fixtures/rustup/toolchain_list.txt");
const CHECK_FIXTURE: &str = include_str!("fixtures/rustup/check.txt");

struct RustupFakeExecutor {
    fail_all: bool,
}

impl RustupFakeExecutor {
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

impl ProcessExecutor for RustupFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();

        if self.fail_all {
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(127),
                    stdout: Vec::new(),
                    stderr: b"rustup: command not found".to_vec(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }

        let program = request.command.program.to_string_lossy().to_string();
        let args = &request.command.args;

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/Users/dev/.cargo/bin/rustup".to_vec()
        } else if program == "rustup" || program.ends_with("/rustup") {
            match args.first().map(String::as_str) {
                Some("--version") => b"rustup 1.28.2 (54dd3d00f 2024-04-24)\n".to_vec(),
                Some("toolchain") => TOOLCHAIN_LIST_FIXTURE.as_bytes().to_vec(),
                Some("check") => CHECK_FIXTURE.as_bytes().to_vec(),
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
    let source = ProcessRustupSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(RustupAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn detect_rustup_through_full_orchestration_path() {
    let executor = Arc::new(RustupFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(ManagerId::Rustup, AdapterRequest::Detect(DetectRequest))
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
            assert_eq!(info.version, Some("1.28.2".to_string()));
        }
        other => panic!("expected Detection response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_installed_toolchains_through_full_orchestration_path() {
    let executor = Arc::new(RustupFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Rustup,
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
            assert_eq!(packages.len(), 3); // stable, nightly, 1.75.0
            assert!(
                packages
                    .iter()
                    .all(|p| p.package.manager == ManagerId::Rustup)
            );
            assert_eq!(packages[0].package.name, "stable-x86_64-apple-darwin");
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_outdated_toolchains_through_full_orchestration_path() {
    let executor = Arc::new(RustupFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Rustup,
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
            assert_eq!(packages.len(), 1); // stable only
            assert_eq!(packages[0].package.name, "stable-x86_64-apple-darwin");
            assert_eq!(packages[0].installed_version.as_deref(), Some("1.82.0"));
            assert_eq!(packages[0].candidate_version, "1.93.0");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn rustup_not_installed_propagates_as_structured_error() {
    let executor = Arc::new(RustupFakeExecutor::failing());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Rustup,
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
            assert_eq!(error.manager, Some(ManagerId::Rustup));
        }
        other => panic!("expected Failed terminal state, got {other:?}"),
    }
}
