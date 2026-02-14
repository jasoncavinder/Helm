use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, ListInstalledRequest, ListOutdatedRequest,
    ManagerAdapter,
};
use crate::models::{
    Capability, CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskId, TaskRecord, TaskStatus,
    TaskType,
};
use crate::orchestration::{
    AdapterExecutionRuntime, AdapterTaskSnapshot, AdapterTaskTerminalState, CancellationMode,
    OrchestrationResult,
};
use crate::persistence::{DetectionStore, PackageStore, SearchCacheStore, TaskStore};

#[derive(Clone)]
pub struct AdapterRuntime {
    execution: AdapterExecutionRuntime,
    adapters: Arc<HashMap<ManagerId, Arc<dyn ManagerAdapter>>>,
    task_store: Option<Arc<dyn TaskStore>>,
    package_store: Option<Arc<dyn PackageStore>>,
    search_cache_store: Option<Arc<dyn SearchCacheStore>>,
    detection_store: Option<Arc<dyn DetectionStore>>,
}

impl AdapterRuntime {
    pub fn new(
        adapters: impl IntoIterator<Item = Arc<dyn ManagerAdapter>>,
    ) -> OrchestrationResult<Self> {
        Self::with_execution(AdapterExecutionRuntime::new(), adapters)
    }

    pub fn with_execution(
        execution: AdapterExecutionRuntime,
        adapters: impl IntoIterator<Item = Arc<dyn ManagerAdapter>>,
    ) -> OrchestrationResult<Self> {
        Self::with_stores(execution, adapters, None, None)
    }

    pub fn with_task_store(
        adapters: impl IntoIterator<Item = Arc<dyn ManagerAdapter>>,
        task_store: Arc<dyn TaskStore>,
    ) -> OrchestrationResult<Self> {
        let start_id = task_store.next_task_id().unwrap_or(0);
        let queue = crate::orchestration::InMemoryAsyncTaskQueue::with_initial_id(start_id);
        Self::with_stores(
            AdapterExecutionRuntime::with_queue(queue),
            adapters,
            Some(task_store),
            None,
        )
    }

    pub fn with_stores(
        execution: AdapterExecutionRuntime,
        adapters: impl IntoIterator<Item = Arc<dyn ManagerAdapter>>,
        task_store: Option<Arc<dyn TaskStore>>,
        package_store: Option<Arc<dyn PackageStore>>,
    ) -> OrchestrationResult<Self> {
        Self::build(execution, adapters, task_store, package_store, None, None)
    }

    pub fn with_all_stores(
        adapters: impl IntoIterator<Item = Arc<dyn ManagerAdapter>>,
        task_store: Arc<dyn TaskStore>,
        package_store: Arc<dyn PackageStore>,
        search_cache_store: Arc<dyn SearchCacheStore>,
        detection_store: Arc<dyn DetectionStore>,
    ) -> OrchestrationResult<Self> {
        let start_id = task_store.next_task_id().unwrap_or(0);
        let queue = crate::orchestration::InMemoryAsyncTaskQueue::with_initial_id(start_id);
        Self::build(
            AdapterExecutionRuntime::with_queue(queue),
            adapters,
            Some(task_store),
            Some(package_store),
            Some(search_cache_store),
            Some(detection_store),
        )
    }

    fn build(
        execution: AdapterExecutionRuntime,
        adapters: impl IntoIterator<Item = Arc<dyn ManagerAdapter>>,
        task_store: Option<Arc<dyn TaskStore>>,
        package_store: Option<Arc<dyn PackageStore>>,
        search_cache_store: Option<Arc<dyn SearchCacheStore>>,
        detection_store: Option<Arc<dyn DetectionStore>>,
    ) -> OrchestrationResult<Self> {
        let mut mapped = HashMap::new();
        for adapter in adapters {
            let manager = adapter.descriptor().id;
            if mapped.insert(manager, adapter).is_some() {
                return Err(CoreError {
                    manager: Some(manager),
                    task: None,
                    action: None,
                    kind: CoreErrorKind::InvalidInput,
                    message: format!("duplicate adapter registration for manager '{manager:?}'"),
                });
            }
        }

        Ok(Self {
            execution,
            adapters: Arc::new(mapped),
            task_store,
            package_store,
            search_cache_store,
            detection_store,
        })
    }

