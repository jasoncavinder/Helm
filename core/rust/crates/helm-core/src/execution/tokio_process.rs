use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use tokio::io::AsyncReadExt;

use crate::execution::{
    CommandSpec, ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput,
    ProcessSpawnRequest, ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskId, TaskType};

pub struct TokioProcessExecutor;

impl ProcessExecutor for TokioProcessExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let prepared = prepare_command_for_spawn(&request, None)?;
        let task_id = request.task_id;
        let manager = request.manager;
        let task_type = request.task_type;
        let action = request.action;
        let command_display = prepared.command_display.clone();
        let (program_path, path_snippet) = process_context_details(&prepared.command);
        let effective_working_dir =
            resolve_effective_working_dir(prepared.command.working_dir.as_deref());
        let working_dir_display = effective_working_dir
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .or_else(|| resolve_working_dir_display(prepared.command.working_dir.as_deref()));
        if let Some(task_id) = task_id {
            crate::execution::task_output_store::record_context(
                task_id,
                Some(command_display.as_str()),
                working_dir_display.as_deref(),
            );
            crate::execution::task_output_store::record_process_context(
                task_id,
                Some(program_path.as_str()),
                path_snippet.as_deref(),
            );
        }

        let mut cmd = tokio::process::Command::new(&prepared.command.program);
        cmd.args(&prepared.command.args);

        for (key, value) in &prepared.command.env {
            cmd.env(key, value);
        }

        if let Some(dir) = &effective_working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.process_group(0);

        let child = cmd.spawn().map_err(|error| {
            let message = append_error_context(
                format!("failed to spawn process: {error}").as_str(),
                program_path.as_str(),
                path_snippet.as_deref(),
            );
            if let Some(task_id) = task_id {
                crate::execution::task_output_store::record_error(
                    task_id,
                    "spawn_failed",
                    message.as_str(),
                    Some("error"),
                    Some(SystemTime::now()),
                );
                crate::execution::task_output_store::append_stderr(task_id, message.as_bytes());
            }
            process_failure(manager, task_type, action, message)
        })?;

        let pid = child.id();
        let started_at = SystemTime::now();
        if let Some(task_id) = task_id {
            crate::execution::task_output_store::record_started_at(task_id, started_at);
        }

        Ok(Box::new(TokioRunningProcess {
            child: Mutex::new(Some(child)),
            pid,
            started_at,
            timeout: request.timeout,
            idle_timeout: request.idle_timeout,
            manager,
            task_type,
            action,
            task_id,
            command_display,
            program_path,
            path_snippet,
        }))
    }
}

struct PreparedSpawnCommand {
    command: CommandSpec,
    command_display: String,
}

static DEFAULT_SUDO_ASKPASS_PATH: OnceLock<Result<PathBuf, String>> = OnceLock::new();

fn prepare_command_for_spawn(
    request: &ProcessSpawnRequest,
    askpass_override: Option<&Path>,
) -> ExecutionResult<PreparedSpawnCommand> {
    if !request.requires_elevation {
        return Ok(PreparedSpawnCommand {
            command: request.command.clone(),
            command_display: format_command_for_display(&request.command),
        });
    }

    let askpass_path = resolve_sudo_askpass_path(
        request.manager,
        request.task_type,
        request.action,
        askpass_override,
    )?;
    let askpass_string = askpass_path.to_string_lossy().to_string();

    let mut elevated = CommandSpec::new("/usr/bin/sudo")
        .arg("-A")
        .arg("--")
        .arg(request.command.program.to_string_lossy().to_string())
        .args(request.command.args.clone())
        .env("SUDO_ASKPASS", askpass_string)
        .env(
            "HELM_SUDO_PROMPT",
            "Helm requires administrator authentication to continue.",
        )
        .env("SUDO_PROMPT", "");

    for (key, value) in &request.command.env {
        elevated = elevated.env(key.clone(), value.clone());
    }

    if let Some(dir) = &request.command.working_dir {
        elevated = elevated.working_dir(dir.clone());
    }

    Ok(PreparedSpawnCommand {
        command_display: format_command_for_display(&elevated),
        command: elevated,
    })
}

fn process_context_details(command: &CommandSpec) -> (String, Option<String>) {
    let program_path = command.program.to_string_lossy().to_string();
    let path_snippet = command
        .env
        .get("PATH")
        .cloned()
        .or_else(|| std::env::var("PATH").ok());
    (program_path, path_snippet)
}

