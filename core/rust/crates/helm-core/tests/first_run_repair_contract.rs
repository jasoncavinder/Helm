#![cfg(unix)]
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::first_run::LocalObservationContext;
use helm_core::first_run_repair::{
    FirstRunRepairPlan, FirstRunRepairStore, RepairVerification, apply_approved_first_run_repair,
    propose_first_run_repair,
};
use helm_core::models::{ManagerId, TaskId, TaskRecord, TaskStatus, TaskType};
use helm_core::persistence::{
    DetectionStore, FirstRunExperience, FirstRunStore, MigrationStore, TaskStore,
};
use helm_core::sqlite::SqliteStore;

struct Fixture {
    root: tempfile::TempDir,
    store: Arc<SqliteStore>,
    context: LocalObservationContext,
    binary: PathBuf,
    stale: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(SqliteStore::new(root.path().join("helm.db")));
        store.migrate_to_latest().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let binary = bin.join("mise");
        write_binary(&binary);
        let stale = root.path().join("old/mise");
        store.set_manager_enabled(ManagerId::Mise, true).unwrap();
        store
            .set_manager_selected_executable_path(ManagerId::Mise, stale.to_str())
            .unwrap();
        store
            .set_manager_selected_install_method(ManagerId::Mise, Some("homebrew"))
            .unwrap();
        store
            .set_manager_timeout_hard_seconds(ManagerId::Mise, Some(90))
            .unwrap();
        Self {
            root,
            store,
            binary,
            stale,
            context: LocalObservationContext {
                search_directories: vec![bin],
                include_system_candidates: false,
            },
        }
    }
    fn plan(&self) -> FirstRunRepairPlan {
        propose_first_run_repair(self.store.as_ref(), &self.context)
            .unwrap()
            .unwrap()
    }
    fn selected(&self) -> Option<String> {
        self.store
            .list_manager_preferences()
            .unwrap()
            .into_iter()
            .find(|p| p.manager == ManagerId::Mise)
            .unwrap()
            .selected_executable_path
    }
}
fn write_binary(path: &std::path::Path) {
    fs::write(path, [0xcf, 0xfa, 0xed, 0xfe, 0, 0, 0, 0]).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

type Hook = Arc<dyn Fn() + Send + Sync>;
struct Executor {
    calls: Mutex<Vec<ProcessSpawnRequest>>,
    stdout: Vec<u8>,
    exit: i32,
    hook: Option<Hook>,
}
impl Default for Executor {
    fn default() -> Self {
        Self {
            calls: Mutex::new(vec![]),
            stdout: b"2026.8.5 macos-arm64\n".to_vec(),
            exit: 0,
            hook: None,
        }
    }
}
struct Process {
    output: ProcessOutput,
    hook: Option<Hook>,
}
impl RunningProcess for Process {
    fn pid(&self) -> Option<u32> {
        None
    }
    fn terminate(&self, _: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }
    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        Box::pin(async move {
            if let Some(hook) = self.hook {
                hook();
            }
            Ok(self.output)
        })
    }
}
impl ProcessExecutor for Executor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        self.calls.lock().unwrap().push(request);
        Ok(Box::new(Process {
            output: ProcessOutput {
                status: ProcessExitStatus::ExitCode(self.exit),
                stdout: self.stdout.clone(),
                stderr: vec![],
                started_at: SystemTime::now(),
                finished_at: SystemTime::now(),
            },
            hook: self.hook.clone(),
        }))
    }
}

#[test]
fn proposal_is_nonexecuting_and_preserves_preferences_acknowledgment_and_tasks() {
    let f = Fixture::new();
    let before = f.store.list_manager_preferences().unwrap();
    let plan = f.plan();
    assert_eq!(plan.fingerprint().len(), 64);
    assert_eq!(plan.executable_path(), f.binary.canonicalize().unwrap());
    assert_eq!(f.store.list_manager_preferences().unwrap(), before);
    assert!(f.store.first_run_repair_receipts().unwrap().is_empty());
    assert!(f.store.list_recent_tasks(10).unwrap().is_empty());
    assert!(
        !f.store
            .first_run_experience_state(FirstRunExperience::CURRENT)
            .unwrap()
            .acknowledged
    );
}

