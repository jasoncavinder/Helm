use std::sync::Arc;

use crate::adapters::cargo::{
    CargoDetectOutput, CargoSource, cargo_detect_request, cargo_install_request,
    cargo_list_installed_request, cargo_search_request, cargo_search_single_request,
    cargo_uninstall_request, cargo_upgrade_request, parse_cargo_search_version,
};
use crate::adapters::cargo_outdated::synthesize_outdated_payload;
use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, SearchQuery, TaskType};

pub struct ProcessCargoSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessCargoSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
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

        if request.command.program.to_str() == Some("cargo")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "cargo",
                &[cargo_bin.as_str(), "/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::Cargo,
            )
        {
            request.command.program = exe;
        }

        request
    }
}

impl CargoSource for ProcessCargoSource {
    fn detect(&self) -> AdapterResult<CargoDetectOutput> {
        let cargo_bin = Self::cargo_bin_dir();

        let executable_path = which_executable(
            self.executor.as_ref(),
            "cargo",
            &[cargo_bin.as_str(), "/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::Cargo,
        );

        let version_output = if let Some(path) = executable_path.as_ref() {
            let mut request = self.configure_request(cargo_detect_request(None));
            request.command.program = path.clone();
            // An existing but unusable proxy is a failed check, not empty inventory.
            run_and_collect_stdout(self.executor.as_ref(), request)?
        } else {
            String::new()
        };
        if executable_path.is_some() && super::cargo::parse_cargo_version(&version_output).is_none()
        {
            return Err(CoreError {
                manager: Some(ManagerId::Cargo), task: Some(TaskType::Detection),
                action: Some(ManagerAction::Detect), kind: CoreErrorKind::ParseFailure,
                message: "The selected Cargo executable returned an unrecognized version; cached inventory is retained.".into(),
            });
        }

        Ok(CargoDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(cargo_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let installed_raw = self.list_installed()?;
        // cargo has no built-in global outdated list command for installed binaries.
        synthesize_outdated_payload(ManagerId::Cargo, &installed_raw, |crate_name| {
            let request = self.configure_request(cargo_search_single_request(None, crate_name));
            let search_output = run_and_collect_stdout(self.executor.as_ref(), request)?;
            Ok(parse_cargo_search_version(&search_output, crate_name))
        })
    }

    fn search(&self, query: &str) -> AdapterResult<String> {
        let search_query = SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let request = self.configure_request(cargo_search_request(None, &search_query));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(cargo_install_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(cargo_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, name: &str, version: &str) -> AdapterResult<String> {
        let request = self.configure_request(cargo_upgrade_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
