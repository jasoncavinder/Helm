#![cfg(unix)]

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
