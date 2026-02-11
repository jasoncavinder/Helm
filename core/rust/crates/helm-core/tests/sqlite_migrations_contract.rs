use helm_core::sqlite::{current_schema_version, migration, migrations};

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
