use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use helm_core::adapters::uv_tool_process::{ProcessUvToolSource, UvToolContext, UvToolReadAdapter};
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, ListInstalledRequest, ManagerAdapter,
    RefreshRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};
use helm_core::persistence::PackageStore;
use helm_core::sqlite::SqliteStore;

const INSTALLED: &str = "black v24.2.0 [required: ==24.2.0]\n- black\n- blackd\n";

fn output(stdout: &str, stderr: &str) -> ExecutionResult<ProcessOutput> {
    Ok(ProcessOutput {
        status: ProcessExitStatus::ExitCode(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        started_at: SystemTime::UNIX_EPOCH,
        finished_at: SystemTime::UNIX_EPOCH,
    })
}

struct Process(ExecutionResult<ProcessOutput>);
impl RunningProcess for Process {
    fn pid(&self) -> Option<u32> {
        None
    }
    fn terminate(&self, _: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }
    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        Box::pin(async move { self.0 })
    }
}

#[derive(Default)]
struct Executor {
    replies: Mutex<VecDeque<ExecutionResult<ProcessOutput>>>,
    requests: Mutex<Vec<ProcessSpawnRequest>>,
}
impl Executor {
    fn push(&self, result: ExecutionResult<ProcessOutput>) {
        self.replies.lock().unwrap().push_back(result);
    }
    fn inventory(&self, stdout: &str, stderr: &str) {
        self.push(output("uv 0.12.9 (9f9286029 2026-09-01)\n", ""));
        self.push(output(stdout, stderr));
    }
}
impl ProcessExecutor for Executor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        self.requests.lock().unwrap().push(request);
        Ok(Box::new(Process(
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected extra command"),
        )))
    }
}

fn context(store: &str) -> UvToolContext {
    UvToolContext::new(PathBuf::from("/selected tools/uv"), PathBuf::from(store)).unwrap()
}

fn adapter(executor: Arc<dyn ProcessExecutor>, store: &str) -> Arc<dyn ManagerAdapter> {
    Arc::new(UvToolReadAdapter::new(ProcessUvToolSource::new(
        executor,
        context(store),
    )))
}

#[test]
fn context_rejects_implicit_or_ambiguous_paths() {
    for path in ["uv", "", "/", "/tools/../uv", "/tools/uv\n", "/tools/uv\0"] {
        assert!(UvToolContext::new(path.into(), "/store".into()).is_err());
        assert!(UvToolContext::new("/tools/uv".into(), path.into()).is_err());
    }
}

#[test]
fn context_normalizes_equivalent_lexical_store_paths() {
    assert_eq!(context("/store//tools/"), context("/store/tools"));
}

#[tokio::test]
async fn offline_inventory_uses_one_explicit_scope_with_task_identity_and_no_writes() {
    let executor = Arc::new(Executor::default());
    executor.inventory(INSTALLED, "");
    let runtime = AdapterRuntime::new([adapter(executor.clone(), "/chosen tool store")]).unwrap();
    runtime.set_network_available(false);
    let task = runtime
        .submit(
            ManagerId::Uv,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let terminal = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match terminal.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::SnapshotSync {
            installed: Some(packages),
            outdated: None,
        })) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].package.name, "black");
            assert_eq!(packages[0].installed_version.as_deref(), Some("24.2.0"));
            assert!(
                !packages[0]
                    .package_identifier
                    .as_ref()
                    .unwrap()
                    .contains("/chosen")
            );
        }
        other => panic!("unexpected result: {other:?}"),
    }
    let requests = executor.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(request.manager, ManagerId::Uv);
        assert_eq!(request.task_id, Some(task));
        assert_eq!(request.command.program, PathBuf::from("/selected tools/uv"));
        assert_eq!(request.command.working_dir, Some(PathBuf::from("/")));
        assert_eq!(request.command.env["UV_TOOL_DIR"], "/chosen tool store");
        assert_eq!(request.command.env["UV_OFFLINE"], "true");
        assert_eq!(request.command.env["UV_PYTHON_DOWNLOADS"], "never");
        assert_eq!(request.private_output_limit, Some(4 * 1024 * 1024));
        assert!(request.timeout.is_some());
        assert!(request.idle_timeout.is_some());
        assert!(!request.requires_elevation);
        assert!(request.privileged_operation.is_none());
    }
    assert_eq!(
        requests[1].command.args,
        [
            "--color",
            "never",
            "--no-progress",
            "tool",
            "list",
            "--show-version-specifiers",
            "--offline",
            "--directory",
            "/",
            "--no-config",
            "--no-cache"
        ]
    );
}

