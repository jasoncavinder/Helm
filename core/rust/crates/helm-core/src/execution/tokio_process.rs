use std::sync::Mutex;
use std::time::{Duration, SystemTime};

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
            let child = child.ok_or_else(|| {
                process_failure(
                    manager,
                    task_type,
                    action,
                    "child process already consumed".to_string(),
                )
            })?;

            let wait_err = |error: std::io::Error| {
                process_failure(
                    manager,
                    task_type,
                    action,
                    format!("failed to wait for process: {error}"),
                )
            };

            // wait_with_output() consumes the child. On timeout, the future
            // (and thus the child) is dropped; we kill via the stored pid.
            let output = if let Some(timeout_duration) = timeout {
                match tokio::time::timeout(timeout_duration, child.wait_with_output()).await {
                    Ok(result) => result.map_err(wait_err)?,
                    Err(_) => {
                        if let Some(pid) = pid {
                            let pgid = -(pid as libc::pid_t);
                            unsafe {
                                libc::kill(pgid, libc::SIGKILL);
                            }
                        }
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
                child.wait_with_output().await.map_err(wait_err)?
            };

            let finished_at = SystemTime::now();

            let status = match output.status.code() {
                Some(code) => ProcessExitStatus::ExitCode(code),
                None => ProcessExitStatus::Terminated,
            };

            Ok(ProcessOutput {
                status,
                stdout: output.stdout,
                stderr: output.stderr,
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
