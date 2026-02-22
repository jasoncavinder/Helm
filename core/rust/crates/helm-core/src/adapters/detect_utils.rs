use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::adapters::process_utils::run_and_collect_stdout;
use crate::execution::{CommandSpec, ProcessExecutor, ProcessSpawnRequest};
use crate::models::{ManagerAction, ManagerId, TaskType};

pub(crate) fn which_executable(
    executor: &dyn ProcessExecutor,
    binary_name: &str,
    extra_paths: &[&str],
    manager: ManagerId,
) -> Option<PathBuf> {
    which_executable_via_which(executor, binary_name, extra_paths, manager)
        .or_else(|| discover_executable_path(binary_name, extra_paths, manager))
}

fn which_executable_via_which(
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

fn discover_executable_path(
    binary_name: &str,
    extra_paths: &[&str],
    manager: ManagerId,
) -> Option<PathBuf> {
    if binary_name.trim().is_empty() {
        return None;
    }

    if binary_name.contains('/') {
        let absolute = PathBuf::from(binary_name);
        return absolute.is_file().then_some(absolute);
    }

    let mut seen = HashSet::new();
    let mut candidates = Vec::new();

    for extra in extra_paths {
        push_candidate_path(
            Path::new(extra).join(binary_name),
            &mut candidates,
            &mut seen,
        );
    }

    if let Some(path_var) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_var) {
            push_candidate_path(dir.join(binary_name), &mut candidates, &mut seen);
        }
    }

    for dir in manager_additional_bin_roots() {
        push_candidate_path(dir.join(binary_name), &mut candidates, &mut seen);
    }

    if let Some(found) = candidates.into_iter().find(|candidate| candidate.is_file()) {
        return Some(found);
    }

    discover_from_versioned_roots(binary_name, manager, &mut seen)
}

fn push_candidate_path(
    candidate: PathBuf,
    candidates: &mut Vec<PathBuf>,
    seen: &mut HashSet<String>,
) {
    let rendered = candidate.to_string_lossy().to_string();
    if rendered.is_empty() {
        return;
    }

    if seen.insert(rendered) {
        candidates.push(candidate);
    }
}

fn manager_additional_bin_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
        PathBuf::from("/run/current-system/sw/bin"),
        PathBuf::from("/nix/var/nix/profiles/default/bin"),
    ];

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".local/bin"));
        roots.push(home.join(".cargo/bin"));
        roots.push(home.join(".asdf/bin"));
        roots.push(home.join(".asdf/shims"));
        roots.push(home.join(".nix-profile/bin"));
    }

    roots
}

fn discover_from_versioned_roots(
    binary_name: &str,
    manager: ManagerId,
    seen: &mut HashSet<String>,
) -> Option<PathBuf> {
    for root in manager_versioned_install_roots(manager) {
        let Ok(tool_dirs) = std::fs::read_dir(root) else {
            continue;
        };

        for tool_dir in tool_dirs.flatten() {
            let tool_path = tool_dir.path();
            if !tool_path.is_dir() {
                continue;
            }

            let Ok(version_dirs) = std::fs::read_dir(&tool_path) else {
                continue;
            };
            for version_dir in version_dirs.flatten() {
                let version_path = version_dir.path();
                if !version_path.is_dir() {
                    continue;
                }

                let candidate = version_path.join("bin").join(binary_name);
                let rendered = candidate.to_string_lossy().to_string();
                if rendered.is_empty() || !seen.insert(rendered) {
                    continue;
                }

                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

fn manager_versioned_install_roots(manager: ManagerId) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if uses_homebrew_cellar(manager) {
        roots.push(PathBuf::from("/opt/homebrew/Cellar"));
        roots.push(PathBuf::from("/usr/local/Cellar"));
    }

    if uses_tool_version_installs(manager)
        && let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
    {
        roots.push(home.join(".asdf/installs"));
        roots.push(home.join(".local/share/mise/installs"));
    }

    roots
}

fn uses_homebrew_cellar(manager: ManagerId) -> bool {
    matches!(
        manager,
        ManagerId::HomebrewFormula
            | ManagerId::HomebrewCask
            | ManagerId::Mise
            | ManagerId::Asdf
            | ManagerId::Rustup
            | ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
            | ManagerId::Mas
            | ManagerId::DockerDesktop
            | ManagerId::Podman
            | ManagerId::Colima
    )
}

fn uses_tool_version_installs(manager: ManagerId) -> bool {
    matches!(
        manager,
        ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
    )
}
