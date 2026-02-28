use std::path::Path;
use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::mise::{
    MiseDetectOutput, MiseInstallSource, MiseSource, MiseUninstallMode, mise_detect_request,
    mise_download_install_script_request, mise_implode_request, mise_list_installed_request,
    mise_list_outdated_request, mise_run_downloaded_install_script_request, mise_upgrade_request,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskType};

pub struct ProcessMiseSource {
    executor: Arc<dyn ProcessExecutor>,
}

fn home_local_bin_root(home: Option<&str>) -> Option<String> {
    let trimmed = home.map(str::trim).filter(|value| !value.is_empty())?;
    Some(format!("{trimmed}/.local/bin"))
}

fn path_with_home_local_bin(home: Option<&str>, existing_path: &str) -> String {
    match home_local_bin_root(home) {
        Some(root) if existing_path.is_empty() => root,
        Some(root) => format!("{root}:{existing_path}"),
        None => existing_path.to_string(),
    }
}

fn safe_manager_only_uninstall_path(path: &Path, home: Option<&str>) -> bool {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return false;
    }
    if path == Path::new("/usr/local/bin/mise") {
        return true;
    }
    if let Some(home) = home {
        return path.starts_with(home);
    }
    false
}

fn core_error(
    kind: CoreErrorKind,
    task: TaskType,
    action: ManagerAction,
    message: impl Into<String>,
) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Mise),
        task: Some(task),
        action: Some(action),
        kind,
        message: message.into(),
    }
}

