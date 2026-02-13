use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::homebrew::{
    HomebrewDetectOutput, HomebrewSource, homebrew_detect_request, homebrew_install_request,
    homebrew_list_installed_request, homebrew_list_outdated_request, homebrew_search_local_request,
    homebrew_uninstall_request,
};
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
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

    fn install_formula(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_install_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall_formula(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(homebrew_uninstall_request(None, name));
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
