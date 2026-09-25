use std::{collections::BTreeSet, path::Path, sync::Arc};

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::{run_and_collect_stdout, run_and_collect_version_output};
use crate::adapters::yarn::{
    YarnDetectOutput, YarnSource, yarn_detect_request, yarn_install_request,
    yarn_list_installed_request, yarn_list_outdated_request, yarn_search_request,
    yarn_uninstall_request, yarn_upgrade_request,
};
use crate::execution::{
    ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest, spawn_validated,
};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, SearchQuery, TaskType};

pub struct ProcessYarnSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessYarnSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn global_request(&self, request: ProcessSpawnRequest) -> AdapterResult<ProcessSpawnRequest> {
        let mut directory_request = yarn_list_installed_request(None);
        directory_request.command.args = vec!["global".into(), "dir".into(), "--silent".into()];
        let directory = run_and_collect_stdout(
            self.executor.as_ref(),
            self.configure_request(directory_request),
        )?;
        let directory = directory.trim();
        if !std::path::Path::new(directory).is_absolute() || directory.chars().any(char::is_control)
        {
            return Err(CoreError {
                manager: Some(ManagerId::Yarn),
                task: Some(TaskType::Refresh),
                action: Some(request.action),
                kind: CoreErrorKind::ParseFailure,
                message: "yarn did not report an absolute global directory".into(),
            });
        }
        let mut request = self.configure_request(request);
        request.command = request
            .command
            .args(["--cwd", directory])
            .working_dir(directory);
        Ok(request)
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        // XPC services have a constrained PATH; include common yarn binary locations.
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("/opt/homebrew/bin:/usr/local/bin:{path}");
        request.command = request
            .command
            .env("PATH", new_path)
            .env("YARN_CONFIG_UPDATE_NOTIFIER", "false")
            .env("YARN_CONFIG_FUND", "false")
            .env("YARN_CONFIG_AUDIT", "false");

