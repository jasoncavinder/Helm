use std::sync::Arc;
use std::time::{Duration, SystemTime};

use helm_core::adapters::homebrew::HomebrewAdapter;
use helm_core::adapters::homebrew_process::ProcessHomebrewSource;
use helm_core::adapters::mise::MiseAdapter;
use helm_core::adapters::mise_process::ProcessMiseSource;
use helm_core::adapters::rustup::RustupAdapter;
use helm_core::adapters::rustup_process::ProcessRustupSource;
use helm_core::adapters::{AdapterRequest, AdapterResponse, ListInstalledRequest, ManagerAdapter};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{ManagerId, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const HOMEBREW_INSTALLED: &str = include_str!("fixtures/homebrew/list_installed_versions.txt");
const HOMEBREW_OUTDATED: &str = include_str!("fixtures/homebrew/list_outdated_verbose.txt");
const MISE_INSTALLED: &str = include_str!("fixtures/mise/ls_json.txt");
const MISE_OUTDATED: &str = include_str!("fixtures/mise/outdated_json.txt");
const RUSTUP_TOOLCHAINS: &str = include_str!("fixtures/rustup/toolchain_list.txt");
const RUSTUP_CHECK: &str = include_str!("fixtures/rustup/check.txt");

struct MultiManagerFakeExecutor;

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9999)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for MultiManagerFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let args = &request.command.args;

        let stdout: Vec<u8> = if program.ends_with("which") {
            // Return appropriate path based on what we're looking for
            let binary = args.first().map(String::as_str).unwrap_or("");
            match binary {
                "brew" => b"/opt/homebrew/bin/brew".to_vec(),
                "mise" => b"/Users/dev/.local/bin/mise".to_vec(),
                "rustup" => b"/Users/dev/.cargo/bin/rustup".to_vec(),
                _ => Vec::new(),
            }
        } else if program == "brew" || program.ends_with("/brew") {
            match args.first().map(String::as_str) {
                Some("--version") => b"Homebrew 4.2.21\n".to_vec(),
                Some("list") => HOMEBREW_INSTALLED.as_bytes().to_vec(),
                Some("outdated") => HOMEBREW_OUTDATED.as_bytes().to_vec(),
                _ => Vec::new(),
            }
        } else if program == "mise" || program.ends_with("/mise") {
            match args.first().map(String::as_str) {
                Some("--version") => b"mise 2026.2.6 macos-x64\n".to_vec(),
                Some("ls") => MISE_INSTALLED.as_bytes().to_vec(),
                Some("outdated") => MISE_OUTDATED.as_bytes().to_vec(),
                _ => Vec::new(),
            }
        } else if program == "rustup" || program.ends_with("/rustup") {
            match args.first().map(String::as_str) {
                Some("--version") => b"rustup 1.28.2 (54dd3d00f 2024-04-24)\n".to_vec(),
                Some("toolchain") => RUSTUP_TOOLCHAINS.as_bytes().to_vec(),
                Some("check") => RUSTUP_CHECK.as_bytes().to_vec(),
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };

        Ok(Box::new(FakeProcess {
            output: ProcessOutput {
                status: ProcessExitStatus::ExitCode(0),
                stdout,
                stderr: Vec::new(),
                started_at: now,
                finished_at: now,
            },
        }))
    }
}

fn build_multi_manager_runtime() -> AdapterRuntime {
    let executor: Arc<dyn ProcessExecutor> = Arc::new(MultiManagerFakeExecutor);

    let homebrew: Arc<dyn ManagerAdapter> = Arc::new(HomebrewAdapter::new(
        ProcessHomebrewSource::new(executor.clone()),
    ));
    let mise: Arc<dyn ManagerAdapter> =
        Arc::new(MiseAdapter::new(ProcessMiseSource::new(executor.clone())));
    let rustup: Arc<dyn ManagerAdapter> =
        Arc::new(RustupAdapter::new(ProcessRustupSource::new(executor)));

    AdapterRuntime::new([homebrew, mise, rustup]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn all_three_managers_can_list_installed_independently() {
    let runtime = build_multi_manager_runtime();

    let homebrew_id = runtime
        .submit(
            ManagerId::HomebrewFormula,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let mise_id = runtime
        .submit(
            ManagerId::Mise,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let rustup_id = runtime
        .submit(
            ManagerId::Rustup,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();

    let timeout = Some(Duration::from_secs(5));

    let homebrew_snap = runtime
        .wait_for_terminal(homebrew_id, timeout)
        .await
        .unwrap();
    let mise_snap = runtime.wait_for_terminal(mise_id, timeout).await.unwrap();
    let rustup_snap = runtime.wait_for_terminal(rustup_id, timeout).await.unwrap();

    assert_eq!(homebrew_snap.runtime.status, TaskStatus::Completed);
    assert_eq!(mise_snap.runtime.status, TaskStatus::Completed);
    assert_eq!(rustup_snap.runtime.status, TaskStatus::Completed);

    // Verify each returned correct data
    match homebrew_snap.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(pkgs))) => {
            assert_eq!(pkgs.len(), 4);
        }
        other => panic!("expected Homebrew InstalledPackages, got {other:?}"),
    }

    match mise_snap.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(pkgs))) => {
            assert_eq!(pkgs.len(), 4); // node, python x2, go
        }
        other => panic!("expected mise InstalledPackages, got {other:?}"),
    }

    match rustup_snap.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(pkgs))) => {
            assert_eq!(pkgs.len(), 3); // stable, nightly, 1.75.0
        }
        other => panic!("expected rustup InstalledPackages, got {other:?}"),
    }
}

#[tokio::test]
async fn refresh_all_ordered_completes_all_managers() {
    let runtime = build_multi_manager_runtime();

    let results = runtime.refresh_all_ordered().await;

    // All three managers should have results
    assert_eq!(results.len(), 3);

    // All should succeed
    for (manager, result) in &results {
        assert!(result.is_ok(), "manager {manager:?} failed: {result:?}");
    }

    // Check that all managers are represented
    let manager_ids: Vec<ManagerId> = results.iter().map(|(id, _)| *id).collect();
    assert!(manager_ids.contains(&ManagerId::Mise));
    assert!(manager_ids.contains(&ManagerId::Rustup));
    assert!(manager_ids.contains(&ManagerId::HomebrewFormula));
}

#[tokio::test]
async fn cross_manager_parallelism_works() {
    let runtime = build_multi_manager_runtime();

    // Submit tasks to all three managers simultaneously
    let (homebrew_id, mise_id, rustup_id) = tokio::join!(
        runtime.submit(
            ManagerId::HomebrewFormula,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        ),
        runtime.submit(
            ManagerId::Mise,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        ),
        runtime.submit(
            ManagerId::Rustup,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        ),
    );

    let timeout = Some(Duration::from_secs(5));

    // All should complete successfully
    let homebrew_snap = runtime
        .wait_for_terminal(homebrew_id.unwrap(), timeout)
        .await
        .unwrap();
    let mise_snap = runtime
        .wait_for_terminal(mise_id.unwrap(), timeout)
        .await
        .unwrap();
    let rustup_snap = runtime
        .wait_for_terminal(rustup_id.unwrap(), timeout)
        .await
        .unwrap();

    assert_eq!(homebrew_snap.runtime.status, TaskStatus::Completed);
    assert_eq!(mise_snap.runtime.status, TaskStatus::Completed);
    assert_eq!(rustup_snap.runtime.status, TaskStatus::Completed);
}
