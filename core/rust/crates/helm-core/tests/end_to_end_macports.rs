use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use helm_core::adapters::macports::MacPortsAdapter;
use helm_core::adapters::macports_process::ProcessMacPortsSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{ManagerId, PackageRef, SearchQuery, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const VERSION_FIXTURE: &str = include_str!("fixtures/macports/version.txt");
const SEARCH_FIXTURE: &str = include_str!("fixtures/macports/search.txt");
const INITIAL_INSTALLED_FIXTURE: &str = "The following ports are currently installed:\n  git @2.49.0_0+credential_osxkeychain (active)\n  git @2.48.1_0+credential_osxkeychain\n";
const INITIAL_OUTDATED_FIXTURE: &str = "The following installed ports are outdated:\n  git @2.49.0_0+credential_osxkeychain < 2.50.0_0+credential_osxkeychain\n";

struct MacPortsFakeState {
    installed_output: String,
    outdated_output: String,
}

struct MacPortsFakeExecutor {
    state: Mutex<MacPortsFakeState>,
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

impl ProcessExecutor for MacPortsFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = request.command.args.clone();

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/opt/local/bin/port".to_vec()
        } else if program == "port" || program.ends_with("/port") {
            let mut state = self.state.lock().expect("macports fake state lock");
            match args.as_slice() {
                [arg] if arg == "version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [arg] if arg == "installed" => state.installed_output.clone().into_bytes(),
                [arg] if arg == "outdated" => state.outdated_output.clone().into_bytes(),
                [arg0, query] if arg0 == "search" && query == "ripgrep" => {
                    SEARCH_FIXTURE.as_bytes().to_vec()
                }
                [arg0, name, version, variant]
                    if arg0 == "install"
                        && name == "git"
                        && version == "@2.49.0_0"
                        && variant == "+credential_osxkeychain" =>
                {
                    state.installed_output = INITIAL_INSTALLED_FIXTURE.to_string();
                    Vec::new()
                }
                [arg0, name, version, variant]
                    if arg0 == "uninstall"
                        && name == "git"
                        && version == "@2.49.0_0"
                        && variant == "+credential_osxkeychain" =>
                {
                    state.installed_output =
                        "The following ports are currently installed:\n  git @2.48.1_0+credential_osxkeychain (active)\n"
                            .to_string();
                    state.outdated_output = "No installed ports are outdated.\n".to_string();
                    Vec::new()
                }
                [arg0, name, version, variant]
                    if arg0 == "upgrade"
                        && name == "git"
                        && version == "@2.49.0_0"
                        && variant == "+credential_osxkeychain" =>
                {
                    state.installed_output =
                        "The following ports are currently installed:\n  git @2.50.0_0+credential_osxkeychain (active)\n  git @2.48.1_0+credential_osxkeychain\n"
                            .to_string();
                    state.outdated_output = "No installed ports are outdated.\n".to_string();
                    Vec::new()
                }
                [arg0, pseudo] if arg0 == "upgrade" && pseudo == "outdated" => {
                    state.installed_output =
                        "The following ports are currently installed:\n  git @2.50.0_0+credential_osxkeychain (active)\n  git @2.48.1_0+credential_osxkeychain\n"
                            .to_string();
                    state.outdated_output = "No installed ports are outdated.\n".to_string();
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
    let source = ProcessMacPortsSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(MacPortsAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

fn build_executor() -> Arc<dyn ProcessExecutor> {
    Arc::new(MacPortsFakeExecutor {
        state: Mutex::new(MacPortsFakeState {
            installed_output: INITIAL_INSTALLED_FIXTURE.to_string(),
            outdated_output: INITIAL_OUTDATED_FIXTURE.to_string(),
        }),
    })
}

#[tokio::test]
async fn macports_detect_and_listing_work_through_orchestration() {
    let runtime = build_runtime(build_executor());

    let detect_task = runtime
        .submit(ManagerId::MacPorts, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    assert_eq!(detect_snapshot.runtime.status, TaskStatus::Completed);
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("2.8.1"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/opt/local/bin/port"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::MacPorts,
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
            assert_eq!(packages[0].package.name, "git+credential_osxkeychain");
            assert_eq!(packages[0].installed_version.as_deref(), Some("2.49.0_0"));
            assert!(packages[0].runtime_state.is_active);
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::MacPorts,
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
            assert_eq!(packages[0].package.name, "git+credential_osxkeychain");
            assert_eq!(packages[0].candidate_version, "2.50.0_0");
            assert!(packages[0].runtime_state.is_active);
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn macports_search_and_mutation_paths_use_exact_targets_through_orchestration() {
    let runtime = build_runtime(build_executor());

    let search_task = runtime
        .submit(
            ManagerId::MacPorts,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "ripgrep".to_string(),
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
            assert_eq!(results[0].result.package.name, "ripgrep");
            assert_eq!(results[0].result.version.as_deref(), Some("14.1.1_0"));
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let package = PackageRef {
        manager: ManagerId::MacPorts,
        name: "git+credential_osxkeychain".to_string(),
    };

    let install_task = runtime
        .submit(
            ManagerId::MacPorts,
            AdapterRequest::Install(InstallRequest {
                package: package.clone(),
                target_name: Some(package.name.clone()),
                version: Some("2.49.0_0".to_string()),
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
            assert_eq!(mutation.package.name, "git+credential_osxkeychain");
            assert_eq!(mutation.after_version.as_deref(), Some("2.49.0_0"));
        }
        other => panic!("expected install mutation, got {other:?}"),
    }

    let runtime = build_runtime(build_executor());
    let uninstall_task = runtime
        .submit(
            ManagerId::MacPorts,
            AdapterRequest::Uninstall(UninstallRequest {
                package: package.clone(),
                target_name: Some(package.name.clone()),
                version: Some("2.49.0_0".to_string()),
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
            assert_eq!(mutation.package.name, "git+credential_osxkeychain");
            assert_eq!(mutation.before_version.as_deref(), Some("2.49.0_0"));
        }
        other => panic!("expected uninstall mutation, got {other:?}"),
    }

    let runtime = build_runtime(build_executor());
    let upgrade_task = runtime
        .submit(
            ManagerId::MacPorts,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(package),
                target_name: Some("git+credential_osxkeychain".to_string()),
                version: Some("2.49.0_0".to_string()),
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
            assert_eq!(mutation.package.name, "git+credential_osxkeychain");
            assert_eq!(mutation.before_version.as_deref(), Some("2.49.0_0"));
            assert_eq!(mutation.after_version.as_deref(), Some("2.50.0_0"));
        }
        other => panic!("expected upgrade mutation, got {other:?}"),
    }
}
