use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::docker_desktop::{
    DockerDesktopDetectOutput, DockerDesktopSource, docker_desktop_detect_request,
    docker_desktop_list_outdated_request,
};
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

pub struct ProcessDockerDesktopSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessDockerDesktopSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(
        &self,
        mut request: ProcessSpawnRequest,
        program_name: &str,
        extra_paths: &[&str],
    ) -> ProcessSpawnRequest {
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{path}", extra_paths.join(":"));
        request.command = request.command.env("PATH", new_path);

        if request.command.program.to_str() == Some(program_name)
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                program_name,
                extra_paths,
                ManagerId::DockerDesktop,
            )
        {
            request.command.program = exe;
        }

        request
    }
}

impl DockerDesktopSource for ProcessDockerDesktopSource {
    fn detect(&self) -> AdapterResult<DockerDesktopDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "docker",
            &[
                "/Applications/Docker.app/Contents/Resources/bin",
                "/opt/homebrew/bin",
                "/usr/local/bin",
            ],
            ManagerId::DockerDesktop,
        );

        let request = self.configure_request(
            docker_desktop_detect_request(None),
            "docker",
            &[
                "/Applications/Docker.app/Contents/Resources/bin",
                "/opt/homebrew/bin",
                "/usr/local/bin",
            ],
        );
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(DockerDesktopDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = self.configure_request(
            docker_desktop_list_outdated_request(None),
            "brew",
            &["/opt/homebrew/bin", "/usr/local/bin"],
        );
        Ok(run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default())
    }
}
