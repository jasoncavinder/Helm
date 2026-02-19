use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::asdf::{
    AsdfDetectOutput, AsdfSource, asdf_detect_request, asdf_install_request, asdf_latest_request,
    asdf_list_all_plugins_request, asdf_list_current_request, asdf_list_plugins_request,
    asdf_uninstall_request, asdf_upgrade_request,
};
use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::ProcessExecutor;
use crate::models::{ManagerId, SearchQuery};

const ASDF_PATH_ROOTS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

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

    fn install(&self, plugin: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(asdf_install_request(None, plugin, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(&self, plugin: &str, version: &str) -> AdapterResult<String> {
        let request = self.configure_request(asdf_uninstall_request(None, plugin, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, plugin: Option<&str>) -> AdapterResult<String> {
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
}
