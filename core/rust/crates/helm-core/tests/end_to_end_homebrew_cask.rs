use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, HomebrewCaskAdapter, InstallRequest,
    ListInstalledRequest, ListOutdatedRequest, ManagerAdapter, ProcessHomebrewCaskSource,
    SearchRequest, UninstallRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{CoreErrorKind, ManagerId, PackageRef, SearchQuery, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const INSTALLED_FIXTURE: &str = include_str!("fixtures/homebrew_cask/installed.json");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/homebrew_cask/outdated.json");
const VERSION_FIXTURE: &str = include_str!("fixtures/homebrew_cask/version.txt");
const SEARCH_FIXTURE: &str = "==> Casks\niterm2: (iTerm2) Terminal emulator as alternative to Apple's Terminal app\niterm2@beta: (iTerm2) Terminal emulator as alternative to Apple's Terminal app\n";

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
        Some(9998)
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
            match args.as_slice() {
                [arg] if arg == "--version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [arg] if arg == "casks" => b"iterm2\niterm2@beta\n".to_vec(),
                [arg0, arg1, arg2, arg3]
                    if arg0 == "search" && arg1 == "--cask" && arg2 == "--desc" =>
                {
                    let _ = arg3;
                    SEARCH_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1, arg2, arg3] if arg0 == "info" && arg1 == "--cask" => {
                    let _ = (arg2, arg3);
                    INSTALLED_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1, arg2] if arg0 == "outdated" && arg1 == "--cask" => {
                    let _ = arg2;
                    OUTDATED_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1, _name] if arg0 == "install" && arg1 == "--cask" => Vec::new(),
                [arg0, arg1, _name] if arg0 == "uninstall" && arg1 == "--cask" => Vec::new(),
                [arg0, arg1] if arg0 == "upgrade" && arg1 == "--cask" => Vec::new(),
                [arg0, arg1, _name] if arg0 == "upgrade" && arg1 == "--cask" => Vec::new(),
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
    let source = ProcessHomebrewCaskSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(HomebrewCaskAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn detect_homebrew_cask_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(RoutingFakeExecutor::normal()));

    let task_id = runtime
        .submit(
            ManagerId::HomebrewCask,
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
            assert_eq!(info.version, Some("4.6.17".to_string()));
        }
        other => panic!("expected Detection response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_installed_homebrew_casks_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(RoutingFakeExecutor::normal()));

    let task_id = runtime
        .submit(
            ManagerId::HomebrewCask,
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
            assert_eq!(packages.len(), 2);
            assert_eq!(packages[0].package.name, "google-chrome");
            assert_eq!(
                packages[0].installed_version.as_deref(),
                Some("133.0.6943.142")
            );
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_outdated_homebrew_casks_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(RoutingFakeExecutor::normal()));

    let task_id = runtime
        .submit(
            ManagerId::HomebrewCask,
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
            assert_eq!(packages.len(), 2);
            assert_eq!(packages[0].package.name, "google-chrome");
            assert_eq!(packages[0].candidate_version, "134.0.6998.89");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn search_homebrew_casks_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(RoutingFakeExecutor::normal()));

    let task_id = runtime
        .submit(
            ManagerId::HomebrewCask,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "iterm".to_string(),
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
            assert_eq!(results.len(), 2);
            assert_eq!(results[0].result.package.name, "iterm2");
            assert_eq!(
                results[0].result.summary.as_deref(),
                Some("Terminal emulator as alternative to Apple's Terminal app")
            );
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }
}

#[tokio::test]
async fn install_uninstall_and_upgrade_homebrew_cask_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(RoutingFakeExecutor::normal()));
    let install_package = PackageRef {
        manager: ManagerId::HomebrewCask,
        name: "iterm2".to_string(),
    };

    for request in [
        AdapterRequest::Install(InstallRequest {
            package: install_package.clone(),
            target_name: None,
            version: None,
        }),
        AdapterRequest::Uninstall(UninstallRequest {
            package: install_package.clone(),
            target_name: None,
            version: None,
        }),
        AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(PackageRef {
                manager: ManagerId::HomebrewCask,
                name: "raycast".to_string(),
            }),
            target_name: None,
            version: None,
        }),
    ] {
        let task_id = runtime
            .submit(ManagerId::HomebrewCask, request)
            .await
            .unwrap();
        let snapshot = runtime
            .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
            .await
            .unwrap();
        assert_eq!(snapshot.runtime.status, TaskStatus::Completed);
        assert!(matches!(
            snapshot.terminal_state,
            Some(AdapterTaskTerminalState::Succeeded(
                AdapterResponse::Mutation(_)
            ))
        ));
    }
}

#[tokio::test]
async fn failing_homebrew_cask_command_propagates_as_structured_error() {
    let runtime = build_runtime(Arc::new(RoutingFakeExecutor::failing()));

    let task_id = runtime
        .submit(
            ManagerId::HomebrewCask,
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
            assert!(error.message.contains("command failed"));
        }
        other => panic!("expected failed terminal state, got {other:?}"),
    }
}
