use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::time::{SystemTime, UNIX_EPOCH};

use helm_core::models::CoreErrorKind;
use helm_core::persistence::{DetectionStore, FirstRunExperience, FirstRunStore, MigrationStore};
use helm_core::sqlite::SqliteStore;
use rusqlite::{Connection, types::Value};

const EXPERIENCE: FirstRunExperience = FirstRunExperience::WayfinderV020;
const KEY: &str = "first_run.experience.wayfinder-v0.20.acknowledged";

fn database(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "helm-first-run-{name}-{}-{nanos}.db",
        std::process::id()
    ))
}

fn store(name: &str) -> SqliteStore {
    let store = SqliteStore::new(database(name));
    store.migrate_to_latest().unwrap();
    store
}

fn settings(connection: &Connection) -> Vec<(String, String)> {
    connection
        .prepare("SELECT key, value FROM app_settings ORDER BY key")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}

fn other_data(connection: &Connection) -> BTreeMap<String, Vec<Vec<Value>>> {
    let tables: Vec<String> = connection.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name != 'app_settings' ORDER BY name"
    ).unwrap().query_map([], |row| row.get(0)).unwrap()
        .collect::<rusqlite::Result<_>>().unwrap();
    tables
        .into_iter()
        .map(|table| {
            let mut query = connection
                .prepare(&format!(
                    "SELECT * FROM \"{}\" ORDER BY rowid",
                    table.replace('"', "\"\"")
                ))
                .unwrap();
            let columns = query.column_count();
            let rows = query
                .query_map([], |row| {
                    (0..columns)
                        .map(|index| row.get::<_, Value>(index))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            (table, rows)
        })
        .collect()
}

#[test]
fn fresh_read_does_not_acknowledge_or_complete_legacy_onboarding() {
    let store = store("fresh");
    let connection = Connection::open(store.database_path()).unwrap();
    let before = settings(&connection);
    for _ in 0..3 {
        let state = store.first_run_experience_state(EXPERIENCE).unwrap();
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::json!({
                "schema_version": 1, "experience_id": "wayfinder-v0.20", "acknowledged": false
            })
        );
    }
    assert_eq!(settings(&connection), before);
    store.acknowledge_first_run_experience(EXPERIENCE).unwrap();
    assert!(!store.cli_onboarding_completed().unwrap());
    assert_eq!(store.cli_accepted_license_terms_version().unwrap(), None);
}

