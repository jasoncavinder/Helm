use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::adapters::poetry::PoetryAdapter;
use helm_core::adapters::poetry_process::ProcessPoetrySource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{ManagerId, PackageRef, SearchQuery};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const VERSION_FIXTURE: &str = include_str!("fixtures/poetry/version.txt");
const LIST_FIXTURE: &str = include_str!("fixtures/poetry/self_show_plugins.txt");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/poetry/self_show_plugins_outdated.txt");

struct PoetryFakeExecutor {
    export_upgraded: AtomicBool,
}

impl PoetryFakeExecutor {
    fn new() -> Self {
        Self {
            export_upgraded: AtomicBool::new(false),
        }
    }
}

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9903)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for PoetryFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = request.command.args.clone();

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/opt/homebrew/bin/poetry".to_vec()
        } else if program == "poetry" || program.ends_with("/poetry") {
            match args.as_slice() {
                [arg] if arg == "--version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [arg0, arg1, arg2, arg3]
                    if arg0 == "self"
                        && arg1 == "show"
                        && arg2 == "plugins"
                        && arg3 == "--no-ansi" =>
                {
                    LIST_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1, arg2, arg3, arg4]
                    if arg0 == "self"
                        && arg1 == "show"
                        && arg2 == "plugins"
                        && arg3 == "--outdated"
                        && arg4 == "--no-ansi" =>
                {
                    if self.export_upgraded.load(Ordering::SeqCst) {
                        Vec::new()
                    } else {
                        OUTDATED_FIXTURE.as_bytes().to_vec()
                    }
                }
                [arg0, arg1, spec]
                    if arg0 == "self" && arg1 == "add" && spec == "poetry-plugin-export@1.9.0" =>
                {
                    Vec::new()
                }
                [arg0, arg1, name]
                    if arg0 == "self" && arg1 == "remove" && name == "poetry-plugin-export" =>
                {
                    Vec::new()
                }
                [arg0, arg1, name]
                    if arg0 == "self" && arg1 == "update" && name == "poetry-plugin-export" =>
                {
                    self.export_upgraded.store(true, Ordering::SeqCst);
                    Vec::new()
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
    let source = ProcessPoetrySource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(PoetryAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn poetry_detect_list_search_and_mutate_through_orchestration() {
    let runtime = build_runtime(Arc::new(PoetryFakeExecutor::new()));

    let detect_task = runtime
        .submit(ManagerId::Poetry, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("2.1.2"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/opt/homebrew/bin/poetry"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Poetry,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let installed_snapshot = runtime
        .wait_for_terminal(installed_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match installed_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert_eq!(packages.len(), 2);
            assert_eq!(packages[0].package.name, "poetry-plugin-bundle");
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Poetry,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();
    let outdated_snapshot = runtime
        .wait_for_terminal(outdated_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match outdated_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].package.name, "poetry-plugin-export");
            assert_eq!(packages[0].candidate_version, "1.9.0");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }

    let search_task = runtime
        .submit(
            ManagerId::Poetry,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "export".to_string(),
                    issued_at: SystemTime::now(),
                },
            }),
        )
        .await
        .unwrap();
    let search_snapshot = runtime
        .wait_for_terminal(search_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match search_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::SearchResults(results))) => {
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].result.package.name, "poetry-plugin-export");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::Poetry,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Poetry,
                    name: "poetry-plugin-export".to_string(),
                },
                target_name: None,
                version: Some("1.9.0".to_string()),
            }),
        )
        .await
        .unwrap();
    let install_snapshot = runtime
        .wait_for_terminal(install_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match install_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "poetry-plugin-export");
            assert_eq!(mutation.before_version.as_deref(), Some("1.8.0"));
            assert_eq!(mutation.after_version.as_deref(), Some("1.9.0"));
        }
        other => panic!("expected install mutation, got {other:?}"),
    }

    let uninstall_task = runtime
        .submit(
            ManagerId::Poetry,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Poetry,
                    name: "poetry-plugin-export".to_string(),
                },
                target_name: None,
                version: None,
            }),
        )
        .await
        .unwrap();
    let uninstall_snapshot = runtime
        .wait_for_terminal(uninstall_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match uninstall_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "poetry-plugin-export");
            assert_eq!(mutation.before_version.as_deref(), Some("1.8.0"));
        }
        other => panic!("expected uninstall mutation, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::Poetry,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Poetry,
                    name: "poetry-plugin-export".to_string(),
                }),
                target_name: None,
                version: None,
            }),
        )
        .await
        .unwrap();
    let upgrade_snapshot = runtime
        .wait_for_terminal(upgrade_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match upgrade_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "poetry-plugin-export");
            assert_eq!(mutation.before_version.as_deref(), Some("1.8.0"));
            assert_eq!(mutation.after_version.as_deref(), Some("1.9.0"));
        }
        other => panic!("expected upgrade mutation, got {other:?}"),
    }
}
