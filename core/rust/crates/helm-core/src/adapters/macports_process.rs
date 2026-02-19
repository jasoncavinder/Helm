use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::macports::{
    MacPortsDetectOutput, MacPortsSource, macports_detect_request, macports_install_request,
    macports_list_installed_request, macports_list_outdated_request, macports_search_request,
    macports_uninstall_request, macports_upgrade_request,
};
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{
    ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest, spawn_validated,
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

    fn install(&self, port_name: &str) -> AdapterResult<String> {
        let request = self.configure_request(macports_install_request(None, port_name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall(&self, port_name: &str) -> AdapterResult<String> {
        let request = self.configure_request(macports_uninstall_request(None, port_name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade(&self, port_name: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(macports_upgrade_request(None, port_name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