    pub fn has_manager(&self, manager: ManagerId) -> bool {
        self.adapters.contains_key(&manager)
    }

    pub fn adapter_list(&self) -> Vec<Arc<dyn ManagerAdapter>> {
        self.adapters.values().cloned().collect()
    }

    pub fn is_manager_enabled(&self, manager: ManagerId) -> bool {
        if let Some(ds) = &self.detection_store {
            match ds.list_manager_preferences() {
                Ok(prefs) => {
                    for (m, enabled) in prefs {
                        if m == manager {
                            return enabled;
                        }
                    }
                    true // default: enabled
                }
                Err(_) => true,
            }
        } else {
            true
        }
    }

    pub fn is_safe_mode(&self) -> bool {
        if let Some(ds) = &self.detection_store {
            ds.safe_mode().unwrap_or(false)
        } else {
            false
        }
    }

    pub async fn refresh_all_ordered(&self) -> Vec<(ManagerId, OrchestrationResult<()>)> {
        let adapter_refs: Vec<&dyn ManagerAdapter> =
            self.adapters.values().map(|a| a.as_ref()).collect();
        let phases = crate::orchestration::authority_order::authority_phases(&adapter_refs);

        let mut all_results = Vec::new();

        for phase in phases {
            let mut handles = Vec::new();

            for manager_id in &phase {
                let manager = *manager_id;

                // Skip managers that the user has disabled
                if !self.is_manager_enabled(manager) {
                    all_results.push((manager, Ok(())));
                    continue;
                }

                let Some(adapter) = self.adapters.get(&manager) else {
                    all_results.push((
                        manager,
                        Err(CoreError {
                            manager: Some(manager),
                            task: None,
                            action: None,
                            kind: CoreErrorKind::InvalidInput,
                            message: format!(
                                "manager '{manager:?}' is in execution phase but has no registered adapter"
                            ),
                        }),
                    ));
                    continue;
                };
                let supports_detect = adapter.descriptor().supports(Capability::Detect);
                let supports_list_installed =
                    adapter.descriptor().supports(Capability::ListInstalled);
                let supports_list_outdated =
                    adapter.descriptor().supports(Capability::ListOutdated);

                let runtime = self.clone();

                handles.push(tokio::spawn(async move {
                    if supports_detect {
                        // Detect first; skip refresh list actions when manager is not installed.
                        match runtime
                            .submit_refresh_request_response(
                                manager,
                                AdapterRequest::Detect(DetectRequest),
                            )
                            .await
                        {
                            Ok(AdapterResponse::Detection(info)) => {
                                if !info.installed {
                                    return vec![(manager, Ok(()))];
                                }
                            }
                            Ok(_) => {}
                            Err(e) => return vec![(manager, Err(e))],
                        }
                    }

                    if supports_list_installed
                        && let Err(e) = runtime
                            .submit_refresh_request(
                                manager,
                                AdapterRequest::ListInstalled(ListInstalledRequest),
                            )
                            .await
                    {
                        return vec![(manager, Err(e))];
                    }

                    if supports_list_outdated
                        && let Err(e) = runtime
                            .submit_refresh_request(
                                manager,
                                AdapterRequest::ListOutdated(ListOutdatedRequest),
                            )
                            .await
                    {
                        return vec![(manager, Err(e))];
                    }

                    vec![(manager, Ok(()))]
                }));
            }

            // Wait for all managers in this phase to complete
            for handle in handles {
                match handle.await {
                    Ok(results) => all_results.extend(results),
                    Err(_join_error) => {
                        // JoinError means the task panicked; we still continue with other phases
                    }
                }
            }
        }

        all_results
    }

