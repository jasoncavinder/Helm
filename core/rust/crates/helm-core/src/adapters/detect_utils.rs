use std::path::PathBuf;

use crate::adapters::process_utils::run_and_collect_stdout;
use crate::execution::{CommandSpec, ProcessExecutor, ProcessSpawnRequest};
use crate::models::{ManagerAction, ManagerId, TaskType};

pub(crate) fn which_executable(
    executor: &dyn ProcessExecutor,
    binary_name: &str,
    extra_paths: &[&str],
    manager: ManagerId,
) -> Option<PathBuf> {
    let system_path = "/usr/bin:/bin:/usr/sbin:/sbin";
    let path = if extra_paths.is_empty() {
        system_path.to_string()
    } else {
        format!("{}:{system_path}", extra_paths.join(":"))
    };

    let request = ProcessSpawnRequest::new(
        manager,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new("/usr/bin/which")
            .arg(binary_name)
            .env("PATH", path),
    );

    match run_and_collect_stdout(executor, request) {
        Ok(output) => {
            let trimmed = output.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        }
        Err(_) => None,
    }
}
