use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::adapters::pnpm::PnpmAdapter;
use helm_core::adapters::pnpm_process::ProcessPnpmSource;
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

const VERSION_FIXTURE: &str = include_str!("fixtures/pnpm/version.txt");
const INSTALLED_FIXTURE: &str = include_str!("fixtures/pnpm/list_global.json");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/pnpm/outdated_global.json");
const SEARCH_FIXTURE: &str = include_str!("fixtures/pnpm/search_array.json");

struct PnpmFakeExecutor {
    typescript_upgraded: AtomicBool,
}

impl PnpmFakeExecutor {
    fn new() -> Self {
        Self {
            typescript_upgraded: AtomicBool::new(false),
        }
    }
}

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9913)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for PnpmFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = request.command.args.clone();

        let (status, stdout): (ProcessExitStatus, Vec<u8>) = if program.ends_with("which") {
            (
                ProcessExitStatus::ExitCode(0),
                b"/opt/homebrew/bin/pnpm".to_vec(),
            )
        } else if program == "pnpm" || program.ends_with("/pnpm") {
            match args.as_slice() {
                [arg] if arg == "--version" => (
                    ProcessExitStatus::ExitCode(0),
                    VERSION_FIXTURE.as_bytes().to_vec(),
                ),
                [arg0, arg1, arg2, arg3]
                    if arg0 == "ls" && arg1 == "-g" && arg2 == "--depth=0" && arg3 == "--json" =>
                {
                    (
                        ProcessExitStatus::ExitCode(0),
                        INSTALLED_FIXTURE.as_bytes().to_vec(),
                    )
                }
                [arg0, arg1, arg2] if arg0 == "outdated" && arg1 == "-g" && arg2 == "--json" => {
                    let stdout = if self.typescript_upgraded.load(Ordering::SeqCst) {
                        b"{}".to_vec()
                    } else {
                        OUTDATED_FIXTURE.as_bytes().to_vec()
                    };
                    (ProcessExitStatus::ExitCode(1), stdout)
                }
                [arg0, arg1, arg2, arg3, query]
                    if arg0 == "search"
                        && arg1 == "--json"
                        && arg2 == "--limit"
                        && arg3 == "20"
                        && query == "typescript" =>
                {
                    (
                        ProcessExitStatus::ExitCode(0),
                        SEARCH_FIXTURE.as_bytes().to_vec(),
                    )
                }
                [arg0, arg1, spec]
                    if arg0 == "install" && arg1 == "-g" && spec == "eslint@9.0.0" =>
                {
                    (ProcessExitStatus::ExitCode(0), Vec::new())
                }
                [arg0, arg1, name]
                    if arg0 == "uninstall" && arg1 == "-g" && name == "typescript" =>
                {
                    (ProcessExitStatus::ExitCode(0), Vec::new())
                }
                [arg0, arg1, name] if arg0 == "update" && arg1 == "-g" && name == "typescript" => {
                    self.typescript_upgraded.store(true, Ordering::SeqCst);
                    (ProcessExitStatus::ExitCode(0), Vec::new())
                }
                [arg0, arg1] if arg0 == "update" && arg1 == "-g" => {
                    self.typescript_upgraded.store(true, Ordering::SeqCst);
                    (ProcessExitStatus::ExitCode(0), Vec::new())
                }
                [arg0, arg1, arg2, query]
                    if arg0 == "search"
                        && arg1 == "--json"
                        && arg2 == "--limit"
                        && query == "typescript" =>
                {
                    (
                        ProcessExitStatus::ExitCode(0),
                        SEARCH_FIXTURE.as_bytes().to_vec(),
                    )
                }
                _ => (ProcessExitStatus::ExitCode(0), Vec::new()),
            }
        } else {
            (ProcessExitStatus::ExitCode(0), Vec::new())
        };

        Ok(Box::new(FakeProcess {
            output: ProcessOutput {
                status,
                stdout,
                stderr: Vec::new(),
                started_at: now,
                finished_at: now,
            },
        }))
    }
}

fn build_runtime(executor: Arc<dyn ProcessExecutor>) -> AdapterRuntime {
    let source = ProcessPnpmSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(PnpmAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn pnpm_detect_list_search_and_mutate_through_orchestration() {
    let runtime = build_runtime(Arc::new(PnpmFakeExecutor::new()));

    let detect_task = runtime
        .submit(ManagerId::Pnpm, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("10.9.2"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/opt/homebrew/bin/pnpm"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Pnpm,
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
                package.package.name == "typescript"
                    && package.installed_version.as_deref() == Some("5.7.2")
            }));
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Pnpm,
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
                package.package.name == "typescript" && package.candidate_version == "5.7.2"
            }));
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }

    let search_task = runtime
        .submit(
            ManagerId::Pnpm,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "typescript".to_string(),
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
            assert_eq!(results.len(), 2);
            assert_eq!(results[0].result.package.name, "typescript");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::Pnpm,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pnpm,
                    name: "eslint".to_string(),
                },
                target_name: None,
                version: Some("9.0.0".to_string()),
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
            assert_eq!(mutation.package.name, "eslint");
            assert_eq!(mutation.before_version, None);
            assert_eq!(mutation.after_version.as_deref(), Some("9.0.0"));
        }
        other => panic!("expected install mutation, got {other:?}"),
    }

    let uninstall_task = runtime
        .submit(
            ManagerId::Pnpm,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pnpm,
                    name: "typescript".to_string(),
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
            assert_eq!(mutation.package.name, "typescript");
            assert_eq!(mutation.before_version.as_deref(), Some("5.7.2"));
            assert_eq!(mutation.after_version, None);
        }
        other => panic!("expected uninstall mutation, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::Pnpm,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pnpm,
                    name: "typescript".to_string(),
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
            assert_eq!(mutation.package.name, "typescript");
            assert_eq!(mutation.before_version.as_deref(), Some("5.6.3"));
            assert_eq!(mutation.after_version.as_deref(), Some("5.7.2"));
        }
        other => panic!("expected upgrade mutation, got {other:?}"),
    }
}
