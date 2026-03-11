use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::adapters::pip::PipAdapter;
use helm_core::adapters::pip_process::ProcessPipSource;
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

const VERSION_FIXTURE: &str = include_str!("fixtures/pip/version.txt");
const INSTALLED_FIXTURE: &str = include_str!("fixtures/pip/list.json");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/pip/outdated.json");

struct PipFakeExecutor {
    black_upgraded: AtomicBool,
}

impl PipFakeExecutor {
    fn new() -> Self {
        Self {
            black_upgraded: AtomicBool::new(false),
        }
    }
}

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9912)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for PipFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = request.command.args.clone();

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/opt/homebrew/bin/python3".to_vec()
        } else if program.ends_with("python3") {
            match args.as_slice() {
                [arg0, arg1, arg2] if arg0 == "-m" && arg1 == "pip" && arg2 == "--version" => {
                    VERSION_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1, arg2, arg3, arg4]
                    if arg0 == "-m"
                        && arg1 == "pip"
                        && arg2 == "list"
                        && arg3 == "--format=json"
                        && arg4 == "--disable-pip-version-check" =>
                {
                    INSTALLED_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1, arg2, arg3, arg4, arg5]
                    if arg0 == "-m"
                        && arg1 == "pip"
                        && arg2 == "list"
                        && arg3 == "--outdated"
                        && arg4 == "--format=json"
                        && arg5 == "--disable-pip-version-check" =>
                {
                    if self.black_upgraded.load(Ordering::SeqCst) {
                        br#"[
  {
    "name": "requests",
    "version": "2.32.2",
    "latest_version": "2.32.3",
    "latest_filetype": "wheel"
  }
]
"#
                        .to_vec()
                    } else {
                        OUTDATED_FIXTURE.as_bytes().to_vec()
                    }
                }
                [arg0, arg1, arg2, arg3, spec]
                    if arg0 == "-m"
                        && arg1 == "pip"
                        && arg2 == "install"
                        && arg3 == "--disable-pip-version-check"
                        && spec == "httpx==0.28.1" =>
                {
                    Vec::new()
                }
                [arg0, arg1, arg2, arg3, arg4, name]
                    if arg0 == "-m"
                        && arg1 == "pip"
                        && arg2 == "uninstall"
                        && arg3 == "-y"
                        && arg4 == "--disable-pip-version-check"
                        && name == "black" =>
                {
                    Vec::new()
                }
                [arg0, arg1, arg2, arg3, arg4, name]
                    if arg0 == "-m"
                        && arg1 == "pip"
                        && arg2 == "install"
                        && arg3 == "--upgrade"
                        && arg4 == "--disable-pip-version-check"
                        && name == "black" =>
                {
                    self.black_upgraded.store(true, Ordering::SeqCst);
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
    let source = ProcessPipSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(PipAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn pip_detect_list_search_and_mutate_through_orchestration() {
    let runtime = build_runtime(Arc::new(PipFakeExecutor::new()));

    let detect_task = runtime
        .submit(ManagerId::Pip, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("24.3.1"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/opt/homebrew/bin/python3"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Pip,
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
            assert_eq!(packages.len(), 3);
            assert!(packages.iter().any(|package| {
                package.package.name == "black"
                    && package.installed_version.as_deref() == Some("24.8.0")
            }));
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Pip,
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
            assert_eq!(packages.len(), 2);
            assert!(packages.iter().any(|package| {
                package.package.name == "black" && package.candidate_version == "24.10.0"
            }));
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }

    let search_task = runtime
        .submit(
            ManagerId::Pip,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "black".to_string(),
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
            assert_eq!(results[0].result.package.name, "black");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::Pip,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pip,
                    name: "httpx".to_string(),
                },
                target_name: None,
                version: Some("0.28.1".to_string()),
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
            assert_eq!(mutation.package.name, "httpx");
            assert_eq!(mutation.before_version, None);
            assert_eq!(mutation.after_version.as_deref(), Some("0.28.1"));
        }
        other => panic!("expected install mutation, got {other:?}"),
    }

    let uninstall_task = runtime
        .submit(
            ManagerId::Pip,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pip,
                    name: "black".to_string(),
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
            assert_eq!(mutation.package.name, "black");
            assert_eq!(mutation.before_version.as_deref(), Some("24.8.0"));
            assert_eq!(mutation.after_version, None);
        }
        other => panic!("expected uninstall mutation, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::Pip,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pip,
                    name: "black".to_string(),
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
            assert_eq!(mutation.package.name, "black");
            assert_eq!(mutation.before_version.as_deref(), Some("24.8.0"));
            assert_eq!(mutation.after_version.as_deref(), Some("24.10.0"));
        }
        other => panic!("expected upgrade mutation, got {other:?}"),
    }
}
