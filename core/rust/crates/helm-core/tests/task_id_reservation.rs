use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use helm_core::models::{ManagerId, TaskId, TaskRecord, TaskStatus, TaskType};
use helm_core::persistence::{MigrationStore, TaskStore};
use helm_core::sqlite::SqliteStore;

fn path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "helm-reserve-{label}-{}-{}.db",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn template() -> TaskRecord {
    TaskRecord {
        id: TaskId(999),
        manager: ManagerId::Npm,
        task_type: TaskType::Refresh,
        status: TaskStatus::Queued,
        created_at: UNIX_EPOCH
            + std::time::Duration::from_secs(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
            ),
    }
}

#[test]
fn reserve_task_process_child() {
    let Some(path) = std::env::var_os("HELM_RESERVATION_CHILD_DB") else {
        return;
    };
    let store = SqliteStore::new(PathBuf::from(path));
    for _ in 0..32 {
        let record = store.reserve_task(&template()).unwrap();
        assert_eq!(record.status, TaskStatus::Queued);
    }
}

#[test]
fn independent_processes_allocate_without_collisions_or_reuse_after_delete() {
    let path = path("processes");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();
    let spawn = || {
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "reserve_task_process_child"])
            .env("HELM_RESERVATION_CHILD_DB", &path)
            .spawn()
            .unwrap()
    };
    let mut first = spawn();
    let mut second = spawn();
    assert!(first.wait().unwrap().success());
    assert!(second.wait().unwrap().success());
    let tasks = store.list_recent_tasks(100).unwrap();
    assert_eq!(tasks.len(), 64);
    assert_eq!(store.next_task_id().unwrap(), 64);
    store.delete_all_tasks().unwrap();
    assert_eq!(store.reserve_task(&template()).unwrap().id, TaskId(64));
    let mut explicit = template();
    explicit.id = TaskId(9000);
    store.create_task(&explicit).unwrap();
    store.delete_task(explicit.id).unwrap();
    assert_eq!(store.reserve_task(&template()).unwrap().id, TaskId(9001));
}

#[test]
fn migration_seeds_existing_ids_preserves_records_and_rolls_back() {
    let path = path("migration");
    let store = SqliteStore::new(&path);
    store.apply_migration(20).unwrap();
    let mut original = template();
    original.id = TaskId(41);
    store.create_task(&original).unwrap();
    store.migrate_to_latest().unwrap();
    store.migrate_to_latest().unwrap();
    assert_eq!(store.list_recent_tasks(10).unwrap(), vec![original.clone()]);
    assert_eq!(store.reserve_task(&template()).unwrap().id, TaskId(42));
    store.apply_migration(20).unwrap();
    assert_eq!(store.list_recent_tasks(10).unwrap().len(), 2);
    store.migrate_to_latest().unwrap();
    assert_eq!(store.reserve_task(&template()).unwrap().id, TaskId(43));
}

#[test]
fn failed_insert_rolls_back_sequence_and_exhaustion_fails_closed() {
    let path = path("rollback");
    let store = SqliteStore::new(&path);
    store.migrate_to_latest().unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_task BEFORE INSERT ON task_records BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert!(store.reserve_task(&template()).is_err());
    assert_eq!(store.next_task_id().unwrap(), 0);
    db.execute_batch("DROP TRIGGER reject_task;").unwrap();
    assert_eq!(store.reserve_task(&template()).unwrap().id, TaskId(0));
    db.execute("UPDATE task_id_sequence SET last_task_id = ?1", [i64::MAX])
        .unwrap();
    assert!(store.reserve_task(&template()).is_err());
    assert!(store.next_task_id().is_err());
    assert_eq!(store.list_recent_tasks(10).unwrap().len(), 1);
}
