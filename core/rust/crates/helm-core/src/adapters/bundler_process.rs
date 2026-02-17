use std::sync::Arc;

use crate::adapters::bundler::{
    BundlerDetectOutput, BundlerSource, bundler_detect_request, bundler_install_request,
    bundler_list_installed_request, bundler_list_outdated_request, bundler_uninstall_request,
    bundler_upgrade_request,
};
use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessBundlerSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessBundlerSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:/usr/bin:{path}");
        request.command = request.command.env("PATH", new_path);

        let replacement = match request.command.program.to_str() {
            Some("bundle") => Some(which_executable(
                self.executor.as_ref(),
                "bundle",
                &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"],
                ManagerId::Bundler,
            )),
            Some("gem") => Some(which_executable(
                self.executor.as_ref(),
                "gem",
                &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"],
                ManagerId::Bundler,
            )),
            _ => None,
        };

        if let Some(Some(exe)) = replacement {
            request.command.program = exe;
        }

        request
    }

    fn run_stdout(&self, request: ProcessSpawnRequest) -> AdapterResult<String> {
        let request = self.configure_request(request);
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

impl BundlerSource for ProcessBundlerSource {
    fn detect(&self) -> AdapterResult<BundlerDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "bundle",
            &["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"],
            ManagerId::Bundler,
        );

        let request = self.configure_request(bundler_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(BundlerDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        self.run_stdout(bundler_list_installed_request(None))
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        self.run_stdout(bundler_list_outdated_request(None))
    }

    fn install(&self, version: Option<&str>) -> AdapterResult<String> {
        self.run_stdout(bundler_install_request(None, version))
    }

    fn uninstall(&self) -> AdapterResult<String> {
        self.run_stdout(bundler_uninstall_request(None))
    }

    fn upgrade(&self) -> AdapterResult<String> {
        self.run_stdout(bundler_upgrade_request(None))
    }
}
