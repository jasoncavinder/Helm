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

    fn prepared_upgrade_request(
        &self,
        name: &str,
        version: &str,
    ) -> AdapterResult<(
        std::path::PathBuf,
        super::cargo_receipt::CargoUpgradeReceipt,
        ProcessSpawnRequest,
    )> {
        use super::cargo_receipt::CargoUpgradeReceipt;
        let (home, root, mut request) =
            self.registry_request(cargo_upgrade_request(None, name, version))?;
        let receipt = CargoUpgradeReceipt::load(root, name)?;
        request.command = receipt.apply(request.command);
        Ok((home, receipt, request))
    }

    fn registry_request(
        &self,
        request: ProcessSpawnRequest,
    ) -> AdapterResult<(std::path::PathBuf, std::path::PathBuf, ProcessSpawnRequest)> {
        use super::cargo_receipt::{install_root, receipt_error};
        if self.cargo_home.is_none()
            && std::env::var_os("CARGO_HOME")
                .is_some_and(|home| !std::path::Path::new(&home).is_absolute())
        {
            return Err(receipt_error(
                "Cargo home must be absolute for a verified mutation",
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
        let mut request = self.configure_request(request);
        request.command = request.command.env("CARGO_HOME", home.to_string_lossy());
        if let Some(rustup) = super::cargo_published_lock::rustup_proxy(&request.command.program) {
            let mut selected = request.clone();
            selected.command.program = rustup;
            selected.command.args = vec!["show".into(), "active-toolchain".into()];
            selected.timeout = Some(std::time::Duration::from_secs(10));
            selected.private_output_limit = Some(16 * 1024);
            let output = run_and_collect_stdout(self.executor.as_ref(), selected)?;
            request.command = request.command.env(
                "RUSTUP_TOOLCHAIN",
                super::cargo_published_lock::active_toolchain(&output)?,
            );
        }
        Ok((home, root, request))
    }

    fn install_locked(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        use super::cargo_fresh_install::FreshInstall;
        use super::cargo_published_lock::{PublishedCargoLock, info_working_directory};
        use super::cargo_receipt::{install_root, receipt_error};
        crate::adapters::validate_package_identifier(
            ManagerId::Cargo,
            ManagerAction::Install,
            name,
        )?;
        let (home, root, mut request) =
            self.registry_request(cargo_install_request(None, name, version))?;
        request.reviewed_program = Some(request.command.program.clone());
        let working_dir = info_working_directory()?;
        let version = if let Some(version) = version {
            version.to_owned()
        } else {
            let mut search = request.clone();
            search.command.args = vec![
                "search".into(),
                "--limit".into(),
                "1".into(),
                "--color".into(),
                "never".into(),
                "--registry".into(),
                "crates-io".into(),
                name.into(),
            ];
            search.command.working_dir = Some(working_dir.clone());
            search.timeout = Some(std::time::Duration::from_secs(30));
            search.private_output_limit = Some(256 * 1024);
            let output = run_and_collect_stdout(self.executor.as_ref(), search)?;
            parse_cargo_search_version(&output, name)
                .ok_or_else(|| receipt_error("No exact crates.io install candidate was found"))?
        };
        if semver::Version::parse(&version).is_err() {
            return Err(receipt_error("An exact Cargo install version is required"));
        }
        let Some(fresh) = FreshInstall::prepare(root.clone(), name)? else {
            // Reinstallation must not reset existing native features/profile/target.
            let binding = self.review_upgrade_token(name, &version)?;
            return self.upgrade_reviewed(name, &version, Some(&binding));
        };
        // An explicitly requested installation may create its storage directories,
        // but never invent receipts or replace existing binaries during preflight.
        std::fs::create_dir_all(&home)
            .and_then(|_| std::fs::create_dir_all(&root))
            .map_err(|_| receipt_error("Cargo installation storage could not be prepared"))?;
        request.command.args = vec![
            "install".into(),
            name.into(),
            "--version".into(),
            version.clone(),
            "--locked".into(),
            "--registry".into(),
            "crates-io".into(),
            "--root".into(),
            root.to_string_lossy().into_owned(),
            "--profile".into(),
            "release".into(),
        ];
        request.command.working_dir = Some(working_dir.clone());
        let identity = self.execution_fingerprint(&home, &root, &request)?;
        let mut info = request.clone();
        info.command.args = vec![
            "info".into(),
            "--registry".into(),
            "crates-io".into(),
            "--color".into(),
            "never".into(),
            format!("{name}@{version}"),
        ];
        info.timeout = Some(std::time::Duration::from_secs(120));
        info.private_output_limit = Some(256 * 1024);
        run_and_collect_stdout(self.executor.as_ref(), info)?;
        let published = PublishedCargoLock::load(self.executor.as_ref(), &home, name, &version)?;
        if install_root(&home, Some(root.clone()))? != root
            || self.execution_fingerprint(&home, &root, &request)? != identity
        {
            return Err(receipt_error(
                "Cargo execution scope changed before installation",
            ));
        }
        fresh.revalidate()?;
        published.revalidate()?;
        let output = run_and_collect_stdout(self.executor.as_ref(), request.clone())?;
        let verified = (|| -> AdapterResult<()> {
            if install_root(&home, Some(root.clone()))? != root
                || self.execution_fingerprint(&home, &root, &request)? != identity
            {
                return Err(receipt_error(
                    "Cargo execution scope changed during installation",
                ));
            }
            published.revalidate()?;
            fresh.verify(&version)
        })();
        verified.map_err(|mut error| {
            error.kind = CoreErrorKind::ProcessFailure;
            error.message = format!(
                "[cargo_receipt_unsupported] Cargo installation may have changed and requires verification: {}",
                error.message
            );
            error
        })?;
        Ok(output)
    }

    pub fn review_upgrade_token(&self, name: &str, version: &str) -> AdapterResult<String> {
        self.review_upgrade_token_for_inventory(name, version, None)
    }

    fn review_upgrade_token_for_inventory(
        &self,
        name: &str,
        version: &str,
        installed: Option<&str>,
    ) -> AdapterResult<String> {
        let (home, receipt, request) = self.prepared_upgrade_request(name, version)?;
        if installed.is_some_and(|installed| installed != receipt.installed_version()) {
            return Err(super::cargo_receipt::receipt_error(
                "Cargo inventory changed during update discovery",
            ));
        }
        self.request_fingerprint(&home, &receipt, &request, version)
    }

    fn request_fingerprint(
        &self,
        home: &std::path::Path,
        receipt: &super::cargo_receipt::CargoUpgradeReceipt,
        request: &ProcessSpawnRequest,
        version: &str,
    ) -> AdapterResult<String> {
        let toolchain_cargo = self.toolchain_cargo(request)?;
        super::cargo_review_scope::fingerprint(
            home,
            receipt,
            &request.command.program,
            request
                .command
                .env
                .get("RUSTUP_TOOLCHAIN")
                .map(String::as_str),
            toolchain_cargo.as_deref(),
            version,
        )
    }

    fn execution_fingerprint(
        &self,
        home: &std::path::Path,
        root: &std::path::Path,
        request: &ProcessSpawnRequest,
    ) -> AdapterResult<String> {
        let toolchain_cargo = self.toolchain_cargo(request)?;
        super::cargo_review_scope::execution_fingerprint(
            home,
            root,
            &request.command.program,
            request
                .command
                .env
                .get("RUSTUP_TOOLCHAIN")
                .map(String::as_str),
            toolchain_cargo.as_deref(),
        )
    }

    fn toolchain_cargo(
        &self,
        request: &ProcessSpawnRequest,
    ) -> AdapterResult<Option<std::path::PathBuf>> {
        let toolchain = request.command.env.get("RUSTUP_TOOLCHAIN");
        let toolchain_cargo = if let (Some(toolchain), Some(rustup)) = (
            toolchain,
            super::cargo_published_lock::rustup_proxy(&request.command.program),
        ) {
            let mut selected = request.clone();
            selected.command.program = rustup;
            selected.reviewed_program = Some(selected.command.program.clone());
            selected.command.args = vec![
                "which".into(),
                "--toolchain".into(),
                toolchain.clone(),
                "cargo".into(),
            ];
            selected.timeout = Some(std::time::Duration::from_secs(10));
            selected.private_output_limit = Some(16 * 1024);
            let output = run_and_collect_stdout(self.executor.as_ref(), selected)?;
            if output.trim().lines().count() != 1 {
                return Err(super::cargo_receipt::receipt_error(
                    "Cargo toolchain path is ambiguous",
                ));
            }
            Some(std::path::PathBuf::from(output.trim()))
        } else {
            None
        };
        Ok(toolchain_cargo)
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
        let payload =
            synthesize_outdated_payload(ManagerId::Cargo, &installed_raw, |crate_name| {
                let request = self.configure_request(cargo_search_single_request(None, crate_name));
                let search_output = run_and_collect_stdout(self.executor.as_ref(), request)?;
                Ok(parse_cargo_search_version(&search_output, crate_name))
            })?;
        let mut rows: Vec<serde_json::Value> = serde_json::from_str(&payload).map_err(|_| {
            super::cargo_receipt::receipt_error("Cargo candidates could not be decoded")
        })?;
        for row in &mut rows {
            let name = row["name"].as_str().unwrap_or_default();
            let version = row["candidate_version"].as_str().unwrap_or_default();
            let binding = match self.review_upgrade_token_for_inventory(
                name,
                version,
                row["installed_version"].as_str(),
            ) {
                Ok(binding) => binding,
                Err(error) if error.kind == CoreErrorKind::Cancelled => return Err(error),
                Err(_) => super::cargo_review_scope::UNAVAILABLE.into(),
            };
            row["package_identifier"] = serde_json::Value::String(binding);
        }
        serde_json::to_string(&rows).map_err(|_| {
            super::cargo_receipt::receipt_error("Cargo candidates could not be encoded")
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
        self.install_locked(name, version).map_err(|mut error| {
            error.action = Some(ManagerAction::Install);
            error.task = Some(TaskType::Install);
            error
        })
    }

    fn uninstall(&self, name: &str) -> AdapterResult<String> {
        let request = self.scope_request(cargo_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, name: &str, version: &str) -> AdapterResult<String> {
        self.upgrade_reviewed(name, version, None)
    }

    fn validate_upgrade_scope(
        &self,
        name: &str,
        version: &str,
        binding: Option<&str>,
    ) -> AdapterResult<()> {
        if let Some(binding) = binding
            && (!super::cargo_review_scope::is_token(binding)
                || self.review_upgrade_token(name, version)? != binding)
        {
            return Err(super::cargo_receipt::receipt_error(
                "Cargo installation changed or has no reviewed scope; refresh and review again",
            ));
        }
        Ok(())
    }

    fn upgrade_reviewed(
        &self,
        name: &str,
        version: &str,
        binding: Option<&str>,
    ) -> AdapterResult<String> {
        use super::cargo_published_lock::{PublishedCargoLock, info_working_directory};
        use super::cargo_receipt::{install_root, receipt_error};
        let (home, receipt, mut request) = self.prepared_upgrade_request(name, version)?;
        if let Some(binding) = binding {
            if !super::cargo_review_scope::is_token(binding)
                || self.request_fingerprint(&home, &receipt, &request, version)? != binding
            {
                return Err(receipt_error(
                    "Cargo installation changed; refresh and review again",
                ));
            }
            request.reviewed_program = Some(request.command.program.clone());
        }
        let execution_identity = binding
            .map(|_| self.execution_fingerprint(&home, receipt.root(), &request))
            .transpose()?;
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
        self.validate_upgrade_scope(name, version, binding)?;
        let output = run_and_collect_stdout(self.executor.as_ref(), request)?;
        if let Some(expected) = execution_identity {
            let unchanged = self.prepared_upgrade_request(name, version).and_then(
                |(home, current, request)| {
                    self.execution_fingerprint(&home, current.root(), &request)
                },
            );
            if unchanged.as_ref().ok() != Some(&expected) {
                let mut error = receipt_error(
                    "Cargo completed but its reviewed execution scope changed; the installation may have changed and requires verification",
                );
                error.kind = CoreErrorKind::ProcessFailure;
                return Err(error);
            }
        }
        published.revalidate().map_err(|mut error| {
            error.kind = CoreErrorKind::ProcessFailure;
            error.message = "[cargo_published_lock_unavailable] Cargo completed, but its published metadata changed during execution; the installation may have changed and requires verification".into();
            error
        })?;
        receipt.verify(version)?;
        Ok(output)
    }
}
