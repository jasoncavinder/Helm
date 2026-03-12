use std::collections::BTreeSet;
use std::sync::Arc;

use crate::adapters::cargo::{parse_cargo_installed, parse_cargo_search_version};
use crate::adapters::cargo_binstall::{
    CargoBinstallDetectOutput, CargoBinstallSource, cargo_binstall_detect_request,
    cargo_binstall_install_request, cargo_binstall_list_installed_request,
    cargo_binstall_search_request, cargo_binstall_search_single_request,
    cargo_binstall_uninstall_request, cargo_binstall_upgrade_request,
};
use crate::adapters::cargo_outdated::synthesize_outdated_payload_for_packages;
use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::{ManagerId, SearchQuery};
use crate::persistence::PackageStore;

pub struct ProcessCargoBinstallSource {
    executor: Arc<dyn ProcessExecutor>,
    package_store: Arc<dyn PackageStore>,
}

impl ProcessCargoBinstallSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>, package_store: Arc<dyn PackageStore>) -> Self {
        Self {
            executor,
            package_store,
        }
    }

    fn cargo_bin_dir() -> String {
        std::env::var_os("CARGO_HOME")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default()
                    .join(".cargo")
            })
            .join("bin")
            .to_string_lossy()
            .to_string()
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let cargo_bin = Self::cargo_bin_dir();
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
        let cargo_bin = Self::cargo_bin_dir();

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

    fn tracked_package_names(&self) -> AdapterResult<BTreeSet<String>> {
        Ok(self
            .package_store
            .list_installed()?
            .into_iter()
            .filter(|package| package.package.manager == ManagerId::CargoBinstall)
            .map(|package| package.package.name)
            .collect())
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(cargo_binstall_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let tracked = self.tracked_package_names()?;
        if tracked.is_empty() {
            return Ok("[]".to_string());
        }

        let installed_raw = self.list_installed()?;
        let installed = parse_cargo_installed(&installed_raw)?
            .into_iter()
            .filter(|package| tracked.contains(&package.package.name))
            .collect();

        synthesize_outdated_payload_for_packages(
            ManagerId::CargoBinstall,
            installed,
            |crate_name| {
                let request =
                    self.configure_request(cargo_binstall_search_single_request(None, crate_name));
                let search_output = run_and_collect_stdout(self.executor.as_ref(), request)?;
                Ok(parse_cargo_search_version(&search_output, crate_name))
            },
        )
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

        let outdated_raw = self.list_outdated()?;
        let outdated =
            crate::adapters::cargo::parse_cargo_outdated(&outdated_raw).map_err(|mut error| {
                error.manager = Some(ManagerId::CargoBinstall);
                error
            })?;
        for package in outdated {
            let request =
                self.configure_request(cargo_binstall_upgrade_request(None, &package.package.name));
            let _ = run_and_collect_stdout(self.executor.as_ref(), request)?;
        }

        Ok(String::new())
    }
}
