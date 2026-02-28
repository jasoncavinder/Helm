use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::rustup::{
    RustupDetectOutput, RustupInstallSource, RustupSource, rustup_check_request,
    rustup_download_install_script_request, rustup_init_install_request,
    rustup_run_downloaded_install_script_request, rustup_self_uninstall_request,
    rustup_self_update_request, rustup_toolchain_list_request, rustup_toolchain_update_request,
};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::{CoreError, CoreErrorKind, ManagerId};

pub struct ProcessRustupSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessRustupSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn configure_request(&self, mut request: ProcessSpawnRequest) -> ProcessSpawnRequest {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let cargo_bin = format!("{home}/.cargo/bin");
        let path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{cargo_bin}:{path}");

        request.command = request.command.env("PATH", new_path);

        // Resolve absolute path to binary if possible.
        // For rustup-init installs, we prefer ~/.cargo/bin/rustup-init first.
        let program = request
            .command
            .program
            .to_str()
            .map(|value| value.to_string());
        if let Some(program) = program.as_deref()
            && (program == "rustup" || program == "rustup-init")
        {
            let direct_path = std::path::Path::new(&cargo_bin).join(program);
            if direct_path.exists() {
                request.command.program = direct_path;
            } else if let Some(exe) = which_executable(
                self.executor.as_ref(),
                program,
                &[cargo_bin.as_str()],
                ManagerId::Rustup,
            ) {
                request.command.program = exe;
            }
        }

        request
    }
}

