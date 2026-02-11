pub mod adapter_execution;
pub mod in_memory;
pub mod runtime_queue;

pub use adapter_execution::{
    AdapterExecutionRuntime, AdapterTaskSnapshot, AdapterTaskTerminalState,
};
pub use in_memory::InMemoryTaskCoordinator;
pub use runtime_queue::{
    InMemoryAsyncTaskQueue, TaskCancellationToken, TaskOperation, TaskRuntimeSnapshot,
};

use std::time::{Duration, SystemTime};

use crate::models::{CoreError, ManagerId, TaskId, TaskRecord, TaskStatus, TaskType};

pub type OrchestrationResult<T> = Result<T, CoreError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskSubmission {
    pub manager: ManagerId,
    pub task_type: TaskType,
    pub requested_at: SystemTime,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationMode {
    Immediate,
    Graceful { grace_period: Duration },
}

pub trait ConcurrencyPolicy: Send + Sync {
    fn can_run_together(&self, first_manager: ManagerId, second_manager: ManagerId) -> bool;
}

pub struct SerialPerManagerPolicy;

impl ConcurrencyPolicy for SerialPerManagerPolicy {
    fn can_run_together(&self, first_manager: ManagerId, second_manager: ManagerId) -> bool {
        first_manager != second_manager
    }
}

pub trait TaskCoordinator: Send + Sync {
    fn enqueue(&self, submission: TaskSubmission) -> OrchestrationResult<TaskRecord>;

    fn start(&self, task_id: TaskId) -> OrchestrationResult<()>;

    fn complete(&self, task_id: TaskId) -> OrchestrationResult<()>;

    fn fail(&self, task_id: TaskId, message: String) -> OrchestrationResult<()>;

    fn cancel(&self, task_id: TaskId, mode: CancellationMode) -> OrchestrationResult<()>;

    fn status(&self, task_id: TaskId) -> OrchestrationResult<TaskStatus>;
}
