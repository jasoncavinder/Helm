use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use tracing::instrument;

use crate::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, ListInstalledRequest, ListOutdatedRequest,
    ManagerAdapter,
};
use crate::install_instances::collect_manager_install_instances;
use crate::manager_dependencies::provenance_requires_manager_dependency;
use crate::manager_policy::manager_enablement_eligibility;
use crate::models::{
    Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction, ManagerId,
    NewTaskLogRecord, TaskId, TaskLogLevel, TaskRecord, TaskStatus, TaskType,
};
use crate::post_install_setup::evaluate_manager_post_install_setup;
use crate::orchestration::{
    AdapterExecutionRuntime, AdapterTaskSnapshot, AdapterTaskTerminalState, CancellationMode,
    OrchestrationResult,
};
use crate::persistence::{
    DetectionStore, ManagerPreference, PackageStore, SearchCacheStore, TaskStore,
};

const TASK_PERSIST_RETRY_ATTEMPTS: usize = 3;
const TASK_PERSIST_RETRY_DELAY_MS: u64 = 15;
const DETECTION_SLOW_WARN_THRESHOLD_MS: u128 = 3_000;
const REFRESH_WAIT_POLICY_TIMEOUT_DETECTION_SECS: u64 = 90;
const REFRESH_WAIT_POLICY_TIMEOUT_SEARCH_SECS: u64 = 120;
const REFRESH_WAIT_POLICY_TIMEOUT_REFRESH_SECS: u64 = 180;
const REFRESH_WAIT_ORCHESTRATION_CAP_DETECTION_SECS: u64 = 120;
const REFRESH_WAIT_ORCHESTRATION_CAP_SEARCH_SECS: u64 = 180;
const REFRESH_WAIT_ORCHESTRATION_CAP_REFRESH_SECS: u64 = 300;

#[derive(Clone)]
pub struct AdapterRuntime {
    execution: AdapterExecutionRuntime,
    adapters: Arc<HashMap<ManagerId, Arc<dyn ManagerAdapter>>>,
    task_store: Option<Arc<dyn TaskStore>>,
    package_store: Option<Arc<dyn PackageStore>>,
    search_cache_store: Option<Arc<dyn SearchCacheStore>>,
    detection_store: Option<Arc<dyn DetectionStore>>,
}

#[derive(Clone, Debug, Default)]
struct ManagerEnablementSnapshot {
    enabled_by_manager: HashMap<ManagerId, bool>,
}