#[test]
fn upgrade_acknowledgment_preserves_all_other_persisted_data() {
    let store = store("upgrade");
    store.set_cli_onboarding_completed(true).unwrap();
    store
        .set_cli_accepted_license_terms_version(Some("prior-accepted-terms"))
        .unwrap();
    store.set_safe_mode(true).unwrap();
    store.set_auto_check_for_updates(false).unwrap();
    store.set_auto_check_frequency_minutes(120).unwrap();
    let connection = Connection::open(store.database_path()).unwrap();
    connection.execute_batch("\
        INSERT INTO app_settings (key, value)
            VALUES ('first_run.experience.future.acknowledged', '1');
        INSERT INTO manager_preferences (manager_id, enabled, selected_executable_path)
            VALUES ('cargo', 0, '/fixture/cargo');
        INSERT INTO pin_records (manager_id, package_name, pin_kind, pinned_version, created_at_unix)
            VALUES ('cargo', 'example', 'virtual', '1.0.0', 1);
        INSERT INTO task_records (task_id, manager_id, task_type, status, created_at_unix)
            VALUES (84, 'cargo', 'refresh', 'failed', 1);
        INSERT INTO installed_package_versions
            (manager_id, package_name, package_identifier, installed_version, pinned, updated_at_unix)
            VALUES ('cargo', 'example', 'example', '1.0.0', 1, 1);
    ").unwrap();
    // No previous application-version metadata is required for an old installation.
    let before_settings = settings(&connection);
    let before_other_data = other_data(&connection);
    assert!(
        !store
            .first_run_experience_state(EXPERIENCE)
            .unwrap()
            .acknowledged
    );
    assert_eq!(settings(&connection), before_settings);
    store.acknowledge_first_run_experience(EXPERIENCE).unwrap();
    assert_eq!(other_data(&connection), before_other_data);
    let mut expected_settings = before_settings;
    expected_settings.push((KEY.into(), "1".into()));
    expected_settings.sort();
    assert_eq!(settings(&connection), expected_settings);
}

#[test]
fn acknowledgment_survives_reopen_and_is_idempotent_for_rc_stable_and_patch_builds() {
    let store = store("reopen");
    store.acknowledge_first_run_experience(EXPERIENCE).unwrap();
    let path = store.database_path().to_owned();
    let before = settings(&Connection::open(&path).unwrap());
    drop(store);
    for _ in 0..3 {
        let reopened = SqliteStore::new(&path);
        reopened.migrate_to_latest().unwrap();
        assert!(
            reopened
                .first_run_experience_state(EXPERIENCE)
                .unwrap()
                .acknowledged
        );
        reopened
            .acknowledge_first_run_experience(EXPERIENCE)
            .unwrap();
        assert_eq!(settings(&Connection::open(&path).unwrap()), before);
    }
}

#[test]
fn unacknowledged_session_remains_pending_after_reopen_and_isolated_profiles_do_not_leak() {
    let first = store("first-profile");
    let second = store("second-profile");
    let path = first.database_path().to_owned();
    assert!(
        !first
            .first_run_experience_state(EXPERIENCE)
            .unwrap()
            .acknowledged
    );
    drop(first);
    let reopened = SqliteStore::new(path);
    assert!(
        !reopened
            .first_run_experience_state(EXPERIENCE)
            .unwrap()
            .acknowledged
    );
    reopened
        .acknowledge_first_run_experience(EXPERIENCE)
        .unwrap();
    assert!(
        !second
            .first_run_experience_state(EXPERIENCE)
            .unwrap()
            .acknowledged
    );
}

#[test]
fn malformed_state_is_not_completion_and_is_not_silently_overwritten() {
    let store = store("malformed");
    let connection = Connection::open(store.database_path()).unwrap();
    for value in ["", "0", "true", " 1", "future-format"] {
        connection
            .execute(
                "INSERT OR REPLACE INTO app_settings (key, value) VALUES (?1, ?2)",
                [KEY, value],
            )
            .unwrap();
        let before = settings(&connection);
        assert_eq!(
            store
                .first_run_experience_state(EXPERIENCE)
                .unwrap_err()
                .kind,
            CoreErrorKind::StorageFailure
        );
        assert_eq!(
            store
                .acknowledge_first_run_experience(EXPERIENCE)
                .unwrap_err()
                .kind,
            CoreErrorKind::StorageFailure
        );
        assert_eq!(settings(&connection), before);
    }
}

#[test]
fn failed_commit_does_not_acknowledge_and_retry_can_succeed() {
    let store = store("failed-write");
    let connection = Connection::open(store.database_path()).unwrap();
    connection
        .execute_batch(
            "\
        CREATE TRIGGER reject_ack BEFORE INSERT ON app_settings
        WHEN NEW.key = 'first_run.experience.wayfinder-v0.20.acknowledged'
        BEGIN SELECT RAISE(ABORT, 'injected write failure'); END;
    ",
        )
        .unwrap();
    assert_eq!(
        store
            .acknowledge_first_run_experience(EXPERIENCE)
            .unwrap_err()
            .kind,
        CoreErrorKind::StorageFailure
    );
    assert!(
        !store
            .first_run_experience_state(EXPERIENCE)
            .unwrap()
            .acknowledged
    );
    connection.execute_batch("DROP TRIGGER reject_ack").unwrap();
    store.acknowledge_first_run_experience(EXPERIENCE).unwrap();
    assert!(
        store
            .first_run_experience_state(EXPERIENCE)
            .unwrap()
            .acknowledged
    );
}

#[test]
fn concurrent_acknowledgments_are_idempotent() {
    let store = store("concurrent");
    let barrier = Arc::new(Barrier::new(4));
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let path = store.database_path().to_owned();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = SqliteStore::new(path);
                barrier.wait();
                store.acknowledge_first_run_experience(EXPERIENCE).unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    assert!(
        store
            .first_run_experience_state(EXPERIENCE)
            .unwrap()
            .acknowledged
    );
}

#[test]
fn unavailable_database_is_not_unacknowledged_state() {
    let store = SqliteStore::new(database("nonexistent-parent").join("helm.db"));
    assert_eq!(
        store
            .first_run_experience_state(EXPERIENCE)
            .unwrap_err()
            .kind,
        CoreErrorKind::StorageFailure
    );
    assert_eq!(
        store
            .acknowledge_first_run_experience(EXPERIENCE)
            .unwrap_err()
            .kind,
        CoreErrorKind::StorageFailure
    );
}

#[test]
fn explicit_database_reset_clears_acknowledgment() {
    let store = store("reset");
    store.acknowledge_first_run_experience(EXPERIENCE).unwrap();
    store.apply_migration(0).unwrap();
    store.migrate_to_latest().unwrap();
    assert!(
        !store
            .first_run_experience_state(EXPERIENCE)
            .unwrap()
            .acknowledged
    );
}

#[test]
fn experience_ids_are_exact_and_not_application_versions() {
    assert_eq!(FirstRunExperience::CURRENT, EXPERIENCE);
    assert_eq!(
        FirstRunExperience::from_id("wayfinder-v0.20"),
        Some(EXPERIENCE)
    );
    for id in [
        "",
        "v0.20.0",
        "v0.20.0-rc.1",
        "v0.20.1",
        "wayfinder-v0.21",
        "wayfinder-v0.20 ",
    ] {
        assert_eq!(FirstRunExperience::from_id(id), None);
    }
}
