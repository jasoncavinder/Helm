use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::adapters::softwareupdate::SoftwareUpdateAdapter;
use helm_core::adapters::softwareupdate_process::ProcessSoftwareUpdateSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, ListOutdatedRequest, ManagerAdapter,
    UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{
    CoreErrorKind, ManagerAction, ManagerId, PackageRef, TaskStatus, TaskType,
};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};
use helm_core::persistence::DetectionStore;
use helm_core::sqlite::SqliteStore;

const VERSION_FIXTURE: &str = include_str!("fixtures/softwareupdate/version.txt");
const LIST_AVAILABLE_FIXTURE: &str = include_str!("fixtures/softwareupdate/list_available.txt");

struct SoftwareUpdateFakeExecutor {
    fail_all: bool,
}

impl SoftwareUpdateFakeExecutor {
    fn normal() -> Self {
        Self { fail_all: false }
    }

    fn failing() -> Self {
        Self { fail_all: true }
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

impl ProcessExecutor for SoftwareUpdateFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();

        if self.fail_all {
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(127),
                    stdout: Vec::new(),
                    stderr: b"sw_vers: command not found".to_vec(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }

        let program = request.command.program.to_string_lossy().to_string();
        let args = &request.command.args;

        let stdout: Vec<u8> = if program.contains("sw_vers") {
            VERSION_FIXTURE.as_bytes().to_vec()
        } else if program.contains("softwareupdate") {
            match args.first().map(String::as_str) {
                Some("-l") => LIST_AVAILABLE_FIXTURE.as_bytes().to_vec(),
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
    let source = ProcessSoftwareUpdateSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SoftwareUpdateAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

fn build_runtime_with_store(
    executor: Arc<dyn ProcessExecutor>,
    store: Arc<SqliteStore>,
) -> AdapterRuntime {
    let source = ProcessSoftwareUpdateSource::new(executor);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SoftwareUpdateAdapter::new(source));
    AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store,
    )
    .expect("runtime creation with store should succeed")
}

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-{test_name}-{nanos}.sqlite3"))
}

#[tokio::test]
async fn detect_softwareupdate_through_full_orchestration_path() {
    let executor = Arc::new(SoftwareUpdateFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::SoftwareUpdate,
            AdapterRequest::Detect(DetectRequest),
        )
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
            assert_eq!(info.version, Some("15.3.1".to_string()));
        }
        other => panic!("expected Detection response, got {other:?}"),
    }
}

#[tokio::test]
async fn list_outdated_includes_restart_required_flag() {
    let executor = Arc::new(SoftwareUpdateFakeExecutor::normal());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::SoftwareUpdate,
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
            // macOS update requires restart
            assert!(packages[0].restart_required);
            // Safari update does not require restart
            assert!(!packages[1].restart_required);
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }
}

#[tokio::test]
async fn softwareupdate_not_installed_propagates_as_structured_error() {
    let executor = Arc::new(SoftwareUpdateFakeExecutor::failing());
    let runtime = build_runtime(executor);

    let task_id = runtime
        .submit(
            ManagerId::SoftwareUpdate,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
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
            assert_eq!(error.manager, Some(ManagerId::SoftwareUpdate));
        }
        other => panic!("expected Failed terminal state, got {other:?}"),
    }
}

#[tokio::test]
async fn safe_mode_blocks_softwareupdate_upgrade_submission() {
    let path = test_db_path("softwareupdate-safe-mode-block");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();
    store.set_safe_mode(true).unwrap();

    let executor = Arc::new(SoftwareUpdateFakeExecutor::normal());
    let runtime = build_runtime_with_store(executor, store);

    let error = runtime
        .submit(
            ManagerId::SoftwareUpdate,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::SoftwareUpdate,
                    name: "__confirm_os_updates__".to_string(),
                }),
            }),
        )
        .await
        .expect_err("safe mode should block softwareupdate upgrade submit");

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::SoftwareUpdate));
    assert_eq!(error.task, Some(TaskType::Upgrade));
    assert_eq!(error.action, Some(ManagerAction::Upgrade));

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn safe_mode_off_allows_confirmed_softwareupdate_upgrade() {
    let path = test_db_path("softwareupdate-safe-mode-off");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();
    store.set_safe_mode(false).unwrap();

    let executor = Arc::new(SoftwareUpdateFakeExecutor::normal());
    let runtime = build_runtime_with_store(executor, store);

    let task_id = runtime
        .submit(
            ManagerId::SoftwareUpdate,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::SoftwareUpdate,
                    name: "__confirm_os_updates__".to_string(),
                }),
            }),
        )
        .await
        .unwrap();

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

    let _ = std::fs::remove_file(path);
}
