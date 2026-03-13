use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::macports::{
    MacPortsDetectOutput, MacPortsSource, collect_macports_self_cleanup_targets,
    macports_detect_request, macports_install_request, macports_list_installed_request,
    macports_list_outdated_request, macports_prefix_from_port_path, macports_search_request,
    macports_uninstall_request, macports_upgrade_request,
};
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{
    CommandSpec, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    spawn_validated,
};
use crate::models::{CoreError, CoreErrorKind, ManagerId, SearchQuery};

pub struct ProcessMacPortsSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessMacPortsSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        // XPC service context has a constrained PATH; ensure MacPorts root is available.
        let path = std::env::var("PATH").unwrap_or_default();
        request.command = request
            .command
            .env("PATH", format!("/opt/local/bin:{path}"));

        if request.command.program.to_str() == Some("port")
            && let Some(executable) = which_executable(
                self.executor.as_ref(),
                "port",
                &["/opt/local/bin"],
                ManagerId::MacPorts,
            )
        {
            request.command.program = executable;
        }

        request
    }

    fn run_and_collect_stdout_accepting(
        &self,
        request: ProcessSpawnRequest,
        allowed_exit_codes: &[i32],
    ) -> AdapterResult<String> {
        let manager = request.manager;
        let task_type = request.task_type;
        let action = request.action;

        let process = spawn_validated(self.executor.as_ref(), request)?;

        let handle = tokio::runtime::Handle::current();
        let output: ProcessOutput = handle.block_on(process.wait())?;

        match output.status {
            ProcessExitStatus::ExitCode(code)
                if code == 0 || allowed_exit_codes.contains(&code) =>
            {
                String::from_utf8(output.stdout).map_err(|error| CoreError {
                    manager: Some(manager),
                    task: Some(task_type),
                    action: Some(action),
                    kind: CoreErrorKind::ParseFailure,
                    message: format!("process stdout is not valid UTF-8: {error}"),
                })
            }
            ProcessExitStatus::ExitCode(code) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(CoreError {
                    manager: Some(manager),
                    task: Some(task_type),
                    action: Some(action),
                    kind: CoreErrorKind::ProcessFailure,
                    message: format!("process exited with code {code}: {stderr}"),
                })
            }
            ProcessExitStatus::Terminated => Err(CoreError {
                manager: Some(manager),
                task: Some(task_type),
                action: Some(action),
                kind: CoreErrorKind::ProcessFailure,
                message: "process was terminated by signal".to_string(),
            }),
        }
    }

    fn command_succeeds(&self, request: ProcessSpawnRequest) -> bool {
        let process = match spawn_validated(self.executor.as_ref(), request) {
            Ok(process) => process,
            Err(_) => return false,
        };

        let handle = tokio::runtime::Handle::current();
        let output: ProcessOutput = match handle.block_on(process.wait()) {
            Ok(output) => output,
            Err(_) => return false,
        };

        matches!(output.status, ProcessExitStatus::ExitCode(0))
    }

    fn macports_self_uninstall_request(&self) -> AdapterResult<ProcessSpawnRequest> {
        let executable = which_executable(
            self.executor.as_ref(),
            "port",
            &["/opt/local/bin"],
            ManagerId::MacPorts,
        )
        .ok_or(CoreError {
            manager: Some(ManagerId::MacPorts),
            task: Some(crate::models::TaskType::Uninstall),
            action: Some(crate::models::ManagerAction::Uninstall),
            kind: CoreErrorKind::ProcessFailure,
            message: "unable to resolve MacPorts executable for manager uninstall".to_string(),
        })?;

        Ok(ProcessSpawnRequest::new(
            ManagerId::MacPorts,
            crate::models::TaskType::Uninstall,
            crate::models::ManagerAction::Uninstall,
            CommandSpec::new(executable).args(["-fp", "uninstall", "installed"]),
        )
        .requires_elevation(true)
        .timeout(std::time::Duration::from_secs(3600)))
    }

    fn dscl_record_exists(&self, record_path: &str) -> bool {
        self.command_succeeds(ProcessSpawnRequest::new(
            ManagerId::MacPorts,
            crate::models::TaskType::Uninstall,
            crate::models::ManagerAction::Uninstall,
            CommandSpec::new("/usr/bin/dscl").args([".", "-read", record_path]),
        ))
    }

    fn run_optional_dscl_delete(&self, record_path: &str) -> AdapterResult<()> {
        let request = ProcessSpawnRequest::new(
            ManagerId::MacPorts,
            crate::models::TaskType::Uninstall,
            crate::models::ManagerAction::Uninstall,
            CommandSpec::new("/usr/bin/dscl").args([".", "-delete", record_path]),
        )
        .requires_elevation(true)
        .timeout(std::time::Duration::from_secs(60));
        let _ = self.run_and_collect_stdout_accepting(request, &[])?;
        Ok(())
    }
}

