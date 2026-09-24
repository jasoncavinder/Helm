#![cfg(unix)]

use std::collections::VecDeque;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use helm_core::adapters::uv_tool_process::{ProcessUvToolSource, UvToolReadAdapter};
use helm_core::adapters::uv_tool_scope::{
    UvExecutableSelection, UvResolvedToolScope, UvScopeDiscovery, UvToolDiscovery,
};
use helm_core::adapters::{AdapterRequest, AdapterResponse, ListInstalledRequest, ManagerAdapter};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess, TokioProcessExecutor,
};
use helm_core::models::{CoreError, CoreErrorKind, ManagerId};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};
use helm_core::persistence::PackageStore;
use helm_core::sqlite::SqliteStore;

fn output(stdout: &str, stderr: &str) -> ExecutionResult<ProcessOutput> {
    Ok(ProcessOutput {
        status: ProcessExitStatus::ExitCode(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        started_at: SystemTime::UNIX_EPOCH,
        finished_at: SystemTime::UNIX_EPOCH,
    })
}

struct Reply {
    result: ExecutionResult<ProcessOutput>,
    after_wait: Option<Box<dyn FnOnce() + Send + Sync>>,
}

impl RunningProcess for Reply {
    fn pid(&self) -> Option<u32> {
        None
    }
    fn terminate(&self, _: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }
    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        Box::pin(async move {
            if let Some(callback) = self.after_wait {
                callback();
            }
            self.result
        })
    }
}

#[derive(Default)]
struct Executor {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<ProcessSpawnRequest>>,
}

impl Executor {
    fn push(&self, result: ExecutionResult<ProcessOutput>) {
        self.replies.lock().unwrap().push_back(Reply {
            result,
            after_wait: None,
        });
    }
    fn version(&self) {
        self.push(output("uv 0.12.9\n", ""));
    }
    fn directory(&self, path: &Path) {
        self.push(output(&format!("{}\n", path.display()), ""));
    }
}

impl ProcessExecutor for Executor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        self.requests.lock().unwrap().push(request);
        Ok(Box::new(
            self.replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected process"),
        ))
    }
}

struct Fixture {
    root: PathBuf,
    executable: PathBuf,
    store: PathBuf,
    executor: Arc<Executor>,
}

impl Fixture {
    fn new() -> Self {
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "helm-uv-scope-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let executable = root.join("bin/uv");
        let store = root.join("tools with spaces");
        executable_file(&executable);
        fs::create_dir(&store).unwrap();
        Self {
            root,
            executable,
            store,
            executor: Arc::new(Executor::default()),
        }
    }

    async fn discover(
        &self,
        selection: UvExecutableSelection,
        store: &Path,
    ) -> ExecutionResult<UvScopeDiscovery> {
        let executor = self.executor.clone();
        let store = store.to_owned();
        tokio::task::spawn_blocking(move || {
            UvToolDiscovery::new(executor).discover(selection, Some(store))
        })
        .await
        .unwrap()
    }

    async fn ready(&self) -> UvResolvedToolScope {
        self.executor.version();
        self.executor.directory(&self.store);
        match self
            .discover(
                UvExecutableSelection::Selected(self.executable.clone()),
                &self.store,
            )
            .await
            .unwrap()
        {
            UvScopeDiscovery::Ready(scope) => scope,
            other => panic!("expected ready: {other:?}"),
        }
    }
}

fn executable_file(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, b"fixture, executed only by the mock executor").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn discovery_uses_canonical_executable_and_uv_reported_store_with_private_offline_commands() {
    let fixture = Fixture::new();
    let scope = fixture.ready().await;
    assert_eq!(
        scope.context().executable(),
        fixture.executable.canonicalize().unwrap()
    );
    assert_eq!(
        scope.context().tool_dir(),
        fixture.store.canonicalize().unwrap()
    );
    assert_eq!(scope.version, "0.12.9");
    let requests = fixture.executor.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    for request in requests.iter() {
        assert_eq!(
            request.command.program,
            fixture.executable.canonicalize().unwrap()
        );
        assert_eq!(request.command.working_dir.as_deref(), Some(Path::new("/")));
        assert_eq!(
            request.command.env["UV_TOOL_DIR"],
            fixture.store.to_str().unwrap()
        );
        assert_eq!(request.command.env["UV_OFFLINE"], "true");
        assert_eq!(request.command.env["UV_PYTHON_DOWNLOADS"], "never");
        assert_eq!(request.private_output_limit, Some(16 * 1024));
        assert_eq!(request.timeout, Some(Duration::from_secs(10)));
        // Shared timeout policy keeps the idle deadline below the hard deadline.
        assert_eq!(request.idle_timeout, Some(Duration::from_secs(9)));
        assert!(!request.requires_elevation);
    }
    assert_eq!(
        requests[1].command.args,
        [
            "--color",
            "never",
            "--no-progress",
            "--offline",
            "--no-config",
            "--no-cache",
            "--directory",
            "/",
            "tool",
            "dir"
        ]
    );
}

