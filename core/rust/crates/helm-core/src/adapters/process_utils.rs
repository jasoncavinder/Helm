use crate::adapters::manager::AdapterResult;
use crate::execution::{
    ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest, spawn_validated,
};
use crate::models::{CoreError, CoreErrorKind};

/// Run a process and return stdout, falling back to stderr if stdout is empty.
/// Used for version detection where some tools output to stderr.
/// Returns empty string only when spawn/wait fails or no textual output is produced.
pub(crate) fn run_and_collect_version_output(
    executor: &dyn ProcessExecutor,
    request: ProcessSpawnRequest,
) -> String {
    let process = match spawn_validated(executor, request) {
        Ok(p) => p,
        Err(_) => return String::new(),
    };

    let handle = tokio::runtime::Handle::current();
    let output: ProcessOutput = match handle.block_on(process.wait()) {
        Ok(o) => o,
        Err(_) => return String::new(),
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if !stdout.trim().is_empty() {
        return stdout;
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !stderr.trim().is_empty() {
        return stderr;
    }

    String::new()
}

pub(crate) fn run_and_collect_stdout(
    executor: &dyn ProcessExecutor,
    request: ProcessSpawnRequest,
) -> AdapterResult<String> {
    let manager = request.manager;
    let task_type = request.task_type;
    let action = request.action;

    let process = spawn_validated(executor, request)?;

    let handle = tokio::runtime::Handle::current();
    let output: ProcessOutput = handle.block_on(process.wait())?;

    match output.status {
        ProcessExitStatus::ExitCode(0) => Ok(String::from_utf8_lossy(&output.stdout).to_string()),
        ProcessExitStatus::ExitCode(code) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(CoreError {
                manager: Some(manager),
                task: Some(task_type),
                action: Some(action),
                kind: CoreErrorKind::ProcessFailure,
                message: format!("process exited with code {code}: {stderr}"),
            })
        }
        ProcessExitStatus::Terminated => Err(CoreError {
            manager: Some(manager),
            task: Some(task_type),
            action: Some(action),
            kind: CoreErrorKind::ProcessFailure,
            message: "process was terminated by signal".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::SystemTime;

    use crate::execution::{
        CommandSpec, ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput,
        ProcessSpawnRequest, ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
    };
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, TaskType};

    use super::run_and_collect_stdout;

    #[derive(Clone)]
    struct StaticExecutor {
        output: ProcessOutput,
    }

    struct StaticProcess {
        output: ProcessOutput,
    }

    impl RunningProcess for StaticProcess {
        fn pid(&self) -> Option<u32> {
            Some(1234)
        }

        fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
            Ok(())
        }

        fn wait(self: Box<Self>) -> ProcessWaitFuture {
            let output = self.output;
            Box::pin(async move { Ok(output) })
        }
    }

    impl ProcessExecutor for StaticExecutor {
        fn spawn(&self, _request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
            Ok(Box::new(StaticProcess {
                output: self.output.clone(),
            }))
        }
    }

    fn make_request() -> ProcessSpawnRequest {
        ProcessSpawnRequest::new(
            ManagerId::Npm,
            TaskType::Refresh,
            ManagerAction::ListInstalled,
            CommandSpec::new("npm").args(["ls", "-g"]),
        )
    }

    #[test]
    fn run_and_collect_stdout_uses_lossy_decode_for_non_utf8_stdout() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        let _guard = runtime.enter();

        let now = SystemTime::now();
        let executor = Arc::new(StaticExecutor {
            output: ProcessOutput {
                status: ProcessExitStatus::ExitCode(0),
                stdout: vec![b'f', b'o', 0x80, b'o'],
                stderr: Vec::new(),
                started_at: now,
                finished_at: now,
            },
        });

        let stdout =
            run_and_collect_stdout(executor.as_ref(), make_request()).expect("stdout should decode");
        assert!(
            stdout.contains("fo"),
            "decoded output should preserve readable prefix: {stdout}"
        );
        assert!(
            stdout.contains('\u{fffd}'),
            "decoded output should include replacement marker for invalid utf8: {stdout:?}"
        );
    }

    #[test]
    fn run_and_collect_stdout_preserves_process_failure_shape() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        let _guard = runtime.enter();

        let now = SystemTime::now();
        let executor = Arc::new(StaticExecutor {
            output: ProcessOutput {
                status: ProcessExitStatus::ExitCode(127),
                stdout: Vec::new(),
                stderr: b"command not found".to_vec(),
                started_at: now,
                finished_at: now,
            },
        });

        let error =
            run_and_collect_stdout(executor.as_ref(), make_request()).expect_err("should fail");
        assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
        assert_eq!(error.manager, Some(ManagerId::Npm));
        assert_eq!(error.task, Some(TaskType::Refresh));
        assert_eq!(error.action, Some(ManagerAction::ListInstalled));
    }
}