    pub async fn submit_refresh_request(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
    ) -> OrchestrationResult<()> {
        let _ = self
            .submit_refresh_request_response(manager, request)
            .await?;
        Ok(())
    }

    pub async fn submit_refresh_request_response(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
    ) -> OrchestrationResult<AdapterResponse> {
        let task_id = self.submit(manager, request).await?;
        let snapshot = self
            .wait_for_terminal(task_id, Some(Duration::from_secs(60)))
            .await?;
        match snapshot.terminal_state {
            Some(AdapterTaskTerminalState::Succeeded(response)) => Ok(response),
            Some(AdapterTaskTerminalState::Failed(e)) => Err(e),
            Some(AdapterTaskTerminalState::Cancelled(e)) => Err(e.unwrap_or(CoreError {
                manager: Some(manager),
                task: None,
                action: None,
                kind: CoreErrorKind::Internal,
                message: "task was cancelled".to_string(),
            })),
            None => Err(CoreError {
                manager: Some(manager),
                task: None,
                action: None,
                kind: CoreErrorKind::Internal,
                message: "task reached terminal state with no result".to_string(),
            }),
        }
    }

    pub async fn submit(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
    ) -> OrchestrationResult<TaskId> {
        let action = request.action();
        let task_type = task_type_for_action(action);

        if manager == ManagerId::SoftwareUpdate
            && action == ManagerAction::Upgrade
            && self.is_safe_mode()
        {
            return Err(CoreError {
                manager: Some(manager),
                task: Some(task_type),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: "safe mode blocks macOS software update upgrades".to_string(),
            });
        }

        let adapter = self
            .adapters
            .get(&manager)
            .cloned()
            .ok_or_else(|| CoreError {
                manager: Some(manager),
                task: Some(task_type),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!("no adapter is registered for manager '{manager:?}'"),
            })?;

        let task_id = self.execution.submit(adapter, request).await?;

        if let Some(task_store) = &self.task_store {
            let record = TaskRecord {
                id: task_id,
                manager,
                task_type,
                status: TaskStatus::Queued,
                created_at: SystemTime::now(),
            };

            if let Err(error) =
                persist_create_task(task_store.clone(), record, manager, task_type, action).await
            {
                let _ = self
                    .execution
                    .cancel(task_id, CancellationMode::Immediate)
                    .await;
                return Err(error);
            }

            spawn_terminal_persistence_watcher(PersistenceWatcherContext {
                execution: self.execution.clone(),
                task_store: task_store.clone(),
                package_store: self.package_store.clone(),
                search_cache_store: self.search_cache_store.clone(),
                detection_store: self.detection_store.clone(),
                task_id,
                manager,
                task_type,
                action,
            });
        }

        Ok(task_id)
    }

    pub async fn status(&self, task_id: TaskId) -> OrchestrationResult<TaskStatus> {
        self.execution.status(task_id).await
    }

    pub async fn cancel(&self, task_id: TaskId, mode: CancellationMode) -> OrchestrationResult<()> {
        self.execution.cancel(task_id, mode).await
    }

    pub async fn snapshot(&self, task_id: TaskId) -> OrchestrationResult<AdapterTaskSnapshot> {
        self.execution.snapshot(task_id).await
    }

    pub async fn wait_for_terminal(
        &self,
        task_id: TaskId,
        timeout_duration: Option<Duration>,
    ) -> OrchestrationResult<AdapterTaskSnapshot> {
        self.execution
            .wait_for_terminal(task_id, timeout_duration)
            .await
    }
}

struct PersistenceWatcherContext {
    execution: AdapterExecutionRuntime,
    task_store: Arc<dyn TaskStore>,
    package_store: Option<Arc<dyn PackageStore>>,
    search_cache_store: Option<Arc<dyn SearchCacheStore>>,
    detection_store: Option<Arc<dyn DetectionStore>>,
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
}

