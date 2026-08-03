use helm_core::doctor::{
    DoctorFinding, DoctorFindingSeverity, FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL,
    ISSUE_CODE_METADATA_ONLY_INSTALL, fingerprint_for_metadata_only_install,
};
use helm_core::models::ManagerId;
use helm_core::persistence::MigrationStore;
use helm_core::persistence::doctor_persistence::{
    DoctorScanScope, DoctorStore, KnowledgeTrustLevel, PersistedDoctorFinding,
};
use helm_core::persistence::repair_knowledge::{KnowledgeEnvelope, sha256_hex};
use helm_core::sqlite::{BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY, SqliteStore, current_schema_version};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-doctor-{test_name}-{nanos}.sqlite3"))
}

fn create_store(name: &str) -> SqliteStore {
    let store = SqliteStore::new(test_db_path(name));
    store.apply_migration(current_schema_version()).unwrap();
    store
}

fn scope() -> DoctorScanScope {
    DoctorScanScope {
        detector_id: "detector-a".to_string(),
        manager_id: ManagerId::Rustup,
    }
}

fn finding(generation: i64, observed_at: i64) -> PersistedDoctorFinding {
    PersistedDoctorFinding {
        fingerprint: fingerprint_for_metadata_only_install(
            ManagerId::Rustup,
            ManagerId::HomebrewFormula,
            "rustup",
        ),
        finding_code: FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL.to_string(),
        issue_code: ISSUE_CODE_METADATA_ONLY_INSTALL.to_string(),
        manager_id: ManagerId::Rustup,
        source_manager_id: Some(ManagerId::HomebrewFormula),
        subject_kind: "package".to_string(),
        subject_value: "rustup".to_string(),
        severity: "warning".to_string(),
        evidence_json: "{}".to_string(),
        detector_id: "detector-a".to_string(),
        detector_version: "1".to_string(),
        first_seen_unix: observed_at,
        last_seen_unix: observed_at,
        latest_observation_generation: generation,
        resolution_state: "active".to_string(),
    }
}

fn bundled_envelope() -> KnowledgeEnvelope {
    KnowledgeEnvelope::parse_and_validate(include_str!("../resources/bundled_knowledge.json"))
        .expect("bundled envelope should be valid")
}

fn resign(envelope: &mut KnowledgeEnvelope) {
    let canonical = envelope.to_canonical_jcs_without_integrity().unwrap();
    envelope.integrity.value = sha256_hex(canonical.as_bytes());
}

#[test]
fn scan_generations_are_allocated_and_late_completion_cannot_resolve_newer_findings() {
    let store = create_store("scan-generation");
    let scan1 = store.start_scan("scan-1", 1_000, &[scope()]).unwrap();
    let scan2 = store.start_scan("scan-2", 1_001, &[scope()]).unwrap();
    assert_eq!(scan2, scan1 + 1);

    store
        .complete_scan("scan-2", scan2, &[finding(scan2, 1_001)], &[scope()], 1_002)
        .unwrap();
    store
        .complete_scan("scan-1", scan1, &[], &[scope()], 1_003)
        .unwrap();
    assert_eq!(store.get_active_findings().unwrap().len(), 1);

    let scan3 = store.start_scan("scan-3", 1_004, &[scope()]).unwrap();
    store
        .complete_scan("scan-3", scan3, &[], &[scope()], 1_005)
        .unwrap();
    assert!(store.get_active_findings().unwrap().is_empty());
}

