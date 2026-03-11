use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use helm_core::adapters::rubygems::RubyGemsAdapter;
use helm_core::adapters::rubygems_process::ProcessRubyGemsSource;
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

const VERSION_FIXTURE: &str = include_str!("fixtures/rubygems/version.txt");
const LIST_FIXTURE: &str = include_str!("fixtures/rubygems/list_local.txt");
const OUTDATED_FIXTURE: &str = include_str!("fixtures/rubygems/outdated.txt");
const SEARCH_FIXTURE: &str =
    "*** REMOTE GEMS ***\nrake (13.2.1)\n    Ruby based make-like utility.\n";

struct RubyGemsFakeExecutor {
    outdated_calls: Mutex<u32>,
}

impl RubyGemsFakeExecutor {
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

impl ProcessExecutor for RubyGemsFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = &request.command.args;

        let stdout: Vec<u8> = if program == "/usr/bin/which" {
            b"/opt/homebrew/bin/gem".to_vec()
        } else if program == "gem" || program.ends_with("/gem") {
            match args.as_slice() {
                [arg] if arg == "--version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [arg0, arg1, arg2] if arg0 == "list" && arg1 == "--local" && arg2 == "--all" => {
                    LIST_FIXTURE.as_bytes().to_vec()
                }
                [arg] if arg == "outdated" => {
                    let mut calls = self.outdated_calls.lock().expect("lock should succeed");
                    *calls += 1;
                    if *calls == 1 {
                        OUTDATED_FIXTURE.as_bytes().to_vec()
                    } else {
                        Vec::new()
                    }
                }
                [arg0, query, arg2, arg3]
                    if arg0 == "search"
                        && query == "rake"
                        && arg2 == "--remote"
                        && arg3 == "--details" =>
                {
                    SEARCH_FIXTURE.as_bytes().to_vec()
                }
                [arg0, gem_name, arg2, version]
                    if arg0 == "install"
                        && gem_name == "rubocop"
                        && arg2 == "--version"
                        && version == "1.72.0" =>
                {
                    Vec::new()
                }
                [arg0, gem_name, arg2, version, arg4]
                    if arg0 == "uninstall"
                        && gem_name == "rubocop"
                        && arg2 == "--version"
                        && version == "1.72.0"
                        && arg4 == "-x" =>
                {
                    Vec::new()
                }
                [arg0, gem_name] if arg0 == "update" && gem_name == "rake" => Vec::new(),
                [arg0] if arg0 == "update" => Vec::new(),
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
    let source = ProcessRubyGemsSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(RubyGemsAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn detect_and_list_rubygems_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(RubyGemsFakeExecutor::normal()));

    let detect_task = runtime
        .submit(ManagerId::RubyGems, AdapterRequest::Detect(DetectRequest))
        .await
        .expect("detect submission should succeed");
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .expect("detect should complete");

    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("3.5.22"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/opt/homebrew/bin/gem"))
            );
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let list_task = runtime
        .submit(
            ManagerId::RubyGems,
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
            assert_eq!(packages.len(), 3);
            assert_eq!(packages[0].package.name, "bundler");
            assert_eq!(packages[0].installed_version.as_deref(), Some("2.5.22"));
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn search_install_uninstall_and_upgrade_rubygems_through_full_orchestration_path() {
    let runtime = build_runtime(Arc::new(RubyGemsFakeExecutor::normal()));

    let search_task = runtime
        .submit(
            ManagerId::RubyGems,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "rake".to_string(),
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
            assert_eq!(results[0].result.package.name, "rake");
            assert_eq!(
                results[0].result.summary.as_deref(),
                Some("Ruby based make-like utility.")
            );
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let install_package = PackageRef {
        manager: ManagerId::RubyGems,
        name: "rubocop".to_string(),
    };
    let uninstall_package = install_package.clone();
    let upgrade_package = PackageRef {
        manager: ManagerId::RubyGems,
        name: "rake".to_string(),
    };

    for request in [
        AdapterRequest::Install(InstallRequest {
            package: install_package,
            target_name: None,
            version: Some("1.72.0".to_string()),
        }),
        AdapterRequest::Uninstall(UninstallRequest {
            package: uninstall_package,
            target_name: None,
            version: Some("1.72.0".to_string()),
        }),
        AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(upgrade_package),
            target_name: None,
            version: Some("13.1.0".to_string()),
        }),
    ] {
        let task_id = runtime
            .submit(ManagerId::RubyGems, request)
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

    let upgrade_all_task = runtime
        .submit(
            ManagerId::RubyGems,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: None,
                target_name: None,
                version: None,
            }),
        )
        .await
        .expect("upgrade-all submission should succeed");
    let upgrade_all_snapshot = runtime
        .wait_for_terminal(upgrade_all_task, Some(Duration::from_secs(5)))
        .await
        .expect("upgrade-all should complete");

    match upgrade_all_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "__all__");
        }
        other => panic!("expected Mutation response, got {other:?}"),
    }
}
