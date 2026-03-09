use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, InstallRequest, ManagerAdapter, MutationResult,
    RefreshRequest, SearchRequest, UninstallRequest,
};
use helm_core::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, OutdatedPackage, PackageRef,
    SearchQuery, TaskId, TaskRecord, TaskStatus, TaskType,
};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};
use helm_core::persistence::{DetectionStore, PackageStore, PersistenceResult, TaskStore};
use helm_core::sqlite::SqliteStore;

const TEST_CAPABILITIES: &[Capability] = &[Capability::Refresh, Capability::Search];
const MUTATION_CAPABILITIES: &[Capability] = &[Capability::Install, Capability::Uninstall];

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-{test_name}-{nanos}.sqlite3"))
}

#[derive(Clone)]
enum AdapterBehavior {
    Succeeds(AdapterResponse),
    Fails(CoreError),
}

struct TestAdapter {
    descriptor: ManagerDescriptor,
    behavior: AdapterBehavior,
}

impl TestAdapter {
    fn new(manager: ManagerId, behavior: AdapterBehavior) -> Self {
        Self::with_capabilities(manager, TEST_CAPABILITIES, behavior)
    }

    fn with_capabilities(
        manager: ManagerId,
        capabilities: &'static [Capability],
        behavior: AdapterBehavior,
    ) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id: manager,
                display_name: "test-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities,
            },
            behavior,
        }
    }
}

#[derive(Default)]
struct RecordingTaskStore {
    records: Mutex<HashMap<TaskId, TaskRecord>>,
    remaining_create_failures: Mutex<usize>,
}

impl RecordingTaskStore {
    fn failing_create() -> Self {
        Self::with_create_failures(5)
    }

    fn with_create_failures(failures: usize) -> Self {
        Self {
            records: Mutex::new(HashMap::new()),
            remaining_create_failures: Mutex::new(failures),
        }
    }

    fn get(&self, task_id: TaskId) -> Option<TaskRecord> {
        self.records.lock().ok()?.get(&task_id).cloned()
    }
}

impl TaskStore for RecordingTaskStore {
    fn create_task(&self, task: &TaskRecord) -> PersistenceResult<()> {
        {
            let mut remaining = self
                .remaining_create_failures
                .lock()
                .map_err(|_| CoreError {
                    manager: None,
                    task: None,
                    action: None,
                    kind: CoreErrorKind::Internal,
                    message: "recording store mutex poisoned".to_string(),
                })?;
            if *remaining > 0 {
                *remaining -= 1;
                return Err(CoreError {
                    manager: None,
                    task: None,
                    action: None,
                    kind: CoreErrorKind::StorageFailure,
                    message: "create_task forced failure".to_string(),
                });
            }
        }

        let mut records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        records.insert(task.id, task.clone());
        Ok(())
    }

    fn update_task(&self, task: &TaskRecord) -> PersistenceResult<()> {
        let mut records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        if !records.contains_key(&task.id) {
            return Err(CoreError {
                manager: None,
                task: None,
                action: None,
                kind: CoreErrorKind::StorageFailure,
                message: "task record not found".to_string(),
            });
        }
        records.insert(task.id, task.clone());
        Ok(())
    }

    fn list_recent_tasks(&self, limit: usize) -> PersistenceResult<Vec<TaskRecord>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        let mut values = records.values().cloned().collect::<Vec<_>>();
        values.sort_by_key(|record| std::cmp::Reverse(record.id.0));
        values.truncate(limit);
        Ok(values)
    }

    fn next_task_id(&self) -> PersistenceResult<u64> {
        let records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        Ok(records.keys().map(|id| id.0).max().map_or(0, |m| m + 1))
    }

    fn prune_completed_tasks(&self, _max_age_secs: i64) -> PersistenceResult<usize> {
        Ok(0)
    }

    fn delete_all_tasks(&self) -> PersistenceResult<()> {
        let mut records = self.records.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "recording store mutex poisoned".to_string(),
        })?;
        records.clear();
        Ok(())
    }
}

impl ManagerAdapter for TestAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, _request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        match &self.behavior {
            AdapterBehavior::Succeeds(response) => Ok(response.clone()),
            AdapterBehavior::Fails(error) => Err(error.clone()),
        }
    }
}

struct SequencedAdapter {
    descriptor: ManagerDescriptor,
    responses: Mutex<Vec<AdapterResult<AdapterResponse>>>,
    call_count: Arc<AtomicUsize>,
}

impl SequencedAdapter {
    fn new(
        manager: ManagerId,
        responses: Vec<AdapterResult<AdapterResponse>>,
        call_count: Arc<AtomicUsize>,
    ) -> Self {
        Self::with_capabilities(manager, TEST_CAPABILITIES, responses, call_count)
    }