        if request.command.program.to_str() == Some("yarn")
            && let Some(exe) = which_executable(
                self.executor.as_ref(),
                "yarn",
                &["/opt/homebrew/bin", "/usr/local/bin"],
                ManagerId::Yarn,
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

fn interpret_allowed_exit_output(
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    code: i32,
    stdout: String,
    stderr_bytes: Vec<u8>,
    allow_empty_stdout_without_stderr: bool,
) -> AdapterResult<String> {
    if stdout
        .lines()
        .chain(String::from_utf8_lossy(&stderr_bytes).lines())
        .any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|value| {
                    value.get("type").and_then(serde_json::Value::as_str) == Some("error")
                })
        })
    {
        return Err(yarn_read_error(
            action,
            "yarn reported an error in its JSON output",
        ));
    }
    if code != 0
        && action == ManagerAction::ListOutdated
        && !stdout.lines().any(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|value| {
                    value.get("type").and_then(serde_json::Value::as_str) == Some("table")
                })
        })
    {
        return Err(CoreError {
            manager: Some(manager),
            task: Some(task_type),
            action: Some(action),
            kind: CoreErrorKind::ProcessFailure,
            message: format!(
                "yarn outdated exited with code {code} without an update table: {}",
                String::from_utf8_lossy(&stderr_bytes)
            ),
        });
    }
    if !stdout.trim().is_empty() {
        return Ok(stdout);
    }

    let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
    if allow_empty_stdout_without_stderr && (code == 0 || stderr.is_empty()) {
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

impl YarnSource for ProcessYarnSource {
    fn detect(&self) -> AdapterResult<YarnDetectOutput> {
        let executable_path = which_executable(
            self.executor.as_ref(),
            "yarn",
            &["/opt/homebrew/bin", "/usr/local/bin"],
            ManagerId::Yarn,
        );

        let request = self.configure_request(yarn_detect_request(None));
        let version_output = run_and_collect_version_output(self.executor.as_ref(), request);

        Ok(YarnDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_installed_global(&self) -> AdapterResult<String> {
        let request = self.global_request(yarn_list_installed_request(None))?;
        let names = global_package_names(
            request
                .command
                .working_dir
                .as_deref()
                .expect("global directory"),
        )?;
        if names.is_empty() {
            return Ok(r#"{"dependencies":{}}"#.into());
        }
        let output = run_and_collect_stdout(self.executor.as_ref(), request)?;
        filter_global_tree(&output, &names)
    }

    fn list_outdated_global(&self) -> AdapterResult<String> {
        // yarn uses exit code 1 to indicate outdated packages were found.
        let request = self.global_request(yarn_list_outdated_request(None))?;
        if global_package_names(
            request
                .command
                .working_dir
                .as_deref()
                .expect("global directory"),
        )?
        .is_empty()
        {
            return Ok(String::new());
        }
        self.run_and_collect_stdout_accepting(request, &[1], true)
    }

    fn search(&self, query: &str) -> AdapterResult<String> {
        // Yarn Classic has no registry search command. Offer exact-name lookup
        // through its own configured registry, without switching to npm's config.
        if query.is_empty() || query.chars().any(char::is_whitespace) {
            return Ok(String::new());
        }
        crate::adapters::validate_package_identifier(
            ManagerId::Yarn,
            ManagerAction::Search,
            query,
        )?;
        let search_query = SearchQuery {
            text: query.to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let request = self.configure_request(yarn_search_request(None, &search_query));
        // Do not interpret failed registry requests as successful empty searches.
        self.run_and_collect_stdout_accepting(request, &[], true)
    }

    fn install_global(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(yarn_install_request(None, name, version));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn uninstall_global(&self, name: &str) -> AdapterResult<String> {
        let request = self.configure_request(yarn_uninstall_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn upgrade_global(&self, name: Option<&str>) -> AdapterResult<String> {
        let request = self.configure_request(yarn_upgrade_request(None, name));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

fn yarn_read_error(action: ManagerAction, message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Yarn),
        task: Some(TaskType::Refresh),
        action: Some(action),
        kind: CoreErrorKind::ParseFailure,
        message: message.into(),
    }
}

fn global_package_names(directory: &Path) -> AdapterResult<BTreeSet<String>> {
    let manifest = match std::fs::read(directory.join("package.json")) {
        Ok(bytes) => bytes,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                && ["package.json", "yarn.lock", "node_modules"]
                    .iter()
                    .all(|entry| {
                        std::fs::symlink_metadata(directory.join(entry))
                            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
                    }) =>
        {
            return Ok(BTreeSet::new());
        }
        Err(error) => {
            return Err(yarn_read_error(
                ManagerAction::ListInstalled,
                &format!("cannot read yarn global manifest: {error}"),
            ));
        }
    };
    let manifest: serde_json::Value = serde_json::from_slice(&manifest).map_err(|error| {
        yarn_read_error(
            ManagerAction::ListInstalled,
            &format!("invalid yarn global manifest: {error}"),
        )
    })?;
    if !manifest.is_object() {
        return Err(yarn_read_error(
            ManagerAction::ListInstalled,
            "invalid yarn global manifest object",
        ));
    }
    let mut names = BTreeSet::new();
    for field in ["dependencies", "devDependencies", "optionalDependencies"] {
        if let Some(dependencies) = manifest.get(field) {
            let dependencies = dependencies.as_object().ok_or_else(|| {
                yarn_read_error(ManagerAction::ListInstalled, "invalid yarn dependency map")
            })?;
            names.extend(dependencies.keys().cloned());
        }
    }
    Ok(names)
}

fn filter_global_tree(output: &str, names: &BTreeSet<String>) -> AdapterResult<String> {
    let mut result = Vec::new();
    let mut saw_tree = false;
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let mut event: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            yarn_read_error(
                ManagerAction::ListInstalled,
                &format!("invalid yarn list event: {error}"),
            )
        })?;
        if event.get("type").and_then(serde_json::Value::as_str) == Some("error") {
            return Err(yarn_read_error(
                ManagerAction::ListInstalled,
                "yarn list reported an error",
            ));
        }
        if event.get("type").and_then(serde_json::Value::as_str) == Some("tree") {
            let trees = event
                .pointer_mut("/data/trees")
                .and_then(serde_json::Value::as_array_mut)
                .ok_or_else(|| {
                    yarn_read_error(ManagerAction::ListInstalled, "missing yarn package tree")
                })?;
            // Hoisted transitive dependencies are not independently managed global packages.
            trees.retain(|tree| {
                tree.get("name")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|name| name.rsplit_once('@'))
                    .is_some_and(|(name, _)| names.contains(name))
            });
            saw_tree = true;
        }
        result.push(event.to_string());
    }
    if !saw_tree {
        return Err(yarn_read_error(
            ManagerAction::ListInstalled,
            "missing yarn package tree",
        ));
    }
    Ok(result.join("\n"))
}

#[cfg(test)]
mod tests {
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, TaskType};

    use super::{filter_global_tree, global_package_names, interpret_allowed_exit_output};

    #[test]
    fn fresh_global_scope_is_empty_but_damaged_scope_is_not() {
        let directory = tempfile::tempdir().unwrap();
        assert!(global_package_names(directory.path()).unwrap().is_empty());
        std::fs::create_dir(directory.path().join("node_modules")).unwrap();
        assert!(global_package_names(directory.path()).is_err());
        std::fs::write(
            directory.path().join("package.json"),
            r#"{"dependencies":{"@scope/tool":"1.0","prettier":"3.5.3"}}"#,
        )
        .unwrap();
        let names = global_package_names(directory.path()).unwrap();
        assert_eq!(names.len(), 2);
        let output = filter_global_tree(r#"{"type":"tree","data":{"trees":[{"name":"transitive@1"},{"name":"@scope/tool@1.0"},{"name":"prettier@3.5.3"}]}}"#, &names).unwrap();
        assert!(!output.contains("transitive"));
        assert!(output.contains("@scope/tool@1.0"));
        assert!(filter_global_tree(r#"{"type":"error","data":"bad lockfile"}"#, &names).is_err());
        std::fs::write(directory.path().join("package.json"), "null").unwrap();
        assert!(global_package_names(directory.path()).is_err());
    }

    #[test]
    fn json_error_is_not_a_successful_search_or_partial_outdated_result() {
        for action in [ManagerAction::Search, ManagerAction::ListOutdated] {
            assert!(
                interpret_allowed_exit_output(
                    ManagerId::Yarn,
                    TaskType::Refresh,
                    action,
                    0,
                    r#"{"type":"error","data":"registry unavailable"}"#.into(),
                    Vec::new(),
                    true
                )
                .is_err()
            );
        }
    }

    #[test]
    fn successful_empty_outdated_allows_nonfatal_warnings() {
        assert!(
            interpret_allowed_exit_output(
                ManagerId::Yarn,
                TaskType::Refresh,
                ManagerAction::ListOutdated,
                0,
                String::new(),
                b"DeprecationWarning: url.parse\n".to_vec(),
                true
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn allowed_exit_accepts_empty_search_output_without_stderr() {
        let stdout = interpret_allowed_exit_output(
            ManagerId::Yarn,
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
            ManagerId::Yarn,
            TaskType::Refresh,
            ManagerAction::ListOutdated,
            1,
            String::new(),
            b"Usage Error: Couldn't find the node_modules state file\n".to_vec(),
            false,
        )
        .expect_err("stderr-bearing exit 1 should remain an error");

        assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
        assert!(error.message.contains("Couldn't find"));
    }

    #[test]
    fn failed_outdated_with_only_warning_is_not_an_empty_inventory() {
        assert!(
            interpret_allowed_exit_output(
                ManagerId::Yarn,
                TaskType::Refresh,
                ManagerAction::ListOutdated,
                1,
                r#"{"type":"warning","data":"no license"}"#.into(),
                Vec::new(),
                true
            )
            .is_err()
        );
    }
}