#[tokio::test]
async fn unsupported_or_failed_version_never_starts_inventory() {
    for version in [
        "",
        "0.12.9",
        "uv 0.12.8",
        "uv 0.12.19",
        "uv 0.13.0",
        "uv 0.12.9rc1",
        "uv 0.12.09",
        "uv 0.12.9\nextra",
        "uv 0.12.9 unknown",
    ] {
        let executor = Arc::new(Executor::default());
        executor.push(output(version, ""));
        let runtime = AdapterRuntime::new([adapter(executor.clone(), "/store")]).unwrap();
        assert!(
            runtime
                .submit_refresh_request_response(
                    ManagerId::Uv,
                    AdapterRequest::Refresh(RefreshRequest)
                )
                .await
                .is_err()
        );
        assert_eq!(executor.requests.lock().unwrap().len(), 1, "{version}");
    }
    for result in [output("uv 0.12.9", "warning"), {
        let mut result = output("uv 0.12.9", "").unwrap();
        result.status = ProcessExitStatus::ExitCode(1);
        Ok(result)
    }] {
        let executor = Arc::new(Executor::default());
        executor.push(result);
        let runtime = AdapterRuntime::new([adapter(executor.clone(), "/store")]).unwrap();
        assert!(
            runtime
                .submit_refresh_request_response(
                    ManagerId::Uv,
                    AdapterRequest::Refresh(RefreshRequest)
                )
                .await
                .is_err()
        );
        assert_eq!(executor.requests.lock().unwrap().len(), 1);
    }
}

