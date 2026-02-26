#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::time::Duration;

use helm_core::execution::{
    CommandSpec, ProcessExitStatus, ProcessSpawnRequest, ProcessTerminationMode,
    TokioProcessExecutor, spawn_validated,
};
use helm_core::models::{CoreErrorKind, ManagerAction, ManagerId, TaskType};

fn echo_request() -> ProcessSpawnRequest {
    ProcessSpawnRequest::new(
        ManagerId::HomebrewFormula,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new("/bin/echo").arg("hello"),
    )
}

fn sleep_request() -> ProcessSpawnRequest {
    ProcessSpawnRequest::new(
        ManagerId::HomebrewFormula,
        TaskType::Refresh,
        ManagerAction::Refresh,
        CommandSpec::new("/bin/sleep").arg("30"),
    )
}

async fn wait_for_pid_file(path: &Path) -> u32 {
    for _ in 0..50 {
        if let Ok(raw) = fs::read_to_string(path)
            && let Ok(pid) = raw.trim().parse::<u32>()
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for child pid file: {}", path.display());
}

#[tokio::test]
async fn spawns_echo_and_captures_stdout() {
    let executor = TokioProcessExecutor;
    let handle = spawn_validated(&executor, echo_request()).expect("spawn should succeed");

    assert!(handle.pid().is_some());

    let output = handle.wait().await.expect("wait should succeed");
    assert_eq!(output.status, ProcessExitStatus::ExitCode(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
    assert!(output.started_at <= output.finished_at);
}

#[tokio::test]
async fn captures_nonzero_exit_code() {
    let executor = TokioProcessExecutor;
    let request = ProcessSpawnRequest::new(
        ManagerId::HomebrewFormula,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new("/usr/bin/false"),
    );

    let handle = spawn_validated(&executor, request).expect("spawn should succeed");
    let output = handle.wait().await.expect("wait should succeed");

    assert_eq!(output.status, ProcessExitStatus::ExitCode(1));
}

#[tokio::test]
async fn timeout_kills_long_running_process() {
    let executor = TokioProcessExecutor;
    let request = sleep_request().timeout(Duration::from_millis(100));

    let handle = spawn_validated(&executor, request).expect("spawn should succeed");
    let error = handle.wait().await.expect_err("should timeout");

    assert_eq!(error.kind, CoreErrorKind::Timeout);
    assert_eq!(error.manager, Some(ManagerId::HomebrewFormula));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
}

#[tokio::test]
async fn timeout_kills_process_group_children_without_orphans() {
    let executor = TokioProcessExecutor;
    let pid_file = std::env::temp_dir().join(format!(
        "helm-timeout-child-{}-{}.pid",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos()
    ));
    let pid_file_string = pid_file.to_string_lossy().to_string();
    let request = ProcessSpawnRequest::new(
        ManagerId::HomebrewFormula,
        TaskType::Refresh,
        ManagerAction::Refresh,
        CommandSpec::new("/bin/sh").args([
            "-c",
            "sleep 30 & child=$!; printf '%s' \"$child\" > \"$1\"; wait \"$child\"",
            "helm-timeout-test",
            pid_file_string.as_str(),
        ]),
    )
    .timeout(Duration::from_millis(150));

    let handle = spawn_validated(&executor, request).expect("spawn should succeed");
    let error = handle.wait().await.expect_err("should timeout");
    assert_eq!(error.kind, CoreErrorKind::Timeout);

    let child_pid = wait_for_pid_file(pid_file.as_path()).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let child_still_running = unsafe { libc::kill(child_pid as libc::pid_t, 0) == 0 };
    fs::remove_file(pid_file).ok();

    assert!(
        !child_still_running,
        "expected timeout to terminate child process group member pid={child_pid}"
    );
}

#[tokio::test]
async fn idle_timeout_kills_silent_long_running_process() {
    let executor = TokioProcessExecutor;
    let request = sleep_request()
        .timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_millis(120));

    let handle = spawn_validated(&executor, request).expect("spawn should succeed");
    let error = handle.wait().await.expect_err("should idle-timeout");

    assert_eq!(error.kind, CoreErrorKind::Timeout);
    assert_eq!(error.manager, Some(ManagerId::HomebrewFormula));
    assert_eq!(error.task, Some(TaskType::Refresh));
    assert_eq!(error.action, Some(ManagerAction::Refresh));
    assert!(
        error.message.contains("no output"),
        "idle timeout should mention output inactivity, got: {}",
        error.message
    );
}

#[tokio::test]
async fn idle_timeout_resets_when_process_is_emitting_output() {
    let executor = TokioProcessExecutor;
    let request = ProcessSpawnRequest::new(
        ManagerId::HomebrewFormula,
        TaskType::Refresh,
        ManagerAction::Refresh,
        CommandSpec::new("/bin/sh")
            .args(["-c", "for i in 1 2 3 4; do echo tick; sleep 0.05; done"]),
    )
    .timeout(Duration::from_secs(10))
    .idle_timeout(Duration::from_millis(120));

    let handle = spawn_validated(&executor, request).expect("spawn should succeed");
    let output = handle.wait().await.expect("wait should succeed");
    assert_eq!(output.status, ProcessExitStatus::ExitCode(0));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("tick"),
        "expected process output to be captured"
    );
}

#[tokio::test]
async fn immediate_terminate_kills_process() {
    let executor = TokioProcessExecutor;
    let handle = spawn_validated(&executor, sleep_request()).expect("spawn should succeed");

    handle
        .terminate(ProcessTerminationMode::Immediate)
        .expect("terminate should succeed");

    let output = handle.wait().await.expect("wait should succeed");
    assert_eq!(output.status, ProcessExitStatus::Terminated);
}

#[tokio::test]
async fn graceful_terminate_sends_sigterm() {
    let executor = TokioProcessExecutor;
    let handle = spawn_validated(&executor, sleep_request()).expect("spawn should succeed");

    handle
        .terminate(ProcessTerminationMode::Graceful {
            grace_period: Duration::from_secs(5),
        })
        .expect("terminate should succeed");

    let output = handle.wait().await.expect("wait should succeed");
    assert_eq!(output.status, ProcessExitStatus::Terminated);
}

#[tokio::test]
async fn spawn_nonexistent_program_returns_process_failure() {
    let executor = TokioProcessExecutor;
    let request = ProcessSpawnRequest::new(
        ManagerId::Npm,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new("/nonexistent/binary"),
    );

    let error = match spawn_validated(&executor, request) {
        Err(e) => e,
        Ok(_) => panic!("expected spawn to fail for nonexistent binary"),
    };

    assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
    assert_eq!(error.manager, Some(ManagerId::Npm));
    assert_eq!(error.task, Some(TaskType::Detection));
    assert_eq!(error.action, Some(ManagerAction::Detect));
}

#[tokio::test]
async fn env_vars_are_passed_to_child() {
    let executor = TokioProcessExecutor;
    let request = ProcessSpawnRequest::new(
        ManagerId::HomebrewFormula,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new("/usr/bin/env").env("HELM_TEST_VAR", "test_value_42"),
    );

    let handle = spawn_validated(&executor, request).expect("spawn should succeed");
    let output = handle.wait().await.expect("wait should succeed");

    assert_eq!(output.status, ProcessExitStatus::ExitCode(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("HELM_TEST_VAR=test_value_42"),
        "expected env var in output, got: {stdout}"
    );
}
