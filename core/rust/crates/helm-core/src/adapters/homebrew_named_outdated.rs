use serde_json::Value;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout_with_exit_policy;
use crate::execution::{ProcessExecutor, ProcessExitStatus, ProcessSpawnRequest};
use crate::models::{ManagerAction, ManagerId};

pub(crate) fn configure_brew_read(mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
    // Delegated reads must not update Homebrew itself or race its update lock.
    request.command = request
        .command
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1");
    request
}

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
        // API download progress is not a diagnostic, but unknown stderr still fails.
        output.status == ProcessExitStatus::ExitCode(1)
            && only_successful_api_progress(&output.stderr)
            && target.is_some_and(|(_, collection, other, name)| {
                valid_named_update(&output.stdout, collection, other, name)
            })
    })
}

fn only_successful_api_progress(stderr: &[u8]) -> bool {
    if stderr.iter().all(u8::is_ascii_whitespace) {
        return true;
    }
    if stderr.len() > 4096 {
        return false;
    }
    let Ok(text) = std::str::from_utf8(stderr) else {
        return false;
    };
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    if lines.next() != Some("==> Downloading Homebrew API data") {
        return false;
    }
    let mut completed = false;
    for line in lines {
        let Some(name) = line
            .strip_prefix("\u{2714}\u{fe0e} JSON API ")
            .or_else(|| line.strip_prefix("\u{2714} JSON API "))
        else {
            return false;
        };
        if !is_api_metadata_filename(name) {
            return false;
        }
        completed = true;
    }
    completed
}

fn is_api_metadata_filename(name: &str) -> bool {
    if matches!(name, "formula.jws.json" | "cask.jws.json") {
        return true;
    }
    let Some(platform) = name
        .strip_prefix("packages.")
        .and_then(|name| name.strip_suffix(".jws.json"))
        .and_then(|platform| {
            platform
                .strip_prefix("arm64_")
                .or_else(|| platform.strip_prefix("x86_64_"))
        })
    else {
        return false;
    };
    !platform.is_empty()
        && platform.len() <= 64
        && platform
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
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

#[cfg(test)]
mod tests {
    use super::only_successful_api_progress;

    #[test]
    fn accepts_only_complete_recognized_api_success_reports() {
        for name in [
            "formula.jws.json",
            "cask.jws.json",
            "packages.arm64_golden_gate.jws.json",
            "packages.x86_64_ventura.jws.json",
        ] {
            for marker in ["\u{2714}", "\u{2714}\u{fe0e}"] {
                assert!(only_successful_api_progress(
                    format!("==> Downloading Homebrew API data\n{marker} JSON API {name}\n")
                        .as_bytes()
                ));
            }
        }
        assert!(only_successful_api_progress(b" \n\r\t"));
        assert!(only_successful_api_progress(
            "==> Downloading Homebrew API data\n\u{2714} JSON API formula.jws.json\n\u{2714} JSON API cask.jws.json\n".as_bytes()
        ));
    }

    #[test]
    fn rejects_incomplete_unknown_oversized_and_malformed_progress() {
        for text in [
            "==> Downloading Homebrew API data\n",
            "\u{2714} JSON API formula.jws.json",
            "==> Downloading Homebrew API data\n\u{2718} JSON API formula.jws.json",
            "==> Downloading Homebrew API data\n\u{2714} JSON API ../formula.jws.json",
            "==> Downloading Homebrew API data\n\u{2714} JSON API packages.arm64_.jws.json",
            "==> Downloading Homebrew API data\n\u{2714} JSON API packages.arm64_os/error.jws.json",
            "==> Downloading Homebrew API data\n\u{2714} JSON API unexpected.json",
            "==> Downloading Homebrew API data\n==> Downloading Homebrew API data\n\u{2714} JSON API formula.jws.json",
            "==> Downloading Homebrew API data\n\u{2714} JSON API formula.jws.json\nWarning: stale metadata",
        ] {
            assert!(!only_successful_api_progress(text.as_bytes()), "{text}");
        }
        assert!(!only_successful_api_progress(b"\xff"));
        assert!(!only_successful_api_progress(
            format!(
                "==> Downloading Homebrew API data\n{}",
                "\u{2714} JSON API formula.jws.json\n".repeat(200)
            )
            .as_bytes()
        ));
    }
}
