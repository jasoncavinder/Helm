use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::mise::MiseAdapter;
use helm_core::adapters::mise_process::ProcessMiseSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{CoreErrorKind, ManagerId, PackageRef, SearchQuery, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const INSTALLED_FIXTURE: &str = include_str!("fixtures/mise/ls_json.txt");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/mise/outdated_json.txt");
const REMOTE_FIXTURE: &str = include_str!("fixtures/mise/ls_remote_all_json.txt");
const REGISTRY_FIXTURE: &str = r#"
[
  {"short":"python","description":"python language","aliases":["python3"]},
  {"short":"java","description":"jdk java"},
  {"short":"zig","description":"zig compiler"}
]
"#;
const TIMEOUT_SENSITIVE_SOAK_ITERATIONS: usize = 20;
const TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET: usize = 0;

fn installed_fixture_for_runtime_home() -> String {
    let runtime_home = std::env::var("HOME").unwrap_or_else(|_| "/Users/dev".to_string());
    INSTALLED_FIXTURE.replace("/Users/dev", runtime_home.as_str())
}

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
            match args.as_slice() {
                [command] if command == "--version" => b"mise 2026.2.6 macos-x64\n".to_vec(),
                [command, flag] if command == "ls" && flag == "--json" => {
                    installed_fixture_for_runtime_home().into_bytes()
                }
                [command, flag] if command == "outdated" && flag == "--json" => {
                    OUTDATED_FIXTURE.as_bytes().to_vec()
                }
                [command, flag1, flag2]
                    if command == "ls-remote" && flag1 == "--all" && flag2 == "--json" =>
                {
                    REMOTE_FIXTURE.as_bytes().to_vec()
                }
                [command, flag] if command == "registry" && flag == "--json" => {
                    REGISTRY_FIXTURE.as_bytes().to_vec()
                }
                [command, scope, target] if command == "use" && scope == "--global" => {
                    format!("using {target}\n").into_bytes()
                }
                [command, yes, target] if command == "uninstall" && yes == "--yes" => {
                    format!("uninstalled {target}\n").into_bytes()
                }
                [command] if command == "upgrade" => b"upgraded all\n".to_vec(),
                [command, target] if command == "upgrade" => {
                    format!("upgraded {target}\n").into_bytes()
                }
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
            let go = packages
                .iter()
                .find(|package| package.package.name == "go")
                .expect("go package should exist");
            assert!(go.runtime_state.is_active);
            assert!(go.runtime_state.has_override);
            assert!(!go.runtime_state.is_default);

            let python_current = packages
                .iter()
                .find(|package| {
                    package.package.name == "python"
                        && package.installed_version.as_deref() == Some("3.12.3")
                })
                .expect("current python package should exist");
            assert!(python_current.runtime_state.is_active);
            assert!(python_current.runtime_state.is_default);
            assert!(!python_current.runtime_state.has_override);
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
            let node = packages
                .iter()
                .find(|package| package.package.name == "node")
                .expect("node outdated package should exist");
            assert!(node.runtime_state.is_active);
            assert!(node.runtime_state.has_override);
            assert!(!node.runtime_state.is_default);

            let python = packages
                .iter()
                .find(|package| package.package.name == "python")
                .expect("python outdated package should exist");
            assert!(python.runtime_state.is_active);
            assert!(python.runtime_state.is_default);
            assert!(!python.runtime_state.has_override);
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn search_mise_remote_catalog_through_full_orchestration_path() {
    let executor = Arc::new(MiseFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Mise,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "zig".to_string(),
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
            assert!(!results.is_empty());
            assert!(
                results
                    .iter()
                    .all(|result| result.result.package.name == "zig")
            );
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }
}

#[tokio::test]
async fn install_mise_tool_through_full_orchestration_path() {
    let executor = Arc::new(MiseFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Mise,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Mise,
                    name: "java@zulu-jre-javafx".to_string(),
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
async fn uninstall_mise_tool_through_full_orchestration_path() {
    let executor = Arc::new(MiseFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Mise,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Mise,
                    name: "python@3.12.3".to_string(),
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
}

#[tokio::test]
async fn upgrade_mise_tool_through_full_orchestration_path() {
    let executor = Arc::new(MiseFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::Mise,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Mise,
                    name: "python@3.12.3".to_string(),
                }),
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

#[tokio::test]
async fn mise_timeout_sensitive_orchestration_soak_budget() {
    let mut failures = 0usize;

    for _ in 0..TIMEOUT_SENSITIVE_SOAK_ITERATIONS {
        let executor = Arc::new(MiseFakeExecutor::normal());
        let runtime = build_runtime(executor);

        let detect_id = runtime
            .submit(ManagerId::Mise, AdapterRequest::Detect(DetectRequest))
            .await
            .expect("detect submit should succeed");
        let installed_id = runtime
            .submit(
                ManagerId::Mise,
                AdapterRequest::ListInstalled(ListInstalledRequest),
            )
            .await
            .expect("list installed submit should succeed");
        let outdated_id = runtime
            .submit(
                ManagerId::Mise,
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
        "mise soak exceeded failure budget: failures={} budget={} iterations={}",
        failures,
        TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET,
        TIMEOUT_SENSITIVE_SOAK_ITERATIONS
    );
}
