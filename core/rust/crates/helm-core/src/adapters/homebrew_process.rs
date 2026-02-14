use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::homebrew::{
    HomebrewDetectOutput, HomebrewSource, homebrew_cleanup_request, homebrew_config_request,
    homebrew_detect_request, homebrew_install_request, homebrew_list_installed_request,
    homebrew_list_outdated_request, homebrew_pin_request, homebrew_search_local_request,
    homebrew_uninstall_request, homebrew_unpin_request, homebrew_upgrade_request,
    parse_homebrew_version,
};
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::{ManagerId, SearchQuery};

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
        let executable_path = which_executable(
            self.executor.as_ref(),
            "brew",
            &["/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::HomebrewFormula,
        );

        let request = homebrew_detect_request(None);
        let mut version_request = self.configure_request(request);
        if let Some(path) = executable_path.as_ref() {
            version_request.command.program = path.clone();
        }

        let mut version_output =
            run_and_collect_version_output(self.executor.as_ref(), version_request);
        if parse_homebrew_version(&version_output).is_none() {
            let mut config_request = self.configure_request(homebrew_config_request(None));
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

    fn install_formula(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_install_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall_formula(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade_formula(&self, name: Option<&str>) -> AdapterResult<String> {
        let target = name.unwrap_or("__all__");
        let request = self.configure_request(homebrew_upgrade_request(None, target));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn cleanup_formula(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_cleanup_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn pin_formula(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_pin_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn unpin_formula(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_unpin_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

impl ProcessHomebrewSource {
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
