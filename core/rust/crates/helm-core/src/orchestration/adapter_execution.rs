use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::Mutex;

use crate::adapters::{
    AdapterRequest, AdapterResponse, ManagerAdapter, execute_with_capability_check,
};
use crate::models::{
    CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskId, TaskStatus, TaskType,
};
use crate::orchestration::{
    CancellationMode, InMemoryAsyncTaskQueue, OrchestrationResult, TaskCancellationToken,
    TaskOperation, TaskRuntimeSnapshot, TaskSubmission,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterTaskTerminalState {
    Succeeded(AdapterResponse),
    Failed(CoreError),
    Cancelled(Option<CoreError>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterTaskSnapshot {
    pub runtime: TaskRuntimeSnapshot,
    pub terminal_state: Option<AdapterTaskTerminalState>,
}

type OutcomeSlot = Arc<Mutex<Option<AdapterTaskTerminalState>>>;

#[derive(Clone, Default)]
pub struct AdapterExecutionRuntime {
    queue: InMemoryAsyncTaskQueue,
    outcomes: Arc<Mutex<HashMap<TaskId, OutcomeSlot>>>,
}

impl AdapterExecutionRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_queue(queue: InMemoryAsyncTaskQueue) -> Self {
        Self {
            queue,
            outcomes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn submit(
        &self,
        adapter: Arc<dyn ManagerAdapter>,
        request: AdapterRequest,
    ) -> OrchestrationResult<TaskId> {
        let manager = adapter.descriptor().id;
        let action = request.action();
        let task_type = task_type_for_action(action);
        let outcome_slot = Arc::new(Mutex::new(None));
        let operation_slot = outcome_slot.clone();

        let operation: TaskOperation = Box::new(move |token: TaskCancellationToken| {
            let adapter = adapter.clone();
            let request = request.clone();
            let operation_slot = operation_slot.clone();

            Box::pin(async move {
                if token.is_cancelled() {
                    let cancelled = cancelled_error(manager, task_type, action);
                    let mut slot = operation_slot.lock().await;
                    *slot = Some(AdapterTaskTerminalState::Cancelled(Some(cancelled.clone())));
                    return Err(cancelled);
                }

                let execute_result = tokio::task::spawn_blocking(move || {
                    execute_with_capability_check(adapter.as_ref(), request)
                })
                .await
                .map_err(|join_error| CoreError {
                    manager: Some(manager),
                    task: Some(task_type),
                    action: Some(action),
                    kind: CoreErrorKind::Internal,
                    message: format!("adapter execution join failure: {join_error}"),
                })?;

                match execute_result {
                    Ok(response) => {
                        let mut slot = operation_slot.lock().await;
                        *slot = Some(AdapterTaskTerminalState::Succeeded(response));
                        Ok(())
                    }
                    Err(error) => {
                        let attributed = attribute_error(error, manager, task_type, action);
                        let terminal = if attributed.kind == CoreErrorKind::Cancelled
                            || token.is_cancelled()
                        {
                            AdapterTaskTerminalState::Cancelled(Some(attributed.clone()))
                        } else {
                            AdapterTaskTerminalState::Failed(attributed.clone())
                        };
                        let mut slot = operation_slot.lock().await;
                        *slot = Some(terminal);
                        Err(attributed)
                    }
                }
            }) as Pin<Box<dyn Future<Output = OrchestrationResult<()>> + Send>>
        });

        let task_id = self
            .queue
            .spawn(
                TaskSubmission {
                    manager,
                    task_type,
                    requested_at: SystemTime::now(),
                },
                operation,
            )
            .await?;

        let mut outcomes = self.outcomes.lock().await;
        outcomes.insert(task_id, outcome_slot);
        Ok(task_id)
    }

    pub async fn status(&self, task_id: TaskId) -> OrchestrationResult<TaskStatus> {
        self.queue.status(task_id).await
    }

    pub async fn cancel(&self, task_id: TaskId, mode: CancellationMode) -> OrchestrationResult<()> {
        self.queue.cancel(task_id, mode).await
    }

    pub async fn snapshot(&self, task_id: TaskId) -> OrchestrationResult<AdapterTaskSnapshot> {
        let runtime = self.queue.snapshot(task_id).await?;
        let terminal_state = self.terminal_state_for(task_id, &runtime).await?;
        Ok(AdapterTaskSnapshot {
            runtime,
            terminal_state,
        })
    }

    pub async fn wait_for_terminal(
        &self,
        task_id: TaskId,
        timeout_duration: Option<Duration>,
    ) -> OrchestrationResult<AdapterTaskSnapshot> {
        let runtime = self
            .queue
            .wait_for_terminal(task_id, timeout_duration)
            .await?;
        let terminal_state = self.terminal_state_for(task_id, &runtime).await?;
        Ok(AdapterTaskSnapshot {
            runtime,
            terminal_state,
        })
    }

    async fn terminal_state_for(
        &self,
        task_id: TaskId,
        runtime: &TaskRuntimeSnapshot,
    ) -> OrchestrationResult<Option<AdapterTaskTerminalState>> {
        let status = runtime.status;
        let outcome_slot = {
            let outcomes = self.outcomes.lock().await;
            outcomes.get(&task_id).cloned()
        };

        if let Some(slot) = outcome_slot {
            let terminal = slot.lock().await.clone();
            if terminal.is_some() {
                return Ok(terminal);
            }
        }

        if status == TaskStatus::Cancelled {
            return Ok(Some(AdapterTaskTerminalState::Cancelled(None)));
        }

        if status == TaskStatus::Failed {
            return Ok(Some(AdapterTaskTerminalState::Failed(
                missing_terminal_state_error(task_id, runtime),
            )));
        }

        if status == TaskStatus::Completed {
            return Err(missing_terminal_state_error(task_id, runtime));
        }

        Ok(None)
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

fn cancelled_error(manager: ManagerId, task_type: TaskType, action: ManagerAction) -> CoreError {
    CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: Some(action),
        kind: CoreErrorKind::Cancelled,
        message: "task cancelled before adapter execution".to_string(),
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

fn missing_terminal_state_error(task_id: TaskId, runtime: &TaskRuntimeSnapshot) -> CoreError {
    CoreError {
        manager: Some(runtime.manager),
        task: Some(runtime.task_type),
        action: Some(default_action_for_task_type(runtime.task_type)),
        kind: CoreErrorKind::Internal,
        message: format!(
            "task '{}' reached terminal status '{status:?}' without adapter terminal state",
            task_id.0,
            status = runtime.status
        ),
    }
}

fn default_action_for_task_type(task_type: TaskType) -> ManagerAction {
    match task_type {
        TaskType::Detection => ManagerAction::Detect,
        TaskType::Refresh => ManagerAction::Refresh,
        TaskType::Search => ManagerAction::Search,
        TaskType::Install => ManagerAction::Install,
        TaskType::Uninstall => ManagerAction::Uninstall,
        TaskType::Upgrade => ManagerAction::Upgrade,
        TaskType::Pin => ManagerAction::Pin,
        TaskType::Unpin => ManagerAction::Unpin,
    }
}

#[cfg(test)]
mod tests {
    use super::task_type_for_action;
    use crate::models::{ManagerAction, TaskType};

    #[test]
    fn list_actions_map_to_refresh_task_type() {
        assert_eq!(
            task_type_for_action(ManagerAction::ListInstalled),
            TaskType::Refresh
        );
        assert_eq!(
            task_type_for_action(ManagerAction::ListOutdated),
            TaskType::Refresh
        );
    }
}
