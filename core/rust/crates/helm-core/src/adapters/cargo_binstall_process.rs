use std::sync::Arc;

use crate::adapters::cargo::{parse_cargo_installed, parse_cargo_search_version};
use crate::adapters::cargo_binstall::{
    CargoBinstallDetectOutput, CargoBinstallSource, cargo_binstall_detect_request,
    cargo_binstall_install_request, cargo_binstall_list_installed_request,
    cargo_binstall_search_request, cargo_binstall_search_single_request,
    cargo_binstall_uninstall_request, cargo_binstall_upgrade_request,
};
use crate::adapters::cargo_outdated::synthesize_outdated_payload;
use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::{ManagerId, SearchQuery};

pub struct ProcessCargoBinstallSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessCargoBinstallSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let home = std::env::var("HOME").unwrap_or_default();
        let cargo_bin = format!("{home}/.cargo/bin");
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{cargo_bin}:/opt/homebrew/bin:/usr/local/bin:{path}");

        request.command = request.command.env("PATH", new_path);

        if request.command.program.to_str() == Some("cargo-binstall")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "cargo-binstall",
                &[cargo_bin.as_str(), "/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::CargoBinstall,
            )
        {
            request.command.program = exe;
        }

        if request.command.program.to_str() == Some("cargo")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "cargo",
                &[cargo_bin.as_str(), "/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::CargoBinstall,
            )
        {
            request.command.program = exe;
        }

        request
    }
}

impl CargoBinstallSource for ProcessCargoBinstallSource {
    fn detect(&self) -> AdapterResult<CargoBinstallDetectOutput> {
        let home = std::env::var("HOME").unwrap_or_default();
        let cargo_bin = format!("{home}/.cargo/bin");

        let executable_path = which_executable(
            self.executor.as_ref(),
            "cargo-binstall",
            &[cargo_bin.as_str(), "/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::CargoBinstall,
        );

        let request = self.configure_request(cargo_binstall_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(CargoBinstallDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(cargo_binstall_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let installed_raw = self.list_installed()?;
        synthesize_outdated_payload(ManagerId::CargoBinstall, &installed_raw, |crate_name| {
            let request =
                self.configure_request(cargo_binstall_search_single_request(None, crate_name));
            let search_output = run_and_collect_stdout(self.executor.as_ref(), request)?;
            Ok(parse_cargo_search_version(&search_output, crate_name))
        })
    }

    fn search(&self, query: &str) -> AdapterResult<String> {
        let search_query = SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let request = self.configure_request(cargo_binstall_search_request(None, &search_query));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(cargo_binstall_install_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(cargo_binstall_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String> {
        if let Some(name) = name {
            let request = self.configure_request(cargo_binstall_upgrade_request(None, name));
            return run_and_collect_stdout(self.executor.as_ref(), request);
        }

        let installed_raw = self.list_installed()?;
        let installed = parse_cargo_installed(&installed_raw)?;
        for package in installed {
            let request =
                self.configure_request(cargo_binstall_upgrade_request(None, &package.package.name));
            let _ = run_and_collect_stdout(self.executor.as_ref(), request)?;
        }

        Ok(String::new())
    }
}
