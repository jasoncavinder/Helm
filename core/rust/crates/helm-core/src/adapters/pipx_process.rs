use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::pipx::{
    PipxDetectOutput, PipxSource, pipx_detect_request, pipx_install_request,
    pipx_list_outdated_request, pipx_list_request, pipx_uninstall_request, pipx_upgrade_request,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessPipxSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessPipxSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        // XPC services have a stripped PATH; include common pipx install locations.
        let home = std::env::var("HOME").unwrap_or_default();
        let home_bin = format!("{home}/.local/bin");
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{home_bin}:/opt/homebrew/bin:/usr/local/bin:{path}");

        request.command = request.command.env("PATH", new_path);

        if request.command.program.to_str() == Some("pipx")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "pipx",
                &[home_bin.as_str(), "/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::Pipx,
            )
        {
            request.command.program = exe;
        }

        request
    }
}

impl PipxSource for ProcessPipxSource {
    fn detect(&self) -> AdapterResult<PipxDetectOutput> {
        let home = std::env::var("HOME").unwrap_or_default();
        let home_bin = format!("{home}/.local/bin");

        let executable_path = which_executable(
            self.executor.as_ref(),
            "pipx",
            &[home_bin.as_str(), "/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::Pipx,
        );

        let request = self.configure_request(pipx_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(PipxDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(pipx_list_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = self.configure_request(pipx_list_outdated_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(pipx_install_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(pipx_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(pipx_upgrade_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
