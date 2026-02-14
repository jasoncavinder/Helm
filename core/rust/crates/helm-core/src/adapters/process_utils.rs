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
        ProcessExitStatus::ExitCode(0) => {
            String::from_utf8(output.stdout).map_err(|error| CoreError {
                manager: Some(manager),
                task: Some(task_type),
                action: Some(action),
                kind: CoreErrorKind::ParseFailure,
                message: format!("process stdout is not valid UTF-8: {error}"),
            })
        }
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
