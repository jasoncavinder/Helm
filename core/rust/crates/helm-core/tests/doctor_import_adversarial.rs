use helm_core::persistence::MigrationStore;
use helm_core::persistence::doctor_persistence::DoctorStore;
use helm_core::persistence::repair_knowledge::{KnowledgeEnvelope, KnowledgeEnvelopeError};
use helm_core::sqlite::{SqliteStore, current_schema_version};
use sha2::Digest;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-doctor-adv-{test_name}-{nanos}.sqlite3"))
}

fn create_store(name: &str) -> SqliteStore {
    let path = test_db_path(name);
    let store = SqliteStore::new(&path);
    store.apply_migration(current_schema_version()).unwrap();
    store
}

#[test]
fn test_strict_unknown_field_rejection() {
    let raw_json = r#"{
      "schema_version": 1,
      "declared_source_id": "test",
      "source_revision": 1,
      "generated_at_unix": 1720000000,
      "unknown_field": "bad",
      "entries": [],
      "integrity": {
        "algorithm": "sha256",
        "value": "fake"
      }
    }"#;
    let res = KnowledgeEnvelope::parse_and_validate(raw_json);
    assert!(
        matches!(res, Err(KnowledgeEnvelopeError::FormatError(msg)) if msg.contains("unknown field"))
    );
}

#[test]
fn test_malformed_tombstone_rejection() {
    let raw_json = r#"{
      "schema_version": 1,
      "declared_source_id": "test",
      "source_revision": 1,
      "generated_at_unix": 1720000000,
      "entries": [
        {
          "knowledge_entry_id": "tombstone.1",
          "revision": 2,
          "state": "tombstone",
          "selector": {},
          "action_id": "manager.apply_post_install_setup_defaults"
        }
      ],
      "integrity": {
        "algorithm": "sha256",
        "value": "fake"
      }
    }"#;
    let res = KnowledgeEnvelope::parse_and_validate(raw_json);
    assert!(
        matches!(res, Err(KnowledgeEnvelopeError::FormatError(msg)) if msg.contains("Tombstone cannot contain executable"))
    );
}

#[test]
fn test_duplicate_tuple_rejection() {
    let raw_json = r#"{
      "schema_version": 1,
      "declared_source_id": "test",
      "source_revision": 1,
      "generated_at_unix": 1720000000,
      "entries": [
        {
          "knowledge_entry_id": "dup.1",
          "revision": 1,
          "state": "active",
          "selector": {}
        },
        {
          "knowledge_entry_id": "dup.1",
          "revision": 1,
          "state": "tombstone",
          "selector": {}
        }
      ],
      "integrity": {
        "algorithm": "sha256",
        "value": "fake"
      }
    }"#;
    let res = KnowledgeEnvelope::parse_and_validate(raw_json);
    assert!(matches!(
        res,
        Err(KnowledgeEnvelopeError::DuplicateEntryRevision)
    ));
}

#[test]
fn test_unsupported_schema_and_algorithm() {
    let raw_json = r#"{
      "schema_version": 2,
      "declared_source_id": "test",
      "source_revision": 1,
      "generated_at_unix": 1720000000,
      "entries": [],
      "integrity": {
        "algorithm": "sha256",
        "value": "fake"
      }
    }"#;
    let res = KnowledgeEnvelope::parse_and_validate(raw_json);
    assert!(matches!(
        res,
        Err(KnowledgeEnvelopeError::UnsupportedSchemaVersion(2))
    ));

    let raw_json2 = r#"{
      "schema_version": 1,
      "declared_source_id": "test",
      "source_revision": 1,
      "generated_at_unix": 1720000000,
      "entries": [],
      "integrity": {
        "algorithm": "md5",
        "value": "fake"
      }
    }"#;
    let res2 = KnowledgeEnvelope::parse_and_validate(raw_json2);
    assert!(matches!(res2, Err(KnowledgeEnvelopeError::UnsupportedAlgorithm(alg)) if alg == "md5"));
}

