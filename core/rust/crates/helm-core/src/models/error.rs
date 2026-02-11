use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::models::{ManagerAction, ManagerId, TaskType};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CoreErrorKind {
    NotInstalled,
    UnsupportedCapability,
    InvalidInput,
    ParseFailure,
    Timeout,
    Cancelled,
    ProcessFailure,
    StorageFailure,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreError {
    pub manager: Option<ManagerId>,
    pub task: Option<TaskType>,
    pub action: Option<ManagerAction>,
    pub kind: CoreErrorKind,
    pub message: String,
}

impl Display for CoreError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl Error for CoreError {}
