use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::asdf::{
    AsdfDetectOutput, AsdfInstallSource, AsdfSource, asdf_add_plugin_request,
    asdf_clone_install_request, asdf_detect_request, asdf_install_request, asdf_latest_request,
    asdf_list_current_request, asdf_list_installed_versions_request, asdf_list_plugins_request,
    asdf_search_plugins_request, asdf_self_update_request, asdf_set_home_version_request,
    asdf_uninstall_request,
};
use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::ProcessExecutor;
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, SearchQuery, TaskType};

const ASDF_PATH_ROOTS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

fn core_error(
    kind: CoreErrorKind,
    task: TaskType,
    action: ManagerAction,
    message: impl Into<String>,
) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Asdf),
        task: Some(task),
        action: Some(action),
        kind,
        message: message.into(),
    }
}

fn default_home_asdf_root() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .map(|home| home.join(".asdf"))
}

fn configured_asdf_dir() -> Option<PathBuf> {
    std::env::var_os("ASDF_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && !path.as_os_str().is_empty())
}

fn configured_asdf_data_dir() -> Option<PathBuf> {
    std::env::var_os("ASDF_DATA_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && !path.as_os_str().is_empty())
}

fn asdf_bin_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(asdf_dir) = configured_asdf_dir() {
        roots.push(asdf_dir.join("bin"));
    }
    if let Some(data_dir) = configured_asdf_data_dir() {
        roots.push(data_dir.join("bin"));
        roots.push(data_dir.join("shims"));
    }
    if let Some(home_root) = default_home_asdf_root() {
        roots.push(home_root.join("bin"));
        roots.push(home_root.join("shims"));
    }
    roots.sort();
    roots.dedup();
    roots
}

fn configured_manager_root() -> Option<(PathBuf, bool)> {
    if let Some(asdf_dir) = configured_asdf_dir() {
        return Some((asdf_dir, true));
    }
    if let Some(data_dir) = configured_asdf_data_dir() {
        return Some((data_dir, true));
    }
    default_home_asdf_root().map(|path| (path, false))
}

fn safe_asdf_install_root(path: &Path, home: Option<&Path>, explicit_env: bool) -> bool {
    if !path.is_absolute() || path.as_os_str().is_empty() || path.parent().is_none() {
        return false;
    }
    if path == Path::new("/") {
        return false;
    }
    if let Some(home) = home
        && !path.starts_with(home)
    {
        return false;
    }
    if explicit_env {
        return true;
    }
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(".asdf"))
}

fn detect_asdf_root_from_path(path: &Path) -> Option<PathBuf> {
    if path.file_name().and_then(|value| value.to_str()) == Some("asdf")
        && let Some(parent) = path.parent()
        && let Some(parent_name) = parent.file_name().and_then(|value| value.to_str())
        && (parent_name.eq_ignore_ascii_case("bin") || parent_name.eq_ignore_ascii_case("shims"))
    {
        return parent.parent().map(Path::to_path_buf);
    }

    path.ancestors().find_map(|ancestor| {
        let name = ancestor.file_name()?.to_str()?;
        if name.eq_ignore_ascii_case(".asdf") {
            Some(ancestor.to_path_buf())
        } else {
            None
        }
    })
}

