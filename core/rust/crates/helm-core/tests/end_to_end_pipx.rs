use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::adapters::pipx::PipxAdapter;
use helm_core::adapters::pipx_process::ProcessPipxSource;
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

const VERSION_FIXTURE: &str = include_str!("fixtures/pipx/version.txt");
const INSTALLED_FIXTURE: &str = include_str!("fixtures/pipx/list_global.json");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/pipx/list_outdated.json");

struct PipxFakeExecutor {
    httpie_upgraded: AtomicBool,
}

impl PipxFakeExecutor {
    fn new() -> Self {
        Self {
            httpie_upgraded: AtomicBool::new(false),
        }
    }
}

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9911)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for PipxFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = request.command.args.clone();

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/Users/test/.local/bin/pipx".to_vec()
        } else if program == "pipx" || program.ends_with("/pipx") {
            match args.as_slice() {
                [arg] if arg == "--version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [arg0, arg1] if arg0 == "list" && arg1 == "--json" => {
                    if self.httpie_upgraded.load(Ordering::SeqCst) {
                        br#"{
  "pipx_spec_version": "0.1",
  "venvs": {
    "black": {
      "metadata": {
        "main_package": {
          "package": "black",
          "package_version": "24.10.0",
          "latest_version": "24.10.0"
        }
      }
    },
    "httpie": {
      "metadata": {
        "main_package": {
          "package": "httpie",
          "package_version": "3.2.4",
          "latest_version": "3.2.4"
        }
      }
    }
  }
}"#
                        .to_vec()
                    } else if request.task_type == helm_core::models::TaskType::Refresh
                        && request.action == helm_core::models::ManagerAction::ListOutdated
                    {
                        OUTDATED_FIXTURE.as_bytes().to_vec()
                    } else {
                        INSTALLED_FIXTURE.as_bytes().to_vec()
                    }
                }
                [arg0, spec] if arg0 == "install" && spec == "poetry==1.8.4" => Vec::new(),
                [arg0, name] if arg0 == "uninstall" && name == "httpie" => Vec::new(),
                [arg0, name] if arg0 == "upgrade" && name == "httpie" => {
                    self.httpie_upgraded.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                [arg0] if arg0 == "upgrade-all" => {
                    self.httpie_upgraded.store(true, Ordering::SeqCst);
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
    let source = ProcessPipxSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(PipxAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn pipx_detect_list_search_and_mutate_through_orchestration() {
    let runtime = build_runtime(Arc::new(PipxFakeExecutor::new()));

    let detect_task = runtime
        .submit(ManagerId::Pipx, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("1.7.1"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/Users/test/.local/bin/pipx"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Pipx,
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
            assert_eq!(packages[0].package.name, "black");
            assert_eq!(packages[1].package.name, "httpie");
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Pipx,
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
            assert_eq!(packages[0].package.name, "httpie");
            assert_eq!(packages[0].candidate_version, "3.2.4");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }

    let search_task = runtime
        .submit(
            ManagerId::Pipx,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "httpie".to_string(),
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
            assert_eq!(results[0].result.package.name, "httpie");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::Pipx,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pipx,
                    name: "poetry".to_string(),
                },
                target_name: None,
                version: Some("1.8.4".to_string()),
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
            assert_eq!(mutation.package.name, "poetry");
            assert_eq!(mutation.before_version, None);
            assert_eq!(mutation.after_version.as_deref(), Some("1.8.4"));
        }
        other => panic!("expected install mutation, got {other:?}"),
    }

    let uninstall_task = runtime
        .submit(
            ManagerId::Pipx,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pipx,
                    name: "httpie".to_string(),
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
            assert_eq!(mutation.package.name, "httpie");
            assert_eq!(mutation.before_version.as_deref(), Some("3.2.2"));
            assert_eq!(mutation.after_version, None);
        }
        other => panic!("expected uninstall mutation, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::Pipx,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pipx,
                    name: "httpie".to_string(),
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
            assert_eq!(mutation.package.name, "httpie");
            assert_eq!(mutation.before_version.as_deref(), Some("3.2.2"));
            assert_eq!(mutation.after_version.as_deref(), Some("3.2.4"));
        }
        other => panic!("expected upgrade mutation, got {other:?}"),
    }
}
