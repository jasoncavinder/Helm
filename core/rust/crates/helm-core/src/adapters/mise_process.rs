use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::mise::{
    MiseDetectOutput, MiseSource, mise_detect_request, mise_list_installed_request,
    mise_list_outdated_request, mise_upgrade_request,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

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

    fn upgrade_tool(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(mise_upgrade_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

#[cfg(test)]
mod tests {
    use super::{home_local_bin_root, path_with_home_local_bin};

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
}
