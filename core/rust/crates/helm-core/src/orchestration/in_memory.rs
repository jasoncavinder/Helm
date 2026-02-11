use std::collections::HashMap;
use std::sync::Mutex;

use crate::models::{CoreError, CoreErrorKind, ManagerId, TaskId, TaskRecord, TaskStatus};
use crate::orchestration::{
    CancellationMode, OrchestrationResult, TaskCoordinator, TaskSubmission,
};

#[derive(Default)]
pub struct InMemoryTaskCoordinator {
    state: Mutex<CoordinatorState>,
}

#[derive(Default)]
struct CoordinatorState {
    next_task_id: u64,
    tasks: HashMap<TaskId, TaskRecord>,
    running_by_manager: HashMap<ManagerId, TaskId>,
}

impl InMemoryTaskCoordinator {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TaskCoordinator for InMemoryTaskCoordinator {
    fn enqueue(&self, submission: TaskSubmission) -> OrchestrationResult<TaskRecord> {
        let mut state = self.state.lock().map_err(|_| CoreError {
            manager: Some(submission.manager),
            task: Some(submission.task_type),
            action: None,
            kind: CoreErrorKind::Internal,
            message: "task coordinator mutex poisoned".to_string(),
        })?;

        let task_id = TaskId(state.next_task_id);
        state.next_task_id = state.next_task_id.saturating_add(1);

        let record = TaskRecord {
            id: task_id,
            manager: submission.manager,
            task_type: submission.task_type,
            status: TaskStatus::Queued,
            created_at: submission.requested_at,
        };
        state.tasks.insert(task_id, record.clone());

        Ok(record)
    }

    fn start(&self, task_id: TaskId) -> OrchestrationResult<()> {
        let mut state = self.lock_state()?;
        let (manager, task_type, status) = {
            let task = self.task_ref(&state, task_id)?;
            (task.manager, task.task_type, task.status)
        };

        if status != TaskStatus::Queued {
            return Err(invalid_task_transition(
                manager,
                task_type,
                task_id,
                status,
                TaskStatus::Running,
            ));
        }

        if state.running_by_manager.contains_key(&manager) {
            return Err(CoreError {
                manager: Some(manager),
                task: Some(task_type),
                action: None,
                kind: CoreErrorKind::InvalidInput,
                message: format!("manager '{manager:?}' already has a running task"),
            });
        }

        if let Some(task) = state.tasks.get_mut(&task_id) {
            task.status = TaskStatus::Running;
        }
        state.running_by_manager.insert(manager, task_id);

        Ok(())
    }

    fn complete(&self, task_id: TaskId) -> OrchestrationResult<()> {
        self.finish(task_id, TaskStatus::Completed)
    }

    fn fail(&self, task_id: TaskId, _message: String) -> OrchestrationResult<()> {
        self.finish(task_id, TaskStatus::Failed)
    }

    fn cancel(&self, task_id: TaskId, _mode: CancellationMode) -> OrchestrationResult<()> {
        let mut state = self.lock_state()?;
        let (manager, task_type, status) = {
            let task = self.task_ref(&state, task_id)?;
            (task.manager, task.task_type, task.status)
        };

        match status {
            TaskStatus::Queued | TaskStatus::Running => {
                if let Some(task) = state.tasks.get_mut(&task_id) {
                    task.status = TaskStatus::Cancelled;
                }
                state.running_by_manager.remove(&manager);
                Ok(())
            }
            _ => Err(invalid_task_transition(
                manager,
                task_type,
                task_id,
                status,
                TaskStatus::Cancelled,
            )),
        }
    }

    fn status(&self, task_id: TaskId) -> OrchestrationResult<TaskStatus> {
        let state = self.lock_state()?;
        Ok(self.task_ref(&state, task_id)?.status)
    }
}

impl InMemoryTaskCoordinator {
    fn finish(&self, task_id: TaskId, terminal: TaskStatus) -> OrchestrationResult<()> {
        let mut state = self.lock_state()?;
        let (manager, task_type, status) = {
            let task = self.task_ref(&state, task_id)?;
            (task.manager, task.task_type, task.status)
        };

        if status != TaskStatus::Running {
            return Err(invalid_task_transition(
                manager, task_type, task_id, status, terminal,
            ));
        }

        if let Some(task) = state.tasks.get_mut(&task_id) {
            task.status = terminal;
        }
        state.running_by_manager.remove(&manager);

        Ok(())
    }

    fn task_ref<'a>(
        &self,
        state: &'a CoordinatorState,
        task_id: TaskId,
    ) -> OrchestrationResult<&'a TaskRecord> {
        state.tasks.get(&task_id).ok_or_else(|| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::InvalidInput,
            message: format!("unknown task id '{}'", task_id.0),
        })
    }

    fn lock_state(&self) -> OrchestrationResult<std::sync::MutexGuard<'_, CoordinatorState>> {
        self.state.lock().map_err(|_| CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::Internal,
            message: "task coordinator mutex poisoned".to_string(),
        })
    }
}

fn invalid_task_transition(
    manager: ManagerId,
    task_type: crate::models::TaskType,
    task_id: TaskId,
    current: TaskStatus,
    desired: TaskStatus,
) -> CoreError {
    CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: None,
        kind: CoreErrorKind::InvalidInput,
        message: format!(
            "task '{}' cannot transition from '{current:?}' to '{desired:?}'",
            task_id.0
        ),
    }
}
