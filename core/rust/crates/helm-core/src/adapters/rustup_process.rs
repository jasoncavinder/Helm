use std::path::Path;
use std::sync::Arc;

use crate::adapters::detect_utils::which_executable;
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::rustup::{
    RustupDetectOutput, RustupSource, rustup_check_request, rustup_self_uninstall_request,
    rustup_self_update_request, rustup_toolchain_list_request, rustup_toolchain_update_request,
};
use crate::execution::{ProcessExecutor, ProcessSpawnRequest};
use crate::models::ManagerId;

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

        // Resolve absolute path to binary if possible
        if request.command.program.to_str() == Some("rustup") {
            // 1. Try direct path resolution first (most reliable)
            let direct_path = std::path::Path::new(&cargo_bin).join("rustup");
            if direct_path.exists() {
                request.command.program = direct_path;
            } else {
                // 2. Fallback to `which` if not found in standard location
                if let Some(exe) = which_executable(
                    self.executor.as_ref(),
                    "rustup",
                    &[cargo_bin.as_str()],
                    ManagerId::Rustup,
                ) {
                    request.command.program = exe;
                }
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

    fn self_uninstall(&self) -> AdapterResult<String> {
        let request = self.configure_request(rustup_self_uninstall_request(None));
        run_and_collect_stdout(self.executor.as_ref(), request)
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