#[test]
fn incomplete_disabled_existing_script_and_ambiguous_candidates_have_no_plan() {
    for case in 0..8 {
        let mut f = Fixture::new();
        match case {
            0 => f.store.set_manager_enabled(ManagerId::Mise, false).unwrap(),
            1 => f.store.set_safe_mode(true).unwrap(),
            2 => {
                fs::create_dir_all(f.stale.parent().unwrap()).unwrap();
                write_binary(&f.stale);
            }
            3 => fs::write(&f.binary, b"#!/bin/sh\necho mise\n").unwrap(),
            4 => f.context.search_directories.push(PathBuf::from("relative")),
            5 => {
                let bin = f.root.path().join("other");
                fs::create_dir(&bin).unwrap();
                write_binary(&bin.join("mise"));
                f.context.search_directories.push(bin);
            }
            6 => fs::set_permissions(&f.binary, fs::Permissions::from_mode(0o644)).unwrap(),
            7 => f
                .store
                .set_manager_selected_executable_path(ManagerId::Mise, None)
                .unwrap(),
            _ => unreachable!(),
        }
        assert!(
            propose_first_run_repair(f.store.as_ref(), &f.context)
                .unwrap()
                .is_none(),
            "case {case}"
        );
    }
}

#[test]
fn aliases_of_the_same_native_binary_are_not_ambiguous() {
    let mut f = Fixture::new();
    let bin = f.root.path().join("alias");
    fs::create_dir(&bin).unwrap();
    symlink(&f.binary, bin.join("mise")).unwrap();
    f.context.search_directories.push(bin);
    assert_eq!(f.plan().executable_path(), f.binary.canonicalize().unwrap());
}

#[tokio::test]
async fn successful_apply_is_verified_and_durable_without_enabling_or_inventory_writes() {
    let f = Fixture::new();
    let executor = Executor::default();
    let before = f.store.list_manager_preferences().unwrap();
    let receipt =
        apply_approved_first_run_repair(f.store.as_ref(), &f.context, &f.plan(), &executor)
            .await
            .unwrap();
    assert_eq!(receipt.verification, RepairVerification::Verified);
    assert_eq!(receipt.observed_version.as_deref(), Some("2026.8.5"));
    let mut expected = before;
    expected[0].selected_executable_path = None;
    assert_eq!(f.store.list_manager_preferences().unwrap(), expected);
    assert!(f.store.list_detections().unwrap().is_empty());
    let reopened = SqliteStore::new(f.store.database_path());
    assert_eq!(reopened.first_run_repair_receipts().unwrap(), vec![receipt]);
    assert!(
        !reopened
            .first_run_experience_state(FirstRunExperience::CURRENT)
            .unwrap()
            .acknowledged
    );
    let calls = executor.calls.lock().unwrap();
    let request = &calls[0];
    assert_eq!(calls.len(), 1);
    assert_eq!(request.command.program, f.binary.canonicalize().unwrap());
    assert_eq!(request.command.args, ["--version"]);
    assert_eq!(request.command.working_dir, Some(PathBuf::from("/")));
    assert_eq!(request.command.env["MISE_OFFLINE"], "1");
    assert_eq!(request.command.env["MISE_AUTO_UPDATE"], "0");
    assert!(!request.requires_elevation);
    assert_eq!(request.private_output_limit, Some(4096));
    assert_eq!(request.timeout, Some(std::time::Duration::from_secs(10)));
}

#[tokio::test]
async fn changed_preference_file_scope_or_policy_rejects_before_mutation() {
    for case in 0..5 {
        let mut f = Fixture::new();
        let plan = f.plan();
        let executor = Executor::default();
        match case {
            0 => f
                .store
                .set_manager_timeout_hard_seconds(ManagerId::Mise, Some(91))
                .unwrap(),
            1 => fs::write(&f.binary, [0xcf, 0xfa, 0xed, 0xfe, 1]).unwrap(),
            2 => f.context.search_directories.clear(),
            3 => f.store.set_safe_mode(true).unwrap(),
            4 => {
                fs::create_dir_all(f.stale.parent().unwrap()).unwrap();
                write_binary(&f.stale);
            }
            _ => unreachable!(),
        }
        let result =
            apply_approved_first_run_repair(f.store.as_ref(), &f.context, &plan, &executor).await;
        assert!(result.is_err(), "case {case}");
        assert_eq!(f.selected().as_deref(), f.stale.to_str());
        assert!(executor.calls.lock().unwrap().is_empty());
        assert!(f.store.first_run_repair_receipts().unwrap().is_empty());
    }
}