fn append_error_context(base: &str, program_path: &str, path_snippet: Option<&str>) -> String {
    let normalized_path = path_snippet
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("<inherit>");
    format!("{base} [program={program_path}] [PATH={normalized_path}]")
}

fn resolve_sudo_askpass_path(
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    askpass_override: Option<&Path>,
) -> ExecutionResult<PathBuf> {
    if let Some(path) = askpass_override {
        validate_askpass_path(path, manager, task_type, action)?;
        return Ok(path.to_path_buf());
    }

    if let Ok(path) = std::env::var("HELM_SUDO_ASKPASS") {
        let path = PathBuf::from(path);
        validate_askpass_path(path.as_path(), manager, task_type, action)?;
        return Ok(path);
    }

    let initialized = DEFAULT_SUDO_ASKPASS_PATH.get_or_init(create_default_sudo_askpass_script);
    match initialized {
        Ok(path) => {
            validate_askpass_path(path.as_path(), manager, task_type, action)?;
            Ok(path.clone())
        }
        Err(message) => Err(process_failure(manager, task_type, action, message.clone())),
    }
}

fn validate_askpass_path(
    path: &Path,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
) -> ExecutionResult<()> {
    let metadata = fs::metadata(path).map_err(|error| {
        process_failure(
            manager,
            task_type,
            action,
            format!(
                "sudo askpass helper is unavailable at '{}': {error}",
                path.display()
            ),
        )
    })?;
    if !metadata.is_file() {
        return Err(process_failure(
            manager,
            task_type,
            action,
            format!(
                "sudo askpass helper path '{}' is not a file",
                path.display()
            ),
        ));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(process_failure(
                manager,
                task_type,
                action,
                format!("sudo askpass helper '{}' is not executable", path.display()),
            ));
        }
    }

    Ok(())
}

fn create_default_sudo_askpass_script() -> Result<PathBuf, String> {
    let script = r#"#!/bin/sh
/usr/bin/osascript <<'APPLESCRIPT'
set promptText to system attribute "HELM_SUDO_PROMPT"
if promptText is "" then set promptText to "Helm requires administrator authentication to continue."
try
    display dialog promptText default answer "" with hidden answer buttons {"Cancel", "OK"} default button "OK"
    return text returned of result
on error number -128
    error number 1
end try
APPLESCRIPT
"#;

    let mut target_path: Option<PathBuf> = None;
    let pid = std::process::id();
    for attempt in 0..16_u8 {
        let candidate = std::env::temp_dir().join(format!("helm-sudo-askpass-{pid}-{attempt}.sh"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                file.write_all(script.as_bytes()).map_err(|error| {
                    format!(
                        "failed to write sudo askpass helper to '{}': {error}",
                        candidate.display()
                    )
                })?;
                target_path = Some(candidate);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create sudo askpass helper file '{}': {error}",
                    candidate.display()
                ));
            }
        }
    }

    let path = target_path.ok_or_else(|| {
        "failed to create unique sudo askpass helper file in temporary directory".to_string()
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(|error| {
            format!(
                "failed to set executable permissions on sudo askpass helper '{}': {error}",
                path.display()
            )
        })?;
    }

    Ok(path)
}

struct TokioRunningProcess {
    child: Mutex<Option<tokio::process::Child>>,
    pid: Option<u32>,
    started_at: SystemTime,
    timeout: Option<Duration>,
    idle_timeout: Option<Duration>,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    task_id: Option<TaskId>,
    command_display: String,
    program_path: String,
    path_snippet: Option<String>,
}

impl RunningProcess for TokioRunningProcess {
    fn pid(&self) -> Option<u32> {
        self.pid
    }

