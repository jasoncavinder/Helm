use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::pip::{
    PipDetectOutput, PipSource, pip_detect_request, pip_install_request, pip_list_outdated_request,
    pip_list_request, pip_uninstall_request, pip_upgrade_request,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessPipSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessPipSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:/usr/bin:{path}");
        request.command = request
            .command
            .env("PATH", new_path)
            .env("PIP_DISABLE_PIP_VERSION_CHECK", "1")
            .env("PIP_NO_INPUT", "1");

        if request.command.program.to_str() == Some("python3")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "python3",
                &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"],
                ManagerId::Pip,
            )
        {
            request.command.program = exe;
        }

        request
    }
}

impl PipSource for ProcessPipSource {
    fn detect(&self) -> AdapterResult<PipDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "python3",
            &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"],
            ManagerId::Pip,
        );

        let request = self.configure_request(pip_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(PipDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(pip_list_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = self.configure_request(pip_list_outdated_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(pip_install_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(pip_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(pip_upgrade_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
