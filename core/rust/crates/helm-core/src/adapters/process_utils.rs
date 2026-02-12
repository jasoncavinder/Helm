use crate::adapters::manager::AdapterResult;
use crate::execution::{
    ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest, spawn_validated,
};
use crate::models::{CoreError, CoreErrorKind};

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