#[tokio::test]
async fn symlink_aliases_collapse_and_store_aliases_share_one_canonical_scope() {
    let fixture = Fixture::new();
    let bin_alias = fixture.root.join("alias-bin");
    fs::create_dir(&bin_alias).unwrap();
    symlink(&fixture.executable, bin_alias.join("uv")).unwrap();
    let store_alias = fixture.root.join("alias-store");
    symlink(&fixture.store, &store_alias).unwrap();
    fixture.executor.version();
    fixture.executor.directory(&store_alias);
    let UvScopeDiscovery::Ready(scope) = fixture
        .discover(
            UvExecutableSelection::SearchDirectories(vec![
                bin_alias.clone(),
                fixture.executable.parent().unwrap().into(),
                bin_alias,
            ]),
            &store_alias,
        )
        .await
        .unwrap()
    else {
        panic!("expected resolved aliases")
    };
    assert_eq!(scope.executable.aliases.len(), 2);
    assert_eq!(
        scope.context().tool_dir(),
        fixture.store.canonicalize().unwrap()
    );
    assert_eq!(fixture.executor.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn distinct_executables_require_selection_without_running_any_candidates() {
    let fixture = Fixture::new();
    let other = fixture.root.join("other/uv");
    executable_file(&other);
    let result = fixture
        .discover(
            UvExecutableSelection::SearchDirectories(vec![
                fixture.executable.parent().unwrap().into(),
                other.parent().unwrap().into(),
            ]),
            &fixture.store,
        )
        .await
        .unwrap();
    assert!(
        matches!(result, UvScopeDiscovery::SelectionRequired(ref candidates) if candidates.len() == 2)
    );
    assert!(fixture.executor.requests.lock().unwrap().is_empty());
    // Explicit selection is honored even when another installation exists.
    fixture.executor.version();
    fixture.executor.directory(&fixture.store);
    let UvScopeDiscovery::Ready(scope) = fixture
        .discover(
            UvExecutableSelection::Selected(other.clone()),
            &fixture.store,
        )
        .await
        .unwrap()
    else {
        panic!("selected scope")
    };
    assert_eq!(scope.context().executable(), other.canonicalize().unwrap());
}

#[tokio::test]
async fn missing_selected_executable_fails_while_empty_search_is_not_found() {
    let fixture = Fixture::new();
    let missing = fixture.root.join("missing/uv");
    assert!(
        fixture
            .discover(
                UvExecutableSelection::Selected(missing.clone()),
                &fixture.store
            )
            .await
            .is_err()
    );
    assert_eq!(
        fixture
            .discover(
                UvExecutableSelection::SearchDirectories(vec![missing.parent().unwrap().into()]),
                &fixture.store
            )
            .await
            .unwrap(),
        UvScopeDiscovery::NotFound
    );
    assert!(fixture.executor.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn invalid_paths_non_executables_and_shims_never_run() {
    let fixture = Fixture::new();
    let shim = fixture.root.join("shims/uv");
    executable_file(&shim);
    let plain = fixture.root.join("plain");
    fs::write(&plain, b"not executable").unwrap();
    let dangling = fixture.root.join("dangling-uv");
    symlink(fixture.root.join("absent"), &dangling).unwrap();
    let dispatcher = fixture.root.join("dispatcher/mise");
    executable_file(&dispatcher);
    let dispatch_alias = fixture.root.join("uv-dispatcher");
    symlink(&dispatcher, &dispatch_alias).unwrap();
    for path in [
        PathBuf::from("uv"),
        PathBuf::from("/tmp/../uv"),
        PathBuf::from("/tmp/uv\n"),
        fixture.store.clone(),
        shim,
        plain,
        dangling,
        dispatch_alias,
    ] {
        assert!(
            fixture
                .discover(
                    UvExecutableSelection::Selected(path.clone()),
                    &fixture.store
                )
                .await
                .is_err(),
            "{path:?}"
        );
    }
    assert!(
        fixture
            .discover(
                UvExecutableSelection::Selected(fixture.executable.clone()),
                Path::new("relative-store")
            )
            .await
            .is_err()
    );
    assert!(
        fixture
            .discover(
                UvExecutableSelection::SearchDirectories(vec![PathBuf::from(".")]),
                &fixture.store
            )
            .await
            .is_err()
    );
    assert!(fixture.executor.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn incompatible_version_stops_before_directory_query() {
    let fixture = Fixture::new();
    fixture.executor.push(output("uv 0.13.0", ""));
    let error = fixture
        .discover(
            UvExecutableSelection::Selected(fixture.executable.clone()),
            &fixture.store,
        )
        .await
        .unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    assert_eq!(fixture.executor.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn selected_alias_to_known_shim_is_rejected_without_execution() {
    assert_alias_to_known_shim_is_rejected(false).await;
}

#[tokio::test]
async fn searched_alias_to_known_shim_is_rejected_without_execution() {
    assert_alias_to_known_shim_is_rejected(true).await;
}

async fn assert_alias_to_known_shim_is_rejected(search: bool) {
    let fixture = Fixture::new();
    let shim = fixture.root.join("asdf/shims/uv");
    executable_file(&shim);
    let bin = fixture.root.join("alias-bin");
    fs::create_dir(&bin).unwrap();
    let alias = bin.join("uv");
    symlink(shim, &alias).unwrap();
    // A shim can return valid probe output; it must be rejected before either probe.
    fixture.executor.version();
    fixture.executor.directory(&fixture.store);
    let selection = if search {
        UvExecutableSelection::SearchDirectories(vec![bin])
    } else {
        UvExecutableSelection::Selected(alias)
    };
    let result = fixture.discover(selection, &fixture.store).await;
    assert_eq!(fixture.executor.requests.lock().unwrap().len(), 0);
    assert!(matches!(result, Err(error) if error.kind == CoreErrorKind::UnsupportedCapability));
}

#[tokio::test]
async fn executable_replaced_during_version_probe_stops_before_directory_query() {
    let fixture = Fixture::new();
    let executable = fixture.executable.clone();
    let moved = fixture.root.join("old-uv");
    fixture.executor.replies.lock().unwrap().push_back(Reply {
        result: output("uv 0.12.9\n", ""),
        after_wait: Some(Box::new(move || {
            fs::rename(&executable, moved).unwrap();
            executable_file(&executable);
        })),
    });
    assert!(
        fixture
            .discover(
                UvExecutableSelection::Selected(fixture.executable.clone()),
                &fixture.store
            )
            .await
            .is_err()
    );
    assert_eq!(fixture.executor.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn discovery_commands_inherit_runtime_task_identity() {
    use helm_core::adapters::{AdapterResult, DetectRequest};
    use helm_core::models::{ActionSafety, DetectionInfo, ManagerAction, ManagerDescriptor};
    struct DiscoveryAdapter {
        discovery: UvToolDiscovery,
        executable: PathBuf,
        store: PathBuf,
    }
    impl ManagerAdapter for DiscoveryAdapter {
        fn descriptor(&self) -> &ManagerDescriptor {
            helm_core::registry::manager(ManagerId::Uv).unwrap()
        }
        fn action_safety(&self, action: ManagerAction) -> ActionSafety {
            action.safety()
        }
        fn execute(&self, _: AdapterRequest) -> AdapterResult<AdapterResponse> {
            let UvScopeDiscovery::Ready(scope) = self.discovery.discover(
                UvExecutableSelection::Selected(self.executable.clone()),
                Some(self.store.clone()),
            )?
            else {
                panic!("ready")
            };
            Ok(AdapterResponse::Detection(DetectionInfo {
                installed: true,
                executable_path: Some(scope.context().executable().into()),
                version: Some(scope.version),
            }))
        }
    }
    let fixture = Fixture::new();
    fixture.executor.version();
    fixture.executor.directory(&fixture.store);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(DiscoveryAdapter {
        discovery: UvToolDiscovery::new(fixture.executor.clone()),
        executable: fixture.executable.clone(),
        store: fixture.store.clone(),
    });
    let runtime = AdapterRuntime::new([adapter]).unwrap();
    let task = runtime
        .submit(ManagerId::Uv, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let terminal = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    assert!(matches!(
        terminal.terminal_state,
        Some(AdapterTaskTerminalState::Succeeded(_))
    ));
    let requests = fixture.executor.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| request.task_id == Some(task)));
}

#[tokio::test]
async fn directory_query_rejects_partial_failed_or_ambiguous_output_without_leaking_it() {
    let fixture = Fixture::new();
    let mut invalid_encoding = output("", "").unwrap();
    invalid_encoding.stdout = vec![0xff];
    let mut nonzero = output(fixture.store.to_str().unwrap(), "").unwrap();
    nonzero.status = ProcessExitStatus::ExitCode(1);
    let mut terminated = nonzero.clone();
    terminated.status = ProcessExitStatus::Terminated;
    for result in [
        output("", ""),
        output("relative/path", ""),
        output("/one\n/two\n", ""),
        output("/tmp/store\0", ""),
        output("/", ""),
        output("/tmp/../store", ""),
        output(&"x".repeat(16 * 1024 + 1), ""),
        output(
            fixture.store.to_str().unwrap(),
            "warning: secret:credential",
        ),
        output("/different-store", ""),
        Ok(invalid_encoding),
        Ok(nonzero),
        Ok(terminated),
        Err(CoreError {
            manager: None,
            action: None,
            task: None,
            kind: CoreErrorKind::Timeout,
            message: "secret:credential".into(),
        }),
    ] {
        fixture.executor.version();
        fixture.executor.push(result);
        let error = fixture
            .discover(
                UvExecutableSelection::Selected(fixture.executable.clone()),
                &fixture.store,
            )
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains("secret:credential"));
    }
}

#[tokio::test]
async fn absent_store_is_not_an_authoritative_empty_inventory_and_non_directory_store_fails() {
    let fixture = Fixture::new();
    let missing = fixture.root.join("not-created");
    fixture.executor.version();
    fixture.executor.directory(&missing);
    assert!(matches!(
        fixture
            .discover(
                UvExecutableSelection::Selected(fixture.executable.clone()),
                &missing
            )
            .await
            .unwrap(),
        UvScopeDiscovery::ToolStoreMissing { .. }
    ));
    assert!(!missing.exists());
    let file = fixture.root.join("store-file");
    fs::write(&file, b"not a store").unwrap();
    let dangling = fixture.root.join("store-symlink");
    symlink(&missing, &dangling).unwrap();
    for path in [&file, &dangling] {
        fixture.executor.version();
        fixture.executor.directory(path);
        assert!(
            fixture
                .discover(
                    UvExecutableSelection::Selected(fixture.executable.clone()),
                    path
                )
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn changed_executable_alias_is_rejected_before_read_without_fallback() {
    let fixture = Fixture::new();
    let alias = fixture.root.join("uv-alias");
    symlink(&fixture.executable, &alias).unwrap();
    fixture.executor.version();
    fixture.executor.directory(&fixture.store);
    let UvScopeDiscovery::Ready(scope) = fixture
        .discover(
            UvExecutableSelection::Selected(alias.clone()),
            &fixture.store,
        )
        .await
        .unwrap()
    else {
        panic!("ready")
    };
    fs::rename(&alias, fixture.root.join("old-alias")).unwrap();
    let other = fixture.root.join("other/uv");
    executable_file(&other);
    symlink(&other, &alias).unwrap();
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(UvToolReadAdapter::new(
        ProcessUvToolSource::new(fixture.executor.clone(), scope.context().clone()),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();
    assert!(
        runtime
            .submit_refresh_request_response(
                ManagerId::Uv,
                AdapterRequest::ListInstalled(ListInstalledRequest)
            )
            .await
            .is_err()
    );
    assert_eq!(fixture.executor.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn configured_store_alias_stays_bound_when_uv_reports_a_canonical_directory() {
    let fixture = Fixture::new();
    let alias = fixture.root.join("configured-store");
    symlink(&fixture.store, &alias).unwrap();
    fixture.executor.version();
    fixture
        .executor
        .directory(&fixture.store.canonicalize().unwrap());
    let UvScopeDiscovery::Ready(scope) = fixture
        .discover(
            UvExecutableSelection::Selected(fixture.executable.clone()),
            &alias,
        )
        .await
        .unwrap()
    else {
        panic!("ready")
    };
    fs::rename(&alias, fixture.root.join("original-store-alias")).unwrap();
    let other = fixture.root.join("another-store");
    fs::create_dir(&other).unwrap();
    symlink(other, alias).unwrap();
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(UvToolReadAdapter::new(
        ProcessUvToolSource::new(fixture.executor.clone(), scope.context().clone()),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();
    assert!(
        runtime
            .submit_refresh_request_response(
                ManagerId::Uv,
                AdapterRequest::ListInstalled(ListInstalledRequest)
            )
            .await
            .is_err()
    );
    assert_eq!(fixture.executor.requests.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn store_replacement_during_read_cannot_clear_committed_inventory() {
    let fixture = Fixture::new();
    let scope = fixture.ready().await;
    let store = Arc::new(SqliteStore::new(fixture.root.join("test.sqlite")));
    store.migrate_to_latest().unwrap();
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(UvToolReadAdapter::new(
        ProcessUvToolSource::new(fixture.executor.clone(), scope.context().clone()),
    ));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();
    fixture.executor.version();
    fixture
        .executor
        .push(output("black v24.2.0\n- black\n", ""));
    assert!(matches!(
        persisted_read(&runtime).await,
        AdapterTaskTerminalState::Succeeded(_)
    ));
    let baseline = store.list_installed().unwrap();
    assert_eq!(baseline.len(), 1);
    fixture.executor.version();
    let original_store = fixture.store.clone();
    let moved_store = fixture.root.join("moved-store");
    fixture.executor.replies.lock().unwrap().push_back(Reply {
        result: output("", "No tools installed\n"),
        after_wait: Some(Box::new(move || {
            fs::rename(&original_store, moved_store).unwrap();
            fs::create_dir(&original_store).unwrap();
        })),
    });
    assert!(matches!(
        persisted_read(&runtime).await,
        AdapterTaskTerminalState::Failed(_)
    ));
    assert_eq!(store.list_installed().unwrap(), baseline);
    let requests = fixture.executor.requests.lock().unwrap().len();
    assert!(matches!(
        persisted_read(&runtime).await,
        AdapterTaskTerminalState::Failed(_)
    ));
    assert_eq!(fixture.executor.requests.lock().unwrap().len(), requests);
    assert_eq!(store.list_installed().unwrap(), baseline);
}

async fn persisted_read(runtime: &AdapterRuntime) -> AdapterTaskTerminalState {
    let (task, persistence) = runtime
        .submit_with_persistence(
            ManagerId::Uv,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let terminal = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    persistence.wait_for_completion().await;
    terminal.terminal_state.unwrap()
}

#[tokio::test]
#[ignore = "opt-in: explicit HELM_UV_CONTRACT_EXECUTABLE; discovers and reads only disposable stores"]
async fn real_uv_discovers_scoped_directory_and_reads_empty_inventory() {
    let executable = PathBuf::from(
        std::env::var_os("HELM_UV_CONTRACT_EXECUTABLE").expect("explicit uv executable"),
    );
    assert!(executable.is_absolute());
    let fixture = Fixture::new();
    let store = fixture.store.clone();
    let selected = executable.clone();
    let discovered = tokio::task::spawn_blocking(move || {
        UvToolDiscovery::new(Arc::new(TokioProcessExecutor))
            .discover(UvExecutableSelection::Selected(selected), Some(store))
    })
    .await
    .unwrap()
    .unwrap();
    let UvScopeDiscovery::Ready(scope) = discovered else {
        panic!("ready store")
    };
    assert_eq!(
        scope.context().tool_dir(),
        fixture.store.canonicalize().unwrap()
    );
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(UvToolReadAdapter::new(
        ProcessUvToolSource::new(Arc::new(TokioProcessExecutor), scope.context().clone()),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();
    runtime.set_network_available(false);
    assert!(
        matches!(runtime.submit_refresh_request_response(ManagerId::Uv, AdapterRequest::ListInstalled(ListInstalledRequest)).await.unwrap(), AdapterResponse::SnapshotSync { installed: Some(packages), outdated: None } if packages.is_empty())
    );
    let missing = fixture.root.join("never-created");
    let path = missing.clone();
    let discovered = tokio::task::spawn_blocking(move || {
        UvToolDiscovery::new(Arc::new(TokioProcessExecutor))
            .discover(UvExecutableSelection::Selected(executable), Some(path))
    })
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(
        discovered,
        UvScopeDiscovery::ToolStoreMissing { .. }
    ));
    assert!(!missing.exists());
}
