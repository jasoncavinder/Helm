#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use helm_core::models::{
    InstalledPackage, ManagerId, PackageRef, PinKind, PinRecord, TaskId, TaskRecord, TaskStatus,
    TaskType,
};
use helm_core::persistence::{PackageStore, PinStore, TaskStore};
use helm_core::sqlite::SqliteStore;
use rusqlite::Connection;

fn package(name: &str, version: &str) -> InstalledPackage {
    InstalledPackage {
        package: PackageRef {
            manager: ManagerId::Pnpm,
            name: name.into(),
        },
        package_identifier: None,
        installed_version: Some(version.into()),
        pinned: name == "held",
        runtime_state: Default::default(),
    }
}

fn pin() -> PinRecord {
    PinRecord {
        package: package("held", "1.0.0").package,
        kind: PinKind::Virtual,
        pinned_version: Some("1.0.0".into()),
        created_at: UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    }
}

fn task(id: u64, status: TaskStatus) -> TaskRecord {
    TaskRecord {
        id: TaskId(id),
        manager: ManagerId::Pnpm,
        task_type: TaskType::Refresh,
        status,
        created_at: UNIX_EPOCH + Duration::from_secs(1_700_000_000),
    }
}

struct Writer(Child);

impl Drop for Writer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

// The parent kills this process at a known boundary instead of racing a timer
// against a transaction. No adapter, package manager or user database is touched.
#[test]
fn interrupted_writer_child() {
    let Some(root) = std::env::var_os("HELM_SQLITE_RECOVERY_FIXTURE") else {
        return;
    };
    let root = Path::new(&root);
    let path = root.join("helm.db");
    let store = SqliteStore::new(&path);
    let reader = Connection::open(&path).unwrap();
    reader.execute_batch("BEGIN;").unwrap();
    let _: i64 = reader
        .query_row("SELECT COUNT(*) FROM task_records", [], |row| row.get(0))
        .unwrap();

    // Keep an older reader open so these production writes remain in the WAL.
    store
        .replace_installed_snapshot(
            ManagerId::Pnpm,
            &[package("fixture", "2.0.0"), package("held", "1.0.0")],
        )
        .unwrap();
    store.update_task(&task(0, TaskStatus::Completed)).unwrap();
    assert_eq!(
        store
            .reserve_task(&task(99, TaskStatus::Queued))
            .unwrap()
            .id,
        TaskId(1)
    );
    let committed_wal_size = fs::metadata(root.join("helm.db-wal")).unwrap().len();
    assert!(committed_wal_size > 32);

    let uncommitted = Connection::open(&path).unwrap();
    if std::env::var("HELM_SQLITE_RECOVERY_PHASE").unwrap() == "uncommitted" {
        uncommitted
            .execute_batch(
                "PRAGMA synchronous = NORMAL;
                 PRAGMA cache_size = 2;
                 PRAGMA cache_spill = ON;
                 BEGIN IMMEDIATE;
                 UPDATE installed_package_versions SET installed_version = 'uncommitted'
                     WHERE package_name = 'fixture';
                 UPDATE task_records SET status = 'failed' WHERE task_id = 0;
                 UPDATE task_id_sequence SET last_task_id = 999;",
            )
            .unwrap();
        uncommitted
            .execute(
                "INSERT INTO app_settings(key, value) VALUES ('uncommitted_probe', ?1)",
                ["x".repeat(256 * 1024)],
            )
            .unwrap();
        assert!(
            fs::metadata(root.join("helm.db-wal")).unwrap().len() > committed_wal_size,
            "exercise spilled uncommitted WAL frames, not only an in-memory transaction"
        );
    }
    fs::write(root.join("ready"), b"ready").unwrap();
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}

fn interrupt_and_reopen(phase: &str) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("helm.db");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();
    store
        .replace_installed_snapshot(
            ManagerId::Pnpm,
            &[package("fixture", "1.0.0"), package("held", "1.0.0")],
        )
        .unwrap();
    store.upsert_pin(&pin()).unwrap();
    assert_eq!(
        store
            .reserve_task(&task(99, TaskStatus::Queued))
            .unwrap()
            .id,
        TaskId(0)
    );

    let mut writer = Writer(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "interrupted_writer_child", "--nocapture"])
            .env("HELM_SQLITE_RECOVERY_FIXTURE", root.path())
            .env("HELM_SQLITE_RECOVERY_PHASE", phase)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !root.path().join("ready").is_file() {
        assert!(
            writer.0.try_wait().unwrap().is_none(),
            "writer exited early"
        );
        assert!(
            Instant::now() < deadline,
            "writer did not reach {phase} boundary"
        );
        thread::sleep(Duration::from_millis(10));
    }
    writer.0.kill().unwrap();
    assert!(!writer.0.wait().unwrap().success());
    assert!(fs::metadata(root.path().join("helm.db-wal")).unwrap().len() > 32);

    // Reopen through the shipping store without removing or recreating sidecars.
    let reopened = SqliteStore::new(&path);
    reopened.migrate_to_latest().unwrap();
    let mut installed = reopened.list_installed().unwrap();
    installed.sort_by(|left, right| left.package.name.cmp(&right.package.name));
    assert_eq!(
        installed,
        vec![package("fixture", "2.0.0"), package("held", "1.0.0")]
    );
    assert_eq!(reopened.list_pins().unwrap(), vec![pin()]);
    let tasks = reopened.list_recent_tasks(10).unwrap();
    assert_eq!(tasks.len(), 2);
    assert!(tasks.contains(&task(0, TaskStatus::Completed)));
    assert!(tasks.contains(&task(1, TaskStatus::Queued)));
    assert_eq!(
        reopened
            .reserve_task(&task(99, TaskStatus::Queued))
            .unwrap()
            .id,
        TaskId(2)
    );
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM app_settings WHERE key = 'uncommitted_probe'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn committed_wal_survives_process_kill_with_packages_pins_and_task_ids() {
    interrupt_and_reopen("committed");
}

#[test]
fn spilled_uncommitted_wal_is_rolled_back_after_process_kill() {
    interrupt_and_reopen("uncommitted");
}
