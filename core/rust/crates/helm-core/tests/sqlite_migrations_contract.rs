use helm_core::persistence::MigrationStore;
use helm_core::sqlite::SqliteStore;
use helm_core::sqlite::{current_schema_version, migration, migrations};
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const AFFECTED_V0180_OVERLAY: &str = include_str!("fixtures/sqlite/v0.18.0-affected-overlay.sql");

fn temp_database(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("helm-{name}-{nanos}.sqlite3"))
}

fn seed_v01712_data(connection: &Connection) {
    connection
        .execute_batch(
            r#"
INSERT INTO app_settings (key, value) VALUES ('fixture_setting', 'preserved');
INSERT INTO manager_preferences (
    manager_id, enabled, selected_executable_path, selected_install_method,
    timeout_hard_seconds, timeout_idle_seconds
) VALUES ('rustup', 1, '/fixture/bin/rustup', 'rustupInit', 900, 120);
INSERT INTO installed_package_versions (
    manager_id, package_name, package_identifier, installed_version, pinned,
    is_active, is_default, has_override, updated_at_unix
) VALUES ('rustup', 'stable', 'stable', '1.80.0', 1, 1, 1, 0, 1785000000);
INSERT INTO outdated_packages (
    manager_id, package_name, package_identifier, installed_version,
    candidate_version, pinned, restart_required, is_active, is_default,
    has_override, updated_at_unix
) VALUES ('rustup', 'stable', 'stable', '1.80.0', '1.81.0', 1, 0, 1, 1, 0, 1785000000);
INSERT INTO task_records (
    task_id, manager_id, task_type, status, created_at_unix
) VALUES (4242, 'rustup', 'refresh', 'completed', 1785000000);
"#,
        )
        .unwrap();
}