pub struct ProcessAsdfSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessAsdfSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(
        &self,
        mut request: crate::execution::ProcessSpawnRequest,
    ) -> crate::execution::ProcessSpawnRequest {
        let path = std::env::var("PATH").unwrap_or_default();
        let mut path_entries = asdf_bin_roots()
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        path_entries.extend(ASDF_PATH_ROOTS.iter().map(|value| value.to_string()));
        if !path.is_empty() {
            path_entries.push(path);
        }
        request.command = request.command.env("PATH", path_entries.join(":"));

        if request.command.program.to_str() == Some("asdf")
            && let Some(executable) = self.detect_executable_path()
        {
            request.command.program = executable;
        }

        request
    }

    fn detect_executable_path(&self) -> Option<PathBuf> {
        let mut dynamic_roots = asdf_bin_roots()
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        let mut search_roots = dynamic_roots.iter().map(String::as_str).collect::<Vec<_>>();
        search_roots.extend_from_slice(ASDF_PATH_ROOTS);

        if let Some(executable) = which_executable(
            self.executor.as_ref(),
            "asdf",
            &search_roots,
            ManagerId::Asdf,
        ) {
            return Some(executable);
        }

        let mut candidates = vec![
            PathBuf::from("/opt/homebrew/bin/asdf"),
            PathBuf::from("/usr/local/bin/asdf"),
        ];
        candidates.extend(
            dynamic_roots
                .drain(..)
                .map(PathBuf::from)
                .map(|root| root.join("asdf")),
        );
        candidates.into_iter().find(|candidate| candidate.exists())
    }

    fn resolve_install_root(&self) -> AdapterResult<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let Some((install_root, explicit_env)) = configured_manager_root() else {
            return Err(core_error(
                CoreErrorKind::InvalidInput,
                TaskType::Install,
                ManagerAction::Install,
                "HOME or ASDF_DIR/ASDF_DATA_DIR is required for asdf installation",
            ));
        };
        if !safe_asdf_install_root(install_root.as_path(), home.as_deref(), explicit_env) {
            return Err(core_error(
                CoreErrorKind::InvalidInput,
                TaskType::Install,
                ManagerAction::Install,
                format!(
                    "resolved asdf install root '{}' is not safe",
                    install_root.display()
                ),
            ));
        }
        Ok(install_root)
    }

    fn install_self_via_official_download(&self) -> AdapterResult<String> {
        let install_root = self.resolve_install_root()?;
        if install_root.exists() {
            if install_root.join("bin/asdf").exists() {
                return Ok(format!(
                    "asdf install root '{}' already exists with executable; skipping clone",
                    install_root.display()
                ));
            }
            return Err(core_error(
                CoreErrorKind::ProcessFailure,
                TaskType::Install,
                ManagerAction::Install,
                format!(
                    "asdf install root '{}' already exists and cannot be initialized safely",
                    install_root.display()
                ),
            ));
        }

        let install_root_string = install_root.to_string_lossy().to_string();
        let request = self.configure_request(asdf_clone_install_request(
            None,
            install_root_string.as_str(),
        ));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn resolve_current_asdf_root(
        &self,
        task_type: TaskType,
        action: ManagerAction,
    ) -> AdapterResult<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if let Some((configured_root, explicit_env)) = configured_manager_root()
            && safe_asdf_install_root(configured_root.as_path(), home.as_deref(), explicit_env)
        {
            return Ok(configured_root);
        }

        let detection = self.detect()?;
        let candidate = detection
            .executable_path
            .as_deref()
            .and_then(detect_asdf_root_from_path)
            .ok_or_else(|| {
                core_error(
                    CoreErrorKind::InvalidInput,
                    task_type,
                    action,
                    "failed to resolve active asdf install root from executable path",
                )
            })?;
        if !safe_asdf_install_root(candidate.as_path(), home.as_deref(), false) {
            return Err(core_error(
                CoreErrorKind::InvalidInput,
                task_type,
                action,
                format!(
                    "refusing asdf action for unsafe root '{}'",
                    candidate.display()
                ),
            ));
        }
        Ok(candidate)
    }
}

impl AsdfSource for ProcessAsdfSource {
    fn detect(&self) -> AdapterResult<AsdfDetectOutput> {
        let executable_path = self.detect_executable_path();
        let request = self.configure_request(asdf_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(AsdfDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_current(&self) -> AdapterResult<String> {
        let request = self.configure_request(asdf_list_current_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_plugins(&self) -> AdapterResult<String> {
        let request = self.configure_request(asdf_list_plugins_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_installed_versions(&self, plugin: &str) -> AdapterResult<String> {
        let request = self.configure_request(asdf_list_installed_versions_request(None, plugin));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn search_plugins(&self, query: &SearchQuery) -> AdapterResult<String> {
        let request = self.configure_request(asdf_search_plugins_request(None, query));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn latest_version(&self, plugin: &str) -> AdapterResult<String> {
        let request = self.configure_request(asdf_latest_request(None, plugin));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn add_plugin(&self, plugin: &str) -> AdapterResult<String> {
        let request = self.configure_request(asdf_add_plugin_request(None, plugin));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install_plugin(&self, plugin: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(asdf_install_request(None, plugin, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall_plugin(&self, plugin: &str, version: &str) -> AdapterResult<String> {
        let request = self.configure_request(asdf_uninstall_request(None, plugin, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn set_home_version(&self, plugin: &str, version: &str) -> AdapterResult<String> {
        let request = self.configure_request(asdf_set_home_version_request(None, plugin, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install_self(&self, source: AsdfInstallSource) -> AdapterResult<String> {
        match source {
            AsdfInstallSource::OfficialDownload => self.install_self_via_official_download(),
        }
    }

    fn self_uninstall(&self) -> AdapterResult<String> {
        let root = self.resolve_current_asdf_root(TaskType::Uninstall, ManagerAction::Uninstall)?;
        match std::fs::remove_dir_all(root.as_path()) {
            Ok(()) => Ok(format!("removed asdf install root '{}'", root.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(format!(
                "asdf install root '{}' was already absent",
                root.display()
            )),
            Err(error) => Err(core_error(
                CoreErrorKind::ProcessFailure,
                TaskType::Uninstall,
                ManagerAction::Uninstall,
                format!(
                    "failed to remove asdf install root '{}': {error}",
                    root.display()
                ),
            )),
        }
    }

    fn self_update(&self) -> AdapterResult<String> {
        let root = self.resolve_current_asdf_root(TaskType::Upgrade, ManagerAction::Upgrade)?;
        if !root.join(".git").exists() {
            return Err(core_error(
                CoreErrorKind::ProcessFailure,
                TaskType::Upgrade,
                ManagerAction::Upgrade,
                format!(
                    "asdf self update requires a git checkout at '{}'",
                    root.display()
                ),
            ));
        }
        let root_string = root.to_string_lossy().to_string();
        let request = self.configure_request(asdf_self_update_request(None, root_string.as_str()));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
