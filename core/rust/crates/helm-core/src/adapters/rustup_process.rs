use std::path::Path;
use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::rustup::{
    RustupDetectOutput, RustupSource, rustup_check_request, rustup_detect_request,
    rustup_self_uninstall_request, rustup_toolchain_list_request,
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
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let cargo_bin = format!("{home}/.cargo/bin");
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{cargo_bin}:{path}");

        request.command = request.command.env("PATH", new_path);

        // Resolve absolute path to binary if possible
        if request.command.program.to_str() == Some("rustup") {
            // 1. Try direct path resolution first (most reliable)
            let direct_path = std::path::Path::new(&cargo_bin).join("rustup");
            if direct_path.exists() {
                request.command.program = direct_path;
            } else {
                // 2. Fallback to `which` if not found in standard location
                if let Some(exe) = which_executable(
                    self.executor.as_ref(),
                    "rustup",
                    &[cargo_bin.as_str()],
                    ManagerId::Rustup,
                ) {
                    request.command.program = exe;
                }
            }
        }

        request
    }
}

impl RustupSource for ProcessRustupSource {
    fn detect(&self) -> AdapterResult<RustupDetectOutput> {
        // Phase 1: instant filesystem check
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let cargo_bin = format!("{home}/.cargo/bin");
        let direct_path = Path::new(&cargo_bin).join("rustup");
        let executable_path = if direct_path.exists() {
            Some(direct_path)
        } else {
            which_executable(
                self.executor.as_ref(),
                "rustup",
                &[&cargo_bin],
                ManagerId::Rustup,
            )
        };

        // Phase 2: best-effort version (timeout is non-fatal)
        let request = rustup_detect_request(None);
        let version_request = self.configure_request(request);
        let version_output =
            run_and_collect_stdout(self.executor.as_ref(), version_request).unwrap_or_default();

        Ok(RustupDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn toolchain_list(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_toolchain_list_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn check(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_check_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn self_uninstall(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_self_uninstall_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
