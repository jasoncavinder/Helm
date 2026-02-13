use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::mas::{
    MasSource, mas_detect_request, mas_list_installed_request, mas_list_outdated_request,
};
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessMasSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessMasSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:{path}");
        request.command = request.command.env("PATH", new_path);

        if request.command.program.to_str() == Some("mas")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "mas",
                &["/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::Mas,
            )
        {
            request.command.program = exe;
        }

        request
    }
}

impl MasSource for ProcessMasSource {
    fn detect(&self) -> AdapterResult<String> {
        let request = self.configure_request(mas_detect_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(mas_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = self.configure_request(mas_list_outdated_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
