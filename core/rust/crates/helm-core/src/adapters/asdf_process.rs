use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::asdf::{
    AsdfDetectOutput, AsdfInstallSource, AsdfSource, asdf_clone_install_request,
    asdf_detect_request, asdf_install_request, asdf_latest_request,
    asdf_list_all_plugins_request, asdf_list_current_request, asdf_list_plugins_request,
    asdf_self_update_request, asdf_uninstall_request, asdf_upgrade_request,
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

fn safe_asdf_install_root(path: &Path, home: Option<&str>) -> bool {
    if !path.is_absolute() || path.as_os_str().is_empty() || path.parent().is_none() {
        return false;
    }
    if path == Path::new("/") {
        return false;
    }
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if !file_name.eq_ignore_ascii_case(".asdf") {
        return false;
    }
    if let Some(home) = home {
        return path.starts_with(home);
    }
    false
}

fn detect_asdf_root_from_path(path: &Path) -> Option<PathBuf> {
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
        // XPC service context has a minimal PATH. Include common asdf install roots.
        let path = std::env::var("PATH").unwrap_or_default();
        let home = std::env::var("HOME").unwrap_or_default();
        let asdf_bin = if home.is_empty() {
            String::new()
        } else {
            format!("{home}/.asdf/bin:{home}/.asdf/shims:")
        };
        request.command = request.command.env(
            "PATH",
            format!("{asdf_bin}/opt/homebrew/bin:/usr/local/bin:{path}"),
        );

        if request.command.program.to_str() == Some("asdf")
            && let Some(executable) = self.detect_executable_path()
        {
            request.command.program = executable;
        }

        request
    }

    fn detect_executable_path(&self) -> Option<PathBuf> {
        let mut dynamic_roots: Vec<String> = Vec::new();
        if let Ok(home) = std::env::var("HOME")
            && !home.trim().is_empty()
        {
            dynamic_roots.push(format!("{home}/.asdf/bin"));
            dynamic_roots.push(format!("{home}/.asdf/shims"));
        }
        let mut search_roots: Vec<&str> = dynamic_roots.iter().map(String::as_str).collect();
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
        for root in dynamic_roots {
            candidates.push(PathBuf::from(root).join("asdf"));
        }

        candidates.into_iter().find(|candidate| candidate.exists())
    }

    fn resolve_install_root(&self) -> AdapterResult<PathBuf> {
        let home = std::env::var("HOME").unwrap_or_default();
        let trimmed_home = home.trim();
        if trimmed_home.is_empty() {
            return Err(core_error(
                CoreErrorKind::InvalidInput,
                TaskType::Install,
                ManagerAction::Install,
                "HOME is required for asdf script installer",
            ));
        }
        let install_root = Path::new(trimmed_home).join(".asdf");
        if !safe_asdf_install_root(install_root.as_path(), Some(trimmed_home)) {
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
        let request =
            self.configure_request(asdf_clone_install_request(None, install_root_string.as_str()));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn resolve_current_asdf_root(&self, task_type: TaskType, action: ManagerAction) -> AdapterResult<PathBuf> {
        let home = std::env::var("HOME").ok();
        let detection = self.detect()?;
        let from_executable = detection
            .executable_path
            .as_deref()
            .and_then(detect_asdf_root_from_path);
        let candidate = from_executable.unwrap_or_else(|| {
            let home_path = home.as_deref().unwrap_or_default();
            Path::new(home_path).join(".asdf")
        });
        if !safe_asdf_install_root(candidate.as_path(), home.as_deref()) {
            return Err(core_error(
                CoreErrorKind::InvalidInput,
                task_type,
                action,
                format!("refusing asdf action for unsafe root '{}'", candidate.display()),
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

    fn list_all_plugins(&self) -> AdapterResult<String> {
        let request = self.configure_request(asdf_list_all_plugins_request(
            None,
            &SearchQuery {
                text: String::new(),
                issued_at: std::time::SystemTime::now(),
            },
        ));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn latest_version(&self, plugin: &str) -> AdapterResult<String> {
        let request = self.configure_request(asdf_latest_request(None, plugin));
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

    fn upgrade_plugins(&self, plugin: Option<&str>) -> AdapterResult<String> {
        if let Some(plugin) = plugin {
            let request = self.configure_request(asdf_upgrade_request(None, plugin));
            return run_and_collect_stdout(self.executor.as_ref(), request);
        }

        let plugin_listing = self.list_plugins()?;
        let mut output = String::new();
        for line in plugin_listing.lines().map(str::trim) {
            if line.is_empty() {
                continue;
            }
            let request = self.configure_request(asdf_upgrade_request(None, line));
            let command_output = run_and_collect_stdout(self.executor.as_ref(), request)?;
            if !command_output.trim().is_empty() {
                if !output.is_empty() {
                    output.push('\n');
                }
                output.push_str(command_output.trim());
            }
        }

        Ok(output)
    }

    fn install_self(&self, source: AsdfInstallSource) -> AdapterResult<String> {
        match source {
            AsdfInstallSource::OfficialDownload => self.install_self_via_official_download(),
        }
    }

    fn self_uninstall(&self) -> AdapterResult<String> {
        let root =
            self.resolve_current_asdf_root(TaskType::Uninstall, ManagerAction::Uninstall)?;
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
                format!("failed to remove asdf install root '{}': {error}", root.display()),
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