    fn with_capabilities(
        manager: ManagerId,
        capabilities: &'static [Capability],
        responses: Vec<AdapterResult<AdapterResponse>>,
        call_count: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id: manager,
                display_name: "sequenced-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities,
            },
            responses: Mutex::new(responses),
            call_count,
        }
    }
}

impl ManagerAdapter for SequencedAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, _request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut responses = self.responses.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "sequenced adapter mutex poisoned".to_string(),
        })?;
        if responses.is_empty() {
            return Err(CoreError {
                manager: None,
                task: None,
                action: None,
                kind: CoreErrorKind::Internal,
                message: "no sequenced adapter response configured".to_string(),
            });
        }
        responses.remove(0)
    }
}

#[tokio::test]
async fn submit_routes_to_registered_adapter() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();

    let task_id = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();
    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);
    assert_eq!(
        snapshot.terminal_state,
        Some(AdapterTaskTerminalState::Succeeded(
            AdapterResponse::Refreshed
        ))
    );
}

#[tokio::test]
async fn submit_returns_structured_error_for_unregistered_manager() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();
    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "fd".to_string(),
            issued_at: SystemTime::now(),
        },
    });

    let error = runtime
        .submit(ManagerId::Pip, request)
        .await
        .expect_err("expected unregistered manager error");

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::Pip));
    assert_eq!(error.task, Some(TaskType::Search));
    assert_eq!(error.action, Some(ManagerAction::Search));
}

#[tokio::test]
async fn submit_returns_structured_error_for_disabled_manager() {
    let path = test_db_path("orchestration-runtime-disabled-manager");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();
    store.set_manager_enabled(ManagerId::Npm, false).unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();

    let error = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect_err("expected disabled manager error");

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::Npm));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
}

#[tokio::test]
async fn submit_returns_structured_error_for_ineligible_system_rubygems_manager() {
    let path = test_db_path("orchestration-runtime-ineligible-rubygems-manager");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();
    store
        .upsert_detection(
            ManagerId::RubyGems,
            &DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from("/usr/bin/gem")),
                version: Some("3.4.10".to_string()),
            },
        )
        .unwrap();
    store
        .set_manager_selected_executable_path(ManagerId::RubyGems, Some("/usr/bin/gem"))
        .unwrap();
    store
        .set_manager_enabled(ManagerId::RubyGems, true)
        .unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::RubyGems,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();

    let error = runtime
        .submit(ManagerId::RubyGems, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect_err("expected ineligible manager error");

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::RubyGems));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
}

#[tokio::test]
async fn submit_returns_structured_error_for_ineligible_system_pip_manager() {
    let path = test_db_path("orchestration-runtime-ineligible-pip-manager");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();
    store
        .upsert_detection(
            ManagerId::Pip,
            &DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from("/usr/bin/python3")),
                version: Some("3.9.6".to_string()),
            },
        )
        .unwrap();
    store
        .set_manager_selected_executable_path(ManagerId::Pip, Some("/usr/bin/python3"))
        .unwrap();
    store.set_manager_enabled(ManagerId::Pip, true).unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Pip,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();

    let error = runtime
        .submit(ManagerId::Pip, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect_err("expected ineligible manager error");

    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::Pip));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
}

#[tokio::test]
async fn duplicate_adapter_registration_is_rejected() {
    let first: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let second: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Fails(CoreError {
            manager: Some(ManagerId::Npm),
            task: Some(TaskType::Refresh),
            action: Some(ManagerAction::Refresh),
            kind: CoreErrorKind::Internal,
            message: "should not be used".to_string(),
        }),
    ));

    let error = AdapterRuntime::new([first, second])
        .err()
        .expect("expected duplicate manager error");
    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    assert_eq!(error.manager, Some(ManagerId::Npm));
}

#[tokio::test]
async fn submit_with_task_store_persists_queued_then_terminal_status() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let task_store = Arc::new(RecordingTaskStore::default());
    let runtime = AdapterRuntime::with_task_store([adapter], task_store.clone()).unwrap();

    let task_id = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();
    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    let mut persisted = None;
    for _ in 0..20 {
        if let Some(record) = task_store.get(task_id)
            && record.status == TaskStatus::Completed
        {
            persisted = Some(record);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let record = persisted.expect("expected completed persisted task record");
    assert_eq!(record.id, task_id);
    assert_eq!(record.manager, ManagerId::Npm);
    assert_eq!(record.task_type, TaskType::Refresh);
    assert_eq!(record.status, TaskStatus::Completed);
}

#[tokio::test]
async fn submit_returns_error_when_initial_task_persistence_fails() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let task_store = Arc::new(RecordingTaskStore::failing_create());
    let runtime = AdapterRuntime::with_task_store([adapter], task_store).unwrap();

    let error = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect_err("expected task store failure");

    assert_eq!(error.kind, CoreErrorKind::StorageFailure);
    assert_eq!(error.manager, Some(ManagerId::Npm));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
}

