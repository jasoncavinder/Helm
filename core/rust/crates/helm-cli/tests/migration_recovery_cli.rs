use helm_core::persistence::MigrationStore;
use helm_core::sqlite::{SqliteStore, current_schema_version, migration};
use rusqlite::Connection;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const AFFECTED_V0180_OVERLAY: &str =
    include_str!("../../helm-core/tests/fixtures/sqlite/v0.18.0-affected-overlay.sql");

#[test]
fn cli_shim_install_recovers_affected_v0180_database() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("helm-cli-migration-recovery-{nanos}.sqlite3"));
    let store = SqliteStore::new(path.clone());
    store.apply_migration(16).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch(AFFECTED_V0180_OVERLAY).unwrap();
    connection
        .execute(
            "INSERT INTO app_settings (key, value) VALUES ('onboarding_completed', 'true')",
            [],
        )
        .unwrap();
    drop(connection);

    let home = std::env::temp_dir().join(format!("helm-cli-recovery-home-{nanos}"));
    let app_bundle = home.join("Helm.app");
    let bundled_cli = app_bundle.join("Contents/Resources/helm-cli");
    fs::create_dir_all(bundled_cli.parent().unwrap()).unwrap();
    fs::write(&bundled_cli, b"fixture").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_helm"))
        .env("HELM_DB_PATH", &path)
        .env("HOME", &home)
        .args([
            "--json",
            "self",
            "install-shim",
            "--app-bundle-path",
            app_bundle.to_str().unwrap(),
            "--app-bundle-id",
            "app.jasoncavinder.Helm.test",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["schema"], "helm.cli.v1.self.install_shim");
    assert_eq!(payload["data"]["accepted"], true);
    assert_eq!(payload["data"]["installed"], true);
    assert!(home.join(".local/bin/helm").is_file());

    assert_eq!(store.current_version().unwrap(), current_schema_version());
    let connection = Connection::open(&path).unwrap();
    let migration_17_name: String = connection
        .query_row(
            "SELECT name FROM helm_schema_migrations WHERE version = 17",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(migration_17_name, migration(17).unwrap().name);
}
