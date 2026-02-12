use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::rustup::{
    RustupSource, rustup_check_request, rustup_detect_request, rustup_toolchain_list_request,
};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessRustupSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessRustupSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let home = std::env::var("HOME").unwrap_or_default();
        let cargo_bin = format!("{home}/.cargo/bin");
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{cargo_bin}:{path}");

        request.command = request.command.env("PATH", new_path);
        request
    }
}

impl RustupSource for ProcessRustupSource {
    fn detect(&self) -> AdapterResult<String> {
        let home = std::env::var("HOME").unwrap_or_default();
        let cargo_bin = format!("{home}/.cargo/bin");
        let _ = which_executable(
            self.executor.as_ref(),
            "rustup",
            &[cargo_bin.as_str()],
            ManagerId::Rustup,
        );

        let request = rustup_detect_request(None);
        let version_request = self.configure_request(request);
        run_and_collect_stdout(self.executor.as_ref(), version_request)
    }

    fn toolchain_list(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_toolchain_list_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn check(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_check_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
