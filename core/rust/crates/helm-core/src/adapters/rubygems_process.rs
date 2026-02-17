use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::adapters::rubygems::{
    RubyGemsDetectOutput, RubyGemsSource, rubygems_detect_request, rubygems_install_request,
    rubygems_list_installed_request, rubygems_list_outdated_request, rubygems_search_request,
    rubygems_uninstall_request, rubygems_upgrade_request,
};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessRubyGemsSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessRubyGemsSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:{path}");
        request.command = request.command.env("PATH", new_path);

        if request.command.program.to_str() == Some("gem")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "gem",
                &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"],
                ManagerId::RubyGems,
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

impl RubyGemsSource for ProcessRubyGemsSource {
    fn detect(&self) -> AdapterResult<RubyGemsDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "gem",
            &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"],
            ManagerId::RubyGems,
        );

        let request = self.configure_request(rubygems_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(RubyGemsDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        self.run_stdout(rubygems_list_installed_request(None))
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        self.run_stdout(rubygems_list_outdated_request(None))
    }

    fn search(&self, query: &str) -> AdapterResult<String> {
        let search_query = crate::models::SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        self.run_stdout(rubygems_search_request(None, &search_query))
    }

    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        self.run_stdout(rubygems_install_request(None, name, version))
    }

    fn uninstall(&self, name: &str) -> AdapterResult<String> {
        self.run_stdout(rubygems_uninstall_request(None, name))
    }

    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String> {
        self.run_stdout(rubygems_upgrade_request(None, name))
    }
}
