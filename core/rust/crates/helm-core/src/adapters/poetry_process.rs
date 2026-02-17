use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::poetry::{
    PoetryDetectOutput, PoetrySource, poetry_detect_request, poetry_install_plugin_request,
    poetry_list_installed_request, poetry_list_outdated_plugins_request,
    poetry_uninstall_plugin_request, poetry_upgrade_plugins_request,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessPoetrySource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessPoetrySource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:{path}");
        request.command = request
            .command
            .env("PATH", new_path)
            .env("POETRY_NO_INTERACTION", "1");

        if request.command.program.to_str() == Some("poetry")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "poetry",
                &["/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::Poetry,
            )
        {
            request.command.program = exe;
        }

        request
    }

    fn run_stdout(&self, request: ProcessSpawnRequest) -> AdapterResult<String> {
        let request = self.configure_request(request);
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

impl PoetrySource for ProcessPoetrySource {
    fn detect(&self) -> AdapterResult<PoetryDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "poetry",
            &["/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::Poetry,
        );

        let request = self.configure_request(poetry_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(PoetryDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_plugins(&self) -> AdapterResult<String> {
        self.run_stdout(poetry_list_installed_request(None))
    }

    fn list_outdated_plugins(&self) -> AdapterResult<String> {
        self.run_stdout(poetry_list_outdated_plugins_request(None))
    }

    fn install_plugin(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        self.run_stdout(poetry_install_plugin_request(None, name, version))
    }

    fn uninstall_plugin(&self, name: &str) -> AdapterResult<String> {
        self.run_stdout(poetry_uninstall_plugin_request(None, name))
    }

    fn upgrade_plugins(&self, name: Option<&str>) -> AdapterResult<String> {
        self.run_stdout(poetry_upgrade_plugins_request(None, name))
    }
}