fn spawn_terminal_persistence_watcher(ctx: PersistenceWatcherContext) {
    let PersistenceWatcherContext {
        execution,
        task_store,
        package_store,
        search_cache_store,
        detection_store,
        task_id,
        manager,
        task_type,
        action,
    } = ctx;

    tokio::spawn(async move {
        // Poll briefly for Running status and persist it so the UI sees the transition
        for _ in 0..50 {
            if let Ok(status) = execution.status(task_id).await {
                if status == TaskStatus::Running {
                    let running_record = TaskRecord {
                        id: task_id,
                        manager,
                        task_type,
                        status: TaskStatus::Running,
                        created_at: SystemTime::now(),
                    };
                    let _ = persist_update_task(
                        task_store.clone(),
                        running_record,
                        manager,
                        task_type,
                        action,
                    )
                    .await;
                    break;
                }
                if matches!(
                    status,
                    TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
                ) {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let terminal = execution.wait_for_terminal(task_id, None).await;
        let snapshot = match terminal {
            Ok(snapshot) => snapshot,
            Err(error) => {
                let attributed = attribute_error(error, manager, task_type, action);
                tracing::error!(
                    manager = ?manager,
                    task_id = task_id.0,
                    task_type = ?task_type,
                    action = ?action,
                    kind = ?attributed.kind,
                    message = %attributed.message,
                    "failed to wait for terminal task while persisting task record"
                );
                return;
            }
        };

        // Persist task result (domain data)
        if let Some(package_store) = package_store
            && let Some(AdapterTaskTerminalState::Succeeded(response)) = &snapshot.terminal_state
            && let Err(error) =
                persist_adapter_response(package_store, response, manager, task_type, action).await
        {
            tracing::error!(
                manager = ?manager,
                task_id = task_id.0,
                task_type = ?task_type,
                action = ?action,
                kind = ?error.kind,
                message = %error.message,
                "failed to persist adapter response data"
            );
        }

        // Persist search results to cache
        if let Some(search_cache_store) = search_cache_store
            && let Some(AdapterTaskTerminalState::Succeeded(response)) = &snapshot.terminal_state
            && let Err(error) =
                persist_search_response(search_cache_store, response, manager, task_type, action)
                    .await
        {
            tracing::error!(
                manager = ?manager,
                task_id = task_id.0,
                task_type = ?task_type,
                action = ?action,
                kind = ?error.kind,
                message = %error.message,
                "failed to persist search cache data"
            );
        }

        // Persist detection results
        if let Some(detection_store) = detection_store
            && let Some(AdapterTaskTerminalState::Succeeded(response)) = &snapshot.terminal_state
            && let Err(error) =
                persist_detection_response(detection_store, response, manager, task_type, action)
                    .await
        {
            tracing::error!(
                manager = ?manager,
                task_id = task_id.0,
                task_type = ?task_type,
                action = ?action,
                kind = ?error.kind,
                message = %error.message,
                "failed to persist detection data"
            );
        }

        let updated = TaskRecord {
            id: snapshot.runtime.id,
            manager: snapshot.runtime.manager,
            task_type: snapshot.runtime.task_type,
            status: snapshot.runtime.status,
            created_at: snapshot.runtime.created_at,
        };

        if let Err(error) = persist_update_task(
            task_store,
            updated,
            snapshot.runtime.manager,
            snapshot.runtime.task_type,
            action,
        )
        .await
        {
            tracing::error!(
                manager = ?manager,
                task_id = task_id.0,
                task_type = ?task_type,
                action = ?action,
                kind = ?error.kind,
                message = %error.message,
                "failed to persist terminal task status"
            );
        }
    });
}

async fn persist_adapter_response(
    package_store: Arc<dyn PackageStore>,
    response: &AdapterResponse,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> OrchestrationResult<()> {
    // Clone data for the blocking thread
    // This might be expensive for large lists, but necessary for thread safety across spawn_blocking
    let response = response.clone();

    tokio::task::spawn_blocking(move || {
        match response {
            AdapterResponse::InstalledPackages(packages) => {
                package_store.upsert_installed(&packages)
            }
            AdapterResponse::OutdatedPackages(packages) => {
                package_store.replace_outdated_snapshot(manager, &packages)
            }
            AdapterResponse::Mutation(mutation) => match mutation.action {
                ManagerAction::Pin => package_store.set_snapshot_pinned(&mutation.package, true),
                ManagerAction::Unpin => package_store.set_snapshot_pinned(&mutation.package, false),
                ManagerAction::Upgrade => package_store.apply_upgrade_result(&mutation.package),
                _ => Ok(()),
            },
            _ => Ok(()), // Other responses not persisted yet
        }
    })
    .await
    .map_err(|join_error| CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: Some(action),
        kind: CoreErrorKind::Internal,
        message: format!("response persistence join failure: {join_error}"),
    })?
    .map_err(|error| attribute_error(error, manager, task_type, action))
}

async fn persist_search_response(
    search_cache_store: Arc<dyn SearchCacheStore>,
    response: &AdapterResponse,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> OrchestrationResult<()> {
    let response = response.clone();

    tokio::task::spawn_blocking(move || match response {
        AdapterResponse::SearchResults(results) => {
            search_cache_store.upsert_search_results(&results)
        }
        _ => Ok(()),
    })
    .await
    .map_err(|join_error| CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: Some(action),
        kind: CoreErrorKind::Internal,
        message: format!("search cache persistence join failure: {join_error}"),
    })?
    .map_err(|error| attribute_error(error, manager, task_type, action))
}

async fn persist_detection_response(
    detection_store: Arc<dyn DetectionStore>,
    response: &AdapterResponse,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> OrchestrationResult<()> {
    let response = response.clone();

    tokio::task::spawn_blocking(move || match response {
        AdapterResponse::Detection(info) => detection_store.upsert_detection(manager, &info),
        _ => Ok(()),
    })
    .await
    .map_err(|join_error| CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: Some(action),
        kind: CoreErrorKind::Internal,
        message: format!("detection persistence join failure: {join_error}"),
    })?
    .map_err(|error| attribute_error(error, manager, task_type, action))
}

async fn persist_create_task(
    task_store: Arc<dyn TaskStore>,
    task_record: TaskRecord,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> OrchestrationResult<()> {
    tokio::task::spawn_blocking(move || task_store.create_task(&task_record))
        .await
        .map_err(|join_error| CoreError {
            manager: Some(manager),
            task: Some(task_type),
            action: Some(action),
            kind: CoreErrorKind::Internal,
            message: format!("task persistence join failure: {join_error}"),
        })?
        .map_err(|error| attribute_error(error, manager, task_type, action))
}

async fn persist_update_task(
    task_store: Arc<dyn TaskStore>,
    task_record: TaskRecord,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> OrchestrationResult<()> {
    tokio::task::spawn_blocking(move || task_store.update_task(&task_record))
        .await
        .map_err(|join_error| CoreError {
            manager: Some(manager),
            task: Some(task_type),
            action: Some(action),
            kind: CoreErrorKind::Internal,
            message: format!("task persistence join failure: {join_error}"),
        })?
        .map_err(|error| attribute_error(error, manager, task_type, action))
}

fn attribute_error(
    error: CoreError,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> CoreError {
    CoreError {
        manager: error.manager.or(Some(manager)),
        task: error.task.or(Some(task_type)),
        action: error.action.or(Some(action)),
        kind: error.kind,
        message: error.message,
    }
}

fn task_type_for_action(action: ManagerAction) -> TaskType {
    match action {
        ManagerAction::Detect => TaskType::Detection,
        ManagerAction::Refresh | ManagerAction::ListInstalled | ManagerAction::ListOutdated => {
            TaskType::Refresh
        }
        ManagerAction::Search => TaskType::Search,
        ManagerAction::Install => TaskType::Install,
        ManagerAction::Uninstall => TaskType::Uninstall,
        ManagerAction::Upgrade => TaskType::Upgrade,
        ManagerAction::Pin => TaskType::Pin,
        ManagerAction::Unpin => TaskType::Unpin,
    }
}