impl ManagerEnablementSnapshot {
    fn is_enabled(&self, manager: ManagerId) -> bool {
        self.enabled_by_manager
            .get(&manager)
            .copied()
            .unwrap_or(true)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RefreshCapabilityPlan {
    detect: bool,
    list_installed: bool,
    list_outdated: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RefreshWaitBudget {
    policy_timeout: Duration,
    orchestration_cap: Duration,
    effective_timeout: Duration,
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

    pub fn supports_capability(&self, manager: ManagerId, capability: Capability) -> bool {
        self.adapters
            .get(&manager)
            .map(|adapter| adapter.descriptor().supports(capability))
            .unwrap_or(false)
    }

    pub fn adapter_list(&self) -> Vec<Arc<dyn ManagerAdapter>> {
        self.adapters.values().cloned().collect()
    }

    pub fn is_manager_enabled(&self, manager: ManagerId) -> bool {
        let Some(ds) = &self.detection_store else {
            return true;
        };

        let prefs = match ds.list_manager_preferences() {
            Ok(value) => value,
            Err(_) => return true,
        };

        let mut enabled = true;
        let mut selected_executable_path: Option<PathBuf> = None;
        for pref in prefs {
            if pref.manager != manager {
                continue;
            }

            enabled = pref.enabled;
            selected_executable_path = pref
                .selected_executable_path
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(PathBuf::from);
            break;
        }
        if !enabled {
            return false;
        }

        let detected_executable_path = match ds.list_detections() {
            Ok(detections) => detections
                .into_iter()
                .find_map(|(id, info)| (id == manager).then_some(info.executable_path).flatten()),
            Err(_) => None,
        };

        let resolved_executable = selected_executable_path.or(detected_executable_path);
        let setup_required = ds
            .list_install_instances(Some(manager))
            .ok()
            .and_then(|instances| {
                evaluate_manager_post_install_setup(manager, Some(instances.as_slice()))
            })
            .is_some_and(|report| report.has_unmet_required());
        manager_enablement_eligibility(manager, resolved_executable.as_deref()).is_eligible
            && !setup_required
    }

    pub fn is_safe_mode(&self) -> bool {
        if let Some(ds) = &self.detection_store {
            ds.safe_mode().unwrap_or(false)
        } else {
            false
        }
    }

    fn manager_enablement_snapshot(&self) -> Option<Arc<ManagerEnablementSnapshot>> {
        let detection_store = self.detection_store.as_ref()?;
        let preferences = detection_store.list_manager_preferences().ok()?;
        let detections = detection_store.list_detections().ok()?;
        let enabled_by_manager =
            build_manager_enablement_map(self.adapters.keys().copied(), &preferences, &detections);
        Some(Arc::new(ManagerEnablementSnapshot { enabled_by_manager }))
    }

    fn manager_is_enabled_from_snapshot(
        &self,
        manager: ManagerId,
        snapshot: Option<&ManagerEnablementSnapshot>,
    ) -> bool {
        snapshot
            .map(|value| value.is_enabled(manager))
            .unwrap_or_else(|| self.is_manager_enabled(manager))
    }

    #[instrument(skip(self))]
    pub async fn detect_all_ordered(&self) -> Vec<(ManagerId, OrchestrationResult<()>)> {
        let adapter_refs: Vec<&dyn ManagerAdapter> =
            self.adapters.values().map(|a| a.as_ref()).collect();
        let phases = crate::orchestration::authority_order::detection_phases(&adapter_refs);

        let mut all_results = Vec::new();

        for phase in phases {
            let enablement_snapshot = self.manager_enablement_snapshot();
            let mut handles = Vec::new();

            for manager_id in &phase {
                let manager = *manager_id;

                if !self.manager_is_enabled_from_snapshot(manager, enablement_snapshot.as_deref()) {
                    all_results.push((manager, Ok(())));
                    continue;
                }

                let Some(adapter) = self.adapters.get(&manager) else {
                    all_results.push((manager, Err(missing_phase_adapter_error(manager))));
                    continue;
                };

                if !adapter.descriptor().supports(Capability::Detect) {
                    all_results.push((manager, Ok(())));
                    continue;
                }

                let runtime = self.clone();
                let enablement_snapshot = enablement_snapshot.clone();
                handles.push(tokio::spawn(async move {
                    let result = runtime
                        .submit_refresh_request_response_with_enablement(
                            manager,
                            AdapterRequest::Detect(DetectRequest),
                            enablement_snapshot.as_deref(),
                        )
                        .await;
                    vec![(manager, reduce_detect_request_result(result))]
                }));
            }

            for handle in handles {
                match handle.await {
                    Ok(results) => all_results.extend(results),
                    Err(_join_error) => {}
                }
            }
        }

        all_results
    }

    #[instrument(skip(self))]
    pub async fn refresh_all_ordered(&self) -> Vec<(ManagerId, OrchestrationResult<()>)> {
        let adapter_refs: Vec<&dyn ManagerAdapter> =
            self.adapters.values().map(|a| a.as_ref()).collect();
        let phases = crate::orchestration::authority_order::authority_phases(&adapter_refs);

        let mut all_results = Vec::new();

        for phase in phases {
            let enablement_snapshot = self.manager_enablement_snapshot();
            let mut handles = Vec::new();

            for manager_id in &phase {
                let manager = *manager_id;

                // Skip managers that the user has disabled
                if !self.manager_is_enabled_from_snapshot(manager, enablement_snapshot.as_deref()) {
                    all_results.push((manager, Ok(())));
                    continue;
                }

                let Some(adapter) = self.adapters.get(&manager) else {
                    all_results.push((manager, Err(missing_phase_adapter_error(manager))));
                    continue;
                };
                let capability_plan = refresh_capability_plan(adapter.as_ref());

                let runtime = self.clone();
                let enablement_snapshot = enablement_snapshot.clone();

                handles.push(tokio::spawn(async move {
                    if capability_plan.detect {
                        // Detect first; skip refresh list actions when manager is not installed.
                        match runtime
                            .submit_refresh_request_response_with_enablement(
                                manager,
                                AdapterRequest::Detect(DetectRequest),
                                enablement_snapshot.as_deref(),
                            )
                            .await
                        {
                            Ok(response) => {
                                if should_skip_refresh_lists_after_detect(&response) {
                                    return vec![(manager, Ok(()))];
                                }
                            }
                            Err(e) => return vec![(manager, Err(e))],
                        }
                    }

                    if capability_plan.list_installed
                        && let Err(e) = runtime
                            .submit_refresh_request_with_enablement(
                                manager,
                                AdapterRequest::ListInstalled(ListInstalledRequest),
                                enablement_snapshot.as_deref(),
                            )
                            .await
                    {
                        return vec![(manager, Err(e))];
                    }

                    if capability_plan.list_outdated
                        && let Err(e) = runtime
                            .submit_refresh_request_with_enablement(
                                manager,
                                AdapterRequest::ListOutdated(ListOutdatedRequest),
                                enablement_snapshot.as_deref(),
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

    #[instrument(skip(self, request), fields(manager = ?manager))]
    pub async fn submit_refresh_request(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
    ) -> OrchestrationResult<()> {
        self.submit_refresh_request_with_enablement(manager, request, None)
            .await
    }

    #[instrument(skip(self, request), fields(manager = ?manager))]
    pub async fn submit_refresh_request_response(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
    ) -> OrchestrationResult<AdapterResponse> {
        self.submit_refresh_request_response_with_enablement(manager, request, None)
            .await
    }

    async fn submit_refresh_request_with_enablement(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
        enablement_snapshot: Option<&ManagerEnablementSnapshot>,
    ) -> OrchestrationResult<()> {
        let _ = self
            .submit_refresh_request_response_with_enablement(manager, request, enablement_snapshot)
            .await?;
        Ok(())
    }

    async fn submit_refresh_request_response_with_enablement(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
        enablement_snapshot: Option<&ManagerEnablementSnapshot>,
    ) -> OrchestrationResult<AdapterResponse> {
        let action = request.action();
        let task_type = task_type_for_action(action);
        let wait_budget = refresh_wait_budget(manager, task_type);
        let mut attempt = 0u8;
        loop {
            attempt = attempt.saturating_add(1);
            let wait_started_at = SystemTime::now();
            let started_at = Instant::now();

            tracing::debug!(
                manager = ?manager,
                task_type = ?task_type,
                action = ?action,
                attempt,
                wait_started_at = ?wait_started_at,
                policy_timeout_ms = wait_budget.policy_timeout.as_millis(),
                orchestration_cap_ms = wait_budget.orchestration_cap.as_millis(),
                effective_timeout_ms = wait_budget.effective_timeout.as_millis(),
                "starting request-response orchestration attempt"
            );

            let task_id = self
                .submit_with_enablement(manager, request.clone(), enablement_snapshot)
                .await
                .map_err(|error| attribute_error(error, manager, task_type, action))?;

            let terminal_result = match self
                .wait_for_terminal(task_id, Some(wait_budget.effective_timeout))
                .await
            {
                Ok(snapshot) => {
                    if task_type == TaskType::Detection {
                        log_detection_timing(manager, task_id, started_at.elapsed(), &snapshot);
                    }

                    match snapshot.terminal_state {
                        Some(AdapterTaskTerminalState::Succeeded(response)) => {
                            tracing::debug!(
                                manager = ?manager,
                                task_type = ?task_type,
                                action = ?action,
                                task_id = task_id.0,
                                attempt,
                                elapsed_ms = started_at.elapsed().as_millis(),
                                status = ?snapshot.runtime.status,
                                terminal = "succeeded",
                                "request-response orchestration attempt completed"
                            );
                            Ok(response)
                        }
                        Some(AdapterTaskTerminalState::Failed(error)) => {
                            let attributed = attribute_error(error, manager, task_type, action);
                            tracing::warn!(
                                manager = ?manager,
                                task_type = ?task_type,
                                action = ?action,
                                task_id = task_id.0,
                                attempt,
                                elapsed_ms = started_at.elapsed().as_millis(),
                                status = ?snapshot.runtime.status,
                                terminal = "failed",
                                kind = ?attributed.kind,
                                message = %attributed.message,
                                "request-response orchestration attempt failed"
                            );
                            Err(attributed)
                        }
                        Some(AdapterTaskTerminalState::Cancelled(error)) => {
                            let cancellation_path = if error.is_some() {
                                "adapter_cancelled"
                            } else {
                                "runtime_cancelled"
                            };
                            let attributed = error.unwrap_or(CoreError {
                                manager: Some(manager),
                                task: Some(task_type),
                                action: Some(action),
                                kind: CoreErrorKind::Cancelled,
                                message: "task was cancelled".to_string(),
                            });
                            tracing::warn!(
                                manager = ?manager,
                                task_type = ?task_type,
                                action = ?action,
                                task_id = task_id.0,
                                attempt,
                                elapsed_ms = started_at.elapsed().as_millis(),
                                status = ?snapshot.runtime.status,
                                terminal = "cancelled",
                                cancellation_path,
                                kind = ?attributed.kind,
                                message = %attributed.message,
                                "request-response orchestration attempt cancelled"
                            );
                            Err(attributed)
                        }
                        None => {
                            let error = CoreError {
                                manager: Some(manager),
                                task: Some(task_type),
                                action: Some(action),
                                kind: CoreErrorKind::Internal,
                                message: "task reached terminal state with no result".to_string(),
                            };
                            tracing::error!(
                                manager = ?manager,
                                task_type = ?task_type,
                                action = ?action,
                                task_id = task_id.0,
                                attempt,
                                elapsed_ms = started_at.elapsed().as_millis(),
                                status = ?snapshot.runtime.status,
                                terminal = "missing_terminal_state",
                                "request-response orchestration attempt ended without terminal payload"
                            );
                            Err(error)
                        }
                    }
                }
                Err(error) => {
                    let attributed = attribute_error(error, manager, task_type, action);
                    tracing::warn!(
                        manager = ?manager,
                        task_type = ?task_type,
                        action = ?action,
                        task_id = task_id.0,
                        attempt,
                        elapsed_ms = started_at.elapsed().as_millis(),
                        terminal = "wait_error",
                        kind = ?attributed.kind,
                        message = %attributed.message,
                        "request-response orchestration wait failed"
                    );
                    Err(attributed)
                }
            };

            match terminal_result {
                Ok(response) => return Ok(response),
                Err(error)
                    if attempt < 2
                        && should_retry_transient_refresh_error(task_type, action, &error) =>
                {
                    tracing::warn!(
                        manager = ?manager,
                        task_type = ?task_type,
                        action = ?action,
                        kind = ?error.kind,
                        message = %error.message,
                        attempt = attempt,
                        max_attempts = 2,
                        "retrying transient refresh/search request once"
                    );
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
    }

    #[instrument(skip(self, request), fields(manager = ?manager))]
    pub async fn submit(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
    ) -> OrchestrationResult<TaskId> {
        self.submit_with_enablement(manager, request, None).await
    }

    async fn submit_with_enablement(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
        enablement_snapshot: Option<&ManagerEnablementSnapshot>,
    ) -> OrchestrationResult<TaskId> {
        let action = request.action();
        let task_type = task_type_for_action(action);

        if !self.manager_is_enabled_from_snapshot(manager, enablement_snapshot) {
            return Err(CoreError {
                manager: Some(manager),
                task: Some(task_type),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!("manager '{manager:?}' is disabled"),
            });
        }

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

            if let Err(error) = persist_append_task_log(
                task_store.clone(),
                NewTaskLogRecord {
                    task_id,
                    manager,
                    task_type,
                    status: Some(TaskStatus::Queued),
                    level: TaskLogLevel::Info,
                    message: "task queued".to_string(),
                    created_at: SystemTime::now(),
                },
                manager,
                task_type,
                action,
            )
            .await
            {
                tracing::warn!(
                    manager = ?manager,
                    task_id = task_id.0,
                    task_type = ?task_type,
                    action = ?action,
                    kind = ?error.kind,
                    message = %error.message,
                    "failed to persist queued task log"
                );
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
        let wait_started_at = SystemTime::now();
        let wait_started = Instant::now();
        let timeout_ms = timeout_duration.map(|value| value.as_millis() as u64);
        let result = self
            .execution
            .wait_for_terminal(task_id, timeout_duration)
            .await;

        match &result {
            Ok(snapshot) => {
                tracing::debug!(
                    task_id = task_id.0,
                    manager = ?snapshot.runtime.manager,
                    task_type = ?snapshot.runtime.task_type,
                    status = ?snapshot.runtime.status,
                    terminal = terminal_state_label(snapshot.terminal_state.as_ref()),
                    cancellation_path = cancellation_path_label(snapshot.terminal_state.as_ref()),
                    timeout_ms,
                    elapsed_ms = wait_started.elapsed().as_millis() as u64,
                    wait_started_at = ?wait_started_at,
                    "task reached terminal state"
                );
            }
            Err(error) => {
                tracing::warn!(
                    task_id = task_id.0,
                    manager = ?error.manager,
                    task = ?error.task,
                    kind = ?error.kind,
                    message = %error.message,
                    timeout_ms,
                    elapsed_ms = wait_started.elapsed().as_millis() as u64,
                    wait_started_at = ?wait_started_at,
                    "wait_for_terminal failed"
                );
            }
        }

        result
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

                    let _ = persist_append_task_log(
                        task_store.clone(),
                        NewTaskLogRecord {
                            task_id,
                            manager,
                            task_type,
                            status: Some(TaskStatus::Running),
                            level: TaskLogLevel::Info,
                            message: "task started".to_string(),
                            created_at: SystemTime::now(),
                        },
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
            // Use terminal timestamp so retention windows for completed/failed tasks
            // are measured from completion/failure, not from original queue time.
            created_at: SystemTime::now(),
        };

        if let Err(error) = persist_update_task(
            task_store.clone(),
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

        let terminal_status = snapshot.runtime.status;
        let terminal_error = terminal_error_details(&snapshot);
        let terminal_level = task_log_level_for_status(terminal_status);
        let terminal_message = task_log_message_for_status(terminal_status, terminal_error);

        if let Err(error) = persist_append_task_log(
            task_store.clone(),
            NewTaskLogRecord {
                task_id: snapshot.runtime.id,
                manager: snapshot.runtime.manager,
                task_type: snapshot.runtime.task_type,
                status: Some(terminal_status),
                level: terminal_level,
                message: terminal_message,
                created_at: SystemTime::now(),
            },
            snapshot.runtime.manager,
            snapshot.runtime.task_type,
            action,
        )
        .await
        {
            tracing::warn!(
                manager = ?manager,
                task_id = task_id.0,
                task_type = ?task_type,
                action = ?action,
                kind = ?error.kind,
                message = %error.message,
                "failed to persist terminal task log"
            );
        }

        let supplemental_notes = crate::execution::drain_task_log_notes(snapshot.runtime.id);
        for note in supplemental_notes {
            if let Err(error) = persist_append_task_log(
                task_store.clone(),
                NewTaskLogRecord {
                    task_id: snapshot.runtime.id,
                    manager: snapshot.runtime.manager,
                    task_type: snapshot.runtime.task_type,
                    status: Some(terminal_status),
                    level: TaskLogLevel::Info,
                    message: note,
                    created_at: SystemTime::now(),
                },
                snapshot.runtime.manager,
                snapshot.runtime.task_type,
                action,
            )
            .await
            {
                tracing::warn!(
                    manager = ?manager,
                    task_id = task_id.0,
                    task_type = ?task_type,
                    action = ?action,
                    kind = ?error.kind,
                    message = %error.message,
                    "failed to persist supplemental task log note"
                );
            }
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
                ManagerAction::Install => package_store
                    .apply_install_result(&mutation.package, mutation.after_version.as_deref()),
                ManagerAction::Uninstall => package_store.apply_uninstall_result(&mutation.package),
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
        AdapterResponse::Detection(info) => {
            detection_store.upsert_detection(manager, &info)?;
            let mut instances = collect_manager_install_instances(manager, &info);
            let selected_executable_path = detection_store
                .list_manager_preferences()?
                .into_iter()
                .find(|preference| preference.manager == manager)
                .and_then(|preference| normalize_nonempty(preference.selected_executable_path));
            let selected_path_update = reconcile_detected_install_instances(
                manager,
                &mut instances,
                selected_executable_path.as_deref(),
            );

            detection_store.replace_install_instances(manager, &instances)?;
            match selected_path_update {
                SelectedExecutablePathUpdate::Keep => Ok(()),
                SelectedExecutablePathUpdate::Set(path) => detection_store
                    .set_manager_selected_executable_path(manager, Some(path.as_str())),
                SelectedExecutablePathUpdate::Clear => {
                    detection_store.set_manager_selected_executable_path(manager, None)
                }
            }
        }
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
    persist_task_record_with_retry(
        task_store,
        task_record,
        manager,
        task_type,
        action,
        TaskStoreOperation::Create,
    )
    .await
}

async fn persist_update_task(
    task_store: Arc<dyn TaskStore>,
    task_record: TaskRecord,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> OrchestrationResult<()> {
    persist_task_record_with_retry(
        task_store,
        task_record,
        manager,
        task_type,
        action,
        TaskStoreOperation::Update,
    )
    .await
}

async fn persist_append_task_log(
    task_store: Arc<dyn TaskStore>,
    entry: NewTaskLogRecord,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> OrchestrationResult<()> {
    let mut remaining_attempts = TASK_PERSIST_RETRY_ATTEMPTS;
    loop {
        let store = task_store.clone();
        let log_entry = entry.clone();
        let op_result = tokio::task::spawn_blocking(move || store.append_task_log(&log_entry))
            .await
            .map_err(|join_error| CoreError {
                manager: Some(manager),
                task: Some(task_type),
                action: Some(action),
                kind: CoreErrorKind::Internal,
                message: format!("task log persistence join failure: {join_error}"),
            })?;

        match op_result {
            Ok(()) => return Ok(()),
            Err(error) => {
                let attributed = attribute_error(error, manager, task_type, action);
                remaining_attempts = remaining_attempts.saturating_sub(1);
                if remaining_attempts == 0 || attributed.kind != CoreErrorKind::StorageFailure {
                    return Err(attributed);
                }

                tokio::time::sleep(Duration::from_millis(TASK_PERSIST_RETRY_DELAY_MS)).await;
            }
        }
    }
}

fn task_log_level_for_status(status: TaskStatus) -> TaskLogLevel {
    match status {
        TaskStatus::Queued | TaskStatus::Running | TaskStatus::Completed => TaskLogLevel::Info,
        TaskStatus::Cancelled => TaskLogLevel::Warn,
        TaskStatus::Failed => TaskLogLevel::Error,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TaskTerminalErrorDetails {
    code: String,
    message: String,
}

fn terminal_error_details(snapshot: &AdapterTaskSnapshot) -> Option<TaskTerminalErrorDetails> {
    let from_terminal_state = match &snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Failed(error))
        | Some(AdapterTaskTerminalState::Cancelled(Some(error))) => Some(error.clone()),
        _ => None,
    };

    if let Some(error) = from_terminal_state {
        return Some(TaskTerminalErrorDetails {
            code: core_error_kind_code(error.kind).to_string(),
            message: error.message,
        });
    }

    snapshot
        .runtime
        .error_message
        .clone()
        .map(|message| TaskTerminalErrorDetails {
            code: "unknown".to_string(),
            message,
        })
}

fn core_error_kind_code(kind: CoreErrorKind) -> &'static str {
    match kind {
        CoreErrorKind::NotInstalled => "not_installed",
        CoreErrorKind::UnsupportedCapability => "unsupported_capability",
        CoreErrorKind::InvalidInput => "invalid_input",
        CoreErrorKind::ParseFailure => "parse_failure",
        CoreErrorKind::Timeout => "timeout",
        CoreErrorKind::Cancelled => "cancelled",
        CoreErrorKind::ProcessFailure => "process_failure",
        CoreErrorKind::StorageFailure => "storage_failure",
        CoreErrorKind::Internal => "internal",
    }
}

fn task_log_message_for_status(
    status: TaskStatus,
    error: Option<TaskTerminalErrorDetails>,
) -> String {
    match status {
        TaskStatus::Queued => "task queued".to_string(),
        TaskStatus::Running => "task started".to_string(),
        TaskStatus::Completed => "task completed".to_string(),
        TaskStatus::Cancelled => error
            .map(|details| format!("task cancelled [{}]: {}", details.code, details.message))
            .unwrap_or_else(|| "task cancelled".to_string()),
        TaskStatus::Failed => error
            .map(|details| format!("task failed [{}]: {}", details.code, details.message))
            .unwrap_or_else(|| "task failed".to_string()),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskStoreOperation {
    Create,
    Update,
}

async fn persist_task_record_with_retry(
    task_store: Arc<dyn TaskStore>,
    task_record: TaskRecord,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    operation: TaskStoreOperation,
) -> OrchestrationResult<()> {
    let mut remaining_attempts = TASK_PERSIST_RETRY_ATTEMPTS;
    loop {
        let store = task_store.clone();
        let record = task_record.clone();
        let op_result = tokio::task::spawn_blocking(move || match operation {
            TaskStoreOperation::Create => store.create_task(&record),
            TaskStoreOperation::Update => store.update_task(&record),
        })
        .await
        .map_err(|join_error| CoreError {
            manager: Some(manager),
            task: Some(task_type),
            action: Some(action),
            kind: CoreErrorKind::Internal,
            message: format!("task persistence join failure: {join_error}"),
        })?;

        match op_result {
            Ok(()) => return Ok(()),
            Err(error) => {
                let attributed = attribute_error(error, manager, task_type, action);
                remaining_attempts = remaining_attempts.saturating_sub(1);
                if remaining_attempts == 0 || attributed.kind != CoreErrorKind::StorageFailure {
                    return Err(attributed);
                }

                tokio::time::sleep(Duration::from_millis(TASK_PERSIST_RETRY_DELAY_MS)).await;
            }
        }
    }
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

fn missing_phase_adapter_error(manager: ManagerId) -> CoreError {
    CoreError {
        manager: Some(manager),
        task: None,
        action: None,
        kind: CoreErrorKind::InvalidInput,
        message: format!(
            "manager '{manager:?}' is in execution phase but has no registered adapter"
        ),
    }
}

fn reduce_detect_request_result(
    result: OrchestrationResult<AdapterResponse>,
) -> OrchestrationResult<()> {
    result.map(|_| ())
}

fn build_refresh_capability_plan(
    supports_detect: bool,
    supports_list_installed: bool,
    supports_list_outdated: bool,
) -> RefreshCapabilityPlan {
    RefreshCapabilityPlan {
        detect: supports_detect,
        list_installed: supports_list_installed,
        list_outdated: supports_list_outdated,
    }
}

fn refresh_capability_plan(adapter: &dyn ManagerAdapter) -> RefreshCapabilityPlan {
    build_refresh_capability_plan(
        adapter.descriptor().supports(Capability::Detect),
        adapter.descriptor().supports(Capability::ListInstalled),
        adapter.descriptor().supports(Capability::ListOutdated),
    )
}

fn should_skip_refresh_lists_after_detect(response: &AdapterResponse) -> bool {
    matches!(response, AdapterResponse::Detection(info) if !info.installed)
}

fn build_manager_enablement_map(
    managers: impl IntoIterator<Item = ManagerId>,
    preferences: &[ManagerPreference],
    detections: &[(ManagerId, DetectionInfo)],
) -> HashMap<ManagerId, bool> {
    let mut preferences_by_manager: HashMap<ManagerId, &ManagerPreference> = HashMap::new();
    for preference in preferences {
        preferences_by_manager
            .entry(preference.manager)
            .or_insert(preference);
    }

    let detections_by_manager: HashMap<ManagerId, Option<PathBuf>> = detections
        .iter()
        .map(|(manager, info)| (*manager, info.executable_path.clone()))
        .collect();

    let mut enabled_by_manager = HashMap::new();
    for manager in managers {
        let preference = preferences_by_manager.get(&manager).copied();
        let selected_executable_path = preference
            .and_then(|value| value.selected_executable_path.as_ref())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);

        let detected_executable_path = detections_by_manager.get(&manager).cloned().flatten();
        let resolved_executable = selected_executable_path.or(detected_executable_path);
        let enabled = preference.map(|value| value.enabled).unwrap_or(true)
            && manager_enablement_eligibility(manager, resolved_executable.as_deref()).is_eligible;
        enabled_by_manager.insert(manager, enabled);
    }

    enabled_by_manager
}

fn normalize_nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else if trimmed.len() == value.len() {
            Some(value)
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SelectedExecutablePathUpdate {
    Keep,
    Set(String),
    Clear,
}

fn reconcile_detected_install_instances(
    manager: ManagerId,
    instances: &mut [crate::models::ManagerInstallInstance],
    selected_executable_path: Option<&str>,
) -> SelectedExecutablePathUpdate {
    let normalized_selected = selected_executable_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if instances.is_empty() {
        return if normalized_selected.is_some() {
            SelectedExecutablePathUpdate::Clear
        } else {
            SelectedExecutablePathUpdate::Keep
        };
    }

    let selected_index = normalized_selected.as_deref().and_then(|value| {
        let selected_path = PathBuf::from(value);
        let selected_canonical = selected_path.canonicalize().ok();
        instances.iter().position(|instance| {
            instance_matches_selected_path(instance, &selected_path, selected_canonical.as_deref())
        })
    });

    let active_index = selected_index
        .or_else(|| instances.iter().position(|instance| instance.is_active))
        .unwrap_or_else(|| recommended_active_instance_index(manager, instances));

    for (index, instance) in instances.iter_mut().enumerate() {
        instance.is_active = index == active_index;
    }

    let resolved_path = instances[active_index]
        .display_path
        .to_string_lossy()
        .to_string();
    if normalized_selected.as_deref() == Some(resolved_path.as_str()) {
        SelectedExecutablePathUpdate::Keep
    } else {
        SelectedExecutablePathUpdate::Set(resolved_path)
    }
}

fn instance_matches_selected_path(
    instance: &crate::models::ManagerInstallInstance,
    selected_path: &std::path::Path,
    selected_canonical: Option<&std::path::Path>,
) -> bool {
    if instance.display_path == selected_path {
        return true;
    }
    if instance
        .alias_paths
        .iter()
        .any(|path| path == selected_path)
    {
        return true;
    }
    if let Some(canonical_path) = instance.canonical_path.as_deref()
        && canonical_path == selected_path
    {
        return true;
    }
    if let Some(selected_canonical) = selected_canonical {
        if instance
            .canonical_path
            .as_deref()
            .is_some_and(|path| path == selected_canonical)
        {
            return true;
        }
        if instance
            .display_path
            .canonicalize()
            .ok()
            .as_deref()
            .is_some_and(|path| path == selected_canonical)
        {
            return true;
        }
        if instance
            .alias_paths
            .iter()
            .filter_map(|path| path.canonicalize().ok())
            .any(|path| path == selected_canonical)
        {
            return true;
        }
    }
    false
}

fn recommended_active_instance_index(
    manager: ManagerId,
    instances: &[crate::models::ManagerInstallInstance],
) -> usize {
    instances
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| compare_instance_recommendation(manager, left, right))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn compare_instance_recommendation(
    manager: ManagerId,
    left: &crate::models::ManagerInstallInstance,
    right: &crate::models::ManagerInstallInstance,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left_bucket = recommendation_bucket(manager, left.provenance);
    let right_bucket = recommendation_bucket(manager, right.provenance);
    if left_bucket != right_bucket {
        return left_bucket.cmp(&right_bucket);
    }

    let left_dependency_rank = dependency_manager_rank(manager, left.provenance);
    let right_dependency_rank = dependency_manager_rank(manager, right.provenance);
    if left_dependency_rank != right_dependency_rank {
        return left_dependency_rank.cmp(&right_dependency_rank);
    }

    let left_class_rank = non_dependency_rank(manager, left.provenance);
    let right_class_rank = non_dependency_rank(manager, right.provenance);
    if left_class_rank != right_class_rank {
        return left_class_rank.cmp(&right_class_rank);
    }

    let left_automation_rank = automation_rank(left.automation_level);
    let right_automation_rank = automation_rank(right.automation_level);
    if left_automation_rank != right_automation_rank {
        return left_automation_rank.cmp(&right_automation_rank);
    }

    match right.confidence.partial_cmp(&left.confidence) {
        Some(Ordering::Equal) | None => {}
        Some(ordering) => return ordering,
    }

    left.display_path
        .to_string_lossy()
        .cmp(&right.display_path.to_string_lossy())
}

fn recommendation_bucket(manager: ManagerId, provenance: crate::models::InstallProvenance) -> u8 {
    if is_official_direct_provenance(manager, provenance) {
        return 0;
    }
    if provenance_requires_manager_dependency(manager, provenance) {
        return 2;
    }
    if provenance == crate::models::InstallProvenance::Unknown {
        return 3;
    }
    1
}

fn is_official_direct_provenance(
    manager: ManagerId,
    provenance: crate::models::InstallProvenance,
) -> bool {
    if provenance == crate::models::InstallProvenance::SourceBuild {
        return true;
    }
    matches!(
        (manager, provenance),
        (
            ManagerId::Rustup,
            crate::models::InstallProvenance::RustupInit
        ) | (ManagerId::Mise, crate::models::InstallProvenance::Mise)
    )
}

fn dependency_manager_rank(manager: ManagerId, provenance: crate::models::InstallProvenance) -> u8 {
    if !provenance_requires_manager_dependency(manager, provenance) {
        return u8::MAX / 2;
    }

    match provenance {
        crate::models::InstallProvenance::Homebrew => 0,
        crate::models::InstallProvenance::Macports => 1,
        crate::models::InstallProvenance::Nix => 2,
        crate::models::InstallProvenance::Asdf => 3,
        crate::models::InstallProvenance::Mise => 4,
        _ => 10,
    }
}

fn non_dependency_rank(manager: ManagerId, provenance: crate::models::InstallProvenance) -> u8 {
    match provenance {
        crate::models::InstallProvenance::RustupInit if manager == ManagerId::Rustup => 0,
        crate::models::InstallProvenance::Mise if manager == ManagerId::Mise => 1,
        crate::models::InstallProvenance::SourceBuild => 2,
        crate::models::InstallProvenance::System => 3,
        crate::models::InstallProvenance::EnterpriseManaged => 4,
        crate::models::InstallProvenance::Unknown => 99,
        _ => 10,
    }
}

fn automation_rank(level: crate::models::AutomationLevel) -> u8 {
    match level {
        crate::models::AutomationLevel::Automatic => 0,
        crate::models::AutomationLevel::NeedsConfirmation => 1,
        crate::models::AutomationLevel::ReadOnly => 2,
    }
}

fn should_retry_transient_refresh_error(
    task_type: TaskType,
    action: ManagerAction,
    error: &CoreError,
) -> bool {
    if !matches!(task_type, TaskType::Refresh | TaskType::Search) {
        return false;
    }
    if !matches!(
        action,
        ManagerAction::Refresh
            | ManagerAction::ListInstalled
            | ManagerAction::ListOutdated
            | ManagerAction::Search
    ) {
        return false;
    }
    if matches!(
        error.kind,
        CoreErrorKind::Cancelled
            | CoreErrorKind::UnsupportedCapability
            | CoreErrorKind::InvalidInput
            | CoreErrorKind::ParseFailure
    ) {
        return false;
    }
    if error.kind == CoreErrorKind::Timeout {
        return true;
    }

    let normalized = error.message.to_ascii_lowercase();
    normalized.contains("temporary failure in name resolution")
        || normalized.contains("name or service not known")
        || normalized.contains("failed to lookup address")
        || normalized.contains("could not resolve host")
        || normalized.contains("check your internet connection")
        || normalized.contains("network request failed")
        || normalized.contains("network is unreachable")
        || normalized.contains("connection timed out")
        || normalized.contains("operation timed out")
        || normalized.contains("timed out")
}

fn default_refresh_wait_policy_timeout(task_type: TaskType) -> Duration {
    match task_type {
        TaskType::Detection => Duration::from_secs(REFRESH_WAIT_POLICY_TIMEOUT_DETECTION_SECS),
        TaskType::Search => Duration::from_secs(REFRESH_WAIT_POLICY_TIMEOUT_SEARCH_SECS),
        TaskType::Refresh => Duration::from_secs(REFRESH_WAIT_POLICY_TIMEOUT_REFRESH_SECS),
        _ => Duration::from_secs(REFRESH_WAIT_POLICY_TIMEOUT_REFRESH_SECS),
    }
}

fn refresh_wait_orchestration_cap(task_type: TaskType) -> Duration {
    match task_type {
        TaskType::Detection => Duration::from_secs(REFRESH_WAIT_ORCHESTRATION_CAP_DETECTION_SECS),
        TaskType::Search => Duration::from_secs(REFRESH_WAIT_ORCHESTRATION_CAP_SEARCH_SECS),
        TaskType::Refresh => Duration::from_secs(REFRESH_WAIT_ORCHESTRATION_CAP_REFRESH_SECS),
        _ => Duration::from_secs(REFRESH_WAIT_ORCHESTRATION_CAP_REFRESH_SECS),
    }
}

fn refresh_wait_budget(manager: ManagerId, task_type: TaskType) -> RefreshWaitBudget {
    let policy_timeout = crate::execution::manager_timeout_profile(manager)
        .and_then(|profile| profile.hard_timeout)
        .unwrap_or_else(|| default_refresh_wait_policy_timeout(task_type));
    let orchestration_cap = refresh_wait_orchestration_cap(task_type);
    RefreshWaitBudget {
        policy_timeout,
        orchestration_cap,
        effective_timeout: policy_timeout.min(orchestration_cap),
    }
}

fn terminal_state_label(state: Option<&AdapterTaskTerminalState>) -> &'static str {
    match state {
        Some(AdapterTaskTerminalState::Succeeded(_)) => "succeeded",
        Some(AdapterTaskTerminalState::Failed(_)) => "failed",
        Some(AdapterTaskTerminalState::Cancelled(_)) => "cancelled",
        None => "none",
    }
}

fn cancellation_path_label(state: Option<&AdapterTaskTerminalState>) -> &'static str {
    match state {
        Some(AdapterTaskTerminalState::Cancelled(Some(_))) => "adapter_cancelled",
        Some(AdapterTaskTerminalState::Cancelled(None)) => "runtime_cancelled",
        _ => "n/a",
    }
}

fn log_detection_timing(
    manager: ManagerId,
    task_id: TaskId,
    elapsed: Duration,
    snapshot: &AdapterTaskSnapshot,
) {
    let elapsed_ms = elapsed.as_millis();
    match &snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            if elapsed_ms >= DETECTION_SLOW_WARN_THRESHOLD_MS {
                tracing::warn!(
                    manager = ?manager,
                    task_id = task_id.0,
                    elapsed_ms,
                    installed = info.installed,
                    version = ?info.version,
                    status = ?snapshot.runtime.status,
                    "manager detection completed slowly"
                );
            } else {
                tracing::info!(
                    manager = ?manager,
                    task_id = task_id.0,
                    elapsed_ms,
                    installed = info.installed,
                    version = ?info.version,
                    status = ?snapshot.runtime.status,
                    "manager detection completed"
                );
            }
        }
        Some(AdapterTaskTerminalState::Succeeded(_)) => {
            tracing::info!(
                manager = ?manager,
                task_id = task_id.0,
                elapsed_ms,
                status = ?snapshot.runtime.status,
                "manager detection completed with non-detection payload"
            );
        }
        Some(AdapterTaskTerminalState::Failed(error)) => {
            tracing::error!(
                manager = ?manager,
                task_id = task_id.0,
                elapsed_ms,
                kind = ?error.kind,
                message = %error.message,
                status = ?snapshot.runtime.status,
                "manager detection failed"
            );
        }
        Some(AdapterTaskTerminalState::Cancelled(Some(error))) => {
            tracing::warn!(
                manager = ?manager,
                task_id = task_id.0,
                elapsed_ms,
                kind = ?error.kind,
                message = %error.message,
                status = ?snapshot.runtime.status,
                "manager detection cancelled"
            );
        }
        Some(AdapterTaskTerminalState::Cancelled(None)) => {
            tracing::warn!(
                manager = ?manager,
                task_id = task_id.0,
                elapsed_ms,
                status = ?snapshot.runtime.status,
                "manager detection cancelled"
            );
        }
        None => {
            tracing::warn!(
                manager = ?manager,
                task_id = task_id.0,
                elapsed_ms,
                status = ?snapshot.runtime.status,
                "manager detection reached terminal state with no payload"
            );
        }
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

#[cfg(test)]
mod tests {
    use super::{
        SelectedExecutablePathUpdate, TaskType, build_manager_enablement_map,
        build_refresh_capability_plan, reconcile_detected_install_instances,
        reduce_detect_request_result, refresh_wait_budget, should_skip_refresh_lists_after_detect,
    };
    use crate::adapters::AdapterResponse;
    use crate::execution::{
        ManagerTimeoutProfile, clear_manager_timeout_profiles, set_manager_timeout_profile,
    };
    use crate::models::{
        AutomationLevel, CoreError, CoreErrorKind, DetectionInfo, InstallInstanceIdentityKind,
        InstallProvenance, ManagerId, ManagerInstallInstance, StrategyKind,
    };
    use crate::persistence::ManagerPreference;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    fn timeout_profile_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("timeout profile test lock should be available")
    }

    fn test_instance(
        manager: ManagerId,
        id: &str,
        path: &str,
        provenance: InstallProvenance,
    ) -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager,
            instance_id: id.to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: path.to_string(),
            display_path: PathBuf::from(path),
            canonical_path: None,
            alias_paths: vec![PathBuf::from(path)],
            is_active: false,
            version: Some("1.0.0".to_string()),
            provenance,
            confidence: 0.9,
            decision_margin: Some(0.2),
            automation_level: AutomationLevel::Automatic,
            uninstall_strategy: StrategyKind::InteractivePrompt,
            update_strategy: StrategyKind::InteractivePrompt,
            remediation_strategy: StrategyKind::ManualRemediation,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        }
    }

    #[test]
    fn refresh_wait_timeout_uses_default_policy_when_no_override_is_set() {
        let _guard = timeout_profile_test_guard();
        clear_manager_timeout_profiles();

        assert_eq!(
            refresh_wait_budget(ManagerId::Npm, TaskType::Detection).effective_timeout,
            Duration::from_secs(90)
        );
        assert_eq!(
            refresh_wait_budget(ManagerId::Npm, TaskType::Search).effective_timeout,
            Duration::from_secs(120)
        );
        assert_eq!(
            refresh_wait_budget(ManagerId::Npm, TaskType::Refresh).effective_timeout,
            Duration::from_secs(180)
        );
    }

    #[test]
    fn refresh_wait_timeout_clamps_policy_to_operation_cap() {
        let _guard = timeout_profile_test_guard();
        clear_manager_timeout_profiles();
        set_manager_timeout_profile(
            ManagerId::Npm,
            ManagerTimeoutProfile {
                hard_timeout: Some(Duration::from_secs(600)),
                idle_timeout: None,
            },
        );

        assert_eq!(
            refresh_wait_budget(ManagerId::Npm, TaskType::Detection).effective_timeout,
            Duration::from_secs(120)
        );
        assert_eq!(
            refresh_wait_budget(ManagerId::Npm, TaskType::Search).effective_timeout,
            Duration::from_secs(180)
        );
        assert_eq!(
            refresh_wait_budget(ManagerId::Npm, TaskType::Refresh).effective_timeout,
            Duration::from_secs(300)
        );

        clear_manager_timeout_profiles();
    }

    #[test]
    fn refresh_wait_timeout_respects_policy_when_below_cap() {
        let _guard = timeout_profile_test_guard();
        clear_manager_timeout_profiles();
        set_manager_timeout_profile(
            ManagerId::Npm,
            ManagerTimeoutProfile {
                hard_timeout: Some(Duration::from_secs(75)),
                idle_timeout: None,
            },
        );

        assert_eq!(
            refresh_wait_budget(ManagerId::Npm, TaskType::Refresh).effective_timeout,
            Duration::from_secs(75)
        );

        clear_manager_timeout_profiles();
    }

    #[test]
    fn build_refresh_capability_plan_reflects_support_flags() {
        assert_eq!(
            build_refresh_capability_plan(true, false, true),
            super::RefreshCapabilityPlan {
                detect: true,
                list_installed: false,
                list_outdated: true,
            }
        );
        assert_eq!(
            build_refresh_capability_plan(false, true, false),
            super::RefreshCapabilityPlan {
                detect: false,
                list_installed: true,
                list_outdated: false,
            }
        );
    }

    #[test]
    fn should_skip_refresh_lists_after_detect_only_for_not_installed_detection() {
        assert!(should_skip_refresh_lists_after_detect(
            &AdapterResponse::Detection(DetectionInfo {
                installed: false,
                executable_path: None,
                version: None,
            })
        ));
        assert!(!should_skip_refresh_lists_after_detect(
            &AdapterResponse::Detection(DetectionInfo {
                installed: true,
                executable_path: None,
                version: Some("1.0.0".to_string()),
            })
        ));
        assert!(!should_skip_refresh_lists_after_detect(
            &AdapterResponse::Refreshed
        ));
    }

    #[test]
    fn reduce_detect_request_result_maps_success_and_error() {
        assert!(reduce_detect_request_result(Ok(AdapterResponse::Refreshed)).is_ok());
        let error = CoreError {
            manager: Some(ManagerId::Npm),
            task: Some(TaskType::Detection),
            action: None,
            kind: CoreErrorKind::Timeout,
            message: "forced failure".to_string(),
        };
        let reduced = reduce_detect_request_result(Err(error.clone()))
            .expect_err("error should be forwarded unchanged");
        assert_eq!(reduced.kind, error.kind);
        assert_eq!(reduced.message, error.message);
    }

    #[test]
    fn build_manager_enablement_map_uses_selected_path_and_policy_eligibility() {
        let preferences = vec![ManagerPreference {
            manager: ManagerId::RubyGems,
            enabled: true,
            selected_executable_path: Some("/usr/bin/gem".to_string()),
            selected_install_method: None,
            timeout_hard_seconds: None,
            timeout_idle_seconds: None,
        }];
        let detections = vec![(
            ManagerId::RubyGems,
            DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/gem")),
                version: Some("3.5.0".to_string()),
            },
        )];

        let map = build_manager_enablement_map([ManagerId::RubyGems], &preferences, &detections);
        assert_eq!(map.get(&ManagerId::RubyGems), Some(&false));
    }

    #[test]
    fn build_manager_enablement_map_falls_back_to_detected_path_when_selected_missing() {
        let preferences = vec![ManagerPreference {
            manager: ManagerId::Pip,
            enabled: true,
            selected_executable_path: None,
            selected_install_method: None,
            timeout_hard_seconds: None,
            timeout_idle_seconds: None,
        }];
        let detections = vec![(
            ManagerId::Pip,
            DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from("/usr/bin/python3")),
                version: Some("3.9.6".to_string()),
            },
        )];

        let map = build_manager_enablement_map([ManagerId::Pip], &preferences, &detections);
        assert_eq!(map.get(&ManagerId::Pip), Some(&false));
    }

    #[test]
    fn build_manager_enablement_map_defaults_enabled_for_unknown_manager() {
        let map = build_manager_enablement_map(
            [ManagerId::Npm],
            &Vec::<ManagerPreference>::new(),
            &Vec::<(ManagerId, DetectionInfo)>::new(),
        );
        let expected = HashMap::from([(ManagerId::Npm, true)]);
        assert_eq!(map, expected);
    }

    #[test]
    fn reconcile_detected_instances_selects_rustup_init_when_selected_missing() {
        let mut instances = vec![
            test_instance(
                ManagerId::Rustup,
                "homebrew",
                "/opt/homebrew/bin/rustup",
                InstallProvenance::Homebrew,
            ),
            test_instance(
                ManagerId::Rustup,
                "rustup-init",
                "/Users/test/.cargo/bin/rustup",
                InstallProvenance::RustupInit,
            ),
        ];
        let update = reconcile_detected_install_instances(
            ManagerId::Rustup,
            &mut instances,
            Some("/tmp/removed-rustup"),
        );

        assert_eq!(
            update,
            SelectedExecutablePathUpdate::Set("/Users/test/.cargo/bin/rustup".to_string())
        );
        assert_eq!(instances[1].is_active, true);
        assert_eq!(instances[0].is_active, false);
    }

    #[test]
    fn reconcile_detected_instances_keeps_matching_selected_path() {
        let mut instances = vec![test_instance(
            ManagerId::Rustup,
            "rustup-init",
            "/Users/test/.cargo/bin/rustup",
            InstallProvenance::RustupInit,
        )];
        let update = reconcile_detected_install_instances(
            ManagerId::Rustup,
            &mut instances,
            Some("/Users/test/.cargo/bin/rustup"),
        );

        assert_eq!(update, SelectedExecutablePathUpdate::Keep);
        assert!(instances[0].is_active);
    }

    #[test]
    fn reconcile_detected_instances_clears_selected_path_when_none_detected() {
        let mut instances: Vec<ManagerInstallInstance> = Vec::new();
        let update = reconcile_detected_install_instances(
            ManagerId::Rustup,
            &mut instances,
            Some("/opt/homebrew/bin/rustup"),
        );
        assert_eq!(update, SelectedExecutablePathUpdate::Clear);
    }
}