impl ProcessMiseSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let home = std::env::var("HOME").ok();
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = path_with_home_local_bin(home.as_deref(), path.as_str());

        request.command = request.command.env("PATH", new_path);

        // Resolve absolute path to binary if possible
        if request.command.program.to_str() == Some("mise") {
            let search_roots_storage = home_local_bin_root(home.as_deref())
                .into_iter()
                .collect::<Vec<String>>();
            let search_roots = search_roots_storage
                .iter()
                .map(String::as_str)
                .collect::<Vec<&str>>();
            if let Some(exe) = which_executable(
                self.executor.as_ref(),
                "mise",
                &search_roots,
                ManagerId::Mise,
            ) {
                request.command.program = exe;
            }
        }

        request
    }

    fn install_self_via_official_download(&self) -> AdapterResult<String> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let temp_dir = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let script_path = temp_dir.join(format!("helm-mise-install-{now}.sh"));
        let script_path_string = script_path.to_string_lossy().to_string();

        let download_request = self.configure_request(mise_download_install_script_request(
            None,
            &script_path_string,
        ));
        if let Err(error) = run_and_collect_stdout(self.executor.as_ref(), download_request) {
            let _ = std::fs::remove_file(&script_path);
            return Err(error);
        }

        let install_result = {
            let run_request = self.configure_request(mise_run_downloaded_install_script_request(
                None,
                &script_path_string,
            ));
            run_and_collect_stdout(self.executor.as_ref(), run_request)
        };

        let _ = std::fs::remove_file(&script_path);
        install_result
    }

    fn install_self_from_existing_binary(&self, source_path: &Path) -> AdapterResult<String> {
        if !source_path.is_file() {
            return Err(core_error(
                CoreErrorKind::InvalidInput,
                TaskType::Install,
                ManagerAction::Install,
                format!(
                    "mise existingBinaryPath source '{}' is not a file",
                    source_path.display()
                ),
            ));
        }

        let home = std::env::var("HOME").unwrap_or_default();
        let trimmed_home = home.trim();
        if trimmed_home.is_empty() {
            return Err(core_error(
                CoreErrorKind::InvalidInput,
                TaskType::Install,
                ManagerAction::Install,
                "HOME is required for mise existingBinaryPath install",
            ));
        }

        let install_dir = Path::new(trimmed_home).join(".local/bin");
        let destination = install_dir.join("mise");
        let temp_destination = install_dir.join(".mise.tmp");
        std::fs::create_dir_all(&install_dir).map_err(|error| {
            core_error(
                CoreErrorKind::ProcessFailure,
                TaskType::Install,
                ManagerAction::Install,
                format!(
                    "failed to create mise install directory '{}': {error}",
                    install_dir.display()
                ),
            )
        })?;

        std::fs::copy(source_path, &temp_destination).map_err(|error| {
            core_error(
                CoreErrorKind::ProcessFailure,
                TaskType::Install,
                ManagerAction::Install,
                format!(
                    "failed to copy mise binary from '{}' to '{}': {error}",
                    source_path.display(),
                    temp_destination.display()
                ),
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&temp_destination, std::fs::Permissions::from_mode(0o755));
        }

        std::fs::rename(&temp_destination, &destination).map_err(|error| {
            let _ = std::fs::remove_file(&temp_destination);
            core_error(
                CoreErrorKind::ProcessFailure,
                TaskType::Install,
                ManagerAction::Install,
                format!(
                    "failed to place mise binary at '{}': {error}",
                    destination.display()
                ),
            )
        })?;

        Ok(format!(
            "installed mise binary from '{}' to '{}'",
            source_path.display(),
            destination.display()
        ))
    }

    fn self_uninstall_manager_only(&self) -> AdapterResult<String> {
        let home = std::env::var("HOME").ok();
        let detection = self.detect()?;
        let Some(path) = detection.executable_path else {
            return Ok("mise executable was not detected; no manager binary removed".to_string());
        };

        if !safe_manager_only_uninstall_path(path.as_path(), home.as_deref()) {
            return Err(core_error(
                CoreErrorKind::ProcessFailure,
                TaskType::Uninstall,
                ManagerAction::Uninstall,
                format!(
                    "refusing manager-only uninstall for unsupported mise path '{}'",
                    path.display()
                ),
            ));
        }

        match std::fs::remove_file(path.as_path()) {
            Ok(()) => Ok(format!("removed mise executable '{}'", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(format!(
                "mise executable '{}' was already absent",
                path.display()
            )),
            Err(error) => Err(core_error(
                CoreErrorKind::ProcessFailure,
                TaskType::Uninstall,
                ManagerAction::Uninstall,
                format!(
                    "failed to remove mise executable '{}': {error}",
                    path.display()
                ),
            )),
        }
    }

    fn self_uninstall_full_cleanup(&self, remove_config: bool) -> AdapterResult<String> {
        let request = self.configure_request(mise_implode_request(None, remove_config));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

impl MiseSource for ProcessMiseSource {
    fn detect(&self) -> AdapterResult<MiseDetectOutput> {
        // Phase 1: instant filesystem check
        let home = std::env::var("HOME").ok();
        let search_roots_storage = home_local_bin_root(home.as_deref())
            .into_iter()
            .collect::<Vec<String>>();
        let search_roots = search_roots_storage
            .iter()
            .map(String::as_str)
            .collect::<Vec<&str>>();
        let executable_path = which_executable(
            self.executor.as_ref(),
            "mise",
            &search_roots,
            ManagerId::Mise,
        );

        // Phase 2: best-effort version (timeout is non-fatal, tries stdout then stderr)
        let request = mise_detect_request(None);
        let version_request = self.configure_request(request);
        let version_output =
            run_and_collect_version_output(self.executor.as_ref(), version_request);

        Ok(MiseDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(mise_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = self.configure_request(mise_list_outdated_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install_self(&self, source: MiseInstallSource) -> AdapterResult<String> {
        match source {
            MiseInstallSource::OfficialDownload => self.install_self_via_official_download(),
            MiseInstallSource::ExistingBinaryPath(path) => {
                self.install_self_from_existing_binary(path.as_path())
            }
        }
    }

    fn self_uninstall(&self, mode: MiseUninstallMode) -> AdapterResult<String> {
        match mode {
            MiseUninstallMode::ManagerOnlyKeepConfig => self.self_uninstall_manager_only(),
            MiseUninstallMode::FullCleanupKeepConfig => self.self_uninstall_full_cleanup(false),
            MiseUninstallMode::FullCleanupRemoveConfig => self.self_uninstall_full_cleanup(true),
        }
    }

    fn upgrade_tool(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(mise_upgrade_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{home_local_bin_root, path_with_home_local_bin, safe_manager_only_uninstall_path};

    #[test]
    fn home_local_bin_root_requires_non_empty_home() {
        assert_eq!(home_local_bin_root(None), None);
        assert_eq!(home_local_bin_root(Some("")), None);
        assert_eq!(home_local_bin_root(Some("   ")), None);
        assert_eq!(
            home_local_bin_root(Some("/Users/jason")),
            Some("/Users/jason/.local/bin".to_string())
        );
    }

    #[test]
    fn path_with_home_local_bin_skips_prefix_when_home_missing() {
        assert_eq!(
            path_with_home_local_bin(None, "/usr/bin:/bin"),
            "/usr/bin:/bin".to_string()
        );
        assert_eq!(path_with_home_local_bin(None, ""), "".to_string());
    }

    #[test]
    fn path_with_home_local_bin_prepends_home_local_bin() {
        assert_eq!(
            path_with_home_local_bin(Some("/Users/jason"), "/usr/bin:/bin"),
            "/Users/jason/.local/bin:/usr/bin:/bin".to_string()
        );
        assert_eq!(
            path_with_home_local_bin(Some("/Users/jason"), ""),
            "/Users/jason/.local/bin".to_string()
        );
    }

    #[test]
    fn manager_only_uninstall_path_guard_accepts_home_and_usr_local() {
        assert!(safe_manager_only_uninstall_path(
            Path::new("/Users/jason/.local/bin/mise"),
            Some("/Users/jason")
        ));
        assert!(safe_manager_only_uninstall_path(
            Path::new("/usr/local/bin/mise"),
            Some("/Users/jason")
        ));
        assert!(!safe_manager_only_uninstall_path(
            Path::new("/opt/homebrew/bin/mise"),
            Some("/Users/jason")
        ));
    }
}
