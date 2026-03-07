pub mod task_log_note_store;
pub mod task_output_store;
pub mod timeout_prompt_store;
#[cfg(unix)]
pub mod tokio_process;

pub use task_output_store::TaskOutputRecord;
#[cfg(unix)]
pub use tokio_process::TokioProcessExecutor;

use std::collections::{BTreeMap, HashMap};
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
    pub idle_timeout: Option<Duration>,
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
            idle_timeout: None,
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

    pub fn idle_timeout(mut self, timeout: Duration) -> Self {
        self.idle_timeout = Some(timeout);
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
        if let Some(idle_timeout) = self.idle_timeout
            && idle_timeout.is_zero()
        {
            return Err(invalid_input(
                self.manager,
                self.task_type,
                self.action,
                "idle timeout must be greater than zero when provided",
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ManagerTimeoutProfile {
    pub hard_timeout: Option<Duration>,
    pub idle_timeout: Option<Duration>,
}

static MANAGER_EXECUTABLE_OVERRIDES: OnceLock<RwLock<ManagerExecutionPreferences>> =
    OnceLock::new();

#[derive(Clone, Debug, Default)]
struct ManagerExecutionPreferences {
    executable_overrides: HashMap<ManagerId, PathBuf>,
    timeout_profiles: HashMap<ManagerId, ManagerTimeoutProfile>,
}

fn manager_execution_preferences() -> &'static RwLock<ManagerExecutionPreferences> {
    MANAGER_EXECUTABLE_OVERRIDES.get_or_init(|| RwLock::new(ManagerExecutionPreferences::default()))
}

fn manager_command_aliases(manager: ManagerId) -> &'static [&'static str] {
    match manager {
        ManagerId::HomebrewFormula | ManagerId::HomebrewCask => &["brew"],
        ManagerId::Mise => &["mise"],
        ManagerId::Asdf => &["asdf"],
        ManagerId::Rustup => &["rustup"],
        ManagerId::Npm => &["npm", "npm-cli.js"],
        ManagerId::Pnpm => &["pnpm", "pnpm.cjs"],
        ManagerId::Yarn => &["yarn", "yarn.js", "yarn.cjs"],
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

fn manager_uses_node_runtime(manager: ManagerId) -> bool {
    matches!(manager, ManagerId::Npm | ManagerId::Pnpm | ManagerId::Yarn)
}

fn command_basename(path: &std::path::Path) -> Option<&str> {
    path.file_name().and_then(|name| name.to_str())
}

fn path_separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

fn path_contains_dir(path_value: &str, dir: &std::path::Path) -> bool {
    path_value
        .split(path_separator())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .any(|entry| std::path::Path::new(entry) == dir)
}

fn prepend_dir_to_path_env(command: &mut CommandSpec, dir: &std::path::Path) {
    if !dir.is_dir() {
        return;
    }
    let dir_display = dir.to_string_lossy().to_string();
    let existing_path = command
        .env
        .get("PATH")
        .cloned()
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    if path_contains_dir(existing_path.as_str(), dir) {
        return;
    }
    let value = if existing_path.trim().is_empty() {
        dir_display
    } else {
        format!(
            "{}{sep}{}",
            dir_display,
            existing_path,
            sep = path_separator()
        )
    };
    command.env.insert("PATH".to_string(), value);
}

fn discover_node_runtime_bin_dir(executable_path: &std::path::Path) -> Option<PathBuf> {
    // Traverse ancestor directories and pick the nearest `bin/` that contains a `node` runtime.
    for ancestor in executable_path.ancestors().skip(1).take(8) {
        let candidate = ancestor.join("bin");
        if candidate.join("node").is_file() {
            return Some(candidate);
        }
    }
    None
}

fn prepend_manager_runtime_path_hints(
    request: &mut ProcessSpawnRequest,
    executable_path: &std::path::Path,
) {
    if !manager_uses_node_runtime(request.manager) {
        return;
    }
    if let Some(node_bin) = discover_node_runtime_bin_dir(executable_path) {
        prepend_dir_to_path_env(&mut request.command, node_bin.as_path());
    }
}

fn apply_manager_executable_override(request: &mut ProcessSpawnRequest) {
    let selected = manager_execution_preferences()
        .read()
        .ok()
        .and_then(|guard| guard.executable_overrides.get(&request.manager).cloned());
    let Some(selected_path) = selected else {
        return;
    };

    let aliases = manager_command_aliases(request.manager);
    if aliases.is_empty() {
        return;
    }

    if request.command.program == selected_path && selected_path.is_file() {
        let program_path = request.command.program.clone();
        prepend_manager_runtime_path_hints(request, program_path.as_path());
        if let Some(parent) = selected_path.parent() {
            prepend_dir_to_path_env(&mut request.command, parent);
        }
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

    let mut selected_parent: Option<PathBuf> = None;
    let selected_is_node_script = manager_uses_node_runtime(request.manager)
        && selected_path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| matches!(ext, "js" | "cjs" | "mjs"));

    if selected_name == current_name {
        if selected_path.is_file() {
            selected_parent = selected_path.parent().map(PathBuf::from);
            request.command.program = selected_path;
        }
    } else if selected_is_node_script && selected_path.is_file() {
        selected_parent = selected_path.parent().map(PathBuf::from);
        request.command.program = selected_path;
    } else if let Some(parent) = selected_path.parent() {
        let sibling = parent.join(current_name);
        if sibling.is_file() {
            selected_parent = Some(parent.to_path_buf());
            request.command.program = sibling;
        }
    }

    let program_path = request.command.program.clone();
    prepend_manager_runtime_path_hints(request, program_path.as_path());
    if let Some(parent) = selected_parent {
        prepend_dir_to_path_env(&mut request.command, parent.as_path());
    }
}

fn clamp_idle_timeout_to_hard_limit(
    idle_timeout: Duration,
    hard_timeout: Option<Duration>,
) -> Option<Duration> {
    let Some(hard_timeout) = hard_timeout else {
        return Some(idle_timeout);
    };
    if hard_timeout <= Duration::from_secs(1) {
        return None;
    }
    Some(idle_timeout.min(hard_timeout - Duration::from_secs(1)))
}

fn default_idle_timeout_for_request(request: &ProcessSpawnRequest) -> Option<Duration> {
    let default_idle = match request.task_type {
        TaskType::Detection => Some(Duration::from_secs(20)),
        TaskType::Search => Some(Duration::from_secs(45)),
        TaskType::CatalogSync => Some(Duration::from_secs(120)),
        TaskType::Refresh => Some(Duration::from_secs(120)),
        TaskType::Install
        | TaskType::Uninstall
        | TaskType::Upgrade
        | TaskType::Configure
        | TaskType::Pin
        | TaskType::Unpin => None,
    }?;
    clamp_idle_timeout_to_hard_limit(default_idle, request.timeout)
}

fn apply_manager_timeout_profile(request: &mut ProcessSpawnRequest) {
    let profile = manager_execution_preferences()
        .read()
        .ok()
        .and_then(|guard| guard.timeout_profiles.get(&request.manager).copied())
        .unwrap_or_default();

    let effective_hard_timeout = profile.hard_timeout.or(request.timeout);
    let default_idle_timeout = default_idle_timeout_for_request(request);
    let effective_idle_timeout = profile
        .idle_timeout
        .or(request.idle_timeout)
        .or(default_idle_timeout)
        .and_then(|duration| clamp_idle_timeout_to_hard_limit(duration, effective_hard_timeout));

    request.timeout = effective_hard_timeout;
    request.idle_timeout = effective_idle_timeout;
}

pub fn set_manager_selected_executable(manager: ManagerId, path: Option<PathBuf>) {
    let Ok(mut guard) = manager_execution_preferences().write() else {
        return;
    };
    if let Some(path) = path {
        guard.executable_overrides.insert(manager, path);
    } else {
        guard.executable_overrides.remove(&manager);
    }
}

pub fn clear_manager_selected_executables() {
    if let Ok(mut guard) = manager_execution_preferences().write() {
        guard.executable_overrides.clear();
    }
}

pub fn set_manager_timeout_profile(manager: ManagerId, profile: ManagerTimeoutProfile) {
    let Ok(mut guard) = manager_execution_preferences().write() else {
        return;
    };
    if profile.hard_timeout.is_none() && profile.idle_timeout.is_none() {
        guard.timeout_profiles.remove(&manager);
    } else {
        guard.timeout_profiles.insert(manager, profile);
    }
}

pub fn clear_manager_timeout_profiles() {
    if let Ok(mut guard) = manager_execution_preferences().write() {
        guard.timeout_profiles.clear();
    }
}

pub fn manager_timeout_profile(manager: ManagerId) -> Option<ManagerTimeoutProfile> {
    manager_execution_preferences()
        .read()
        .ok()
        .and_then(|guard| guard.timeout_profiles.get(&manager).copied())
}

pub fn manager_selected_executable(manager: ManagerId) -> Option<PathBuf> {
    manager_execution_preferences()
        .read()
        .ok()
        .and_then(|guard| guard.executable_overrides.get(&manager).cloned())
}

pub fn replace_manager_execution_preferences(
    executable_overrides: HashMap<ManagerId, PathBuf>,
    timeout_profiles: HashMap<ManagerId, ManagerTimeoutProfile>,
) {
    let Ok(mut guard) = manager_execution_preferences().write() else {
        return;
    };
    guard.executable_overrides = executable_overrides;
    guard.timeout_profiles = timeout_profiles;
}

pub fn spawn_validated(
    executor: &dyn ProcessExecutor,
    mut request: ProcessSpawnRequest,
) -> ExecutionResult<Box<dyn RunningProcess>> {
    if request.task_id.is_none() {
        request.task_id = crate::task_context::current_task_id();
    }
    apply_manager_executable_override(&mut request);
    apply_manager_timeout_profile(&mut request);
    request.validate()?;
    executor.spawn(request)
}

pub fn task_output(task_id: TaskId) -> Option<TaskOutputRecord> {
    task_output_store::get(task_id)
}

pub fn record_task_log_note(note: &str) {
    if let Some(task_id) = crate::task_context::current_task_id() {
        task_log_note_store::append(task_id, note);
    }
}

pub fn drain_task_log_notes(task_id: TaskId) -> Vec<String> {
    task_log_note_store::drain(task_id)
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
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

        fn captured_path_env(&self) -> Option<String> {
            self.request
                .lock()
                .expect("capture lock poisoned")
                .as_ref()
                .expect("expected captured request")
                .command
                .env
                .get("PATH")
                .cloned()
        }

        fn captured_timeouts(&self) -> (Option<Duration>, Option<Duration>) {
            let request = self
                .request
                .lock()
                .expect("capture lock poisoned")
                .as_ref()
                .expect("expected captured request")
                .clone();
            (request.timeout, request.idle_timeout)
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

    fn execution_test_lock() -> &'static Mutex<()> {
        static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        TEST_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn spawn_validated_uses_selected_executable_for_matching_alias() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
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
        clear_manager_timeout_profiles();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn spawn_validated_resolves_sibling_alias_when_selected_binary_name_differs() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
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
        clear_manager_timeout_profiles();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn spawn_validated_prepends_selected_parent_to_path_for_node_manager_overrides() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let temp_dir = test_temp_dir("npm-path-prefix");
        let selected_npm = temp_dir.join("bin").join("npm");
        create_placeholder_binary(&selected_npm);

        set_manager_selected_executable(ManagerId::Npm, Some(selected_npm.clone()));

        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::Npm,
            TaskType::Refresh,
            ManagerAction::ListOutdated,
            CommandSpec::new("npm")
                .args(["outdated", "-g", "--json"])
                .env("PATH", "/usr/bin:/bin"),
        );

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        assert_eq!(executor.captured_program(), selected_npm);

        let captured_path = executor
            .captured_path_env()
            .expect("PATH should be present on captured request");
        let parent = selected_npm
            .parent()
            .expect("selected npm should have parent")
            .to_string_lossy()
            .to_string();
        assert!(
            captured_path.starts_with(format!("{parent}:").as_str()),
            "selected executable parent should be prepended to PATH, got: {captured_path}"
        );
        assert!(
            captured_path.contains("/usr/bin:/bin"),
            "original PATH entries should remain in PATH, got: {captured_path}"
        );

        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn spawn_validated_does_not_duplicate_selected_parent_in_path() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let temp_dir = test_temp_dir("npm-path-no-dup");
        let selected_npm = temp_dir.join("bin").join("npm");
        create_placeholder_binary(&selected_npm);

        set_manager_selected_executable(ManagerId::Npm, Some(selected_npm.clone()));

        let parent = selected_npm
            .parent()
            .expect("selected npm should have parent")
            .to_string_lossy()
            .to_string();
        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::Npm,
            TaskType::Refresh,
            ManagerAction::ListInstalled,
            CommandSpec::new("npm")
                .args(["ls", "-g", "--depth=0", "--json"])
                .env("PATH", format!("{parent}:/usr/bin:/bin")),
        );

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        let captured_path = executor
            .captured_path_env()
            .expect("PATH should be present on captured request");
        let count = captured_path
            .split(':')
            .filter(|entry| *entry == parent.as_str())
            .count();
        assert_eq!(
            count, 1,
            "selected executable parent should appear only once in PATH: {captured_path}"
        );

        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn spawn_validated_selects_node_script_override_and_prepends_runtime_bin() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let temp_dir = test_temp_dir("npm-script-override");
        let install_root = temp_dir.join("installs").join("node").join("24.13.1");
        let selected_script = install_root
            .join("lib")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js");
        let node_bin = install_root.join("bin");
        let node_binary = node_bin.join("node");
        create_placeholder_binary(&selected_script);
        create_placeholder_binary(&node_binary);

        set_manager_selected_executable(ManagerId::Npm, Some(selected_script.clone()));

        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::Npm,
            TaskType::Refresh,
            ManagerAction::ListInstalled,
            CommandSpec::new("npm")
                .args(["ls", "-g", "--depth=0", "--json"])
                .env("PATH", "/usr/bin:/bin"),
        );

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        assert_eq!(executor.captured_program(), selected_script);

        let captured_path = executor
            .captured_path_env()
            .expect("PATH should be present on captured request");
        let selected_parent = selected_script
            .parent()
            .expect("selected script should have parent")
            .to_string_lossy()
            .to_string();
        let node_bin_display = node_bin.to_string_lossy().to_string();
        let sep = path_separator();
        assert!(
            captured_path
                .starts_with(format!("{selected_parent}{sep}{node_bin_display}{sep}").as_str()),
            "selected script parent and node runtime bin should lead PATH, got: {captured_path}"
        );
        assert!(
            captured_path.contains("/usr/bin:/bin"),
            "original PATH entries should remain in PATH, got: {captured_path}"
        );

        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn spawn_validated_prepends_node_runtime_when_program_is_selected_script() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let temp_dir = test_temp_dir("npm-script-direct");
        let install_root = temp_dir.join("installs").join("node").join("24.13.1");
        let selected_script = install_root
            .join("lib")
            .join("node_modules")
            .join("npm")
            .join("bin")
            .join("npm-cli.js");
        let node_bin = install_root.join("bin");
        let node_binary = node_bin.join("node");
        create_placeholder_binary(&selected_script);
        create_placeholder_binary(&node_binary);

        set_manager_selected_executable(ManagerId::Npm, Some(selected_script.clone()));

        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::Npm,
            TaskType::Refresh,
            ManagerAction::ListInstalled,
            CommandSpec::new(selected_script.to_string_lossy().to_string())
                .args(["ls", "-g", "--depth=0", "--json"])
                .env("PATH", "/usr/bin:/bin"),
        );

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        assert_eq!(executor.captured_program(), selected_script);

        let captured_path = executor
            .captured_path_env()
            .expect("PATH should be present on captured request");
        let selected_parent = selected_script
            .parent()
            .expect("selected script should have parent")
            .to_string_lossy()
            .to_string();
        let node_bin_display = node_bin.to_string_lossy().to_string();
        let sep = path_separator();
        assert!(
            captured_path
                .starts_with(format!("{selected_parent}{sep}{node_bin_display}{sep}").as_str()),
            "selected script parent and node runtime bin should lead PATH, got: {captured_path}"
        );

        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let _ = fs::remove_dir_all(temp_dir);
    }

    #[test]
    fn spawn_validated_applies_default_idle_timeout_for_refresh_tasks() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();

        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::Npm,
            TaskType::Refresh,
            ManagerAction::ListOutdated,
            CommandSpec::new("npm").args(["outdated", "-g", "--json"]),
        )
        .timeout(Duration::from_secs(300));

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        let (hard_timeout, idle_timeout) = executor.captured_timeouts();
        assert_eq!(hard_timeout, Some(Duration::from_secs(300)));
        assert_eq!(idle_timeout, Some(Duration::from_secs(120)));

        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
    }

    #[test]
    fn spawn_validated_applies_manager_timeout_profile_overrides() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        set_manager_timeout_profile(
            ManagerId::Npm,
            ManagerTimeoutProfile {
                hard_timeout: Some(Duration::from_secs(500)),
                idle_timeout: Some(Duration::from_secs(240)),
            },
        );

        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::Npm,
            TaskType::Refresh,
            ManagerAction::ListInstalled,
            CommandSpec::new("npm").args(["ls", "-g", "--depth=0", "--json"]),
        )
        .timeout(Duration::from_secs(120));

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        let (hard_timeout, idle_timeout) = executor.captured_timeouts();
        assert_eq!(hard_timeout, Some(Duration::from_secs(500)));
        assert_eq!(idle_timeout, Some(Duration::from_secs(240)));

        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
    }

    #[test]
    fn spawn_validated_clamps_idle_timeout_to_hard_timeout_limit() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        set_manager_timeout_profile(
            ManagerId::Npm,
            ManagerTimeoutProfile {
                hard_timeout: Some(Duration::from_secs(100)),
                idle_timeout: Some(Duration::from_secs(200)),
            },
        );

        let executor = CapturingExecutor::default();
        let request = ProcessSpawnRequest::new(
            ManagerId::Npm,
            TaskType::Refresh,
            ManagerAction::ListInstalled,
            CommandSpec::new("npm").args(["ls", "-g", "--depth=0", "--json"]),
        );

        let _ = spawn_validated(&executor, request).expect("spawn should succeed");
        let (hard_timeout, idle_timeout) = executor.captured_timeouts();
        assert_eq!(hard_timeout, Some(Duration::from_secs(100)));
        assert_eq!(idle_timeout, Some(Duration::from_secs(99)));

        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
    }

    #[test]
    fn replace_manager_execution_preferences_avoids_empty_read_window() {
        let _lock = execution_test_lock()
            .lock()
            .expect("execution test lock poisoned");
        clear_manager_selected_executables();
        clear_manager_timeout_profiles();

        let temp_dir = test_temp_dir("execution-preference-swap");
        let npm_a = temp_dir.join("profile-a").join("npm");
        let npm_b = temp_dir.join("profile-b").join("npm");
        create_placeholder_binary(&npm_a);
        create_placeholder_binary(&npm_b);

        let mut executable_map_a = HashMap::new();
        executable_map_a.insert(ManagerId::Npm, npm_a.clone());
        let mut executable_map_b = HashMap::new();
        executable_map_b.insert(ManagerId::Npm, npm_b.clone());

        let mut timeout_map_a = HashMap::new();
        timeout_map_a.insert(
            ManagerId::Npm,
            ManagerTimeoutProfile {
                hard_timeout: Some(Duration::from_secs(300)),
                idle_timeout: Some(Duration::from_secs(120)),
            },
        );
        let mut timeout_map_b = HashMap::new();
        timeout_map_b.insert(
            ManagerId::Npm,
            ManagerTimeoutProfile {
                hard_timeout: Some(Duration::from_secs(500)),
                idle_timeout: Some(Duration::from_secs(240)),
            },
        );

        replace_manager_execution_preferences(executable_map_a.clone(), timeout_map_a.clone());

        let stop_reader = Arc::new(AtomicBool::new(false));
        let observed_empty = Arc::new(AtomicBool::new(false));
        let reader_stop = Arc::clone(&stop_reader);
        let reader_empty = Arc::clone(&observed_empty);

        let reader = std::thread::spawn(move || {
            while !reader_stop.load(Ordering::Relaxed) {
                if manager_selected_executable(ManagerId::Npm).is_none()
                    || manager_timeout_profile(ManagerId::Npm).is_none()
                {
                    reader_empty.store(true, Ordering::Relaxed);
                    break;
                }
            }
        });

        for _ in 0..500 {
            replace_manager_execution_preferences(executable_map_b.clone(), timeout_map_b.clone());
            replace_manager_execution_preferences(executable_map_a.clone(), timeout_map_a.clone());
        }

        stop_reader.store(true, Ordering::Relaxed);
        reader.join().expect("reader thread should join");
        assert!(
            !observed_empty.load(Ordering::Relaxed),
            "reader observed empty executable/timeout profile window during swaps"
        );

        clear_manager_selected_executables();
        clear_manager_timeout_profiles();
        let _ = fs::remove_dir_all(temp_dir);
    }
}
