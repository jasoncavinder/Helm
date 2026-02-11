use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, HomebrewAdapter, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, ProcessHomebrewSource, SearchRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{CoreErrorKind, ManagerId, SearchQuery, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const INSTALLED_FIXTURE: &str = include_str!("fixtures/homebrew/list_installed_versions.txt");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/homebrew/list_outdated_verbose.txt");
const SEARCH_FIXTURE: &str = include_str!("fixtures/homebrew/search_local.txt");

struct RoutingFakeExecutor {
    fail_all: bool,
}

impl RoutingFakeExecutor {
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

impl ProcessExecutor for RoutingFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();

        if self.fail_all {
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(1),
                    stdout: Vec::new(),
                    stderr: b"command failed".to_vec(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }

        let program = request.command.program.to_string_lossy().to_string();
        let args = &request.command.args;

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/opt/homebrew/bin/brew".to_vec()
        } else if program == "brew" || program.ends_with("/brew") {
            match args.first().map(String::as_str) {
                Some("--version") => b"Homebrew 4.2.21\n".to_vec(),
                Some("list") => INSTALLED_FIXTURE.as_bytes().to_vec(),
                Some("outdated") => OUTDATED_FIXTURE.as_bytes().to_vec(),
                Some("search") => SEARCH_FIXTURE.as_bytes().to_vec(),
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
    let source = ProcessHomebrewSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(HomebrewAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn detect_through_full_orchestration_path() {
    let executor = Arc::new(RoutingFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::HomebrewFormula,
            AdapterRequest::Detect(DetectRequest),
        )
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
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/opt/homebrew/bin/brew"))
            );
            assert_eq!(info.version, Some("4.2.21".to_string()));
        }
        other => panic!("expected Detection response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_installed_through_full_orchestration_path() {
    let executor = Arc::new(RoutingFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::HomebrewFormula,
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
            assert_eq!(packages.len(), 4);
            assert_eq!(packages[0].package.name, "python@3.12");
            assert_eq!(packages[0].installed_version.as_deref(), Some("3.12.3"));
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_outdated_through_full_orchestration_path() {
    let executor = Arc::new(RoutingFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::HomebrewFormula,
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
            assert_eq!(packages.len(), 3);
            assert_eq!(packages[0].package.name, "git");
            assert_eq!(packages[0].installed_version.as_deref(), Some("2.44.0"));
            assert_eq!(packages[0].candidate_version, "2.45.1");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn search_through_full_orchestration_path() {
    let executor = Arc::new(RoutingFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::HomebrewFormula,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "rip".to_string(),
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
            assert_eq!(results.len(), 3);
            assert_eq!(results[0].result.package.name, "ripgrep");
            assert_eq!(results[1].result.package.name, "ripgrep-all");
            assert_eq!(results[2].result.package.name, "ripsecret");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }
}

#[tokio::test]
async fn process_failure_propagates_as_structured_error() {
    let executor = Arc::new(RoutingFakeExecutor::failing());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::HomebrewFormula,
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
            assert_eq!(error.manager, Some(ManagerId::HomebrewFormula));
        }
        other => panic!("expected Failed terminal state, got {other:?}"),
    }
}
