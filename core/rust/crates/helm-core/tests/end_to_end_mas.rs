use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::adapters::mas::MasAdapter;
use helm_core::adapters::mas_process::ProcessMasSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{CoreErrorKind, ManagerId, PackageRef, SearchQuery, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const VERSION_FIXTURE: &str = include_str!("fixtures/mas/version.txt");
const LIST_FIXTURE: &str = include_str!("fixtures/mas/list.txt");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/mas/outdated.txt");
const SEARCH_FIXTURE: &str = "409183694  Keynote              (14.3)\n";

struct MasFakeExecutor {
    fail_all: bool,
    keynote_upgraded: AtomicBool,
    upgrade_all_applied: AtomicBool,
}

impl MasFakeExecutor {
    fn normal() -> Self {
        Self {
            fail_all: false,
            keynote_upgraded: AtomicBool::new(false),
            upgrade_all_applied: AtomicBool::new(false),
        }
    }

    fn failing() -> Self {
        Self {
            fail_all: true,
            keynote_upgraded: AtomicBool::new(false),
            upgrade_all_applied: AtomicBool::new(false),
        }
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

impl ProcessExecutor for MasFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();

        if self.fail_all {
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(127),
                    stdout: Vec::new(),
                    stderr: b"mas: command not found".to_vec(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }

        let program = request.command.program.to_string_lossy().to_string();
        let args = &request.command.args;

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/opt/homebrew/bin/mas".to_vec()
        } else if program == "mas" || program.ends_with("/mas") {
            match args.as_slice() {
                [arg] if arg == "version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [arg] if arg == "list" => LIST_FIXTURE.as_bytes().to_vec(),
                [arg] if arg == "outdated" => {
                    if self.upgrade_all_applied.load(Ordering::SeqCst) {
                        Vec::new()
                    } else if self.keynote_upgraded.load(Ordering::SeqCst) {
                        b"497799835  Xcode               (16.1 -> 16.2)\n".to_vec()
                    } else {
                        OUTDATED_FIXTURE.as_bytes().to_vec()
                    }
                }
                [arg0, query] if arg0 == "search" && query == "Keynote" => {
                    SEARCH_FIXTURE.as_bytes().to_vec()
                }
                [arg0, app_id] if arg0 == "install" && app_id == "409183694" => Vec::new(),
                [arg0, app_id] if arg0 == "get" && app_id == "409183694" => Vec::new(),
                [arg0, app_id] if arg0 == "uninstall" && app_id == "409183694" => Vec::new(),
                [arg0, app_id] if arg0 == "upgrade" && app_id == "409183694" => {
                    self.keynote_upgraded.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                [arg0] if arg0 == "upgrade" => {
                    self.upgrade_all_applied.store(true, Ordering::SeqCst);
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
    let source = ProcessMasSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(MasAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn detect_mas_through_full_orchestration_path() {
    let executor = Arc::new(MasFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(ManagerId::Mas, AdapterRequest::Detect(DetectRequest))
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
            assert_eq!(info.version, Some("1.8.7".to_string()));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/opt/homebrew/bin/mas"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_installed_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(MasFakeExecutor::normal()));

    let task_id = runtime
        .submit(
            ManagerId::Mas,
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
            assert_eq!(packages[0].package_identifier.as_deref(), Some("497799835"));
            assert_eq!(packages[1].package.name, "Keynote");
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_outdated_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(MasFakeExecutor::normal()));

    let task_id = runtime
        .submit(
            ManagerId::Mas,
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
            assert_eq!(packages[0].package_identifier.as_deref(), Some("497799835"));
            assert_eq!(packages[1].candidate_version, "14.3");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn search_install_uninstall_and_upgrade_mas_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(MasFakeExecutor::normal()));

    let search_task = runtime
        .submit(
            ManagerId::Mas,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "Keynote".to_string(),
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
            assert_eq!(results[0].result.package.name, "Keynote");
            assert_eq!(
                results[0].result.package_identifier.as_deref(),
                Some("409183694")
            );
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let package = PackageRef {
        manager: ManagerId::Mas,
        name: "Keynote".to_string(),
    };

    for request in [
        AdapterRequest::Install(InstallRequest {
            package: package.clone(),
            target_name: None,
            version: None,
        }),
        AdapterRequest::Uninstall(UninstallRequest {
            package: package.clone(),
            target_name: None,
            version: None,
        }),
        AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(package.clone()),
            target_name: None,
            version: None,
        }),
        AdapterRequest::Upgrade(UpgradeRequest {
            package: None,
            target_name: None,
            version: None,
        }),
    ] {
        let task_id = runtime.submit(ManagerId::Mas, request).await.unwrap();
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
async fn mas_not_installed_propagates_as_structured_error() {
    let runtime = build_runtime(Arc::new(MasFakeExecutor::failing()));

    let task_id = runtime
        .submit(
            ManagerId::Mas,
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
            assert_eq!(error.manager, Some(ManagerId::Mas));
        }
        other => panic!("expected Failed terminal state, got {other:?}"),
    }
}
