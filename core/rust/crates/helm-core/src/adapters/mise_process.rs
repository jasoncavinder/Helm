use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::mise::{
    MiseDetectOutput, MiseInstallSource, MiseRegistryPackage, MiseRemotePackage, MiseSource,
    MiseUninstallMode, mise_detect_request, mise_download_install_script_request,
    mise_implode_request, mise_install_tool_request, mise_list_installed_request,
    mise_list_outdated_request, mise_list_remote_request, mise_registry_request,
    mise_run_downloaded_install_script_request, mise_upgrade_request, parse_mise_registry_catalog,
    parse_mise_remote_catalog,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskType};

pub struct ProcessMiseSource {
    executor: Arc<dyn ProcessExecutor>,
    remote_catalog_cache: Mutex<Option<MiseRemoteCatalogCache>>,
}

#[derive(Clone, Debug)]
struct MiseRemoteCatalogCache {
    packages: Vec<MiseRemotePackage>,
    fetched_at: Instant,
}

const REMOTE_CATALOG_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

fn mise_registry_summary_key(name: &str) -> String {
    let normalized = name.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return String::new();
    }
    let base = normalized
        .split_once('@')
        .map(|(lhs, _)| lhs)
        .unwrap_or(normalized.as_str())
        .trim()
        .to_string();
    if base.is_empty() { normalized } else { base }
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
        Self {
            executor,
            remote_catalog_cache: Mutex::new(None),
        }
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

    fn get_cached_remote_packages(&self) -> Option<Vec<MiseRemotePackage>> {
        let cache = self.remote_catalog_cache.lock().ok()?;
        let entry = cache.as_ref()?;
        if entry.fetched_at.elapsed() > REMOTE_CATALOG_CACHE_TTL {
            return None;
        }
        Some(entry.packages.clone())
    }

    fn set_cached_remote_packages(&self, packages: &[MiseRemotePackage]) {
        if let Ok(mut cache) = self.remote_catalog_cache.lock() {
            *cache = Some(MiseRemoteCatalogCache {
                packages: packages.to_vec(),
                fetched_at: Instant::now(),
            });
        }
    }

    fn load_registry_packages(&self) -> AdapterResult<Vec<MiseRegistryPackage>> {
        let request = self.configure_request(mise_registry_request(None));
        let raw = run_and_collect_stdout(self.executor.as_ref(), request)?;
        parse_mise_registry_catalog(raw.as_str())
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

    fn list_remote_packages(&self) -> AdapterResult<Vec<MiseRemotePackage>> {
        if let Some(cached) = self.get_cached_remote_packages() {
            return Ok(cached);
        }

        let mut retrieval_errors: Vec<CoreError> = Vec::new();
        let registry_packages = match self.load_registry_packages() {
            Ok(packages) => packages,
            Err(error) => {
                crate::execution::record_task_log_note(
                    format!(
                        "[helm] mise catalog registry source failed; continuing with ls-remote only: {}",
                        error.message
                    )
                    .as_str(),
                );
                retrieval_errors.push(error);
                Vec::new()
            }
        };
        let mut registry_summary_by_name: HashMap<String, String> = HashMap::new();
        for package in &registry_packages {
            if let Some(summary) = package.summary.as_ref() {
                let name_key = mise_registry_summary_key(package.name.as_str());
                if !name_key.is_empty() {
                    registry_summary_by_name
                        .entry(name_key)
                        .or_insert_with(|| summary.clone());
                }
            }
        }

        let request = self.configure_request(mise_list_remote_request(None));
        let mut packages = match run_and_collect_stdout(self.executor.as_ref(), request) {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    let error = core_error(
                        CoreErrorKind::ParseFailure,
                        TaskType::Search,
                        ManagerAction::Search,
                        "mise ls-remote returned empty stdout",
                    );
                    crate::execution::record_task_log_note(
                        "[helm] mise catalog ls-remote source returned empty stdout; continuing with registry fallback",
                    );
                    retrieval_errors.push(error);
                    Vec::new()
                } else {
                    match parse_mise_remote_catalog(raw.as_str()) {
                        Ok(parsed) => parsed,
                        Err(error) => {
                            crate::execution::record_task_log_note(
                                format!(
                                    "[helm] mise catalog ls-remote source parse failed; continuing with registry fallback: {}",
                                    error.message
                                )
                                .as_str(),
                            );
                            retrieval_errors.push(error);
                            Vec::new()
                        }
                    }
                }
            }
            Err(error) => {
                crate::execution::record_task_log_note(
                    format!(
                        "[helm] mise catalog ls-remote source failed; continuing with registry fallback: {}",
                        error.message
                    )
                    .as_str(),
                );
                retrieval_errors.push(error);
                Vec::new()
            }
        };
        for package in &mut packages {
            if package.summary.is_none() {
                let name_key = mise_registry_summary_key(package.name.as_str());
                if let Some(summary) = registry_summary_by_name.get(name_key.as_str()) {
                    package.summary = Some(summary.clone());
                }
            }
        }

        let mut seen_registry_names: HashSet<String> = packages
            .iter()
            .map(|package| package.name.trim().to_ascii_lowercase())
            .filter(|name| !name.is_empty())
            .collect();
        for registry_package in &registry_packages {
            let name_key = registry_package.name.trim().to_ascii_lowercase();
            if name_key.is_empty() || !seen_registry_names.insert(name_key) {
                continue;
            }
            packages.push(MiseRemotePackage {
                name: registry_package.name.clone(),
                latest_version: None,
                summary: registry_package.summary.clone(),
            });
        }

        if packages.is_empty() && !registry_packages.is_empty() {
            crate::execution::record_task_log_note(
                "[helm] mise catalog loaded from registry-only fallback (ls-remote data unavailable)",
            );
        }
        if packages.is_empty()
            && registry_packages.is_empty()
            && let Some(error) = retrieval_errors.into_iter().next()
        {
            return Err(error);
        }

        packages.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
                .then_with(|| left.latest_version.cmp(&right.latest_version))
        });
        self.set_cached_remote_packages(&packages);
        Ok(packages)
    }

    fn install_self(&self, source: MiseInstallSource) -> AdapterResult<String> {
        match source {
            MiseInstallSource::OfficialDownload => self.install_self_via_official_download(),
            MiseInstallSource::ExistingBinaryPath(path) => {
                self.install_self_from_existing_binary(path.as_path())
            }
        }
    }

    fn install_tool(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(mise_install_tool_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::SystemTime;

    use crate::adapters::mise::MiseSource;
    use crate::execution::{
        ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
        ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
    };

    const REMOTE_FIXTURE: &str = include_str!("../../tests/fixtures/mise/ls_remote_all_json.txt");
    const REGISTRY_FIXTURE: &str = r#"
[
  {"short":"python","description":"python language","aliases":["python3"]},
  {"short":"java","description":"jdk java"},
  {"short":"jq","description":"JSON processor"}
]
"#;

    struct FakeProcess {
        output: ProcessOutput,
    }

    impl RunningProcess for FakeProcess {
        fn pid(&self) -> Option<u32> {
            Some(4242)
        }

        fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
            Ok(())
        }

        fn wait(self: Box<Self>) -> ProcessWaitFuture {
            let output = self.output;
            Box::pin(async move { Ok(output) })
        }
    }

    #[derive(Default)]
    struct FakeExecutor {
        remote_calls: AtomicUsize,
    }

    impl FakeExecutor {
        fn remote_calls(&self) -> usize {
            self.remote_calls.load(Ordering::SeqCst)
        }
    }

    impl ProcessExecutor for FakeExecutor {
        fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
            let now = SystemTime::now();
            let program = request.command.program.to_string_lossy().to_string();
            let args = request.command.args;

            let stdout = if program.ends_with("which") {
                b"/Users/test/.local/bin/mise".to_vec()
            } else if (program == "mise" || program.ends_with("/mise"))
                && args
                    .first()
                    .is_some_and(|value| value.as_str() == "ls-remote")
            {
                self.remote_calls.fetch_add(1, Ordering::SeqCst);
                REMOTE_FIXTURE.as_bytes().to_vec()
            } else if (program == "mise" || program.ends_with("/mise"))
                && args
                    .first()
                    .is_some_and(|value| value.as_str() == "registry")
            {
                REGISTRY_FIXTURE.as_bytes().to_vec()
            } else {
                Vec::new()
            };

            Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(0),
                    stdout,
                    stderr: Vec::new(),
                    started_at: now,
                    finished_at: now,
                },
            }))
        }
    }

    struct RegistryOnlyExecutor;

    impl ProcessExecutor for RegistryOnlyExecutor {
        fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
            let now = SystemTime::now();
            let program = request.command.program.to_string_lossy().to_string();
            let args = request.command.args;

            let stdout = if program.ends_with("which") {
                b"/Users/test/.local/bin/mise".to_vec()
            } else if (program == "mise" || program.ends_with("/mise"))
                && args
                    .first()
                    .is_some_and(|value| value.as_str() == "ls-remote")
            {
                Vec::new()
            } else if (program == "mise" || program.ends_with("/mise"))
                && args
                    .first()
                    .is_some_and(|value| value.as_str() == "registry")
            {
                REGISTRY_FIXTURE.as_bytes().to_vec()
            } else {
                Vec::new()
            };

            Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(0),
                    stdout,
                    stderr: Vec::new(),
                    started_at: now,
                    finished_at: now,
                },
            }))
        }
    }

    struct InvalidRemoteJsonExecutor;

    impl ProcessExecutor for InvalidRemoteJsonExecutor {
        fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
            let now = SystemTime::now();
            let program = request.command.program.to_string_lossy().to_string();
            let args = request.command.args;

            let stdout = if program.ends_with("which") {
                b"/Users/test/.local/bin/mise".to_vec()
            } else if (program == "mise" || program.ends_with("/mise"))
                && args
                    .first()
                    .is_some_and(|value| value.as_str() == "ls-remote")
            {
                b"{not valid json".to_vec()
            } else if (program == "mise" || program.ends_with("/mise"))
                && args
                    .first()
                    .is_some_and(|value| value.as_str() == "registry")
            {
                REGISTRY_FIXTURE.as_bytes().to_vec()
            } else {
                Vec::new()
            };

            Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(0),
                    stdout,
                    stderr: Vec::new(),
                    started_at: now,
                    finished_at: now,
                },
            }))
        }
    }

    struct VariantQualifiedRemoteExecutor;

    impl ProcessExecutor for VariantQualifiedRemoteExecutor {
        fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
            let now = SystemTime::now();
            let program = request.command.program.to_string_lossy().to_string();
            let args = request.command.args;

            let stdout = if program.ends_with("which") {
                b"/Users/test/.local/bin/mise".to_vec()
            } else if (program == "mise" || program.ends_with("/mise"))
                && args
                    .first()
                    .is_some_and(|value| value.as_str() == "ls-remote")
            {
                br#"[{"tool":"python@mambaforge","version":"24.11.0-1","created_at":"2024-11-01T00:00:00Z"}]"#
                    .to_vec()
            } else if (program == "mise" || program.ends_with("/mise"))
                && args
                    .first()
                    .is_some_and(|value| value.as_str() == "registry")
            {
                REGISTRY_FIXTURE.as_bytes().to_vec()
            } else {
                Vec::new()
            };

            Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(0),
                    stdout,
                    stderr: Vec::new(),
                    started_at: now,
                    finished_at: now,
                },
            }))
        }
    }

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

    #[test]
    fn remote_catalog_is_cached_between_calls() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        let _guard = runtime.enter();

        let executor = Arc::new(FakeExecutor::default());
        let source = super::ProcessMiseSource::new(executor.clone());

        let first = source
            .list_remote_packages()
            .expect("initial remote catalog load should succeed");
        let second = source
            .list_remote_packages()
            .expect("cached remote catalog load should succeed");

        assert!(!first.is_empty());
        assert_eq!(first, second);
        assert_eq!(executor.remote_calls(), 1);
    }

    #[test]
    fn remote_catalog_merges_registry_summaries_and_registry_only_names() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        let _guard = runtime.enter();

        let executor = Arc::new(FakeExecutor::default());
        let source = super::ProcessMiseSource::new(executor);
        let packages = source
            .list_remote_packages()
            .expect("remote catalog load should succeed");

        let python = packages
            .iter()
            .find(|package| package.name == "python")
            .expect("python should exist");
        assert_eq!(python.summary.as_deref(), Some("python language"));

        let python_alias = packages
            .iter()
            .find(|package| package.name == "python3")
            .expect("python3 alias should be included from registry");
        assert!(python_alias.latest_version.is_none());
        assert_eq!(python_alias.summary.as_deref(), Some("python language"));
    }

    #[test]
    fn remote_catalog_uses_registry_fallback_when_ls_remote_is_empty() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        let _guard = runtime.enter();

        let executor = Arc::new(RegistryOnlyExecutor);
        let source = super::ProcessMiseSource::new(executor);
        let packages = source
            .list_remote_packages()
            .expect("registry fallback load should succeed");

        assert!(!packages.is_empty());
        assert!(packages.iter().any(|package| package.name == "java"));
        assert!(packages.iter().any(|package| package.name == "python3"));
    }

    #[test]
    fn remote_catalog_uses_registry_fallback_when_ls_remote_json_is_invalid() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        let _guard = runtime.enter();

        let executor = Arc::new(InvalidRemoteJsonExecutor);
        let source = super::ProcessMiseSource::new(executor);
        let packages = source
            .list_remote_packages()
            .expect("registry fallback should succeed even with invalid ls-remote JSON");

        assert!(!packages.is_empty());
        assert!(packages.iter().any(|package| package.name == "java"));
        assert!(packages.iter().any(|package| package.name == "python3"));
    }

    #[test]
    fn remote_catalog_enriches_variant_qualified_tools_from_registry_summary() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime should build");
        let _guard = runtime.enter();

        let executor = Arc::new(VariantQualifiedRemoteExecutor);
        let source = super::ProcessMiseSource::new(executor);
        let packages = source
            .list_remote_packages()
            .expect("variant-qualified remote package load should succeed");

        let qualified_python = packages
            .iter()
            .find(|package| package.name == "python@mambaforge")
            .expect("qualified python tool should exist");
        assert_eq!(qualified_python.summary.as_deref(), Some("python language"));
    }
}
