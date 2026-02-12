use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::homebrew::{
    HomebrewDetectOutput, HomebrewSource, homebrew_detect_request, homebrew_list_installed_request,
    homebrew_list_outdated_request, homebrew_search_local_request,
};
use crate::adapters::manager::AdapterResult;
use crate::execution::{
    CommandSpec, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    spawn_validated,
};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, SearchQuery, TaskType};

pub struct ProcessHomebrewSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessHomebrewSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }
}

impl HomebrewSource for ProcessHomebrewSource {
    fn detect(&self) -> AdapterResult<HomebrewDetectOutput> {
        // We first try to find brew in the enhanced PATH
        let which_request = ProcessSpawnRequest::new(
            ManagerId::HomebrewFormula,
            TaskType::Detection,
            ManagerAction::Detect,
            CommandSpec::new("/usr/bin/which").arg("brew").env(
                "PATH",
                "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            ),
        );

        let executable_path = match run_and_collect_stdout(self.executor.as_ref(), which_request) {
            Ok(path) => {
                let trimmed = path.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                }
            }
            Err(_) => None,
        };

        let request = homebrew_detect_request(None);
        let version_request = self.configure_request(request);

        let version_output =
            run_and_collect_stdout(self.executor.as_ref(), version_request).unwrap_or_default();

        Ok(HomebrewDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed_formulae(&self) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated_formulae(&self) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_list_outdated_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn search_local_formulae(&self, query: &str) -> AdapterResult<String> {
        let search_query = SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let request = self.configure_request(homebrew_search_local_request(None, &search_query));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

impl ProcessHomebrewSource {
    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        // XPC services have a stripped down PATH. We must explicitly add Homebrew paths.
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:{path}");

        request.command = request.command.env("PATH", new_path);
        request
    }
}

fn run_and_collect_stdout(
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
