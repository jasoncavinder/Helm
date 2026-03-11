use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use helm_core::adapters::bundler::BundlerAdapter;
use helm_core::adapters::bundler_process::ProcessBundlerSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{ManagerId, PackageRef, SearchQuery, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const VERSION_FIXTURE: &str = include_str!("fixtures/bundler/version.txt");
const LIST_FIXTURE: &str = include_str!("fixtures/bundler/list_local.txt");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/bundler/outdated.txt");

struct BundlerFakeExecutor {
    outdated_calls: Mutex<u32>,
}

impl BundlerFakeExecutor {
    fn normal() -> Self {
        Self {
            outdated_calls: Mutex::new(0),
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

impl ProcessExecutor for BundlerFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = &request.command.args;

        let stdout: Vec<u8> = if program == "/usr/bin/which" {
            match args.as_slice() {
                [binary] if binary == "bundle" => b"/opt/homebrew/bin/bundle".to_vec(),
                [binary] if binary == "gem" => b"/opt/homebrew/bin/gem".to_vec(),
                _ => Vec::new(),
            }
        } else if program == "bundle" || program.ends_with("/bundle") {
            match args.as_slice() {
                [arg] if arg == "--version" => VERSION_FIXTURE.as_bytes().to_vec(),
                _ => Vec::new(),
            }
        } else if program == "gem" || program.ends_with("/gem") {
            match args.as_slice() {
                [arg0, arg1, arg2, arg3]
                    if arg0 == "list"
                        && arg1 == "--local"
                        && arg2 == "--all"
                        && arg3 == "bundler" =>
                {
                    LIST_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1] if arg0 == "outdated" && arg1 == "bundler" => {
                    let mut calls = self.outdated_calls.lock().expect("lock should succeed");
                    *calls += 1;
                    if *calls == 1 {
                        OUTDATED_FIXTURE.as_bytes().to_vec()
                    } else {
                        Vec::new()
                    }
                }
                [arg0, arg1, arg2, version]
                    if arg0 == "install"
                        && arg1 == "bundler"
                        && arg2 == "--version"
                        && version == "2.5.23" =>
                {
                    Vec::new()
                }
                [arg0, arg1, arg2, version, arg4]
                    if arg0 == "uninstall"
                        && arg1 == "bundler"
                        && arg2 == "--version"
                        && version == "2.5.22"
                        && arg4 == "-x" =>
                {
                    Vec::new()
                }
                [arg0, arg1] if arg0 == "update" && arg1 == "bundler" => Vec::new(),
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
    let source = ProcessBundlerSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(BundlerAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn detect_and_list_bundler_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(BundlerFakeExecutor::normal()));

    let detect_task = runtime
        .submit(ManagerId::Bundler, AdapterRequest::Detect(DetectRequest))
        .await
        .expect("detect submission should succeed");
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .expect("detect should complete");

    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("2.5.22"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/opt/homebrew/bin/bundle"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let list_task = runtime
        .submit(
            ManagerId::Bundler,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .expect("list installed submission should succeed");
    let list_snapshot = runtime
        .wait_for_terminal(list_task, Some(Duration::from_secs(5)))
        .await
        .expect("list installed should complete");

    assert_eq!(list_snapshot.runtime.status, TaskStatus::Completed);
    match list_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert_eq!(packages.len(), 2);
            assert_eq!(packages[0].package.name, "bundler");
            assert_eq!(packages[0].installed_version.as_deref(), Some("2.5.22"));
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn search_install_uninstall_and_upgrade_bundler_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(BundlerFakeExecutor::normal()));

    let search_task = runtime
        .submit(
            ManagerId::Bundler,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "bund".to_string(),
                    issued_at: SystemTime::now(),
                },
            }),
        )
        .await
        .expect("search submission should succeed");
    let search_snapshot = runtime
        .wait_for_terminal(search_task, Some(Duration::from_secs(5)))
        .await
        .expect("search should complete");

    match search_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::SearchResults(results))) => {
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].result.package.name, "bundler");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let package = PackageRef {
        manager: ManagerId::Bundler,
        name: "bundler".to_string(),
    };

    for request in [
        AdapterRequest::Install(InstallRequest {
            package: package.clone(),
            target_name: None,
            version: Some("2.5.23".to_string()),
        }),
        AdapterRequest::Uninstall(UninstallRequest {
            package: package.clone(),
            target_name: None,
            version: Some("2.5.22".to_string()),
        }),
        AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(package.clone()),
            target_name: None,
            version: Some("2.5.22".to_string()),
        }),
    ] {
        let task_id = runtime
            .submit(ManagerId::Bundler, request)
            .await
            .expect("mutation submission should succeed");
        let snapshot = runtime
            .wait_for_terminal(task_id, Some(Duration::from_secs(5)))
            .await
            .expect("mutation should complete");

        assert_eq!(snapshot.runtime.status, TaskStatus::Completed);
        match snapshot.terminal_state {
            Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(_))) => {}
            other => panic!("expected Mutation response, got {other:?}"),
        }
    }
}
