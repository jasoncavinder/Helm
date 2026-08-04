use helm_core::persistence::MigrationStore;
use helm_core::sqlite::SqliteStore;
use helm_core::sqlite::{current_schema_version, migration, migrations};
use std::time::{SystemTime, UNIX_EPOCH};

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
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("helm-migration-19-{nanos}.sqlite3"));
    let store = SqliteStore::new(path);

    store.apply_migration(18).unwrap();
    assert_eq!(store.current_version().unwrap(), 18);
    store.apply_migration(19).unwrap();
    assert_eq!(store.current_version().unwrap(), 19);

    let connection = rusqlite::Connection::open(store.database_path()).unwrap();
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