#[tokio::test]
async fn supported_version_detection_preserves_selected_executable() {
    for version in ["uv 0.12.9", "uv 0.12.18 (01cb90c1a)"] {
        let executor = Arc::new(Executor::default());
        executor.push(output(version, ""));
        let runtime = AdapterRuntime::new([adapter(executor, "/store")]).unwrap();
        match runtime
            .submit_refresh_request_response(ManagerId::Uv, AdapterRequest::Detect(DetectRequest))
            .await
            .unwrap()
        {
            AdapterResponse::Detection(detection) => {
                assert!(detection.installed);
                assert_eq!(
                    detection.executable_path,
                    Some(PathBuf::from("/selected tools/uv"))
                );
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}

#[tokio::test]
async fn store_scopes_have_distinct_package_identifiers() {
    let mut identifiers = Vec::new();
    for store in ["/first/store", "/second/store"] {
        let executor = Arc::new(Executor::default());
        executor.inventory(INSTALLED, "");
        let runtime = AdapterRuntime::new([adapter(executor, store)]).unwrap();
        match runtime
            .submit_refresh_request_response(ManagerId::Uv, AdapterRequest::Refresh(RefreshRequest))
            .await
            .unwrap()
        {
            AdapterResponse::SnapshotSync {
                installed: Some(packages),
                ..
            } => identifiers.push(packages[0].package_identifier.clone()),
            other => panic!("unexpected response: {other:?}"),
        }
    }
    assert_ne!(identifiers[0], identifiers[1]);
}

#[tokio::test]
async fn rejected_refreshes_preserve_committed_inventory_and_confirmed_empty_can_replace_it() {
    let path = std::env::temp_dir().join(format!(
        "helm-uv-read-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();
    let executor = Arc::new(Executor::default());
    let runtime = AdapterRuntime::with_all_stores(
        [adapter(executor.clone(), "/store")],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();
    executor.inventory(INSTALLED, "");
    persisted_refresh(&runtime).await.unwrap();
    let expected = store.list_installed().unwrap();
    assert_eq!(expected.len(), 1);
    for result in [
        output("", ""),
        output("not a tool listing", ""),
        output(
            INSTALLED,
            "warning: https://secret:password@example.invalid/ skipped a tool",
        ),
        Err(CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Timeout,
            message: "private timeout details".into(),
        }),
        Err(CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Cancelled,
            message: "private cancelled details".into(),
        }),
        {
            let mut result = output(INSTALLED, "").unwrap();
            result.status = ProcessExitStatus::ExitCode(1);
            Ok(result)
        },
    ] {
        executor.push(output("uv 0.12.9", ""));
        executor.push(result);
        let error = persisted_refresh(&runtime).await.unwrap_err();
        assert!(!format!("{error:?}").contains("password"));
        assert!(!format!("{error:?}").contains("private"));
        assert_eq!(store.list_installed().unwrap(), expected);
    }
    executor.inventory("", "No tools installed\n");
    persisted_refresh(&runtime).await.unwrap();
    assert!(store.list_installed().unwrap().is_empty());
    // The disposable database is intentionally retained for debugging; never a user database.
}

async fn persisted_refresh(runtime: &AdapterRuntime) -> ExecutionResult<AdapterResponse> {
    let (task, persistence) = runtime
        .submit_with_persistence(ManagerId::Uv, AdapterRequest::Refresh(RefreshRequest))
        .await?;
    let snapshot = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await?;
    persistence.wait_for_completion().await;
    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(response)) => Ok(response),
        Some(AdapterTaskTerminalState::Failed(error))
        | Some(AdapterTaskTerminalState::Cancelled(Some(error))) => Err(error),
        other => panic!("unexpected terminal state: {other:?}"),
    }
}

#[tokio::test]
async fn mutations_are_rejected_before_any_process_runs() {
    let executor = Arc::new(Executor::default());
    let adapter = adapter(executor.clone(), "/store");
    let error = adapter
        .execute(AdapterRequest::Upgrade(UpgradeRequest {
            package: None,
            target_name: None,
            version: None,
        }))
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    assert_eq!(error.action, Some(ManagerAction::Upgrade));
    assert!(executor.requests.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_uv_inventory_terminates_the_task_scoped_process() {
    use helm_core::execution::{CommandSpec, TokioProcessExecutor};
    use helm_core::orchestration::CancellationMode;
    use std::sync::atomic::{AtomicU32, Ordering};
    struct SlowInventory {
        pid: AtomicU32,
    }
    impl ProcessExecutor for SlowInventory {
        fn spawn(
            &self,
            mut request: ProcessSpawnRequest,
        ) -> ExecutionResult<Box<dyn RunningProcess>> {
            if request.action == ManagerAction::Detect {
                return Ok(Box::new(Process(output("uv 0.12.9", ""))));
            }
            // Stand in for a blocked uv read without touching tools or using a shell.
            request.command = CommandSpec::new("/bin/sleep").arg("30");
            let process = TokioProcessExecutor.spawn(request)?;
            self.pid.store(process.pid().unwrap(), Ordering::SeqCst);
            Ok(process)
        }
    }
    let executor = Arc::new(SlowInventory {
        pid: AtomicU32::new(0),
    });
    let runtime = AdapterRuntime::new([adapter(executor.clone(), "/store")]).unwrap();
    let task = runtime
        .submit(ManagerId::Uv, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while executor.pid.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    runtime
        .cancel(task, CancellationMode::Immediate)
        .await
        .unwrap();
    let terminal = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    assert!(matches!(
        terminal.terminal_state,
        Some(AdapterTaskTerminalState::Cancelled(_))
    ));
    let pid = executor.pid.load(Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(5), async {
        while unsafe { libc::kill(pid as i32, 0) == 0 } {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("task cancellation should reap the process");
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "opt-in: HELM_UV_CONTRACT_EXECUTABLE must be an absolute uv path; only a new disposable store is read"]
async fn real_uv_empty_inventory_runs_through_helm_process_and_task_boundaries() {
    let executable =
        PathBuf::from(std::env::var_os("HELM_UV_CONTRACT_EXECUTABLE").expect("explicit uv path"));
    assert!(executable.is_absolute() && executable.is_file());
    let tool_dir = std::env::temp_dir().join(format!(
        "helm-uv-adapter-empty-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&tool_dir).unwrap();
    let source = ProcessUvToolSource::new(
        Arc::new(helm_core::execution::TokioProcessExecutor),
        UvToolContext::new(executable.canonicalize().unwrap(), tool_dir).unwrap(),
    );
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(UvToolReadAdapter::new(source));
    let runtime = AdapterRuntime::new([adapter]).unwrap();
    runtime.set_network_available(false);
    match runtime
        .submit_refresh_request_response(
            ManagerId::Uv,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap()
    {
        AdapterResponse::SnapshotSync {
            installed: Some(packages),
            outdated: None,
        } => assert!(packages.is_empty()),
        other => panic!("unexpected response: {other:?}"),
    }
}
