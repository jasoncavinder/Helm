pub mod task_output_store;
#[cfg(unix)]
pub mod tokio_process;

pub use task_output_store::TaskOutputRecord;
#[cfg(unix)]
pub use tokio_process::TokioProcessExecutor;

use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{OnceLock, RwLock};
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

static MANAGER_EXECUTABLE_OVERRIDES: OnceLock<
    RwLock<std::collections::HashMap<ManagerId, PathBuf>>,
> = OnceLock::new();

fn manager_executable_overrides() -> &'static RwLock<std::collections::HashMap<ManagerId, PathBuf>>
{
    MANAGER_EXECUTABLE_OVERRIDES.get_or_init(|| RwLock::new(std::collections::HashMap::new()))
}

fn manager_command_aliases(manager: ManagerId) -> &'static [&'static str] {
    match manager {
        ManagerId::HomebrewFormula | ManagerId::HomebrewCask => &["brew"],
        ManagerId::Mise => &["mise"],
        ManagerId::Asdf => &["asdf"],
        ManagerId::Rustup => &["rustup"],
        ManagerId::Npm => &["npm"],
        ManagerId::Pnpm => &["pnpm"],
        ManagerId::Yarn => &["yarn"],
        ManagerId::Cargo => &["cargo"],
        ManagerId::CargoBinstall => &["cargo-binstall", "cargo"],
        ManagerId::Pip => &["python3", "pip3", "pip"],
        ManagerId::Pipx => &["pipx"],
        ManagerId::Poetry => &["poetry"],
        ManagerId::RubyGems => &["gem"],
        ManagerId::Bundler => &["bundle", "gem"],
        ManagerId::MacPorts => &["port"],
        ManagerId::NixDarwin => &["darwin-rebuild", "nix-env", "nix"],
        ManagerId::Mas => &["mas"],
        ManagerId::DockerDesktop => &["docker"],
        ManagerId::Podman => &["podman"],
        ManagerId::Colima => &["colima"],
        ManagerId::XcodeCommandLineTools => &["xcode-select"],
        ManagerId::SoftwareUpdate => &["softwareupdate"],
        _ => &[],
    }
}

fn command_basename(path: &std::path::Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

fn apply_manager_executable_override(request: &mut ProcessSpawnRequest) {
    let selected = manager_executable_overrides()
        .read()
        .ok()
        .and_then(|guard| guard.get(&request.manager).cloned());
    let Some(selected_path) = selected else {
        return;
    };

    let aliases = manager_command_aliases(request.manager);
    if aliases.is_empty() {
        return;
    }

    let Some(current_name) = command_basename(request.command.program.as_path()) else {
        return;
    };
    if !aliases.contains(&current_name) {
        return;
    }

    let Some(selected_name) = command_basename(selected_path.as_path()) else {
        return;
    };
    if !aliases.contains(&selected_name) {
        return;
    }

    if selected_name == current_name {
        if selected_path.is_file() {
            request.command.program = selected_path;
        }
        return;
    }

    if let Some(parent) = selected_path.parent() {
        let sibling = parent.join(current_name);
        if sibling.is_file() {
            request.command.program = sibling;
        }
    }
}

pub fn set_manager_selected_executable(manager: ManagerId, path: Option<PathBuf>) {
    let Ok(mut guard) = manager_executable_overrides().write() else {
        return;
    };
    if let Some(path) = path {
        guard.insert(manager, path);
    } else {
        guard.remove(&manager);
    }
}

pub fn clear_manager_selected_executables() {
    if let Ok(mut guard) = manager_executable_overrides().write() {
        guard.clear();
    }
}

pub fn manager_selected_executable(manager: ManagerId) -> Option<PathBuf> {
    manager_executable_overrides()
        .read()
        .ok()
        .and_then(|guard| guard.get(&manager).cloned())
}

pub fn spawn_validated(
    executor: &dyn ProcessExecutor,
    mut request: ProcessSpawnRequest,
) -> ExecutionResult<Box<dyn RunningProcess>> {
    if request.task_id.is_none() {
        request.task_id = crate::task_context::current_task_id();
    }
    apply_manager_executable_override(&mut request);
    request.validate()?;
    executor.spawn(request)
}

pub fn task_output(task_id: TaskId) -> Option<TaskOutputRecord> {
    task_output_store::get(task_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct CapturingExecutor {
        request: Mutex<Option<ProcessSpawnRequest>>,
    }

    impl CapturingExecutor {
        fn captured_program(&self) -> PathBuf {
            self.request
                .lock()
                .expect("capture lock poisoned")
                .as_ref()
                .expect("expected captured request")
                .command
                .program
                .clone()
        }
    }

    impl ProcessExecutor for CapturingExecutor {
        fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
            *self.request.lock().expect("capture lock poisoned") = Some(request);
            Ok(Box::new(NoopRunningProcess))
        }
    }

    struct NoopRunningProcess;

    impl RunningProcess for NoopRunningProcess {
        fn pid(&self) -> Option<u32> {
            None
        }

        fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
            Ok(())
        }

        fn wait(self: Box<Self>) -> ProcessWaitFuture {
            Box::pin(async move {
                let now = SystemTime::now();
                Ok(ProcessOutput {
                    status: ProcessExitStatus::ExitCode(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    started_at: now,
                    finished_at: now,
                })
            })
        }
    }

    fn test_temp_dir(test_name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("helm-exec-{test_name}-{nanos}"))
    }

    fn create_placeholder_binary(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("failed to create temp test directory");
        }
        fs::write(path, b"#!/bin/sh\nexit 0\n").expect("failed to write placeholder executable");
    }

    #[test]
    fn spawn_validated_uses_selected_executable_for_matching_alias() {
        clear_manager_selected_executables();
        let temp_dir = test_temp_dir("selected-brew");
        let selected_program = temp_dir.join("brew");
        create_placeholder_binary(&selected_program);

        set_manager_selected_executable(ManagerId::HomebrewFormula, Some(selected_program.clone()));

        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::HomebrewFormula,
            TaskType::Refresh,
            ManagerAction::ListInstalled,
            CommandSpec::new("brew"),
        );

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        assert_eq!(executor.captured_program(), selected_program);

        clear_manager_selected_executables();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn spawn_validated_resolves_sibling_alias_when_selected_binary_name_differs() {
        clear_manager_selected_executables();
        let temp_dir = test_temp_dir("pip-sibling");
        let selected_pip3 = temp_dir.join("pip3");
        let sibling_python3 = temp_dir.join("python3");
        create_placeholder_binary(&selected_pip3);
        create_placeholder_binary(&sibling_python3);

        set_manager_selected_executable(ManagerId::Pip, Some(selected_pip3));

        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::Pip,
            TaskType::Refresh,
            ManagerAction::ListInstalled,
            CommandSpec::new("python3"),
        );

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        assert_eq!(executor.captured_program(), sibling_python3);

        clear_manager_selected_executables();
        let _ = fs::remove_dir_all(temp_dir);
    }
}
