use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::pnpm::{
    PnpmDetectOutput, PnpmSource, pnpm_detect_request, pnpm_install_request,
    pnpm_list_installed_request, pnpm_list_outdated_request, pnpm_search_request,
    pnpm_uninstall_request, pnpm_upgrade_request,
};
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::execution::{
    ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest, spawn_validated,
};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, SearchQuery, TaskType};

pub struct ProcessPnpmSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessPnpmSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        // XPC services have a constrained PATH; include common pnpm binary locations.
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:{path}");
        request.command = request
            .command
            .env("PATH", new_path)
            .env("PNPM_CONFIG_UPDATE_NOTIFIER", "false")
            .env("PNPM_CONFIG_FUND", "false")
            .env("PNPM_CONFIG_AUDIT", "false");

        if request.command.program.to_str() == Some("pnpm")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "pnpm",
                &["/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::Pnpm,
            )
        {
            request.command.program = exe;
        }

        request
    }

    fn run_and_collect_stdout_accepting(
        &self,
        request: ProcessSpawnRequest,
        allowed_exit_codes: &[i32],
        allow_empty_stdout_without_stderr: bool,
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
                let stdout = String::from_utf8(output.stdout).map_err(|error| CoreError {
                    manager: Some(manager),
                    task: Some(task_type),
                    action: Some(action),
                    kind: CoreErrorKind::ParseFailure,
                    message: format!("process stdout is not valid UTF-8: {error}"),
                })?;
                if action == ManagerAction::ListOutdated
                    && code == 1
                    && output.stderr.is_empty()
                    && stdout
                        .trim_start()
                        .starts_with("ERR_PNPM_NO_IMPORTER_MANIFEST_FOUND")
                {
                    // pnpm 10 has no global manifest before the first install.
                    // Verify the selected scope is absent; never hide a damaged
                    // existing store or interpret another error as no updates.
                    let mut root_request = pnpm_list_outdated_request(None);
                    root_request.command.args = vec!["root".into(), "-g".into()];
                    let root = run_and_collect_stdout(
                        self.executor.as_ref(),
                        self.configure_request(root_request),
                    )?;
                    if missing_global_manifest_is_empty(&stdout, &root) {
                        return Ok("{}".to_string());
                    }
                }
                interpret_allowed_exit_output(
                    manager,
                    task_type,
                    action,
                    code,
                    stdout,
                    output.stderr,
                    allow_empty_stdout_without_stderr,
                )
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

fn missing_global_manifest_is_empty(diagnostic: &str, root_output: &str) -> bool {
    use std::path::{Component, Path};

    let root = Path::new(root_output.trim());
    if !root.is_absolute()
        || root.file_name().is_none_or(|name| name != "node_modules")
        || root_output.trim().chars().any(char::is_control)
        || root
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return false;
    }
    let Some(project) = root.parent() else {
        return false;
    };
    let Some(message) = diagnostic
        .trim()
        .strip_prefix("ERR_PNPM_NO_IMPORTER_MANIFEST_FOUND")
    else {
        return false;
    };
    if message.trim()
        != format!(
            "No package.json (or package.yaml, or package.json5) was found in \"{}\".",
            project.display()
        )
    {
        return false;
    }
    for (index, path) in project.ancestors().enumerate() {
        match std::fs::symlink_metadata(path) {
            Ok(_) => {
                return index > 0
                    && std::fs::canonicalize(path).is_ok_and(|resolved| resolved.is_dir());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return false,
        }
    }
    false
}

fn interpret_allowed_exit_output(
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    code: i32,
    stdout: String,
    stderr_bytes: Vec<u8>,
    allow_empty_stdout_without_stderr: bool,
) -> AdapterResult<String> {
    if !stdout.trim().is_empty() {
        return Ok(stdout);
    }

    let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
    if allow_empty_stdout_without_stderr && stderr.is_empty() {
        return Ok(String::new());
    }

    Err(CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: Some(action),
        kind: CoreErrorKind::ProcessFailure,
        message: if stderr.is_empty() {
            format!("process exited with code {code} without usable output")
        } else {
            format!("process exited with code {code}: {stderr}")
        },
    })
}

impl PnpmSource for ProcessPnpmSource {
    fn detect(&self) -> AdapterResult<PnpmDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "pnpm",
            &["/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::Pnpm,
        );

