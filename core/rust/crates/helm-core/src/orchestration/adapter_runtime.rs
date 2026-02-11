use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::adapters::{AdapterRequest, AdapterResponse, ManagerAdapter};
use crate::models::{
    CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskId, TaskRecord, TaskStatus, TaskType,
};
use crate::orchestration::{
    AdapterExecutionRuntime, AdapterTaskSnapshot, AdapterTaskTerminalState, CancellationMode,
    OrchestrationResult,
};
use crate::persistence::{PackageStore, TaskStore};

#[derive(Clone)]
pub struct AdapterRuntime {
    execution: AdapterExecutionRuntime,
    adapters: Arc<HashMap<ManagerId, Arc<dyn ManagerAdapter>>>,
    task_store: Option<Arc<dyn TaskStore>>,
    package_store: Option<Arc<dyn PackageStore>>,
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
        Self::with_stores(
            AdapterExecutionRuntime::new(),
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
        })
    }

    pub fn has_manager(&self, manager: ManagerId) -> bool {
        self.adapters.contains_key(&manager)
    }

    pub async fn submit(
        &self,
        manager: ManagerId,
        request: AdapterRequest,
    ) -> OrchestrationResult<TaskId> {
        let action = request.action();
        let task_type = task_type_for_action(action);
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

            spawn_terminal_persistence_watcher(
                self.execution.clone(),
                task_store.clone(),
                self.package_store.clone(),
                task_id,
                manager,
                task_type,
                action,
            );
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

fn spawn_terminal_persistence_watcher(
    execution: AdapterExecutionRuntime,
    task_store: Arc<dyn TaskStore>,
    package_store: Option<Arc<dyn PackageStore>>,
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) {
    tokio::spawn(async move {
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
        if let Some(package_store) = package_store {
            if let Some(AdapterTaskTerminalState::Succeeded(response)) = &snapshot.terminal_state {
                if let Err(error) = persist_adapter_response(
                    package_store,
                    response,
                    manager,
                    task_type,
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
                        "failed to persist adapter response data"
                    );
                }
            }
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
                package_store.upsert_outdated(&packages)
            }
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
