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

        let mut cmd = tokio::process::Command::new(&prepared.command.program);
        cmd.args(&prepared.command.args);

        for (key, value) in &prepared.command.env {
            cmd.env(key, value);
        }

        if let Some(dir) = &prepared.command.working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        cmd.process_group(0);

        let child = cmd.spawn().map_err(|error| {
            process_failure(
                request.manager,
                request.task_type,
                request.action,
                format!("failed to spawn process: {error}"),
            )
        })?;

        let pid = child.id();
        let started_at = SystemTime::now();

        Ok(Box::new(TokioRunningProcess {
            child: Mutex::new(Some(child)),
            pid,
            started_at,
            timeout: request.timeout,
            manager: request.manager,
            task_type: request.task_type,
            action: request.action,
            task_id: request.task_id,
            command_display: prepared.command_display,
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
                format!(
                    "sudo askpass helper '{}' is not executable",
                    path.display()
                ),
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
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    task_id: Option<TaskId>,
    command_display: String,
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
        let started_at = self.started_at;
        let manager = self.manager;
        let task_type = self.task_type;
        let action = self.action;
        let pid = self.pid;
        let task_id = self.task_id;
        let command_display = self.command_display;

        Box::pin(async move {
            let mut child = child.ok_or_else(|| {
                process_failure(
                    manager,
                    task_type,
                    action,
                    "child process already consumed".to_string(),
                )
            })?;

            if let Some(task_id) = task_id {
                crate::execution::task_output_store::record_command(task_id, &command_display);
            }

            let stdout_reader = {
                let mut stdout = child.stdout.take();
                let stream_task_id = task_id;
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

            let wait_err = |error: std::io::Error| {
                process_failure(
                    manager,
                    task_type,
                    action,
                    format!("failed to wait for process: {error}"),
                )
            };

            // Wait for process exit first, then collect output with a short bounded read window.
            // This avoids hanging forever when descendant processes inherit stdout/stderr fds.
            let status = if let Some(timeout_duration) = timeout {
                match tokio::time::timeout(timeout_duration, child.wait()).await {
                    Ok(result) => result.map_err(wait_err)?,
                    Err(_) => {
                        if let Some(pid) = pid {
                            let pgid = -(pid as libc::pid_t);
                            unsafe {
                                libc::kill(pgid, libc::SIGKILL);
                            }
                        }
                        let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
                        stdout_reader.abort();
                        stderr_reader.abort();
                        return Err(CoreError {
                            manager: Some(manager),
                            task: Some(task_type),
                            action: Some(action),
                            kind: CoreErrorKind::Timeout,
                            message: format!(
                                "process timed out after {}ms",
                                timeout_duration.as_millis()
                            ),
                        });
                    }
                }
            } else {
                child.wait().await.map_err(wait_err)?
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

            let status = match status.code() {
                Some(code) => ProcessExitStatus::ExitCode(code),
                None => ProcessExitStatus::Terminated,
            };

            if let Some(task_id) = task_id {
                crate::execution::task_output_store::record(
                    task_id,
                    Some(command_display.as_str()),
                    &stdout,
                    &stderr,
                );
            }

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
    use super::prepare_command_for_spawn;
    use crate::execution::{CommandSpec, ProcessSpawnRequest};
    use crate::models::{ManagerAction, ManagerId, TaskType};
    use std::fs;
    use std::path::PathBuf;

    fn base_request() -> ProcessSpawnRequest {
        ProcessSpawnRequest::new(
            ManagerId::SoftwareUpdate,
            TaskType::Upgrade,
            ManagerAction::Upgrade,
            CommandSpec::new("/usr/sbin/softwareupdate").arg("--install").arg("--all"),
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
}