#[tokio::test]
async fn failed_or_invalid_version_is_applied_but_not_verified() {
    for (stdout, exit) in [
        (b"2026.8.5".to_vec(), 1),
        (b"garbage".to_vec(), 0),
        (b"2026.8.5\nsecond line".to_vec(), 0),
        (vec![b'a'; 4097], 0),
    ] {
        let f = Fixture::new();
        let executor = Executor {
            stdout,
            exit,
            ..Executor::default()
        };
        let receipt =
            apply_approved_first_run_repair(f.store.as_ref(), &f.context, &f.plan(), &executor)
                .await
                .unwrap();
        assert!(receipt.applied);
        assert_eq!(receipt.verification, RepairVerification::Failed);
        assert!(receipt.observed_version.is_none());
        assert_eq!(f.selected(), None);
        assert_eq!(f.store.first_run_repair_receipts().unwrap(), vec![receipt]);
    }
}

#[tokio::test]
async fn concurrent_preference_change_is_not_overwritten_or_marked_verified() {
    let f = Fixture::new();
    let store = f.store.clone();
    let executor = Executor {
        hook: Some(Arc::new(move || {
            store
                .set_manager_selected_executable_path(ManagerId::Mise, Some("/new/user-choice"))
                .unwrap();
        })),
        ..Executor::default()
    };
    let receipt =
        apply_approved_first_run_repair(f.store.as_ref(), &f.context, &f.plan(), &executor)
            .await
            .unwrap();
    assert_eq!(receipt.verification, RepairVerification::Failed);
    assert_eq!(f.selected().as_deref(), Some("/new/user-choice"));
    assert_eq!(
        receipt.reason.as_deref(),
        Some("preference_or_policy_changed")
    );
}

#[tokio::test]
async fn binary_changed_during_verification_is_not_success() {
    let f = Fixture::new();
    let binary = f.binary.clone();
    let executor = Executor {
        hook: Some(Arc::new(move || {
            fs::write(&binary, b"changed").unwrap();
        })),
        ..Executor::default()
    };
    let receipt =
        apply_approved_first_run_repair(f.store.as_ref(), &f.context, &f.plan(), &executor)
            .await
            .unwrap();
    assert_eq!(receipt.verification, RepairVerification::Failed);
    assert_eq!(receipt.reason.as_deref(), Some("evidence_changed"));
}

#[test]
fn interruption_and_reopening_retains_unverified_receipt_without_replay() {
    let f = Fixture::new();
    let plan = f.plan();
    let receipt = f.store.apply_first_run_repair(&plan).unwrap().unwrap();
    let reopened = SqliteStore::new(f.store.database_path());
    reopened.migrate_to_latest().unwrap();
    assert_eq!(reopened.first_run_repair_receipts().unwrap(), vec![receipt]);
    assert_eq!(
        reopened.first_run_repair_receipts().unwrap()[0].verification,
        RepairVerification::Unverified
    );
    assert!(reopened.apply_first_run_repair(&plan).unwrap().is_none());
    assert!(
        propose_first_run_repair(&reopened, &f.context)
            .unwrap()
            .is_none()
    );
}

#[test]
fn simultaneous_store_connections_apply_at_most_once() {
    let f = Fixture::new();
    let plan = f.plan();
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let store = SqliteStore::new(f.store.database_path());
            let barrier = barrier.clone();
            let plan = plan.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.apply_first_run_repair(&plan).unwrap()
            })
        })
        .collect();
    barrier.wait();
    assert_eq!(
        threads
            .into_iter()
            .filter_map(|t| t.join().unwrap())
            .count(),
        1
    );
    assert_eq!(f.store.first_run_repair_receipts().unwrap().len(), 1);
}

#[test]
fn receipt_failure_rolls_back_preference_change() {
    let f = Fixture::new();
    let plan = f.plan();
    let conn = rusqlite::Connection::open(f.store.database_path()).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_receipt BEFORE INSERT ON first_run_repair_receipts
        BEGIN SELECT RAISE(ABORT, 'injected'); END;",
    )
    .unwrap();
    assert!(f.store.apply_first_run_repair(&plan).is_err());
    assert_eq!(f.selected().as_deref(), f.stale.to_str());
    assert!(f.store.first_run_repair_receipts().unwrap().is_empty());
}