#[test]
fn failed_or_undeclared_scopes_do_not_resolve_findings() {
    let store = create_store("scope-resolution");
    let scan1 = store.start_scan("scan-1", 1_000, &[scope()]).unwrap();
    store
        .complete_scan("scan-1", scan1, &[finding(scan1, 1_000)], &[scope()], 1_001)
        .unwrap();

    let _scan2 = store.start_scan("scan-2", 1_002, &[scope()]).unwrap();
    store.fail_scan("scan-2", 1_003).unwrap();
    assert_eq!(store.get_active_findings().unwrap().len(), 1);

    let scan3 = store.start_scan("scan-3", 1_004, &[scope()]).unwrap();
    let undeclared = DoctorScanScope {
        detector_id: "detector-b".to_string(),
        manager_id: ManagerId::Rustup,
    };
    assert!(
        store
            .complete_scan("scan-3", scan3, &[], &[undeclared], 1_005)
            .is_err()
    );
    assert_eq!(store.get_active_findings().unwrap().len(), 1);

    let scan4 = store.start_scan("scan-4", 1_006, &[scope()]).unwrap();
    let mut finding_from_failed_scope = finding(scan4, 1_006);
    finding_from_failed_scope.detector_id = "detector-b".to_string();
    assert!(
        store
            .complete_scan(
                "scan-4",
                scan4,
                &[finding_from_failed_scope],
                &[scope()],
                1_007,
            )
            .is_err()
    );
    store.fail_scan("scan-4", 1_007).unwrap();
    assert_eq!(store.get_active_findings().unwrap().len(), 1);
}

#[test]
fn bundled_import_is_idempotent_and_export_is_deterministic() {
    let store = create_store("import-export");
    let envelope = bundled_envelope();
    store
        .import_knowledge(
            BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY,
            &envelope,
            KnowledgeTrustLevel::Bundled,
        )
        .unwrap();
    store
        .import_knowledge(
            BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY,
            &envelope,
            KnowledgeTrustLevel::Bundled,
        )
        .unwrap();

    let first = store
        .export_knowledge(BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY)
        .unwrap();
    let second = store
        .export_knowledge(BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY)
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.integrity.value, envelope.integrity.value);
}

#[test]
fn source_and_entry_equivocation_are_rejected_transactionally() {
    let store = create_store("equivocation");
    let envelope = bundled_envelope();
    store
        .import_knowledge(
            BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY,
            &envelope,
            KnowledgeTrustLevel::Bundled,
        )
        .unwrap();

    let mut same_source_revision = envelope.clone();
    same_source_revision.entries[0]
        .content_keys
        .as_mut()
        .unwrap()
        .title = "app.repair.changed.title".to_string();
    resign(&mut same_source_revision);
    let error = store
        .import_knowledge(
            BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY,
            &same_source_revision,
            KnowledgeTrustLevel::Bundled,
        )
        .unwrap_err();
    assert!(error.to_string().contains("source equivocation"));

    let mut higher_source_revision = same_source_revision;
    higher_source_revision.source_revision += 1;
    resign(&mut higher_source_revision);
    let error = store
        .import_knowledge(
            BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY,
            &higher_source_revision,
            KnowledgeTrustLevel::Bundled,
        )
        .unwrap_err();
    assert!(error.to_string().contains("entry equivocation"));
    assert_eq!(
        store
            .export_knowledge(BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY)
            .unwrap(),
        envelope
    );
}

#[test]
fn effective_knowledge_matches_normalized_finding_selectors() {
    let store = create_store("effective-knowledge");
    store
        .import_knowledge(
            BUNDLED_REPAIR_KNOWLEDGE_SOURCE_KEY,
            &bundled_envelope(),
            KnowledgeTrustLevel::Bundled,
        )
        .unwrap();
    let doctor_finding = DoctorFinding {
        finding_code: FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL.to_string(),
        issue_code: ISSUE_CODE_METADATA_ONLY_INSTALL.to_string(),
        fingerprint: fingerprint_for_metadata_only_install(
            ManagerId::Rustup,
            ManagerId::HomebrewFormula,
            "rustup",
        ),
        manager_id: ManagerId::Rustup.as_str().to_string(),
        source_manager_id: Some(ManagerId::HomebrewFormula.as_str().to_string()),
        package_name: Some("rustup".to_string()),
        severity: DoctorFindingSeverity::Warning,
        summary: "test".to_string(),
        evidence_primary: None,
        evidence_secondary: None,
    };
    let effective = store.get_effective_knowledge(&doctor_finding).unwrap();
    let options = effective
        .iter()
        .map(|entry| entry.option_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(effective.len(), 2);
    assert!(options.contains(&"reinstall_manager_via_homebrew"));
    assert!(options.contains(&"remove_stale_package_entry"));
}
