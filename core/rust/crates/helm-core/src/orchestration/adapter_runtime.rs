use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::adapters::{AdapterRequest, ManagerAdapter};
use crate::models::{
    CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskId, TaskStatus, TaskType,
};
use crate::orchestration::{
    AdapterExecutionRuntime, AdapterTaskSnapshot, CancellationMode, OrchestrationResult,
};

#[derive(Clone)]
pub struct AdapterRuntime {
    execution: AdapterExecutionRuntime,
    adapters: Arc<HashMap<ManagerId, Arc<dyn ManagerAdapter>>>,
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
        let adapter = self
            .adapters
            .get(&manager)
            .cloned()
            .ok_or_else(|| CoreError {
                manager: Some(manager),
                task: Some(task_type_for_action(action)),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!("no adapter is registered for manager '{manager:?}'"),
            })?;

        self.execution.submit(adapter, request).await
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