        let request = self.configure_request(pnpm_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(PnpmDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed_global(&self) -> AdapterResult<String> {
        let request = self.configure_request(pnpm_list_installed_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_outdated_global(&self) -> AdapterResult<String> {
        // pnpm uses exit code 1 to indicate outdated packages were found.
        let request = self.configure_request(pnpm_list_outdated_request(None));
        self.run_and_collect_stdout_accepting(request, &[1], false)
    }

    fn search(&self, query: &str) -> AdapterResult<String> {
        let search_query = SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let request = self.configure_request(pnpm_search_request(None, &search_query));
        // pnpm may return exit code 1 for no matches while still writing JSON/JSONL output.
        self.run_and_collect_stdout_accepting(request, &[1], true)
    }

    fn install_global(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(pnpm_install_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall_global(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(pnpm_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade_global(&self, name: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(pnpm_upgrade_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::SystemTime;

    use crate::execution::{
        ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
        ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
    };
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, TaskType};

    use super::{PnpmSource, ProcessPnpmSource, interpret_allowed_exit_output};

    struct FixtureProcess(ProcessOutput);

    impl RunningProcess for FixtureProcess {
        fn pid(&self) -> Option<u32> {
            None
        }

        fn terminate(&self, _: ProcessTerminationMode) -> ExecutionResult<()> {
            Ok(())
        }

        fn wait(self: Box<Self>) -> ProcessWaitFuture {
            Box::pin(async move { Ok(self.0) })
        }
    }

    struct MissingScopeExecutor {
        root: PathBuf,
        diagnostic: String,
        stderr: Vec<u8>,
        exit: i32,
        root_exit: i32,
    }

    impl ProcessExecutor for MissingScopeExecutor {
        fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
            let (code, stdout, stderr) = match request.command.args.first().map(String::as_str) {
                Some("outdated") => (self.exit, self.diagnostic.clone(), self.stderr.clone()),
                Some("root") => (self.root_exit, self.root.display().to_string(), Vec::new()),
                _ if request.command.program.ends_with("which") => {
                    (0, "/certification/pnpm".to_string(), Vec::new())
                }
                _ => panic!("unexpected pnpm fixture request: {:?}", request.command),
            };
            Ok(Box::new(FixtureProcess(ProcessOutput {
                status: ProcessExitStatus::ExitCode(code),
                stdout: stdout.into_bytes(),
                stderr,
                started_at: SystemTime::now(),
                finished_at: SystemTime::now(),
            })))
        }
    }

    fn missing_scope_executor(project: &std::path::Path) -> MissingScopeExecutor {
        MissingScopeExecutor {
            root: project.join("node_modules"),
            diagnostic: format!(
                "\u{2009}ERR_PNPM_NO_IMPORTER_MANIFEST_FOUND\u{2009} No package.json (or package.yaml, or package.json5) was found in \"{}\".\n",
                project.display()
            ),
            stderr: Vec::new(),
            exit: 1,
            root_exit: 0,
        }
    }

    fn outdated_from_fixture(executor: MissingScopeExecutor) -> super::AdapterResult<String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            tokio::task::spawn_blocking(move || {
                ProcessPnpmSource::new(Arc::new(executor)).list_outdated_global()
            })
            .await
            .unwrap()
        })
    }

    #[test]
    fn missing_global_project_is_empty_without_creating_the_scope() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("global/5");
        assert_eq!(
            outdated_from_fixture(missing_scope_executor(&project)).unwrap(),
            "{}"
        );
        assert!(!project.exists());
    }

    #[test]
    fn missing_manifest_is_not_empty_for_existing_or_dangling_projects() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("global/5");
        std::fs::create_dir_all(&project).unwrap();
        let executor = missing_scope_executor(&project);
        let original = executor.diagnostic.clone();
        assert_eq!(outdated_from_fixture(executor).unwrap(), original);
        std::fs::write(project.join("package.json"), "{malformed").unwrap();
        assert_eq!(
            outdated_from_fixture(missing_scope_executor(&project)).unwrap(),
            original
        );
        #[cfg(unix)]
        {
            let alias = directory.path().join("dangling");
            std::os::unix::fs::symlink(directory.path().join("missing"), &alias).unwrap();
            for project in [&alias, &alias.join("global/5")] {
                let executor = missing_scope_executor(project);
                let original = executor.diagnostic.clone();
                assert_eq!(outdated_from_fixture(executor).unwrap(), original);
            }
        }
    }

    #[test]
    fn missing_global_scope_never_masks_other_process_or_path_failures() {
        let directory = tempfile::tempdir().unwrap();
        let project = directory.path().join("global/5");
        for case in 0..7 {
            let mut executor = missing_scope_executor(&project);
            match case {
                0 => executor.root = directory.path().join("other/node_modules"),
                1 => executor.stderr = b"permission denied".to_vec(),
                2 => executor.diagnostic.push_str("registry failed\n"),
                3 => executor.root = PathBuf::from("relative/node_modules"),
                4 => executor.root = directory.path().join("global/5/not-node-modules"),
                5 => executor.root_exit = 2,
                _ => executor.exit = 2,
            }
            let original = executor.diagnostic.clone();
            let result = outdated_from_fixture(executor);
            if case >= 5 {
                assert!(result.is_err(), "case {case}: {result:?}");
            } else {
                assert_eq!(result.unwrap(), original, "case {case}");
            }
        }
    }

    #[test]
    fn allowed_exit_accepts_empty_search_output_without_stderr() {
        let stdout = interpret_allowed_exit_output(
            ManagerId::Pnpm,
            TaskType::Search,
            ManagerAction::Search,
            1,
            String::new(),
            Vec::new(),
            true,
        )
        .expect("empty search output should be treated as no matches");

        assert!(stdout.is_empty());
    }

    #[test]
    fn allowed_exit_rejects_empty_outdated_output_with_stderr() {
        let error = interpret_allowed_exit_output(
            ManagerId::Pnpm,
            TaskType::Refresh,
            ManagerAction::ListOutdated,
            1,
            String::new(),
            b"pnpm: registry failure\n".to_vec(),
            false,
        )
        .expect_err("stderr-bearing exit 1 should remain an error");

        assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
        assert!(error.message.contains("registry failure"));
    }
}
