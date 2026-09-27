use super::{ExecutionResult, ProcessSpawnRequest};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId};

fn needs_toolchain(request: &ProcessSpawnRequest) -> bool {
    let program = request.command.program.to_str().unwrap_or_default();
    if matches!(
        program,
        "/usr/bin/python3"
            | "/usr/bin/git"
            | "/usr/bin/clang"
            | "/usr/bin/cc"
            | "/usr/bin/make"
            | "/usr/bin/install_name_tool"
            | "/usr/bin/xcrun"
            | "/usr/bin/xcodebuild"
    ) {
        return true;
    }
    let name = request
        .command
        .program
        .file_name()
        .and_then(|name| name.to_str());
    let verb = request.command.args.first().map(String::as_str);
    (matches!(
        (name, verb),
        (Some("cargo"), Some("install")) | (Some("port"), Some("install" | "upgrade"))
    ) && matches!(
        request.action,
        ManagerAction::Install | ManagerAction::Upgrade
    )) || (request.manager == ManagerId::HomebrewFormula
        && name == Some("brew")
        && matches!(verb, Some("install" | "upgrade")))
}

fn binary_install(request: &ProcessSpawnRequest) -> bool {
    request.manager == ManagerId::CargoBinstall
        && matches!(
            request.action,
            ManagerAction::Install | ManagerAction::Upgrade
        )
        && request
            .command
            .program
            .file_name()
            .is_some_and(|name| name == "cargo-binstall")
}

fn prepare_with_probe(
    mut request: ProcessSpawnRequest,
    probe: impl FnOnce(&ProcessSpawnRequest) -> bool,
) -> ExecutionResult<ProcessSpawnRequest> {
    let required = needs_toolchain(&request);
    let binary = binary_install(&request);
    if (!required && !binary) || probe(&request) {
        return Ok(request);
    }
    if binary {
        // Keep binary-only installation available; never silently invoke Apple's
        // compiler stubs through cargo-binstall's source-build fallback.
        let mut disabled = request
            .command
            .env
            .get("BINSTALL_DISABLE_STRATEGIES")
            .cloned()
            .or_else(|| {
                (!request
                    .command
                    .env_remove
                    .iter()
                    .any(|key| key == "BINSTALL_DISABLE_STRATEGIES"))
                .then(|| std::env::var("BINSTALL_DISABLE_STRATEGIES").ok())
                .flatten()
            })
            .unwrap_or_default();
        if !disabled.split(',').any(|value| value.trim() == "compile") {
            if !disabled.is_empty() {
                disabled.push(',');
            }
            disabled.push_str("compile");
        }
        request
            .command
            .env
            .insert("BINSTALL_DISABLE_STRATEGIES".into(), disabled);
        super::record_task_log_note(
            "Apple developer tools are unavailable: binary installation is allowed, but source fallback is disabled. Install Command Line Tools with xcode-select --install if compilation is needed.",
        );
        return Ok(request);
    }
    Err(CoreError {
        manager: Some(request.manager), task: Some(request.task_type), action: Some(request.action),
        kind: CoreErrorKind::InvalidInput,
        message: "[developer_tools_required] This operation requires Apple Command Line Tools or a configured Xcode installation. Run xcode-select --install in Terminal, complete Apple's installer, then retry. Helm itself does not require developer tools to launch or view cached data.".into(),
    })
}

#[cfg(target_os = "macos")]
pub(super) fn prepare(request: ProcessSpawnRequest) -> ExecutionResult<ProcessSpawnRequest> {
    prepare_with_probe(request, selected_toolchain_available)
}

#[cfg(target_os = "macos")]
fn selected_toolchain_available(request: &ProcessSpawnRequest) -> bool {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // xcode-select is an OS utility, not a developer-tool shim. --print-path is
    // read-only and does not display Apple's install prompt.
    let mut command = Command::new("/usr/bin/xcode-select");
    command
        .arg("--print-path")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if request
        .command
        .env_remove
        .iter()
        .any(|key| key == "DEVELOPER_DIR")
    {
        command.env_remove("DEVELOPER_DIR");
    }
    if let Some(value) = request.command.env.get("DEVELOPER_DIR") {
        command.env("DEVELOPER_DIR", value);
    }
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return false,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
    let mut output = String::new();
    if child
        .stdout
        .take()
        .is_none_or(|mut stdout| stdout.read_to_string(&mut output).is_err())
    {
        return false;
    }
    let root = std::path::Path::new(output.trim());
    root.is_absolute()
        && (root.join("usr/bin/clang").is_file()
            || root
                .join("Toolchains/XcodeDefault.xctoolchain/usr/bin/clang")
                .is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::CommandSpec;
    use crate::models::TaskType;

    fn request(
        manager: ManagerId,
        program: &str,
        args: &[&str],
        action: ManagerAction,
    ) -> ProcessSpawnRequest {
        ProcessSpawnRequest::new(
            manager,
            TaskType::Install,
            action,
            CommandSpec::new(program).args(args.iter().copied()),
        )
    }

    #[test]
    fn read_only_and_independent_tools_do_not_probe_or_require_clt() {
        for request in [
            request(
                ManagerId::HomebrewFormula,
                "brew",
                &["info", "--installed"],
                ManagerAction::ListInstalled,
            ),
            request(
                ManagerId::Pip,
                "/opt/homebrew/bin/python3",
                &["-m", "pip", "--version"],
                ManagerAction::Detect,
            ),
            request(
                ManagerId::Npm,
                "npm",
                &["install", "-g", "prettier"],
                ManagerAction::Install,
            ),
            request(
                ManagerId::CargoBinstall,
                "cargo",
                &["install", "--list"],
                ManagerAction::ListInstalled,
            ),
        ] {
            assert!(
                prepare_with_probe(request, |_| panic!("unexpected developer tools probe")).is_ok()
            );
        }
    }

    #[test]
    fn apple_shims_and_compilation_are_blocked_before_spawning_when_tools_are_missing() {
        for request in [
            request(
                ManagerId::Pip,
                "/usr/bin/python3",
                &["-m", "pip", "--version"],
                ManagerAction::Detect,
            ),
            request(
                ManagerId::Cargo,
                "cargo",
                &["install", "choose"],
                ManagerAction::Install,
            ),
            request(
                ManagerId::MacPorts,
                "/opt/local/bin/port",
                &["upgrade", "hello"],
                ManagerAction::Upgrade,
            ),
            request(
                ManagerId::HomebrewFormula,
                "brew",
                &["install", "hello"],
                ManagerAction::Install,
            ),
        ] {
            assert!(prepare_with_probe(request.clone(), |_| true).is_ok());
            let error = prepare_with_probe(request, |_| false).unwrap_err();
            assert_eq!(error.kind, CoreErrorKind::InvalidInput);
            assert!(error.message.contains("xcode-select --install"));
        }
    }

    #[test]
    fn binary_install_still_works_without_tools_and_preserves_disabled_strategies() {
        let mut request = request(
            ManagerId::CargoBinstall,
            "cargo-binstall",
            &["ripgrep", "--no-confirm"],
            ManagerAction::Install,
        );
        request
            .command
            .env
            .insert("BINSTALL_DISABLE_STRATEGIES".into(), "quick-install".into());
        let prepared = prepare_with_probe(request, |_| false).unwrap();
        assert_eq!(
            prepared.command.env["BINSTALL_DISABLE_STRATEGIES"],
            "quick-install,compile"
        );
    }
}
