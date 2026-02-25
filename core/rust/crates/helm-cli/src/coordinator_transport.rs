use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

pub(crate) const COORDINATOR_BOOTSTRAP_LOCK_FILE: &str = ".bootstrap.lock";
pub(crate) const COORDINATOR_BOOTSTRAP_LOCK_WAIT_TIMEOUT_MS: u64 = 5_000;
pub(crate) const COORDINATOR_BOOTSTRAP_LOCK_STALE_SECS: u64 = 15;
pub(crate) const COORDINATOR_DAEMON_READY_TIMEOUT_MS: u64 = 3_000;
pub(crate) const PS_COMMAND_PATH: &str = "/bin/ps";

pub(crate) fn coordinator_request_timeout(timeout_seconds: u64) -> Duration {
    Duration::from_secs(timeout_seconds.max(1))
}

pub(crate) fn coordinator_response_poll_interval(elapsed: Duration) -> Duration {
    match elapsed.as_millis() {
        0..=200 => Duration::from_millis(10),
        201..=1_000 => Duration::from_millis(25),
        1_001..=5_000 => Duration::from_millis(100),
        _ => Duration::from_millis(250),
    }
}

pub(crate) fn coordinator_server_idle_poll_interval(empty_iterations: u32) -> Duration {
    if empty_iterations <= 10 {
        Duration::from_millis(25)
    } else if empty_iterations <= 40 {
        Duration::from_millis(100)
    } else {
        Duration::from_millis(250)
    }
}

pub(crate) fn coordinator_bootstrap_lock_poll_interval(elapsed: Duration) -> Duration {
    match elapsed.as_millis() {
        0..=500 => Duration::from_millis(25),
        501..=2_000 => Duration::from_millis(50),
        _ => Duration::from_millis(100),
    }
}

pub(crate) fn coordinator_startup_poll_interval(elapsed: Duration) -> Duration {
    match elapsed.as_millis() {
        0..=500 => Duration::from_millis(10),
        501..=2_000 => Duration::from_millis(25),
        _ => Duration::from_millis(50),
    }
}

pub(crate) fn coordinator_socket_path(database_path: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    database_path.hash(&mut hasher);
    let suffix = format!("{:x}", hasher.finish());
    let root = env::var("TMPDIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    root.join(format!("helm-cli-coordinator-{suffix}"))
}

pub(crate) fn coordinator_ready_file(state_dir: &Path) -> PathBuf {
    state_dir.join("ready.json")
}

pub(crate) fn coordinator_requests_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("requests")
}

pub(crate) fn coordinator_responses_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("responses")
}

pub(crate) fn coordinator_request_file(state_dir: &Path, request_id: &str) -> PathBuf {
    coordinator_requests_dir(state_dir).join(format!("{request_id}.json"))
}

pub(crate) fn coordinator_response_file(state_dir: &Path, request_id: &str) -> PathBuf {
    coordinator_responses_dir(state_dir).join(format!("{request_id}.json"))
}

pub(crate) fn coordinator_bootstrap_lock_file(state_dir: &Path) -> PathBuf {
    state_dir.join(COORDINATOR_BOOTSTRAP_LOCK_FILE)
}

pub(crate) fn coordinator_bootstrap_lock_is_stale(lock_file: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(lock_file) else {
        return false;
    };
    let Some(elapsed) = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
    else {
        return false;
    };
    elapsed >= Duration::from_secs(COORDINATOR_BOOTSTRAP_LOCK_STALE_SECS)
}

pub(crate) fn file_modified_unix_seconds(path: &Path) -> Option<i64> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_secs()).ok()
}

pub(crate) fn process_is_alive(pid: u32) -> bool {
    let output = std::process::Command::new(PS_COMMAND_PATH)
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("pid=")
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    !stdout.trim().is_empty()
}

pub(crate) fn coordinator_process_looks_owned(pid: u32, state_dir: &Path) -> bool {
    let output = std::process::Command::new(PS_COMMAND_PATH)
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("command=")
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if command.is_empty() {
        return false;
    }
    command.contains("__coordinator__") && command.contains(state_dir.to_string_lossy().as_ref())
}
