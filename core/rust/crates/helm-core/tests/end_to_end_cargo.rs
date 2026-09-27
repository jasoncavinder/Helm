use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::adapters::cargo::CargoAdapter;
use helm_core::adapters::cargo_process::ProcessCargoSource;
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

const VERSION_FIXTURE: &str = include_str!("fixtures/cargo/version.txt");
const INSTALLED_FIXTURE: &str = include_str!("fixtures/cargo/install_list.txt");
const SEARCH_FIXTURE: &str = include_str!("fixtures/cargo/search.txt");

fn installed_fixture_with_bat_version(version: &str) -> String {
    INSTALLED_FIXTURE.replace("bat v0.24.0:", &format!("bat v{version}:"))
}

struct CargoFakeExecutor {
    bat_upgraded: AtomicBool,
    rargs_installed: AtomicBool,
    ripgrep_removed: AtomicBool,
}

impl CargoFakeExecutor {
    fn new() -> Self {
        Self {
            bat_upgraded: AtomicBool::new(false),
            rargs_installed: AtomicBool::new(false),
            ripgrep_removed: AtomicBool::new(false),
        }
    }
}

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9901)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for CargoFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = request.command.args.clone();

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/Users/test/.cargo/bin/cargo".to_vec()
        } else if program == "cargo" || program.ends_with("/cargo") {
            match args.as_slice() {
                [arg] if arg == "--version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [arg0, arg1] if arg0 == "install" && arg1 == "--list" => {
                    let mut installed = if self.bat_upgraded.load(Ordering::SeqCst) {
                        installed_fixture_with_bat_version("0.25.0")
                    } else {
                        INSTALLED_FIXTURE.to_string()
                    };
                    if self.rargs_installed.load(Ordering::SeqCst) {
                        installed.push_str("\nrargs v0.3.0:\n    rargs\n");
                    }
                    if self.ripgrep_removed.load(Ordering::SeqCst) {
                        installed = installed
                            .lines()
                            .filter(|line| !line.starts_with("ripgrep v") && line.trim() != "rg")
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                    installed.into_bytes()
                }
                [arg0, arg1, arg2, arg3, arg4, query]
                    if arg0 == "search"
                        && arg1 == "--limit"
                        && arg2 == "20"
                        && arg3 == "--color"
                        && arg4 == "never"
                        && query == "rip" =>
                {
                    SEARCH_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1, arg2, arg3, arg4, crate_name]
                    if arg0 == "search"
                        && arg1 == "--limit"
                        && arg2 == "1"
                        && arg3 == "--color"
                        && arg4 == "never" =>
                {
                    match crate_name.as_str() {
                        "bat" => b"bat = \"0.25.0\" # a cat clone with wings\n".to_vec(),
                        "zellij" => b"zellij = \"0.42.1\" # terminal workspace\n".to_vec(),
                        "ripgrep" => b"ripgrep = \"14.1.1\" # search tool\n".to_vec(),
                        _ => Vec::new(),
                    }
                }
                [arg0, crate_name] if arg0 == "install" && crate_name == "rargs" => {
                    self.rargs_installed.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                [arg0, crate_name] if arg0 == "uninstall" && crate_name == "ripgrep" => {
                    self.ripgrep_removed.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                [arg0, arg1, crate_name, flag, version]
                    if arg0 == "install"
                        && arg1 == "--force"
                        && crate_name == "bat"
                        && flag == "--version"
                        && version == "0.25.0" =>
                {
                    self.bat_upgraded.store(true, Ordering::SeqCst);
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
    let source = ProcessCargoSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(CargoAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

struct UnavailableCargoExecutor {
    failure: AtomicU8,
}

impl ProcessExecutor for UnavailableCargoExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let program = request.command.program.to_string_lossy();
        assert!(
            program.ends_with("which")
                || (program.ends_with("cargo")
                    && (request.command.args == ["--version"]
                        || request.command.args == ["install", "--list"]
                        || request
                            .command
                            .args
                            .first()
                            .is_some_and(|arg| arg == "search"))),
            "read-only recovery must not invoke repair or another installer: {:?}",
            request.command
        );
        let failure = self.failure.load(Ordering::SeqCst);
        if program.ends_with("cargo") && request.command.args == ["--version"] && failure != 0 {
            let now = SystemTime::now();
            let stderr = match failure {
                1 => {
                    "the 'cargo' binary, normally provided by the 'cargo' component, is not applicable to the 'stable-aarch64-apple-darwin' toolchain"
                }
                2 => "",
                3 => "ordinary command failure",
                _ => {
                    "error: 'cargo' is not installed for the toolchain 'stable-aarch64-apple-darwin'"
                }
            };
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(if failure == 2 { 0 } else { 1 }),
                    stdout: Vec::new(),
                    stderr: stderr.as_bytes().to_vec(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }
        CargoFakeExecutor::new().spawn(request)
    }
}

#[tokio::test]
async fn unusable_cargo_proxy_retains_inventory_and_recovers_without_toolchain_changes() {
    use helm_core::adapters::RefreshRequest;
    use helm_core::persistence::PackageStore;
    use helm_core::sqlite::SqliteStore;
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteStore::new(root.path().join("helm.db")));
    store.migrate_to_latest().unwrap();
    let executor = Arc::new(UnavailableCargoExecutor {
        failure: AtomicU8::new(0),
    });
    let adapter: Arc<dyn ManagerAdapter> =
        Arc::new(CargoAdapter::new(ProcessCargoSource::new(executor.clone())));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();
    let mut before = None;
    let mut outdated_before = None;
    for failure in [0, 1, 2, 3, 4, 0] {
        executor.failure.store(failure, Ordering::SeqCst);
        let (task, persistence) = runtime
            .submit_with_persistence(ManagerId::Cargo, AdapterRequest::Refresh(RefreshRequest))
            .await
            .unwrap();
        let snapshot = runtime
            .wait_for_terminal(task, Some(Duration::from_secs(5)))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), persistence.wait_for_completion())
            .await
            .unwrap();
        if failure != 0 {
            let Some(AdapterTaskTerminalState::Failed(error)) = snapshot.terminal_state else {
                panic!("unusable Cargo must fail rather than publishing an empty snapshot");
            };
            if failure == 1 || failure == 4 {
                assert!(error.message.starts_with("[cargo_toolchain_unavailable]"));
                assert!(error.message.contains("stable-aarch64-apple-darwin"));
            } else if failure == 2 {
                assert_eq!(error.kind, helm_core::models::CoreErrorKind::ParseFailure);
            } else {
                assert!(error.message.contains("ordinary command failure"));
                assert!(!error.message.contains("[cargo_toolchain_unavailable]"));
            }
        } else {
            assert!(matches!(
                snapshot.terminal_state,
                Some(AdapterTaskTerminalState::Succeeded(
                    AdapterResponse::SnapshotSync {
                        installed: Some(_),
                        ..
                    }
                ))
            ));
        }
        let installed = store.list_installed().unwrap();
        assert_eq!(installed.len(), 3);
        if let Some(before) = before.as_ref() {
            assert_eq!(before, &installed);
        }
        before = Some(installed);
        let outdated = store.list_outdated().unwrap();
        assert!(!outdated.is_empty());
        if let Some(before) = outdated_before.as_ref() {
            assert_eq!(before, &outdated);
        }
        outdated_before = Some(outdated);
    }
}

#[tokio::test]
async fn cargo_detect_list_search_and_mutate_through_orchestration() {
    let runtime = build_runtime(Arc::new(CargoFakeExecutor::new()));

    let detect_task = runtime
        .submit(ManagerId::Cargo, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("1.84.1"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/Users/test/.cargo/bin/cargo"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Cargo,
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
            assert_eq!(packages[0].package.name, "bat");
            assert_eq!(packages[0].installed_version.as_deref(), Some("0.24.0"));
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Cargo,
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
            assert_eq!(packages[0].package.name, "bat");
            assert_eq!(packages[0].candidate_version, "0.25.0");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }

    let search_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "rip".to_string(),
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
            assert_eq!(results[0].result.package.name, "ripgrep");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Cargo,
                    name: "rargs".to_string(),
                },
                target_name: None,
                version: None,
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
            assert_eq!(mutation.package.name, "rargs");
            assert_eq!(mutation.action, helm_core::models::ManagerAction::Install);
        }
        other => panic!("expected install mutation, got {other:?}"),
    }

    let uninstall_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Cargo,
                    name: "ripgrep".to_string(),
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
            assert_eq!(mutation.package.name, "ripgrep");
            assert_eq!(mutation.before_version.as_deref(), Some("14.1.1"));
        }
        other => panic!("expected uninstall mutation, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Cargo,
                    name: "bat".to_string(),
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
    assert_eq!(upgrade_snapshot.runtime.status, TaskStatus::Completed);
    match upgrade_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "bat");
            assert_eq!(mutation.before_version.as_deref(), Some("0.24.0"));
            assert_eq!(mutation.after_version.as_deref(), Some("0.25.0"));
        }
        other => panic!("expected upgrade mutation, got {other:?}"),
    }
}
