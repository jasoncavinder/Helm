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

const VERSION_FIXTURE: &str = "1.17.6\n";
const INSTALLED_FIXTURE: &str = include_str!("fixtures/pipx/list_global.json");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/pipx/list_outdated.json");

struct PipxFakeExecutor {
    httpie_upgraded: AtomicBool,
    httpie_removed: AtomicBool,
    poetry_installed: AtomicBool,
}

impl PipxFakeExecutor {
    fn new() -> Self {
        Self {
            httpie_upgraded: AtomicBool::new(false),
            httpie_removed: AtomicBool::new(false),
            poetry_installed: AtomicBool::new(false),
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
                [arg0, arg1, arg2]
                    if arg0 == "list" && arg1 == "--outdated" && arg2 == "--json" =>
                {
                    if self.httpie_upgraded.load(Ordering::SeqCst) {
                        br#"{"venvs":{}}"#.to_vec()
                    } else {
                        OUTDATED_FIXTURE.as_bytes().to_vec()
                    }
                }
                [arg0, arg1] if arg0 == "list" && arg1 == "--json" => {
                    let mut inventory: serde_json::Value =
                        serde_json::from_str(INSTALLED_FIXTURE).unwrap();
                    let venvs = inventory["venvs"].as_object_mut().unwrap();
                    if self.httpie_removed.load(Ordering::SeqCst) {
                        venvs.remove("httpie");
                    } else if self.httpie_upgraded.load(Ordering::SeqCst) {
                        venvs.get_mut("httpie").unwrap()["metadata"]["main_package"]["package_version"] =
                            serde_json::json!("3.2.4");
                    }
                    if self.poetry_installed.load(Ordering::SeqCst) {
                        venvs.insert("poetry".into(), serde_json::json!({"metadata":{"main_package":{"package":"poetry","package_version":"1.8.4"}}}));
                    }
                    serde_json::to_vec(&inventory).unwrap()
                }
                [arg0, spec] if arg0 == "install" && spec == "poetry==1.8.4" => {
                    self.poetry_installed.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                [arg0, name] if arg0 == "uninstall" && name == "httpie" => {
                    self.httpie_removed.store(true, Ordering::SeqCst);
                    Vec::new()
                }
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
            assert_eq!(info.version.as_deref(), Some("1.17.6"));
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
