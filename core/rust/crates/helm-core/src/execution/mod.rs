use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{Duration, SystemTime};

use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskId, TaskType};

pub type ExecutionResult<T> = Result<T, CoreError>;

pub type ProcessWaitFuture = Pin<Box<dyn Future<Output = ExecutionResult<ProcessOutput>> + Send>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub working_dir: Option<PathBuf>,
}

impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            working_dir: None,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn working_dir(mut self, working_dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(working_dir.into());
        self
    }

    pub fn validate(
        &self,
        manager: ManagerId,
        task_type: TaskType,
        action: ManagerAction,
    ) -> ExecutionResult<()> {
        if self.program.as_os_str().is_empty() {
            return Err(invalid_input(
                manager,
                task_type,
                action,
                "command program path must not be empty",
            ));
        }

        if self
            .args
            .iter()
            .any(|arg| arg.is_empty() || arg.contains('\0'))
        {
            return Err(invalid_input(
                manager,
                task_type,
                action,
                "command args must be non-empty and must not contain NUL bytes",
            ));
        }

        if self
            .env
            .iter()
            .any(|(key, value)| key.is_empty() || key.contains('\0') || value.contains('\0'))
        {
            return Err(invalid_input(
                manager,
                task_type,
                action,
                "environment keys and values must be non-empty and must not contain NUL bytes",
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessSpawnRequest {
    pub manager: ManagerId,
    pub task_id: Option<TaskId>,
    pub task_type: TaskType,
    pub action: ManagerAction,
    pub command: CommandSpec,
    pub requires_elevation: bool,
    pub timeout: Option<Duration>,
    pub requested_at: SystemTime,
}

impl ProcessSpawnRequest {
    pub fn new(
        manager: ManagerId,
        task_type: TaskType,
        action: ManagerAction,
        command: CommandSpec,
    ) -> Self {
        Self {
            manager,
            task_id: None,
            task_type,
            action,
            command,
            requires_elevation: false,
            timeout: None,
            requested_at: SystemTime::now(),
        }
    }

    pub fn task_id(mut self, task_id: TaskId) -> Self {
        self.task_id = Some(task_id);
        self
    }

    pub fn requires_elevation(mut self, requires_elevation: bool) -> Self {
        self.requires_elevation = requires_elevation;
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn validate(&self) -> ExecutionResult<()> {
        self.command
            .validate(self.manager, self.task_type, self.action)?;

        if let Some(timeout) = self.timeout
            && timeout.is_zero()
        {
            return Err(invalid_input(
                self.manager,
                self.task_type,
                self.action,
                "timeout must be greater than zero when provided",
            ));
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessTerminationMode {
    Graceful { grace_period: Duration },
    Immediate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessExitStatus {
    ExitCode(i32),
    Terminated,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessOutput {
    pub status: ProcessExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
}

pub trait RunningProcess: Send + Sync {
    fn pid(&self) -> Option<u32>;

    fn terminate(&self, mode: ProcessTerminationMode) -> ExecutionResult<()>;

    fn wait(self: Box<Self>) -> ProcessWaitFuture;
}

pub trait ProcessExecutor: Send + Sync {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>>;
}

pub fn spawn_validated(
    executor: &dyn ProcessExecutor,
    request: ProcessSpawnRequest,
) -> ExecutionResult<Box<dyn RunningProcess>> {
    request.validate()?;
    executor.spawn(request)
}

fn invalid_input(
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    message: &str,
) -> CoreError {
    CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: Some(action),
        kind: CoreErrorKind::InvalidInput,
        message: message.to_string(),
    }
}
