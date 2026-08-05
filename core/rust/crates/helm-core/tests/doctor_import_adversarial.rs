use helm_core::persistence::MigrationStore;
use helm_core::persistence::doctor_persistence::{DoctorStore, KnowledgeTrustLevel};
use helm_core::persistence::repair_knowledge::{
    KnowledgeEnvelope, KnowledgeEnvelopeError, KnowledgeSignature, sha256_hex,
};
use helm_core::sqlite::{SqliteStore, current_schema_version};
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
    let store = SqliteStore::new(test_db_path(name));
    store.apply_migration(current_schema_version()).unwrap();
    store
}

fn user_envelope(revision: u64) -> KnowledgeEnvelope {
    let mut envelope =
        KnowledgeEnvelope::parse_and_validate(include_str!("../resources/bundled_knowledge.json"))
            .unwrap();
    envelope.declared_source_id = "community_test".to_string();
    envelope.source_revision = revision;
    resign(&mut envelope);
    envelope
}

fn resign(envelope: &mut KnowledgeEnvelope) {
    let canonical = envelope.to_canonical_jcs_without_integrity().unwrap();
    envelope.integrity.value = sha256_hex(canonical.as_bytes());
}

#[test]
fn unknown_fields_are_rejected_at_every_closed_schema_level() {
    let top_level = include_str!("../resources/bundled_knowledge.json").replacen(
        "\"entries\": [",
        "\"unexpected\": true, \"entries\": [",
        1,
    );
    assert!(matches!(
        KnowledgeEnvelope::parse_and_validate(&top_level),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("unknown field")
    ));

    let selector = include_str!("../resources/bundled_knowledge.json").replacen(
        "\"finding_code\": \"selected_executable_path_stale\"",
        "\"finding_code\": \"selected_executable_path_stale\", \"command\": \"rm\"",
        1,
    );
    assert!(matches!(
        KnowledgeEnvelope::parse_and_validate(&selector),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("unknown field")
    ));

    let mut signed = user_envelope(1);
    signed.integrity.signature = Some(KnowledgeSignature {
        algorithm: "ed25519".to_string(),
        key_id: "test".to_string(),
        value: "signature".to_string(),
    });
    resign(&mut signed);
    let mut signature = serde_json::to_value(signed).unwrap();
    signature["integrity"]["signature"]["command"] = serde_json::json!("rm");
    assert!(matches!(
        KnowledgeEnvelope::parse_and_validate(&serde_json::to_string(&signature).unwrap()),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("unknown field")
    ));
}

#[test]
fn malformed_tombstones_and_incomplete_active_entries_fail_closed() {
    let mut tombstone = user_envelope(1);
    tombstone.entries[0].state = "tombstone".to_string();
    resign(&mut tombstone);
    assert!(matches!(
        tombstone.validate(),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("tombstone")
    ));

    let mut active = user_envelope(1);
    active.entries[0].action_id = None;
    resign(&mut active);
    assert!(matches!(
        active.validate(),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("action_id")
    ));
}

#[test]
fn duplicate_entry_revisions_and_unsorted_entries_are_rejected() {
    let mut duplicate = user_envelope(1);
    duplicate.entries.insert(1, duplicate.entries[0].clone());
    resign(&mut duplicate);
    assert!(matches!(
        duplicate.validate(),
        Err(KnowledgeEnvelopeError::DuplicateEntryRevision)
    ));

    let mut unsorted = user_envelope(1);
    unsorted.entries.swap(0, 1);
    resign(&mut unsorted);
    assert!(matches!(
        unsorted.validate(),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("sorted")
    ));
}

#[test]
fn unknown_actions_rebinding_policy_weakening_and_literal_bindings_are_rejected() {
    let mut unknown = user_envelope(1);
    unknown.entries[0].action_id = Some("malicious.action".to_string());
    resign(&mut unknown);
    assert!(matches!(
        unknown.validate(),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("action binding")
    ));

    let mut rebound = user_envelope(1);
    rebound.entries[0].action_id = Some("homebrew.reinstall_formula".to_string());
    resign(&mut rebound);
    assert!(matches!(
        rebound.validate(),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("action binding")
    ));

    let mut weakened = user_envelope(1);
    let destructive = weakened
        .entries
        .iter_mut()
        .find(|entry| entry.option_id.as_deref() == Some("remove_stale_package_entry"))
        .unwrap();
    destructive.policy.as_mut().unwrap().requires_confirmation = false;
    destructive.policy.as_mut().unwrap().automation_level = "automatic".to_string();
    resign(&mut weakened);
    assert!(matches!(
        weakened.validate(),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("weakens")
    ));

    let mut literal_binding = user_envelope(1);
    literal_binding.entries[0]
        .parameter_bindings
        .get_or_insert_default()
        .insert("manager_id".to_string(), "/bin/sh".to_string());
    resign(&mut literal_binding);
    assert!(matches!(
        literal_binding.validate(),
        Err(KnowledgeEnvelopeError::FormatError(message)) if message.contains("parameter_bindings")
    ));
}

#[test]
fn unsupported_schema_algorithm_and_checksum_are_rejected() {
    let mut schema = user_envelope(1);
    schema.schema_version = 2;
    resign(&mut schema);
    assert!(matches!(
        schema.validate(),
        Err(KnowledgeEnvelopeError::UnsupportedSchemaVersion(2))
    ));

    let mut algorithm = user_envelope(1);
    algorithm.integrity.algorithm = "md5".to_string();
    assert!(matches!(
        algorithm.validate(),
        Err(KnowledgeEnvelopeError::UnsupportedAlgorithm(value)) if value == "md5"
    ));

    let mut checksum = user_envelope(1);
    checksum.entries[0].revision += 1;
    assert!(matches!(
        checksum.validate(),
        Err(KnowledgeEnvelopeError::IntegrityMismatch { .. })
    ));
}

#[test]
fn source_downgrade_protected_namespace_and_trust_escalation_are_rejected() {
    let store = create_store("source-policy");
    let revision_two = user_envelope(2);
    store
        .import_knowledge(
            "user:test",
            &revision_two,
            KnowledgeTrustLevel::UserImported,
        )
        .unwrap();

    let downgrade = user_envelope(1);
    let error = store
        .import_knowledge("user:test", &downgrade, KnowledgeTrustLevel::UserImported)
        .unwrap_err();
    assert!(error.to_string().contains("downgrade"));

    let mut protected = user_envelope(3);
    protected.declared_source_id = "helm_evil".to_string();
    resign(&mut protected);
    let error = store
        .import_knowledge("user:evil", &protected, KnowledgeTrustLevel::UserImported)
        .unwrap_err();
    assert!(error.to_string().contains("protected source namespace"));

    let error = store
        .import_knowledge(
            "verified:test",
            &user_envelope(1),
            KnowledgeTrustLevel::VerifiedSigned,
        )
        .unwrap_err();
    assert!(error.to_string().contains("configured local verifier"));
}