#[tokio::test]
async fn submit_retries_initial_task_persistence_and_succeeds_after_transient_failure() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Succeeds(AdapterResponse::Refreshed),
    ));
    let task_store = Arc::new(RecordingTaskStore::with_create_failures(1));
    let runtime = AdapterRuntime::with_task_store([adapter], task_store.clone()).unwrap();

    let task_id = runtime
        .submit(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect("transient create failure should be retried");
    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(1)))
        .await
        .expect("task should reach terminal state");

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    let mut persisted = None;
    for _ in 0..20 {
        if let Some(record) = task_store.get(task_id)
            && record.status == TaskStatus::Completed
        {
            persisted = Some(record);
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let record = persisted.expect("expected completed persisted task record");
    assert_eq!(record.id, task_id);
    assert_eq!(record.manager, ManagerId::Npm);
    assert_eq!(record.task_type, TaskType::Refresh);
}

#[tokio::test]
async fn submit_refresh_request_response_returns_attributed_failure() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::new(
        ManagerId::Npm,
        AdapterBehavior::Fails(CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::ProcessFailure,
            message: "forced adapter failure".to_string(),
        }),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();

    let error = runtime
        .submit_refresh_request_response(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect_err("expected attributed refresh failure");

    assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
    assert_eq!(error.manager, Some(ManagerId::Npm));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
}

#[tokio::test]
async fn submit_refresh_request_response_retries_once_on_timeout() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SequencedAdapter::new(
        ManagerId::Npm,
        vec![
            Err(CoreError {
                manager: None,
                task: None,
                action: None,
                kind: CoreErrorKind::Timeout,
                message: "operation timed out".to_string(),
            }),
            Ok(AdapterResponse::Refreshed),
        ],
        call_count.clone(),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();

    let response = runtime
        .submit_refresh_request_response(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect("expected transient timeout to be retried once");

    assert_eq!(response, AdapterResponse::Refreshed);
    assert_eq!(call_count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn submit_refresh_request_response_does_not_retry_parse_failure() {
    let call_count = Arc::new(AtomicUsize::new(0));
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SequencedAdapter::new(
        ManagerId::Npm,
        vec![
            Err(CoreError {
                manager: None,
                task: None,
                action: None,
                kind: CoreErrorKind::ParseFailure,
                message: "invalid parse payload".to_string(),
            }),
            Ok(AdapterResponse::Refreshed),
        ],
        call_count.clone(),
    ));
    let runtime = AdapterRuntime::new([adapter]).unwrap();

    let error = runtime
        .submit_refresh_request_response(ManagerId::Npm, AdapterRequest::Refresh(RefreshRequest))
        .await
        .expect_err("expected parse failure without retry");

    assert_eq!(error.kind, CoreErrorKind::ParseFailure);
    assert_eq!(error.manager, Some(ManagerId::Npm));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
    assert_eq!(call_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn refresh_all_ordered_recomputes_enablement_after_preference_update() {
    let path = test_db_path("orchestration-runtime-enablement-refresh-invalidation");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();
    store.set_manager_enabled(ManagerId::Npm, true).unwrap();
    store
        .upsert_detection(
            ManagerId::Npm,
            &DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/npm")),
                version: Some("10.9.0".to_string()),
            },
        )
        .unwrap();

    let call_count = Arc::new(AtomicUsize::new(0));
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SequencedAdapter::with_capabilities(
        ManagerId::Npm,
        &[Capability::ListInstalled],
        vec![
            Ok(AdapterResponse::InstalledPackages(vec![])),
            Ok(AdapterResponse::InstalledPackages(vec![])),
        ],
        call_count.clone(),
    ));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();

    let first_results = runtime.refresh_all_ordered().await;
    assert_eq!(first_results.len(), 1);
    assert!(first_results[0].1.is_ok());
    assert_eq!(call_count.load(Ordering::SeqCst), 1);

    store.set_manager_enabled(ManagerId::Npm, false).unwrap();

    let second_results = runtime.refresh_all_ordered().await;
    assert_eq!(second_results.len(), 1);
    assert!(second_results[0].1.is_ok());
    assert_eq!(
        call_count.load(Ordering::SeqCst),
        1,
        "disabled manager should be skipped after preference update"
    );
}

#[tokio::test]
async fn detect_persists_install_instances_alongside_detection_rows() {
    let path = test_db_path("orchestration-runtime-detect-install-instances");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();
    let rustup_path = std::env::temp_dir().join(format!(
        "helm-rustup-test-{}-{}.bin",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos()
    ));
    std::fs::write(&rustup_path, b"#!/bin/sh\nexit 0\n").unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::with_capabilities(
        ManagerId::Rustup,
        &[Capability::Detect, Capability::Refresh],
        AdapterBehavior::Succeeds(AdapterResponse::Detection(DetectionInfo {
            installed: true,
            executable_path: Some(rustup_path.clone()),
            version: Some("1.28.2".to_string()),
        })),
    ));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();

    runtime
        .submit_refresh_request_response(ManagerId::Rustup, AdapterRequest::Refresh(RefreshRequest))
        .await
        .unwrap();

    let mut persisted = false;
    for _ in 0..20 {
        let detections = store.list_detections().unwrap();
        let instances = store
            .list_install_instances(Some(ManagerId::Rustup))
            .unwrap();
        if detections
            .iter()
            .any(|(manager, info)| *manager == ManagerId::Rustup && info.installed)
            && !instances.is_empty()
        {
            persisted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(
        persisted,
        "expected rustup detection and install instances to persist"
    );

    let _ = std::fs::remove_file(rustup_path);
}

#[tokio::test]
async fn install_mutation_updates_cached_snapshots_without_manual_refresh() {
    let path = test_db_path("orchestration-runtime-install-snapshot");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    let package = PackageRef {
        manager: ManagerId::Npm,
        name: "eslint".to_string(),
    };
    store
        .upsert_outdated(&[OutdatedPackage {
            package: package.clone(),
            installed_version: Some("9.24.0".to_string()),
            candidate_version: "9.25.0".to_string(),
            pinned: false,
            restart_required: false,
            runtime_state: Default::default(),
        }])
        .unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::with_capabilities(
        ManagerId::Npm,
        MUTATION_CAPABILITIES,
        AdapterBehavior::Succeeds(AdapterResponse::Mutation(MutationResult {
            package: package.clone(),
            action: ManagerAction::Install,
            before_version: Some("9.24.0".to_string()),
            after_version: Some("9.25.0".to_string()),
        })),
    ));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();

    runtime
        .submit_refresh_request_response(
            ManagerId::Npm,
            AdapterRequest::Install(InstallRequest {
                package: package.clone(),
                version: Some("9.25.0".to_string()),
            }),
        )
        .await
        .unwrap();

    let mut persisted = false;
    for _ in 0..20 {
        let installed = store.list_installed().unwrap();
        let outdated = store.list_outdated().unwrap();
        let installed_entry = installed.iter().find(|entry| entry.package == package);
        if installed_entry.and_then(|entry| entry.installed_version.as_deref()) == Some("9.25.0")
            && outdated.iter().all(|entry| entry.package != package)
        {
            persisted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(
        persisted,
        "expected install mutation to refresh cached installed/outdated snapshots"
    );
}

#[tokio::test]
async fn uninstall_mutation_removes_cached_snapshots_without_manual_refresh() {
    let path = test_db_path("orchestration-runtime-uninstall-snapshot");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    let package = PackageRef {
        manager: ManagerId::Pnpm,
        name: "typescript".to_string(),
    };
    store
        .upsert_installed(&[helm_core::models::InstalledPackage {
            package: package.clone(),
            installed_version: Some("5.8.3".to_string()),
            pinned: false,
            runtime_state: Default::default(),
        }])
        .unwrap();
    store
        .upsert_outdated(&[OutdatedPackage {
            package: package.clone(),
            installed_version: Some("5.8.3".to_string()),
            candidate_version: "5.9.0".to_string(),
            pinned: false,
            restart_required: false,
            runtime_state: Default::default(),
        }])
        .unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(TestAdapter::with_capabilities(
        ManagerId::Pnpm,
        MUTATION_CAPABILITIES,
        AdapterBehavior::Succeeds(AdapterResponse::Mutation(MutationResult {
            package: package.clone(),
            action: ManagerAction::Uninstall,
            before_version: Some("5.8.3".to_string()),
            after_version: None,
        })),
    ));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();

    runtime
        .submit_refresh_request_response(
            ManagerId::Pnpm,
            AdapterRequest::Uninstall(UninstallRequest {
                package: package.clone(),
                version: None,
            }),
        )
        .await
        .unwrap();

    let mut persisted = false;
    for _ in 0..20 {
        let installed = store.list_installed().unwrap();
        let outdated = store.list_outdated().unwrap();
        if installed.iter().all(|entry| entry.package != package)
            && outdated.iter().all(|entry| entry.package != package)
        {
            persisted = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(
        persisted,
        "expected uninstall mutation to remove package from cached installed/outdated snapshots"
    );
}
