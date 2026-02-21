use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::models::{ManagerId, TaskId, TaskStatus, TaskType};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TaskLogRecord {
    pub id: u64,
    pub task_id: TaskId,
    pub manager: ManagerId,
    pub task_type: TaskType,
    pub status: Option<TaskStatus>,
    pub level: TaskLogLevel,
    pub message: String,
    pub created_at: SystemTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NewTaskLogRecord {
    pub task_id: TaskId,
    pub manager: ManagerId,
    pub task_type: TaskType,
    pub status: Option<TaskStatus>,
    pub level: TaskLogLevel,
    pub message: String,
    pub created_at: SystemTime,
}
