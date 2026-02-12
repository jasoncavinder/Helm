use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::mise::{
    MiseSource, mise_detect_request, mise_list_installed_request, mise_list_outdated_request,
};
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessMiseSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessMiseSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let home = std::env::var("HOME").unwrap_or_default();
        let mise_bin = format!("{home}/.local/bin");
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{mise_bin}:{path}");

        request.command = request.command.env("PATH", new_path);
        request
    }
}

impl MiseSource for ProcessMiseSource {
    fn detect(&self) -> AdapterResult<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        let mise_bin = format!("{home}/.local/bin");
        let _ = which_executable(
            self.executor.as_ref(),
            "mise",
            &[mise_bin.as_str()],
            ManagerId::Mise,
        );

        let request = mise_detect_request(None);
        let version_request = self.configure_request(request);
        run_and_collect_stdout(self.executor.as_ref(), version_request)
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(mise_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = self.configure_request(mise_list_outdated_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
