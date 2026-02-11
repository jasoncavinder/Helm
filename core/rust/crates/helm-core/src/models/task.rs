use std::time::SystemTime;

use crate::models::ManagerId;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TaskId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TaskType {
    Detection,
    Refresh,
    Search,
    Install,
    Uninstall,
    Upgrade,
    Pin,
    Unpin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TaskStatus {
    Queued,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRecord {
    pub id: TaskId,
    pub manager: ManagerId,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub created_at: SystemTime,
}