fn assert_v01712_data_preserved(connection: &Connection) {
    let setting: String = connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'fixture_setting'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(setting, "preserved");

    let preference: (i64, String, String, i64, i64) = connection
        .query_row(
            "SELECT enabled, selected_executable_path, selected_install_method,
                    timeout_hard_seconds, timeout_idle_seconds
             FROM manager_preferences WHERE manager_id = 'rustup'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        preference,
        (
            1,
            "/fixture/bin/rustup".into(),
            "rustupInit".into(),
            900,
            120,
        )
    );

    let installed_version: String = connection
        .query_row(
            "SELECT installed_version FROM installed_package_versions
             WHERE manager_id = 'rustup' AND package_identifier = 'stable'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(installed_version, "1.80.0");

    let candidate_version: String = connection
        .query_row(
            "SELECT candidate_version FROM outdated_packages
             WHERE manager_id = 'rustup' AND package_identifier = 'stable'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(candidate_version, "1.81.0");

    let task_status: String = connection
        .query_row(
            "SELECT status FROM task_records WHERE task_id = 4242",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(task_status, "completed");
}

fn table_exists(connection: &Connection, table: &str) -> bool {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn migration_versions_are_strictly_increasing() {
    let entries = migrations();
    assert!(!entries.is_empty());

    let mut previous = 0;
    for entry in entries {
        assert!(entry.version > previous);
        previous = entry.version;
    }
}

#[test]
fn migration_lookup_and_schema_version_are_consistent() {
    let latest = current_schema_version();
    let latest_entry = migration(latest).expect("latest migration must exist");
    assert_eq!(latest_entry.version, latest);
}

#[test]
fn migration_sql_is_defined_for_up_and_down_paths() {
    for entry in migrations() {
        assert!(!entry.up_sql.trim().is_empty(), "up sql must not be empty");
        assert!(
            !entry.down_sql.trim().is_empty(),
            "down sql must not be empty"
        );
    }
}

#[test]
fn migration_19_upgrades_existing_repair_knowledge_schema() {
    let store = SqliteStore::new(temp_database("migration-19"));

    store.apply_migration(18).unwrap();
    assert_eq!(store.current_version().unwrap(), 18);
    store.apply_migration(19).unwrap();
    assert_eq!(store.current_version().unwrap(), 19);

    let connection = Connection::open(store.database_path()).unwrap();
    let mut statement = connection
        .prepare("PRAGMA table_info(repair_knowledge_entries)")
        .unwrap();
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert!(columns.iter().any(|column| column == "recommendation_rank"));
}

#[test]
fn fresh_schema_migration_is_idempotent() {
    let store = SqliteStore::new(temp_database("migration-fresh"));

    store.migrate_to_latest().unwrap();
    store.migrate_to_latest().unwrap();

    assert_eq!(store.current_version().unwrap(), current_schema_version());
    let connection = Connection::open(store.database_path()).unwrap();
    assert!(table_exists(&connection, "repair_knowledge_entries"));
    let bundled_entries: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM repair_knowledge_entries WHERE source_key = 'bundled:helm'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(bundled_entries > 0);
}

#[test]
fn v01712_schema_migrates_idempotently_without_data_loss() {
    let store = SqliteStore::new(temp_database("migration-v01712"));

    store.apply_migration(16).unwrap();
    let connection = Connection::open(store.database_path()).unwrap();
    seed_v01712_data(&connection);
    drop(connection);

    store.migrate_to_latest().unwrap();
    store.migrate_to_latest().unwrap();

    assert_eq!(store.current_version().unwrap(), current_schema_version());
    let connection = Connection::open(store.database_path()).unwrap();
    assert_v01712_data_preserved(&connection);
}

#[test]
fn affected_v0180_schema_recovers_idempotently_without_data_loss() {
    let store = SqliteStore::new(temp_database("migration-v0180-affected"));

    store.apply_migration(16).unwrap();
    let connection = Connection::open(store.database_path()).unwrap();
    seed_v01712_data(&connection);
    connection.execute_batch(AFFECTED_V0180_OVERLAY).unwrap();
    drop(connection);

    store.migrate_to_latest().unwrap();
    store.migrate_to_latest().unwrap();

    assert_eq!(store.current_version().unwrap(), current_schema_version());
    let connection = Connection::open(store.database_path()).unwrap();
    assert_v01712_data_preserved(&connection);
    let migration_17_name: String = connection
        .query_row(
            "SELECT name FROM helm_schema_migrations WHERE version = 17",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(migration_17_name, migration(17).unwrap().name);
    assert!(!table_exists(&connection, "advisory_cache"));
    let bundled_entries: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM repair_knowledge_entries WHERE source_key = 'bundled:helm'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(bundled_entries > 0);
}

#[test]
fn recovered_schema_rolls_back_cleanly_and_can_be_reinitialized() {
    let store = SqliteStore::new(temp_database("migration-v0180-reset"));

    store.apply_migration(16).unwrap();
    let connection = Connection::open(store.database_path()).unwrap();
    connection.execute_batch(AFFECTED_V0180_OVERLAY).unwrap();
    drop(connection);

    store.migrate_to_latest().unwrap();
    store.apply_migration(0).unwrap();

    assert_eq!(store.current_version().unwrap(), 0);
    let connection = Connection::open(store.database_path()).unwrap();
    assert!(!table_exists(&connection, "advisory_cache"));
    assert!(!table_exists(&connection, "app_settings"));
    assert!(!table_exists(&connection, "repair_knowledge_entries"));
    drop(connection);

    store.migrate_to_latest().unwrap();
    assert_eq!(store.current_version().unwrap(), current_schema_version());
}

#[test]
fn unknown_replaced_migration_identity_fails_closed() {
    let store = SqliteStore::new(temp_database("migration-unknown-identity"));

    store.apply_migration(16).unwrap();
    let connection = Connection::open(store.database_path()).unwrap();
    seed_v01712_data(&connection);
    connection
        .execute(
            "INSERT INTO helm_schema_migrations (version, name, applied_at_unix)
             VALUES (17, 'unknown_development_schema', 1785514652)",
            [],
        )
        .unwrap();
    drop(connection);

    let error = store.migrate_to_latest().unwrap_err();
    assert!(error.message.contains("unexpected migration identity"));

    let connection = Connection::open(store.database_path()).unwrap();
    assert_v01712_data_preserved(&connection);
    assert!(!table_exists(&connection, "repair_knowledge_entries"));
}

#[test]
fn replaced_migration_reconciliation_rolls_back_on_schema_failure() {
    let store = SqliteStore::new(temp_database("migration-v0180-rollback"));

    store.apply_migration(16).unwrap();
    let connection = Connection::open(store.database_path()).unwrap();
    seed_v01712_data(&connection);
    connection.execute_batch(AFFECTED_V0180_OVERLAY).unwrap();
    connection
        .execute_batch("CREATE TABLE doctor_findings (fingerprint TEXT PRIMARY KEY);")
        .unwrap();
    drop(connection);

    let error = store.migrate_to_latest().unwrap_err();
    assert!(error.message.contains("no such column: resolution_state"));

    let connection = Connection::open(store.database_path()).unwrap();
    assert_v01712_data_preserved(&connection);
    let migration_17_name: String = connection
        .query_row(
            "SELECT name FROM helm_schema_migrations WHERE version = 17",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(migration_17_name, "add_advisory_cache");
    assert!(!table_exists(&connection, "repair_knowledge_entries"));
    assert!(!table_exists(&connection, "doctor_scans"));
}

#[test]
fn replaced_migration_reconciliation_restores_legacy_table_after_late_failure() {
    let store = SqliteStore::new(temp_database("migration-v0180-late-rollback"));

    store.apply_migration(16).unwrap();
    let connection = Connection::open(store.database_path()).unwrap();
    connection.execute_batch(AFFECTED_V0180_OVERLAY).unwrap();
    connection
        .execute_batch(
            r#"
INSERT INTO advisory_cache (
    manager_id, package_name, affected_versions, severity, summary,
    fixed_version, source, fetched_at_epoch_ms, expires_at_epoch_ms
) VALUES (
    'homebrew', 'fixture', '*', 'high', 'preserve on rollback',
    NULL, 'fixture', 1785000000000, 1786000000000
);
CREATE TRIGGER reject_migration_17_identity_update
BEFORE UPDATE OF name ON helm_schema_migrations
WHEN OLD.version = 17
BEGIN
    SELECT RAISE(FAIL, 'forced migration identity update failure');
END;
"#,
        )
        .unwrap();
    drop(connection);

    let error = store.migrate_to_latest().unwrap_err();
    assert!(
        error
            .message
            .contains("forced migration identity update failure")
    );

    let connection = Connection::open(store.database_path()).unwrap();
    let migration_17_name: String = connection
        .query_row(
            "SELECT name FROM helm_schema_migrations WHERE version = 17",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(migration_17_name, "add_advisory_cache");
    let legacy_entries: i64 = connection
        .query_row("SELECT COUNT(*) FROM advisory_cache", [], |row| row.get(0))
        .unwrap();
    assert_eq!(legacy_entries, 1);
    assert!(!table_exists(&connection, "repair_knowledge_entries"));
    assert!(!table_exists(&connection, "doctor_scans"));
}