    fn terminate(&self, mode: ProcessTerminationMode) -> ExecutionResult<()> {
        let Some(pid) = self.pid else {
            return Ok(());
        };

        let signal = match mode {
            ProcessTerminationMode::Immediate => libc::SIGKILL,
            ProcessTerminationMode::Graceful { .. } => libc::SIGTERM,
        };

        let pgid = -(pid as libc::pid_t);
        let result = unsafe { libc::kill(pgid, signal) };

        if result != 0 {
            let os_error = std::io::Error::last_os_error();
            if os_error.raw_os_error() != Some(libc::ESRCH) {
                return Err(process_failure(
                    self.manager,
                    self.task_type,
                    self.action,
                    format!("failed to send signal {signal} to process group {pid}: {os_error}"),
                ));
            }
        }

        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let child = self.child.into_inner().ok().flatten();
        let timeout = self.timeout;
        let idle_timeout = self.idle_timeout;
        let started_at = self.started_at;
        let manager = self.manager;
        let task_type = self.task_type;
        let action = self.action;
        let pid = self.pid;
        let task_id = self.task_id;
        let command_display = self.command_display;
        let program_path = self.program_path;
        let path_snippet = self.path_snippet;

        Box::pin(async move {
            let mut child = child.ok_or_else(|| {
                let message = "child process already consumed".to_string();
                if let Some(task_id) = task_id {
                    crate::execution::task_output_store::record_error(
                        task_id,
                        "internal_child_consumed",
                        message.as_str(),
                        Some("error"),
                        Some(SystemTime::now()),
                    );
                    crate::execution::task_output_store::append_stderr(task_id, message.as_bytes());
                }
                process_failure(manager, task_type, action, message)
            })?;

            if let Some(task_id) = task_id {
                crate::execution::task_output_store::record_command(task_id, &command_display);
            }

            let (activity_tx, mut activity_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

            let stdout_reader = {
                let mut stdout = child.stdout.take();
                let stream_task_id = task_id;
                let activity_tx = activity_tx.clone();
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    if let Some(mut handle) = stdout.take() {
                        let mut chunk = vec![0_u8; 4096];
                        loop {
                            match handle.read(&mut chunk).await {
                                Ok(0) => break,
                                Ok(read_count) => {
                                    let bytes = &chunk[..read_count];
                                    buffer.extend_from_slice(bytes);
                                    let _ = activity_tx.send(());
                                    if let Some(task_id) = stream_task_id {
                                        crate::execution::task_output_store::append_stdout(
                                            task_id, bytes,
                                        );
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                    buffer
                })
            };
            let stderr_reader = {
                let mut stderr = child.stderr.take();
                let stream_task_id = task_id;
                let activity_tx = activity_tx.clone();
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    if let Some(mut handle) = stderr.take() {
                        let mut chunk = vec![0_u8; 4096];
                        loop {
                            match handle.read(&mut chunk).await {
                                Ok(0) => break,
                                Ok(read_count) => {
                                    let bytes = &chunk[..read_count];
                                    buffer.extend_from_slice(bytes);
                                    let _ = activity_tx.send(());
                                    if let Some(task_id) = stream_task_id {
                                        crate::execution::task_output_store::append_stderr(
                                            task_id, bytes,
                                        );
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                    }
                    buffer
                })
            };
            drop(activity_tx);

            let wait_err = |error: std::io::Error| {
                let message = append_error_context(
                    format!("failed to wait for process: {error}").as_str(),
                    program_path.as_str(),
                    path_snippet.as_deref(),
                );
                if let Some(task_id) = task_id {
                    crate::execution::task_output_store::record_error(
                        task_id,
                        "wait_failed",
                        message.as_str(),
                        Some("error"),
                        Some(SystemTime::now()),
                    );
                    crate::execution::task_output_store::append_stderr(task_id, message.as_bytes());
                }
                process_failure(manager, task_type, action, message)
            };

            let mut wait_future = Box::pin(child.wait());
            let started_instant = tokio::time::Instant::now();
            let mut last_activity_instant = started_instant;
            let mut activity_channel_open = true;

            let status = loop {
                tokio::select! {
                    result = &mut wait_future => {
                        break result.map_err(wait_err)?;
                    }
                    activity = activity_rx.recv(), if activity_channel_open => {
                        match activity {
                            Some(_) => {
                                last_activity_instant = tokio::time::Instant::now();
                            }
                            None => {
                                activity_channel_open = false;
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {}
                }

                let now = tokio::time::Instant::now();
                let timeout_state = timeout
                    .filter(|duration| now.duration_since(started_instant) >= *duration)
                    .map(|duration| {
                        (
                            "hard_timeout",
                            format!(
                                "process reached hard timeout after {}ms",
                                duration.as_millis()
                            ),
                        )
                    })
                    .or_else(|| {
                        idle_timeout
                            .filter(|duration| {
                                now.duration_since(last_activity_instant) >= *duration
                            })
                            .map(|duration| {
                                (
                                    "idle_timeout",
                                    format!(
                                        "process produced no output for {}ms",
                                        duration.as_millis()
                                    ),
                                )
                            })
                    });

                if let Some((timeout_code, timeout_reason)) = timeout_state {
                    if let Some(pid) = pid {
                        let pgid = -(pid as libc::pid_t);
                        unsafe {
                            libc::kill(pgid, libc::SIGKILL);
                        }
                    }
                    let _ = tokio::time::timeout(Duration::from_secs(1), &mut wait_future).await;
                    stdout_reader.abort();
                    stderr_reader.abort();
                    let finished_at = SystemTime::now();
                    let message = append_error_context(
                        timeout_reason.as_str(),
                        program_path.as_str(),
                        path_snippet.as_deref(),
                    );
                    if let Some(task_id) = task_id {
                        crate::execution::task_output_store::record_terminal_metadata(
                            task_id,
                            started_at,
                            finished_at,
                            None,
                            Some("timeout"),
                        );
                        crate::execution::task_output_store::record_error(
                            task_id,
                            timeout_code,
                            message.as_str(),
                            Some("timeout"),
                            Some(finished_at),
                        );
                        crate::execution::task_output_store::append_stderr(
                            task_id,
                            message.as_bytes(),
                        );
                    }
                    return Err(CoreError {
                        manager: Some(manager),
                        task: Some(task_type),
                        action: Some(action),
                        kind: CoreErrorKind::Timeout,
                        message,
                    });
                }
            };

            let read_deadline = Duration::from_millis(250);
            let stdout = match tokio::time::timeout(read_deadline, stdout_reader).await {
                Ok(Ok(buffer)) => buffer,
                _ => Vec::new(),
            };
            let stderr = match tokio::time::timeout(read_deadline, stderr_reader).await {
                Ok(Ok(buffer)) => buffer,
                _ => Vec::new(),
            };

            let finished_at = SystemTime::now();

            let (
                status,
                exit_code,
                termination_reason,
                terminal_error_code,
                terminal_error_message,
            ) = match status.code() {
                Some(code) => {
                    let reason = if code == 0 { None } else { Some("error") };
                    let error_code = if code == 0 {
                        None
                    } else {
                        Some("non_zero_exit")
                    };
                    let error_message = if code == 0 {
                        None
                    } else {
                        Some(format!("process exited with code {code}"))
                    };
                    (
                        ProcessExitStatus::ExitCode(code),
                        Some(code),
                        reason,
                        error_code,
                        error_message,
                    )
                }
                None => (
                    ProcessExitStatus::Terminated,
                    None,
                    Some("signal"),
                    Some("terminated_by_signal"),
                    Some("process terminated by signal".to_string()),
                ),
            };

            if let Some(task_id) = task_id {
                crate::execution::task_output_store::record_terminal_metadata(
                    task_id,
                    started_at,
                    finished_at,
                    exit_code,
                    termination_reason,
                );
                if let (Some(code), Some(message)) = (terminal_error_code, terminal_error_message) {
                    crate::execution::task_output_store::record_error(
                        task_id,
                        code,
                        message.as_str(),
                        termination_reason,
                        Some(finished_at),
                    );
                }
            }

            if let Some(task_id) = task_id {
                crate::execution::task_output_store::record(
                    task_id,
                    Some(command_display.as_str()),
                    &stdout,
                    &stderr,
                );
            };

            Ok(ProcessOutput {
                status,
                stdout,
                stderr,
                started_at,
                finished_at,
            })
        })
    }
}

fn format_command_for_display(command: &CommandSpec) -> String {
    let mut parts = Vec::with_capacity(command.args.len() + 1);
    parts.push(shell_escape(&command.program.to_string_lossy()));
    parts.extend(command.args.iter().map(|arg| shell_escape(arg)));

    parts.join(" ")
}

fn resolve_effective_working_dir(requested_working_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = requested_working_dir
        && path.is_dir()
    {
        return Some(path.to_path_buf());
    }

    if let Ok(current_dir) = std::env::current_dir()
        && current_dir.is_dir()
    {
        return Some(current_dir);
    }

    if let Ok(home_dir) = std::env::var("HOME") {
        let home = PathBuf::from(home_dir);
        if home.is_dir() {
            return Some(home);
        }
    }

    let temp_dir = std::env::temp_dir();
    if temp_dir.is_dir() {
        return Some(temp_dir);
    }

    None
}

fn resolve_working_dir_display(requested_working_dir: Option<&Path>) -> Option<String> {
    if let Some(path) = requested_working_dir {
        return Some(path.to_string_lossy().to_string());
    }
    match std::env::current_dir() {
        Ok(path) => Some(path.to_string_lossy().to_string()),
        Err(error) => Some(format!("<unavailable: {error}>")),
    }
}

fn shell_escape(text: &str) -> String {
    if text.is_empty() {
        return "''".to_string();
    }

    let is_simple = text.chars().all(|character| {
        character.is_ascii_alphanumeric()
            || matches!(character, '-' | '_' | '.' | '/' | ':' | '@' | '=' | '+')
    });
    if is_simple {
        return text.to_string();
    }

    format!("'{}'", text.replace('\'', "'\\''"))
}

fn process_failure(
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    message: String,
) -> CoreError {
    CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: Some(action),
        kind: CoreErrorKind::ProcessFailure,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::{prepare_command_for_spawn, resolve_effective_working_dir};
    use crate::execution::{CommandSpec, ProcessSpawnRequest};
    use crate::models::{ManagerAction, ManagerId, TaskType};
    use std::fs;
    use std::path::PathBuf;

    fn base_request() -> ProcessSpawnRequest {
        ProcessSpawnRequest::new(
            ManagerId::SoftwareUpdate,
            TaskType::Upgrade,
            ManagerAction::Upgrade,
            CommandSpec::new("/usr/sbin/softwareupdate")
                .arg("--install")
                .arg("--all"),
        )
    }

    #[test]
    fn prepare_command_keeps_non_elevated_requests_unchanged() {
        let request = base_request().requires_elevation(false);
        let prepared =
            prepare_command_for_spawn(&request, Some(PathBuf::from("/tmp/unused").as_path()))
                .expect("prepare should succeed");

        assert_eq!(
            prepared.command.program,
            PathBuf::from("/usr/sbin/softwareupdate")
        );
        assert_eq!(
            prepared.command.args,
            vec!["--install".to_string(), "--all".to_string()]
        );
        assert!(
            !prepared.command_display.contains("sudo"),
            "display should not include sudo for non-elevated command"
        );
    }

    #[test]
    fn prepare_command_wraps_elevated_requests_with_sudo() {
        let askpass_path = std::env::temp_dir().join("helm-askpass-test.sh");
        fs::write(&askpass_path, "#!/bin/sh\nexit 0\n").expect("should write askpass test file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&askpass_path, fs::Permissions::from_mode(0o700))
                .expect("should chmod askpass test file");
        }

        let request = base_request().requires_elevation(true);
        let prepared = prepare_command_for_spawn(&request, Some(askpass_path.as_path()))
            .expect("prepare should succeed");

        assert_eq!(prepared.command.program, PathBuf::from("/usr/bin/sudo"));
        assert_eq!(
            prepared.command.args,
            vec![
                "-A".to_string(),
                "--".to_string(),
                "/usr/sbin/softwareupdate".to_string(),
                "--install".to_string(),
                "--all".to_string()
            ]
        );
        assert_eq!(
            prepared.command.env.get("SUDO_ASKPASS"),
            Some(&askpass_path.to_string_lossy().to_string())
        );
        assert!(
            prepared.command_display.contains("sudo"),
            "display should include sudo wrapper"
        );

        let _ = fs::remove_file(askpass_path);
    }

    #[test]
    fn resolve_effective_working_dir_prefers_existing_requested_dir() {
        let requested = std::env::temp_dir().join("helm-tokio-process-cwd-existing");
        fs::create_dir_all(&requested).expect("failed to create temp directory");

        let resolved = resolve_effective_working_dir(Some(requested.as_path()))
            .expect("expected working directory to resolve");
        assert_eq!(resolved, requested);

        let _ = fs::remove_dir_all(resolved);
    }

    #[test]
    fn resolve_effective_working_dir_falls_back_when_requested_dir_is_missing() {
        let requested = std::env::temp_dir().join("helm-tokio-process-cwd-missing");
        let _ = fs::remove_dir_all(&requested);

        let resolved = resolve_effective_working_dir(Some(requested.as_path()))
            .expect("expected fallback working directory");
        assert!(
            resolved.is_dir(),
            "fallback working directory should exist and be a directory"
        );
        assert_ne!(
            resolved, requested,
            "missing requested working directory should not be selected"
        );
    }
}