impl RustupSource for ProcessRustupSource {
    fn detect(&self) -> AdapterResult<RustupDetectOutput> {
        // Phase 1: instant filesystem check
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let cargo_bin = format!("{home}/.cargo/bin");
        let direct_path = Path::new(&cargo_bin).join("rustup");
        let executable_path = if direct_path.exists() {
            Some(direct_path)
        } else {
            which_executable(
                self.executor.as_ref(),
                "rustup",
                &[&cargo_bin],
                ManagerId::Rustup,
            )
        };

        // Phase 2: best-effort version detection.
        // Uses std::process::Command with read() instead of the executor's
        // wait_with_output(). Rustup spawns background subprocesses (e.g.,
        // self-update checks) that inherit stdout pipe handles, causing
        // tokio's wait_with_output() to block on pipe EOF for 10+ seconds.
        // read() returns as soon as data is available without waiting for EOF.
        let version_output = match &executable_path {
            Some(exe) => {
                let path_env = std::env::var("PATH").unwrap_or_default();
                let env_path = format!("{cargo_bin}:{path_env}");
                collect_version_output(exe, &env_path)
            }
            None => String::new(),
        };

        Ok(RustupDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn toolchain_list(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_toolchain_list_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn check(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_check_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install_self(&self, source: RustupInstallSource) -> AdapterResult<String> {
        match source {
            RustupInstallSource::OfficialDownload => self.install_self_via_official_download(),
            RustupInstallSource::ExistingBinaryPath(path) => {
                let request = self.configure_request(rustup_init_install_request(None, path));
                run_and_collect_stdout(self.executor.as_ref(), request)
            }
        }
    }

    fn self_uninstall(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_self_uninstall_request(None));
        match run_and_collect_stdout(self.executor.as_ref(), request) {
            Ok(output) => Ok(output),
            Err(error) if is_rustup_home_not_empty_failure(&error) => {
                self.retry_self_uninstall_after_cleanup(error)
            }
            Err(error) => Err(error),
        }
    }

    fn update_toolchain(&self, toolchain: &str) -> AdapterResult<String> {
        let request = self.configure_request(rustup_toolchain_update_request(None, toolchain));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn self_update(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_self_update_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

impl ProcessRustupSource {
    fn retry_self_uninstall_after_cleanup(
        &self,
        initial_error: CoreError,
    ) -> AdapterResult<String> {
        let Some(rustup_home) = resolve_rustup_home_path() else {
            return Err(append_error_context(
                initial_error,
                "rustup reported rustup_home cleanup failure but Helm could not resolve RUSTUP_HOME",
            ));
        };

        if !is_safe_rustup_home_path(&rustup_home) {
            return Err(append_error_context(
                initial_error,
                format!(
                    "refusing fallback cleanup for unsafe rustup_home path '{}'",
                    rustup_home.display()
                ),
            ));
        }

        if let Err(cleanup_error) = remove_rustup_home_with_retry(&rustup_home) {
            return Err(append_error_context(
                initial_error,
                format!(
                    "failed to remove leftover rustup_home '{}': {cleanup_error}",
                    rustup_home.display()
                ),
            ));
        }

        let retry_request = self.configure_request(rustup_self_uninstall_request(None));
        match run_and_collect_stdout(self.executor.as_ref(), retry_request) {
            Ok(output) => Ok(output),
            Err(retry_error) if is_missing_program_spawn_failure(&retry_error) => {
                // The first uninstall attempt may have removed the rustup binary before returning
                // a rustup_home cleanup error. Treat "program not found" on retry as success.
                Ok(String::new())
            }
            Err(retry_error) => Err(append_error_context(
                retry_error,
                format!(
                    "rustup self uninstall retry failed after cleaning '{}'",
                    rustup_home.display()
                ),
            )),
        }
    }

    fn install_self_via_official_download(&self) -> AdapterResult<String> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let temp_dir = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let script_path = temp_dir.join(format!("helm-rustup-init-{now}.sh"));
        let script_path_string = script_path.to_string_lossy().to_string();

        let download_request = self.configure_request(rustup_download_install_script_request(
            None,
            &script_path_string,
        ));
        if let Err(error) = run_and_collect_stdout(self.executor.as_ref(), download_request) {
            let _ = std::fs::remove_file(&script_path);
            return Err(error);
        }

        let install_result = {
            let install_request = self.configure_request(
                rustup_run_downloaded_install_script_request(None, &script_path_string),
            );
            run_and_collect_stdout(self.executor.as_ref(), install_request)
        };

        let _ = std::fs::remove_file(&script_path);
        install_result
    }
}

fn is_rustup_home_not_empty_failure(error: &CoreError) -> bool {
    if error.kind != CoreErrorKind::ProcessFailure {
        return false;
    }
    let message = error.message.to_ascii_lowercase();
    message.contains("could not remove 'rustup_home' directory")
        && (message.contains("directory not empty") || message.contains("os error 66"))
}

fn is_missing_program_spawn_failure(error: &CoreError) -> bool {
    if error.kind != CoreErrorKind::ProcessFailure {
        return false;
    }
    let message = error.message.to_ascii_lowercase();
    message.contains("failed to spawn process")
        && (message.contains("no such file or directory") || message.contains("os error 2"))
}

fn resolve_rustup_home_path() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("RUSTUP_HOME") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let home = std::env::var("HOME").ok()?;
    let trimmed = home.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(Path::new(trimmed).join(".rustup"))
}

fn is_safe_rustup_home_path(path: &Path) -> bool {
    if !path.is_absolute() || path.as_os_str().is_empty() || path.parent().is_none() {
        return false;
    }

    if path == Path::new("/") {
        return false;
    }

    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        if path == home {
            return false;
        }
    }

    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    file_name.eq_ignore_ascii_case(".rustup") || file_name.eq_ignore_ascii_case("rustup")
}

fn remove_rustup_home_with_retry(path: &Path) -> std::io::Result<()> {
    const MAX_ATTEMPTS: usize = 3;
    const RETRY_DELAY: Duration = Duration::from_millis(250);
    const ENOTEMPTY_MACOS: i32 = 66;

    for attempt in 1..=MAX_ATTEMPTS {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                let retryable = matches!(
                    error.kind(),
                    std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::ResourceBusy
                ) || error.raw_os_error() == Some(ENOTEMPTY_MACOS);
                if retryable && attempt < MAX_ATTEMPTS {
                    std::thread::sleep(RETRY_DELAY);
                    continue;
                }
                return Err(error);
            }
        }
    }

    Ok(())
}

fn append_error_context(mut error: CoreError, context: impl AsRef<str>) -> CoreError {
    let context = context.as_ref().trim();
    if !context.is_empty() {
        error.message = format!("{} [{context}]", error.message);
    }
    error
}

/// Collect version output from rustup using std::process::Command.
///
/// `read()` returns as soon as data is available — it does NOT wait for pipe
/// EOF. After reading, the pipe handle is dropped (closing the read end),
/// so `child.wait()` only waits for process exit without blocking on pipes.
fn collect_version_output(program: &Path, env_path: &str) -> String {
    use std::io::Read;

    let mut child = match std::process::Command::new(program)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .env("PATH", env_path)
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    // read() returns as soon as data is available, not on EOF.
    // Version strings are short (< 100 bytes), so one read() suffices.
    let mut buf = [0u8; 4096];
    let n = child
        .stdout
        .as_mut()
        .and_then(|pipe| pipe.read(&mut buf).ok())
        .unwrap_or(0);

    // Drop pipes before wait() to avoid blocking on pipe EOF.
    child.stdout.take();

    // Wait for the main process to exit.
    let success = child.wait().is_ok_and(|s| s.success());

    if success && n > 0 {
        String::from_utf8_lossy(&buf[..n]).to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_missing_program_spawn_failure, is_rustup_home_not_empty_failure,
        is_safe_rustup_home_path, remove_rustup_home_with_retry,
    };
    use crate::models::{CoreError, CoreErrorKind};
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn process_failure(message: &str) -> CoreError {
        CoreError {
            manager: None,
            task: None,
            action: None,
            kind: CoreErrorKind::ProcessFailure,
            message: message.to_string(),
        }
    }

    #[test]
    fn rustup_home_not_empty_failure_detection_matches_expected_shape() {
        let error = process_failure(
            "process exited with code 1: error: could not remove 'rustup_home' directory: '/Users/test/.rustup'\n\nCaused by:\n    Directory not empty (os error 66)",
        );
        assert!(is_rustup_home_not_empty_failure(&error));
    }

    #[test]
    fn missing_program_spawn_failure_detection_matches_expected_shape() {
        let error = process_failure(
            "failed to spawn process: No such file or directory (os error 2) [program=rustup]",
        );
        assert!(is_missing_program_spawn_failure(&error));
    }

    #[test]
    fn rustup_home_safety_check_rejects_unsafe_paths() {
        assert!(!is_safe_rustup_home_path(Path::new("/")));
        assert!(!is_safe_rustup_home_path(Path::new("/tmp")));
        assert!(!is_safe_rustup_home_path(Path::new("relative/.rustup")));
        assert!(is_safe_rustup_home_path(Path::new("/tmp/.rustup")));
    }

    #[test]
    fn remove_rustup_home_with_retry_removes_directory_tree() {
        let mut root = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        root.push(format!(
            "helm-rustup-home-test-{now}-{}",
            std::process::id()
        ));
        let rustup_home = root.join(".rustup");
        let nested = rustup_home.join("toolchains/stable");

        std::fs::create_dir_all(&nested).expect("test rustup home should be created");
        std::fs::write(nested.join("sentinel.txt"), "data")
            .expect("test sentinel file should be written");

        remove_rustup_home_with_retry(&rustup_home).expect("cleanup should succeed");
        assert!(!rustup_home.exists());

        let _ = std::fs::remove_dir_all(root);
    }
}
