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
    installation_root: Option<std::path::PathBuf>,
    cargo_home: Option<std::path::PathBuf>,
}

impl ProcessCargoSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self {
            executor,
            installation_root: None,
            cargo_home: None,
        }
    }

    /// Bind an explicitly selected installation root, including isolated certification scopes.
    pub fn with_installation_root(
        executor: Arc<dyn ProcessExecutor>,
        root: std::path::PathBuf,
    ) -> Self {
        Self {
            executor,
            installation_root: Some(root),
            cargo_home: None,
        }
    }

    /// Bind both metadata/cache and installation scopes without changing process-global state.
    pub fn with_installation_scope(
        executor: Arc<dyn ProcessExecutor>,
        root: std::path::PathBuf,
        cargo_home: std::path::PathBuf,
    ) -> Self {
        Self {
            executor,
            installation_root: Some(root),
            cargo_home: Some(cargo_home),
        }
    }

    fn cargo_home(&self) -> std::path::PathBuf {
        if let Some(home) = &self.cargo_home {
            return home.clone();
        }
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
    }

    fn cargo_bin_dir(&self) -> String {
        self.cargo_home().join("bin").to_string_lossy().to_string()
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let cargo_bin = self.cargo_bin_dir();
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{cargo_bin}:/opt/homebrew/bin:/usr/local/bin:{path}");

        request.command = request.command.env("PATH", new_path);
        if let Some(home) = &self.cargo_home {
            request.command = request.command.env("CARGO_HOME", home.to_string_lossy());
        }

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

    fn scope_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        if let Some(root) = &self.installation_root {
            request.command = request.command.args(["--root", &root.to_string_lossy()]);
        }
        self.configure_request(request)
    }
}

impl CargoSource for ProcessCargoSource {
    fn detect(&self) -> AdapterResult<CargoDetectOutput> {
        let cargo_bin = self.cargo_bin_dir();

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
        let request = self.scope_request(cargo_list_installed_request(None));
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
        let request = self.scope_request(cargo_install_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(&self, name: &str) -> AdapterResult<String> {
        let request = self.scope_request(cargo_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, name: &str, version: &str) -> AdapterResult<String> {
        use super::cargo_published_lock::{PublishedCargoLock, info_working_directory};
        use super::cargo_receipt::{CargoUpgradeReceipt, install_root, receipt_error};
        if self.cargo_home.is_none()
            && std::env::var_os("CARGO_HOME")
                .is_some_and(|home| !std::path::Path::new(&home).is_absolute())
        {
            return Err(receipt_error(
                "Cargo home must be absolute for a verified upgrade",
            ));
        }
        if std::env::vars_os().any(|(key, value)| {
            let key = key.to_string_lossy();
            key.starts_with("CARGO_SOURCE_")
                || key == "CARGO_REGISTRIES_CRATES_IO_INDEX"
                || (key == "CARGO_REGISTRY_DEFAULT" && value != "crates-io")
        }) {
            return Err(receipt_error(
                "Cargo source environment overrides require manual review",
            ));
        }
        let explicit = self
            .installation_root
            .clone()
            .or_else(|| std::env::var_os("CARGO_INSTALL_ROOT").map(std::path::PathBuf::from));
        let home = self.cargo_home();
        if !home.is_absolute() || home.to_str().is_none() {
            return Err(receipt_error("Cargo home must be absolute UTF-8"));
        }
        let root = install_root(&home, explicit)?;
        let receipt = CargoUpgradeReceipt::load(root, name)?;
        let mut request = self.configure_request(cargo_upgrade_request(None, name, version));
        request.command = receipt.apply(request.command);
        request.command = request.command.env("CARGO_HOME", home.to_string_lossy());
        if let Some(rustup) = super::cargo_published_lock::rustup_proxy(&request.command.program) {
            // A Rustup proxy chooses by cwd. Bind the caller's selection before
            // running metadata outside its project so both stages use one toolchain.
            let mut selected = request.clone();
            selected.command.program = rustup;
            selected.command.args = vec!["show".into(), "active-toolchain".into()];
            selected.timeout = Some(std::time::Duration::from_secs(10));
            selected.private_output_limit = Some(16 * 1024);
            let output = run_and_collect_stdout(self.executor.as_ref(), selected)?;
            let toolchain = super::cargo_published_lock::active_toolchain(&output)?;
            request.command = request.command.env("RUSTUP_TOOLCHAIN", toolchain);
        }
        let mut info = request.clone();
        info.command.args = vec![
            "info".into(),
            "--registry".into(),
            "crates-io".into(),
            "--color".into(),
            "never".into(),
            format!("{name}@{version}"),
        ];
        info.command.working_dir = Some(info_working_directory()?);
        info.timeout = Some(std::time::Duration::from_secs(120));
        info.private_output_limit = Some(256 * 1024);
        receipt.revalidate()?;
        run_and_collect_stdout(self.executor.as_ref(), info)?;
        let published = PublishedCargoLock::load(self.executor.as_ref(), &home, name, version)?;
        // Source/config and install receipt can change while the candidate downloads.
        install_root(&home, Some(receipt.root().to_path_buf()))?;
        receipt.revalidate()?;
        published.revalidate()?;
        let output = run_and_collect_stdout(self.executor.as_ref(), request)?;
        published.revalidate().map_err(|mut error| {
            error.kind = CoreErrorKind::ProcessFailure;
            error.message = "[cargo_published_lock_unavailable] Cargo completed, but its published metadata changed during execution; the installation may have changed and requires verification".into();
            error
        })?;
        receipt.verify(version)?;
        Ok(output)
    }
}
