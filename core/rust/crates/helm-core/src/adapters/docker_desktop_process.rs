use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::docker_desktop::{
    DockerDesktopDetectOutput, DockerDesktopSource, docker_desktop_detect_request_for_plist,
    docker_desktop_homebrew_info_request, docker_desktop_list_outdated_request,
};
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
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

fn locate_docker_desktop_app() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications/Docker.app")];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Applications/Docker.app"));
    }
    candidates.into_iter().find(|path| path.exists())
}

fn resolve_docker_desktop_executable_path(app_path: &Path) -> Option<PathBuf> {
    let candidates = [
        app_path.join("Contents/MacOS/Docker Desktop"),
        app_path.join("Contents/MacOS/Docker"),
    ];
    candidates.into_iter().find(|path| path.exists())
}

impl DockerDesktopSource for ProcessDockerDesktopSource {
    fn detect(&self) -> AdapterResult<DockerDesktopDetectOutput> {
        let app_path = locate_docker_desktop_app();
        let executable_path = app_path
            .as_ref()
            .and_then(|path| resolve_docker_desktop_executable_path(path.as_path()))
            .or(app_path.clone());
        let version_output = if let Some(app_path) = &app_path {
            let plist_path = app_path.join("Contents/Info.plist");
            let request =
                docker_desktop_detect_request_for_plist(None, &plist_path.to_string_lossy());
            run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default()
        } else {
            String::new()
        };

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
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn homebrew_info(&self) -> AdapterResult<String> {
        let request = self.configure_request(
            docker_desktop_homebrew_info_request(None),
            "brew",
            &["/opt/homebrew/bin", "/usr/local/bin"],
        );
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
