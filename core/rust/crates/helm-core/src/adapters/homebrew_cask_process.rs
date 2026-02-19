use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::homebrew::parse_homebrew_version;
use crate::adapters::homebrew_cask::{
    HomebrewCaskDetectOutput, HomebrewCaskSource, homebrew_cask_config_request,
    homebrew_cask_detect_request, homebrew_cask_list_installed_request,
    homebrew_cask_list_outdated_request,
};
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessHomebrewCaskSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessHomebrewCaskSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        // XPC services have a stripped down PATH. We must explicitly add Homebrew paths.
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:{path}");

        request.command = request
            .command
            .env("PATH", new_path)
            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .env("HOMEBREW_NO_ENV_HINTS", "1");
        request
    }
}

impl HomebrewCaskSource for ProcessHomebrewCaskSource {
    fn detect(&self) -> AdapterResult<HomebrewCaskDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "brew",
            &["/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::HomebrewCask,
        );

        let request = homebrew_cask_detect_request(None);
        let mut version_request = self.configure_request(request);
        if let Some(path) = executable_path.as_ref() {
            version_request.command.program = path.clone();
        }

        let mut version_output =
            run_and_collect_version_output(self.executor.as_ref(), version_request);
        if parse_homebrew_version(&version_output).is_none() {
            let mut config_request = self.configure_request(homebrew_cask_config_request(None));
            if let Some(path) = executable_path.as_ref() {
                config_request.command.program = path.clone();
            }
            let config_output =
                run_and_collect_version_output(self.executor.as_ref(), config_request);
            if !config_output.trim().is_empty() {
                if !version_output.trim().is_empty() {
                    version_output.push('\n');
                }
                version_output.push_str(&config_output);
            }
        }

        Ok(HomebrewCaskDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed_casks(&self) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_cask_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated_casks(&self) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_cask_list_outdated_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