#[test]
fn test_downgrade_rejection() {
    let store = create_store("downgrade_rejection");

    let mut envelope = KnowledgeEnvelope {
        schema_version: 1,
        declared_source_id: "test_source".to_string(),
        source_revision: 2,
        generated_at_unix: 0,
        entries: vec![],
        integrity: helm_core::persistence::repair_knowledge::KnowledgeIntegrity {
            algorithm: "sha256".to_string(),
            value: "".to_string(),
            signature: None,
        },
    };

    let canonical = envelope.to_canonical_jcs_without_integrity().unwrap();
    let mut hasher = sha2::Sha256::new();
    sha2::Digest::update(&mut hasher, canonical.as_bytes());
    envelope.integrity.value = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    store
        .import_knowledge("user_imported", &envelope, "user_imported")
        .unwrap();

    // Attempt downgrade
    let mut downgrade = envelope.clone();
    downgrade.source_revision = 1;
    let canonical = downgrade.to_canonical_jcs_without_integrity().unwrap();
    let mut hasher = sha2::Sha256::new();
    sha2::Digest::update(&mut hasher, canonical.as_bytes());
    downgrade.integrity.value = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    let err = store
        .import_knowledge("user_imported", &downgrade, "user_imported")
        .unwrap_err();
    assert!(err.to_string().contains("Downgrades are rejected"));
}

#[test]
fn test_protected_helm_namespace_rejection() {
    let store = create_store("namespace_rejection");

    let mut envelope = KnowledgeEnvelope {
        schema_version: 1,
        declared_source_id: "helm_evil".to_string(), // starts with helm_
        source_revision: 1,
        generated_at_unix: 0,
        entries: vec![],
        integrity: helm_core::persistence::repair_knowledge::KnowledgeIntegrity {
            algorithm: "sha256".to_string(),
            value: "".to_string(),
            signature: None,
        },
    };

    // Valid checksum
    let canonical = envelope.to_canonical_jcs_without_integrity().unwrap();
    let mut hasher = sha2::Sha256::new();
    sha2::Digest::update(&mut hasher, canonical.as_bytes());
    envelope.integrity.value = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    let err = store
        .import_knowledge("user_imported_123", &envelope, "user_imported")
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Unauthorized Helm namespace claim")
    );
}

#[test]
fn test_protected_option_rebinding_rejection() {
    let store = create_store("option_rebinding");

    let mut envelope = KnowledgeEnvelope {
        schema_version: 1,
        declared_source_id: "test".to_string(),
        source_revision: 1,
        generated_at_unix: 0,
        entries: vec![helm_core::persistence::repair_knowledge::KnowledgeEntry {
            knowledge_entry_id: "entry.1".to_string(),
            revision: 1,
            state: "active".to_string(),
            selector: helm_core::persistence::repair_knowledge::KnowledgeSelector {
                fingerprint: None,
                finding_code: None,
                issue_code: None,
                manager_id: None,
                source_manager_id: None,
                subject_kind: None,
                subject_value: None,
                package_name: None,
            },
            option_id: Some("reinstall_manager_via_homebrew".to_string()),
            action_id: Some("malicious.action".to_string()), // Rebinding to malicious action
            policy: None,
            parameter_bindings: None,
            content_keys: None,
        }],
        integrity: helm_core::persistence::repair_knowledge::KnowledgeIntegrity {
            algorithm: "sha256".to_string(),
            value: "".to_string(),
            signature: None,
        },
    };

    let canonical = envelope.to_canonical_jcs_without_integrity().unwrap();
    let mut hasher = sha2::Sha256::new();
    sha2::Digest::update(&mut hasher, canonical.as_bytes());
    envelope.integrity.value = hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();

    let err = store
        .import_knowledge("verified_signed", &envelope, "verified_signed")
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Attempt to rebind protected option ID")
    );
}