impl MacPortsSource for ProcessMacPortsSource {
    fn detect(&self) -> AdapterResult<MacPortsDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "port",
            &["/opt/local/bin"],
            ManagerId::MacPorts,
        );

        let request = self.configure_request(macports_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(MacPortsDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed(&self) -> AdapterResult<String> {
        let request = self.configure_request(macports_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = self.configure_request(macports_list_outdated_request(None));
        self.run_and_collect_stdout_accepting(request, &[1])
    }

    fn search(&self, query: &str) -> AdapterResult<String> {
        let search_query = SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let request = self.configure_request(macports_search_request(None, &search_query));
        self.run_and_collect_stdout_accepting(request, &[1])
    }

    fn self_uninstall(&self) -> AdapterResult<String> {
        let self_request = self.configure_request(self.macports_self_uninstall_request()?);
        let port_executable = self_request.command.program.clone();
        let prefix = macports_prefix_from_port_path(port_executable.as_path()).ok_or(CoreError {
            manager: Some(ManagerId::MacPorts),
            task: Some(crate::models::TaskType::Uninstall),
            action: Some(crate::models::ManagerAction::Uninstall),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "refusing MacPorts manager uninstall because '{}' is not a recognized '<prefix>/bin/port' executable path",
                port_executable.display()
            ),
        })?;

        let mut notes = Vec::new();
        crate::execution::record_task_log_note(
            format!(
                "running MacPorts manager uninstall flow for prefix '{}'",
                prefix.display()
            )
            .as_str(),
        );
        let uninstall_ports_output = self.run_and_collect_stdout_accepting(self_request, &[])?;
        if !uninstall_ports_output.trim().is_empty() {
            notes.push(uninstall_ports_output.trim().to_string());
        }

        if self.dscl_record_exists("/Users/macports") {
            self.run_optional_dscl_delete("/Users/macports")?;
            notes.push("removed macports user account".to_string());
        }
        if self.dscl_record_exists("/Groups/macports") {
            self.run_optional_dscl_delete("/Groups/macports")?;
            notes.push("removed macports group".to_string());
        }

        let cleanup = collect_macports_self_cleanup_targets(prefix.as_path());
        let cleanup_paths = cleanup
            .directories
            .iter()
            .chain(cleanup.files.iter())
            .map(|path| path.to_string_lossy().to_string())
            .filter(|path| !path.trim().is_empty())
            .collect::<Vec<_>>();
        if !cleanup_paths.is_empty() {
            crate::execution::record_task_log_note(
                format!("removing {} MacPorts cleanup paths", cleanup_paths.len()).as_str(),
            );
            let cleanup_request = ProcessSpawnRequest::new(
                ManagerId::MacPorts,
                crate::models::TaskType::Uninstall,
                crate::models::ManagerAction::Uninstall,
                CommandSpec::new("/bin/rm")
                    .args(["-rf"])
                    .args(cleanup_paths.iter().cloned()),
            )
            .requires_elevation(true)
            .timeout(std::time::Duration::from_secs(600));
            let _ = self.run_and_collect_stdout_accepting(cleanup_request, &[])?;
        }

        if notes.is_empty() {
            Ok("MacPorts uninstall flow completed.".to_string())
        } else {
            Ok(notes.join("\n"))
        }
    }

    fn install(
        &self,
        port_name: &str,
        version: Option<&str>,
        variants: &[String],
    ) -> AdapterResult<String> {
        let request =
            self.configure_request(macports_install_request(None, port_name, version, variants));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(
        &self,
        port_name: &str,
        version: Option<&str>,
        variants: &[String],
    ) -> AdapterResult<String> {
        let request = self.configure_request(macports_uninstall_request(
            None, port_name, version, variants,
        ));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(
        &self,
        port_name: Option<&str>,
        version: Option<&str>,
        variants: &[String],
    ) -> AdapterResult<String> {
        let request =
            self.configure_request(macports_upgrade_request(None, port_name, version, variants));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
