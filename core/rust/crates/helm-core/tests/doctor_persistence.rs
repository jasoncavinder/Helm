use helm_core::models::ManagerId;
use helm_core::persistence::MigrationStore;
use helm_core::persistence::doctor_persistence::{DoctorStore, PersistedDoctorFinding};
use helm_core::persistence::repair_knowledge::KnowledgeEnvelope;
use helm_core::sqlite::{SqliteStore, current_schema_version};
use serde_json::json;
use sha2::Digest;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-doctor-{test_name}-{nanos}.sqlite3"))
}

#[test]
fn test_doctor_scan_lifecycle_and_finding_resolution() {
    let path = test_db_path("scan_lifecycle");
    let store = SqliteStore::new(&path);
    store.apply_migration(current_schema_version()).unwrap();

    // Start a scan
    store.start_scan("scan-1", 1, 1000).unwrap();

    let finding1 = PersistedDoctorFinding {
        fingerprint: "f1".to_string(),
        finding_code: "code1".to_string(),
        issue_code: "issue1".to_string(),
        manager_id: ManagerId::HomebrewFormula,
        source_manager_id: None,
        subject_kind: "package".to_string(),
        subject_value: "rustup".to_string(),
        severity: "warning".to_string(),
        evidence_json: "{}".to_string(),
        detector_id: "detector-a".to_string(),
        detector_version: "1.0".to_string(),
        first_seen_unix: 1000,
        last_seen_unix: 1000,
        latest_observation_generation: 1,
        resolution_state: "active".to_string(),
    };

    store.upsert_findings(1, &[finding1]).unwrap();

    let active = store.get_active_findings().unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].fingerprint, "f1");

    // Start scan 2, same detector but doesn't find f1
    store.start_scan("scan-2", 2, 2000).unwrap();
    store
        .mark_resolved_for_scopes(2, &[("detector-a".to_string(), ManagerId::HomebrewFormula)])
        .unwrap();

    let active = store.get_active_findings().unwrap();
    assert!(active.is_empty());
}

#[test]
fn test_knowledge_import_export_and_equivocation() {
    let path = test_db_path("import_export");
    let store = SqliteStore::new(&path);
    store.apply_migration(current_schema_version()).unwrap();

    let raw_json = include_str!("../resources/bundled_knowledge.json");
    let envelope = KnowledgeEnvelope::parse_and_validate(raw_json).expect("valid JSON envelope");

    // Import knowledge
    store
        .import_knowledge("bundled", &envelope, "bundled")
        .unwrap();

    // Export knowledge
    let exported = store.export_knowledge("bundled").unwrap();
    assert_eq!(exported.schema_version, 1);
    assert_eq!(exported.entries.len(), 4);

    // Import same -> idempotent
    assert!(
        store
            .import_knowledge("bundled", &envelope, "bundled")
            .is_ok()
    );

    // Import same revision, different checksum -> equivocation error
    let mut bad_envelope = envelope.clone();
    bad_envelope.entries[0].state = "tombstone".to_string(); // Alter content
    let canonical = bad_envelope.to_canonical_jcs_without_integrity().unwrap();
    let mut hasher = sha2::Sha256::new();
    sha2::Digest::update(&mut hasher, canonical.as_bytes());
    bad_envelope.integrity.value = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    let err = store
        .import_knowledge("bundled", &bad_envelope, "bundled")
        .unwrap_err();
    assert!(err.to_string().contains("Equivocation detected"));
}

#[test]
fn test_get_effective_knowledge() {
    let path = test_db_path("effective_knowledge");
    let store = SqliteStore::new(&path);
    store.apply_migration(current_schema_version()).unwrap();

    let raw_json = include_str!("../resources/bundled_knowledge.json");
    let envelope = KnowledgeEnvelope::parse_and_validate(raw_json).expect("valid JSON envelope");

    store
        .import_knowledge("bundled", &envelope, "bundled")
        .unwrap();

    let effective = store
        .get_effective_knowledge(r#"{"finding_code":"homebrew_metadata_only_install"}"#)
        .unwrap();
    assert_eq!(effective.len(), 2);

    let options: Vec<_> = effective.iter().map(|e| e.option_id.as_str()).collect();
    assert!(options.contains(&"reinstall_manager_via_homebrew"));
    assert!(options.contains(&"remove_stale_package_entry"));
}
