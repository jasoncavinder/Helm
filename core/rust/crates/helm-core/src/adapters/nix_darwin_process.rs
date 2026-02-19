use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::nix_darwin::{
    NixDarwinDetectOutput, NixDarwinSource, nix_darwin_detect_request, nix_darwin_install_request,
    nix_darwin_list_installed_request, nix_darwin_list_outdated_request, nix_darwin_search_request,
    nix_darwin_uninstall_request, nix_darwin_upgrade_request,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::ProcessExecutor;
use crate::models::{ManagerId, SearchQuery};

const NIX_PATH_ROOTS: &[&str] = &[
    "/run/current-system/sw/bin",
    "/nix/var/nix/profiles/default/bin",
    "/opt/homebrew/bin",
    "/usr/local/bin",
];

pub struct ProcessNixDarwinSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessNixDarwinSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(
        &self,
        mut request: crate::execution::ProcessSpawnRequest,
    ) -> crate::execution::ProcessSpawnRequest {
        // XPC service context has a constrained PATH; include nix-darwin and nix roots.
        let path = std::env::var("PATH").unwrap_or_default();
        let home = std::env::var("HOME").unwrap_or_default();
        let profile_bin = if home.is_empty() {
            String::new()
        } else {
            format!("{home}/.nix-profile/bin:")
        };
        request.command = request.command.env(
            "PATH",
            format!(
                "{profile_bin}/run/current-system/sw/bin:/nix/var/nix/profiles/default/bin:/opt/homebrew/bin:/usr/local/bin:{path}"
            ),
        );

        if request.command.program.to_str() == Some("darwin-rebuild")
            && let Some(executable) = which_executable(
                self.executor.as_ref(),
                "darwin-rebuild",
                NIX_PATH_ROOTS,
                ManagerId::NixDarwin,
            )
        {
            request.command.program = executable;
        }

        if request.command.program.to_str() == Some("nix-env")
            && let Some(executable) = which_executable(
                self.executor.as_ref(),
                "nix-env",
                NIX_PATH_ROOTS,
                ManagerId::NixDarwin,
            )
        {
            request.command.program = executable;
        }

        request
    }
}

impl NixDarwinSource for ProcessNixDarwinSource {
    fn detect(&self) -> AdapterResult<NixDarwinDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "darwin-rebuild",
            NIX_PATH_ROOTS,
            ManagerId::NixDarwin,
        );

        let request = self.configure_request(nix_darwin_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(NixDarwinDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(nix_darwin_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = self.configure_request(nix_darwin_list_outdated_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn search(&self, query: &str) -> AdapterResult<String> {
        let search_query = SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let request = self.configure_request(nix_darwin_search_request(None, &search_query));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install(&self, package_name: &str) -> AdapterResult<String> {
        let request = self.configure_request(nix_darwin_install_request(None, package_name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(&self, package_name: &str) -> AdapterResult<String> {
        let request = self.configure_request(nix_darwin_uninstall_request(None, package_name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, package_name: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(nix_darwin_upgrade_request(None, package_name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
