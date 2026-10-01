use serde_json::Value;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout_with_exit_policy;
use crate::execution::{ProcessExecutor, ProcessExitStatus, ProcessSpawnRequest};
use crate::models::{ManagerAction, ManagerId};

pub(crate) fn run_named_brew_outdated(
    executor: &dyn ProcessExecutor,
    request: ProcessSpawnRequest,
) -> AdapterResult<String> {
    let target = match request.manager {
        ManagerId::Podman => Some(("--formula", "formulae", "casks", "podman")),
        ManagerId::Colima => Some(("--formula", "formulae", "casks", "colima")),
        ManagerId::DockerDesktop => Some(("--cask", "casks", "formulae", "docker-desktop")),
        _ => None,
    }
    .filter(|(flag, _, _, name)| {
        request.action == ManagerAction::ListOutdated
            && request
                .command
                .program
                .file_name()
                .is_some_and(|name| name == "brew")
            && request.command.args == ["outdated", "--json=v2", flag, name]
    });

    run_and_collect_stdout_with_exit_policy(executor, request, |output| {
        // Homebrew uses exit 1 when a *named* outdated query finds an update.
        // Do not grant this exception to empty, unrelated or diagnostic output.
        output.status == ProcessExitStatus::ExitCode(1)
            && output.stderr.iter().all(u8::is_ascii_whitespace)
            && target.is_some_and(|(_, collection, other, name)| {
                valid_named_update(&output.stdout, collection, other, name)
            })
    })
}

fn valid_named_update(stdout: &[u8], collection: &str, other: &str, name: &str) -> bool {
    let Ok(json) = serde_json::from_slice::<Value>(stdout) else {
        return false;
    };
    let Some(entries) = json.get(collection).and_then(Value::as_array) else {
        return false;
    };
    if entries.len() != 1
        || !json
            .get(other)
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        return false;
    }
    let entry = &entries[0];
    entry.get("name").and_then(Value::as_str) == Some(name)
        && entry
            .get("current_version")
            .and_then(Value::as_str)
            .is_some_and(|version| !version.trim().is_empty())
        && entry
            .get("installed_versions")
            .and_then(Value::as_array)
            .is_some_and(|versions| {
                !versions.is_empty()
                    && versions.iter().all(|version| {
                        version
                            .as_str()
                            .is_some_and(|version| !version.trim().is_empty())
                    })
            })
}
