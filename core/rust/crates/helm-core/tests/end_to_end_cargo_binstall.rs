use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::adapters::cargo_binstall::CargoBinstallAdapter;
use helm_core::adapters::cargo_binstall_process::ProcessCargoBinstallSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, UninstallRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{CoreErrorKind, ManagerId, PackageRef};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};
use helm_core::persistence::PackageStore;
use helm_core::sqlite::store::SqliteStore;

const VERSION_FIXTURE: &str = include_str!("fixtures/cargo_binstall/version.txt");
const INSTALLED_FIXTURE: &str = include_str!("fixtures/cargo_binstall/install_list.txt");

fn installed_fixture_with_bat_version(version: &str) -> String {
    INSTALLED_FIXTURE.replace("bat v0.24.0:", &format!("bat v{version}:"))
}

struct CargoBinstallFakeExecutor {
    bat_upgraded: AtomicBool,
}

impl CargoBinstallFakeExecutor {
    fn new() -> Self {
        Self {
            bat_upgraded: AtomicBool::new(false),
        }
    }
}

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9902)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for CargoBinstallFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = request.command.args.clone();

        let stdout: Vec<u8> = if program.ends_with("which") {
            if request
                .command
                .args
                .first()
                .is_some_and(|arg| arg == "cargo-binstall")
            {
                b"/Users/test/.cargo/bin/cargo-binstall".to_vec()
            } else {
                b"/Users/test/.cargo/bin/cargo".to_vec()
            }
        } else if program == "cargo-binstall" || program.ends_with("/cargo-binstall") {
            match args.as_slice() {
                [arg] if arg == "--version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [crate_name] if crate_name == "bat" => Vec::new(),
                [arg0, crate_name] if arg0 == "--force" && crate_name == "bat" => {
                    self.bat_upgraded.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                _ => Vec::new(),
            }
        } else if program == "cargo" || program.ends_with("/cargo") {
            match args.as_slice() {
                [arg0, arg1] if arg0 == "install" && arg1 == "--list" => {
                    if self.bat_upgraded.load(Ordering::SeqCst) {
                        installed_fixture_with_bat_version("0.25.0").into_bytes()
                    } else {
                        INSTALLED_FIXTURE.as_bytes().to_vec()
                    }
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
                        "ripgrep" => b"ripgrep = \"14.1.1\" # search tool\n".to_vec(),
                        "zellij" => b"zellij = \"0.42.1\" # terminal workspace\n".to_vec(),
                        _ => Vec::new(),
                    }
                }
                [arg0, arg1] if arg0 == "uninstall" && arg1 == "bat" => Vec::new(),
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

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-{test_name}-{nanos}.sqlite3"))
}

fn build_runtime_with_store(
    executor: Arc<dyn ProcessExecutor>,
    store: Arc<SqliteStore>,
) -> AdapterRuntime {
    let source = ProcessCargoBinstallSource::new(executor, store.clone());
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(CargoBinstallAdapter::new(source));
    AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store,
    )
    .expect("runtime creation with store should succeed")
}

async fn wait_for_tracked_package_count(store: &SqliteStore, expected: usize) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let count = store
            .list_installed()
            .expect("list_installed should succeed")
            .into_iter()
            .filter(|package| package.package.manager == ManagerId::CargoBinstall)
            .count();
        if count == expected {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        // Give the background persistence watcher time to commit the mutation
        // instead of continuously reopening the database in a tight loop.
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let actual = store
        .list_installed()
        .expect("list_installed should succeed")
        .into_iter()
        .filter(|package| package.package.manager == ManagerId::CargoBinstall)
        .count();
    panic!("timed out waiting for cargo-binstall tracked package count {expected}; saw {actual}");
}

#[tokio::test]
async fn cargo_binstall_tracks_only_helm_managed_packages() {
    let db_path = test_db_path("cargo-binstall-tracking");
    let store = Arc::new(SqliteStore::new(&db_path));
    store.migrate_to_latest().unwrap();

    let runtime =
        build_runtime_with_store(Arc::new(CargoBinstallFakeExecutor::new()), store.clone());

    let detect_task = runtime
        .submit(
            ManagerId::CargoBinstall,
            AdapterRequest::Detect(DetectRequest),
        )
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("1.12.1"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/Users/test/.cargo/bin/cargo-binstall"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let initial_list = runtime
        .submit(
            ManagerId::CargoBinstall,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let initial_snapshot = runtime
        .wait_for_terminal(initial_list, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match initial_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert!(packages.is_empty());
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::CargoBinstall,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::CargoBinstall,
                    name: "bat".to_string(),
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
            assert_eq!(mutation.package.name, "bat");
            assert_eq!(mutation.after_version.as_deref(), Some("0.24.0"));
        }
        other => panic!("expected install mutation, got {other:?}"),
    }
    wait_for_tracked_package_count(store.as_ref(), 1).await;

    let installed_task = runtime
        .submit(
            ManagerId::CargoBinstall,
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
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].package.name, "bat");
            assert_eq!(packages[0].installed_version.as_deref(), Some("0.24.0"));
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::CargoBinstall,
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
            assert_eq!(packages[0].package.name, "bat");
            assert_eq!(packages[0].candidate_version, "0.25.0");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::CargoBinstall,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::CargoBinstall,
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
    match upgrade_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "bat");
            assert_eq!(mutation.before_version.as_deref(), Some("0.24.0"));
            assert_eq!(mutation.after_version.as_deref(), Some("0.25.0"));
        }
        other => panic!("expected upgrade mutation, got {other:?}"),
    }

    let uninstall_task = runtime
        .submit(
            ManagerId::CargoBinstall,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::CargoBinstall,
                    name: "bat".to_string(),
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
            assert_eq!(mutation.package.name, "bat");
            assert_eq!(mutation.before_version.as_deref(), Some("0.25.0"));
        }
        other => panic!("expected uninstall mutation, got {other:?}"),
    }
    wait_for_tracked_package_count(store.as_ref(), 0).await;

    let final_list = runtime
        .submit(
            ManagerId::CargoBinstall,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let final_snapshot = runtime
        .wait_for_terminal(final_list, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match final_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert!(packages.is_empty());
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let _ = std::fs::remove_file(db_path);
}

#[tokio::test]
async fn cargo_binstall_rejects_untracked_uninstall() {
    let db_path = test_db_path("cargo-binstall-untracked");
    let store = Arc::new(SqliteStore::new(&db_path));
    store.migrate_to_latest().unwrap();

    let runtime =
        build_runtime_with_store(Arc::new(CargoBinstallFakeExecutor::new()), store.clone());

    let uninstall_task = runtime
        .submit(
            ManagerId::CargoBinstall,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::CargoBinstall,
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
        Some(AdapterTaskTerminalState::Failed(error)) => {
            assert_eq!(error.kind, CoreErrorKind::NotInstalled);
        }
        other => panic!("expected failed uninstall, got {other:?}"),
    }

    let _ = std::fs::remove_file(db_path);
}
