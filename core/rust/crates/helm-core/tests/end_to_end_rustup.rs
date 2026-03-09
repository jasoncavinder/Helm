use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::rustup::RustupAdapter;
use helm_core::adapters::rustup_process::ProcessRustupSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{CoreErrorKind, ManagerId, PackageRef, SearchQuery, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const TOOLCHAIN_LIST_FIXTURE: &str = include_str!("fixtures/rustup/toolchain_list.txt");
const CHECK_FIXTURE: &str = include_str!("fixtures/rustup/check.txt");
const TIMEOUT_SENSITIVE_SOAK_ITERATIONS: usize = 20;
const TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET: usize = 0;

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
            match args.as_slice() {
                [command] if command == "--version" => {
                    b"rustup 1.28.2 (54dd3d00f 2024-04-24)\n".to_vec()
                }
                [command] if command == "show" => {
                    b"Default host: x86_64-apple-darwin\nrustup home: /Users/dev/.rustup\n".to_vec()
                }
                [command, subcommand] if command == "toolchain" && subcommand == "list" => {
                    TOOLCHAIN_LIST_FIXTURE.as_bytes().to_vec()
                }
                [command, subcommand, toolchain]
                    if command == "toolchain" && subcommand == "install" =>
                {
                    format!("installed {toolchain}\n").into_bytes()
                }
                [command, subcommand, toolchain]
                    if command == "toolchain" && subcommand == "uninstall" =>
                {
                    format!("uninstalled {toolchain}\n").into_bytes()
                }
                [command, toolchain, binary, flag]
                    if command == "run" && binary == "rustc" && flag == "--version" =>
                {
                    match toolchain.as_str() {
                        "stable-x86_64-apple-darwin" => {
                            b"rustc 1.82.0 (abc123 2025-01-01)\n".to_vec()
                        }
                        "nightly-x86_64-apple-darwin" => {
                            b"rustc 1.86.0-nightly (abc1234 2025-01-15)\n".to_vec()
                        }
                        "1.75.0-x86_64-apple-darwin" => {
                            b"rustc 1.75.0 (def456 2024-01-01)\n".to_vec()
                        }
                        _ => Vec::new(),
                    }
                }
                [command] if command == "check" => CHECK_FIXTURE.as_bytes().to_vec(),
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
            assert_eq!(packages[0].installed_version.as_deref(), Some("1.82.0"));
            assert!(packages[0].runtime_state.is_active);
            assert!(packages[0].runtime_state.is_default);
            assert!(!packages[0].runtime_state.has_override);
            assert_eq!(
                packages[1].installed_version.as_deref(),
                Some("1.86.0-nightly")
            );
            assert!(packages[1].runtime_state.is_empty());
            assert_eq!(packages[2].installed_version.as_deref(), Some("1.75.0"));
            assert!(packages[2].runtime_state.is_empty());
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
            assert!(packages[0].runtime_state.is_active);
            assert!(packages[0].runtime_state.is_default);
            assert!(!packages[0].runtime_state.has_override);
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn search_toolchains_through_full_orchestration_path() {
    let executor = Arc::new(RustupFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Rustup,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "1.92.0".to_string(),
                    issued_at: SystemTime::now(),
                },
            }),
        )
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::SearchResults(results))) => {
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].result.package.name, "1.92.0-x86_64-apple-darwin");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }
}

#[tokio::test]
async fn install_toolchain_through_full_orchestration_path() {
    let executor = Arc::new(RustupFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Rustup,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Rustup,
                    name: "stable-x86_64-apple-darwin".to_string(),
                },
                version: None,
            }),
        )
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);
}

#[tokio::test]
async fn uninstall_toolchain_through_full_orchestration_path() {
    let executor = Arc::new(RustupFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Rustup,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Rustup,
                    name: "stable-x86_64-apple-darwin".to_string(),
                },
                version: None,
            }),
        )
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);
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

#[tokio::test]
async fn rustup_timeout_sensitive_orchestration_soak_budget() {
    let mut failures = 0usize;

    for _ in 0..TIMEOUT_SENSITIVE_SOAK_ITERATIONS {
        let executor = Arc::new(RustupFakeExecutor::normal());
        let runtime = build_runtime(executor);

        let detect_id = runtime
            .submit(ManagerId::Rustup, AdapterRequest::Detect(DetectRequest))
            .await
            .expect("detect submit should succeed");
        let installed_id = runtime
            .submit(
                ManagerId::Rustup,
                AdapterRequest::ListInstalled(ListInstalledRequest),
            )
            .await
            .expect("list installed submit should succeed");
        let outdated_id = runtime
            .submit(
                ManagerId::Rustup,
                AdapterRequest::ListOutdated(ListOutdatedRequest),
            )
            .await
            .expect("list outdated submit should succeed");

        for task_id in [detect_id, installed_id, outdated_id] {
            let snapshot = runtime
                .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
                .await
                .expect("task should reach terminal state");
            if snapshot.runtime.status != TaskStatus::Completed {
                failures += 1;
            }
        }
    }

    assert!(
        failures <= TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET,
        "rustup soak exceeded failure budget: failures={} budget={} iterations={}",
        failures,
        TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET,
        TIMEOUT_SENSITIVE_SOAK_ITERATIONS
    );
}
