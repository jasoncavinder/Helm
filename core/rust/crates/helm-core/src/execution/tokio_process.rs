use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use tokio::io::AsyncReadExt;

use crate::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskType};

pub struct TokioProcessExecutor;

impl ProcessExecutor for TokioProcessExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let mut cmd = tokio::process::Command::new(&request.command.program);
        cmd.args(&request.command.args);

        for (key, value) in &request.command.env {
            cmd.env(key, value);
        }

        if let Some(dir) = &request.command.working_dir {
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
        }))
    }
}

struct TokioRunningProcess {
    child: Mutex<Option<tokio::process::Child>>,
    pid: Option<u32>,
    started_at: SystemTime,
    timeout: Option<Duration>,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
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

        Box::pin(async move {
            let mut child = child.ok_or_else(|| {
                process_failure(
                    manager,
                    task_type,
                    action,
                    "child process already consumed".to_string(),
                )
            })?;

            let stdout_reader = {
                let mut stdout = child.stdout.take();
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    if let Some(mut handle) = stdout.take() {
                        let _ = handle.read_to_end(&mut buffer).await;
                    }
                    buffer
                })
            };
            let stderr_reader = {
                let mut stderr = child.stderr.take();
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    if let Some(mut handle) = stderr.take() {
                        let _ = handle.read_to_end(&mut buffer).await;
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