#[tokio::test]
async fn failure_to_save_verification_retains_applied_unverified_truth() {
    let f = Fixture::new();
    let database = f.store.database_path().to_path_buf();
    let executor =
        Executor {
            hook: Some(Arc::new(move || {
                rusqlite::Connection::open(&database).unwrap().execute_batch(
            "CREATE TRIGGER fail_finalization BEFORE UPDATE ON first_run_repair_receipts
             BEGIN SELECT RAISE(ABORT, 'injected'); END;"
        ).unwrap();
            })),
            ..Executor::default()
        };
    assert!(
        apply_approved_first_run_repair(f.store.as_ref(), &f.context, &f.plan(), &executor)
            .await
            .is_err()
    );
    assert_eq!(f.selected(), None);
    let receipts = f.store.first_run_repair_receipts().unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].verification, RepairVerification::Unverified);
    assert!(receipts[0].applied);
}

#[tokio::test]
async fn finalized_receipt_is_immutable_and_cannot_be_reused_for_another_action() {
    let f = Fixture::new();
    let plan = f.plan();
    let verified =
        apply_approved_first_run_repair(f.store.as_ref(), &f.context, &plan, &Executor::default())
            .await
            .unwrap();
    let mut late = verified.clone();
    late.verification = RepairVerification::Failed;
    late.observed_version = None;
    late.reason = Some("late_callback".into());
    assert_eq!(
        f.store.finish_first_run_repair(&plan, &late).unwrap(),
        verified
    );
    late.applied = false;
    assert!(f.store.finish_first_run_repair(&plan, &late).is_err());
    assert_eq!(f.store.first_run_repair_receipts().unwrap(), vec![verified]);
}

#[tokio::test]
async fn abandoned_async_verification_never_replays_or_claims_success() {
    struct PendingExecutor;
    struct PendingProcess;
    impl ProcessExecutor for PendingExecutor {
        fn spawn(&self, _: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
            Ok(Box::new(PendingProcess))
        }
    }
    impl RunningProcess for PendingProcess {
        fn pid(&self) -> Option<u32> {
            None
        }
        fn terminate(&self, _: ProcessTerminationMode) -> ExecutionResult<()> {
            Ok(())
        }
        fn wait(self: Box<Self>) -> ProcessWaitFuture {
            Box::pin(std::future::pending())
        }
    }
    let f = Fixture::new();
    let plan = f.plan();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(30),
            apply_approved_first_run_repair(f.store.as_ref(), &f.context, &plan, &PendingExecutor)
        )
        .await
        .is_err()
    );
    let reopened = SqliteStore::new(f.store.database_path());
    assert_eq!(
        reopened.first_run_repair_receipts().unwrap()[0].verification,
        RepairVerification::Unverified
    );
    assert!(reopened.apply_first_run_repair(&plan).unwrap().is_none());
}

#[test]
fn migration_21_preserves_settings_and_repeated_startup_and_reset() {
    let root = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(root.path().join("old.db"));
    store.apply_migration(21).unwrap();
    store
        .set_manager_selected_executable_path(ManagerId::Mise, Some("/old/path"))
        .unwrap();
    store
        .acknowledge_first_run_experience(FirstRunExperience::CURRENT)
        .unwrap();
    let before = store.list_manager_preferences().unwrap();
    store.migrate_to_latest().unwrap();
    store.migrate_to_latest().unwrap();
    assert_eq!(store.list_manager_preferences().unwrap(), before);
    assert!(
        store
            .first_run_experience_state(FirstRunExperience::CURRENT)
            .unwrap()
            .acknowledged
    );
    assert!(store.first_run_repair_receipts().unwrap().is_empty());
    store.apply_migration(0).unwrap();
    store.migrate_to_latest().unwrap();
    assert!(store.first_run_repair_receipts().unwrap().is_empty());
}

#[test]
fn active_task_prevents_atomic_preference_change() {
    let f = Fixture::new();
    let plan = f.plan();
    f.store
        .create_task(&TaskRecord {
            id: TaskId(77),
            manager: ManagerId::Mise,
            task_type: TaskType::Refresh,
            status: TaskStatus::Running,
            created_at: SystemTime::now(),
        })
        .unwrap();
    assert!(f.store.apply_first_run_repair(&plan).unwrap().is_none());
    assert_eq!(f.selected().as_deref(), f.stale.to_str());
}
