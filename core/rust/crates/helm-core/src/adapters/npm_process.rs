use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::npm::{
    NpmDetectOutput, NpmSource, npm_detect_request, npm_install_request,
    npm_list_installed_request, npm_list_outdated_request, npm_search_request,
    npm_uninstall_request, npm_upgrade_request,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{
    ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest, spawn_validated,
};
use crate::models::{CoreError, CoreErrorKind, ManagerId, SearchQuery};

pub struct ProcessNpmSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessNpmSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        // XPC services have a constrained PATH; include common npm binary locations.
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:{path}");
        request.command = request
            .command
            .env("PATH", new_path)
            .env("NPM_CONFIG_UPDATE_NOTIFIER", "false")
            .env("NPM_CONFIG_FUND", "false")
            .env("NPM_CONFIG_AUDIT", "false");

        if request.command.program.to_str() == Some("npm")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "npm",
                &["/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::Npm,
            )
        {
            request.command.program = exe;
        }

        request
    }

    fn run_and_collect_stdout_accepting(
        &self,
        request: ProcessSpawnRequest,
        allowed_exit_codes: &[i32],
    ) -> AdapterResult<String> {
        let manager = request.manager;
        let task_type = request.task_type;
        let action = request.action;

        let process = spawn_validated(self.executor.as_ref(), request)?;

        let handle = tokio::runtime::Handle::current();
        let output: ProcessOutput = handle.block_on(process.wait())?;

        match output.status {
            ProcessExitStatus::ExitCode(code)
                if code == 0 || allowed_exit_codes.contains(&code) =>
            {
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
}

impl NpmSource for ProcessNpmSource {
    fn detect(&self) -> AdapterResult<NpmDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "npm",
            &["/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::Npm,
        );

        let request = self.configure_request(npm_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(NpmDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed_global(&self) -> AdapterResult<String> {
        let request = self.configure_request(npm_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated_global(&self) -> AdapterResult<String> {
        // npm uses exit code 1 to indicate outdated packages were found.
        let request = self.configure_request(npm_list_outdated_request(None));
        self.run_and_collect_stdout_accepting(request, &[1])
    }

    fn search(&self, query: &str) -> AdapterResult<String> {
        let search_query = SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let request = self.configure_request(npm_search_request(None, &search_query));
        // npm may return exit code 1 for no matches while still writing JSON/JSONL output.
        self.run_and_collect_stdout_accepting(request, &[1])
    }

    fn install_global(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(npm_install_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall_global(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(npm_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade_global(&self, name: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(npm_upgrade_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
