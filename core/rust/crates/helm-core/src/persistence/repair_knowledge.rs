use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

use crate::models::ManagerId;
use crate::repair::{RepairAutomationLevel, repair_action_definition, validate_knowledge_binding};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeEnvelope {
    pub schema_version: u32,
    pub declared_source_id: String,
    pub source_revision: u64,
    pub generated_at_unix: u64,
    pub entries: Vec<KnowledgeEntry>,
    pub integrity: KnowledgeIntegrity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSelector {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finding_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manager_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_manager_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePolicy {
    pub requires_confirmation: bool,
    pub automation_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

impl KnowledgePolicy {
    pub fn automation_level(&self) -> Option<RepairAutomationLevel> {
        match self.automation_level.as_str() {
            "automatic" => Some(RepairAutomationLevel::Automatic),
            "needs_confirmation" => Some(RepairAutomationLevel::NeedsConfirmation),
            "read_only" => Some(RepairAutomationLevel::ReadOnly),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeContentKeys {
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeEntry {
    pub knowledge_entry_id: String,
    pub revision: u64,
    pub state: String,
    pub selector: KnowledgeSelector,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<KnowledgePolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_bindings: Option<BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_keys: Option<KnowledgeContentKeys>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeIntegrity {
    pub algorithm: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<KnowledgeSignature>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeSignature {
    pub algorithm: String,
    pub key_id: String,
    pub value: String,
}

#[derive(Debug, thiserror::Error)]
pub enum KnowledgeEnvelopeError {
    #[error("Unsupported schema version: {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("Invalid state: {0}")]
    InvalidState(String),
    #[error("Duplicate entry revision")]
    DuplicateEntryRevision,
    #[error("JSON canonicalization error: {0}")]
    CanonicalizationError(String),
    #[error("Integrity mismatch: expected {expected}, calculated {calculated}")]
    IntegrityMismatch {
        expected: String,
        calculated: String,
    },
    #[error("Unsupported algorithm: {0}")]
    UnsupportedAlgorithm(String),
    #[error("Invalid envelope format: {0}")]
    FormatError(String),
}

impl KnowledgeEnvelope {
    pub fn parse_and_validate(json: &str) -> Result<Self, KnowledgeEnvelopeError> {
        let envelope: KnowledgeEnvelope = serde_json::from_str(json)
            .map_err(|e| KnowledgeEnvelopeError::FormatError(e.to_string()))?;

        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> Result<(), KnowledgeEnvelopeError> {
        validate_envelope_structure(self)?;

        let canonical_str = self.to_canonical_jcs_without_integrity()?;
        let calculated = sha256_hex(canonical_str.as_bytes());

        if calculated != self.integrity.value {
            return Err(KnowledgeEnvelopeError::IntegrityMismatch {
                expected: self.integrity.value.clone(),
                calculated,
            });
        }

        Ok(())
    }

    pub fn to_canonical_jcs_without_integrity(&self) -> Result<String, KnowledgeEnvelopeError> {
        let mut val = serde_json::to_value(self)
            .map_err(|e| KnowledgeEnvelopeError::FormatError(e.to_string()))?;
        if let Some(obj) = val.as_object_mut() {
            obj.remove("integrity");
        }
        serde_jcs::to_string(&val)
            .map_err(|e| KnowledgeEnvelopeError::CanonicalizationError(e.to_string()))
    }
}

fn validate_envelope_structure(envelope: &KnowledgeEnvelope) -> Result<(), KnowledgeEnvelopeError> {
    if envelope.schema_version != 1 {
        return Err(KnowledgeEnvelopeError::UnsupportedSchemaVersion(
            envelope.schema_version,
        ));
    }

    if envelope.integrity.algorithm != "sha256" {
        return Err(KnowledgeEnvelopeError::UnsupportedAlgorithm(
            envelope.integrity.algorithm.clone(),
        ));
    }
    if !is_lower_hex_64(envelope.integrity.value.as_str()) {
        return Err(KnowledgeEnvelopeError::FormatError(
            "integrity.value must be 64 lowercase hexadecimal characters".to_string(),
        ));
    }
    if !is_identifier(envelope.declared_source_id.as_str()) {
        return Err(KnowledgeEnvelopeError::FormatError(
            "declared_source_id must be a lowercase ASCII identifier".to_string(),
        ));
    }
    if let Some(signature) = envelope.integrity.signature.as_ref()
        && (!is_identifier(signature.algorithm.as_str())
            || !is_identifier(signature.key_id.as_str())
            || signature.value.trim().is_empty())
    {
        return Err(KnowledgeEnvelopeError::FormatError(
            "signature metadata is malformed".to_string(),
        ));
    }

    let mut seen = HashSet::new();
    let mut last_id_rev: Option<(&str, u64)> = None;
    for entry in &envelope.entries {
        validate_entry(entry)?;
        if !seen.insert((entry.knowledge_entry_id.as_str(), entry.revision)) {
            return Err(KnowledgeEnvelopeError::DuplicateEntryRevision);
        }
        if let Some((last_id, last_revision)) = last_id_rev
            && (entry.knowledge_entry_id.as_str() < last_id
                || (entry.knowledge_entry_id == last_id && entry.revision <= last_revision))
        {
            return Err(KnowledgeEnvelopeError::FormatError(
                "entries must be sorted by knowledge_entry_id and revision".to_string(),
            ));
        }
        last_id_rev = Some((entry.knowledge_entry_id.as_str(), entry.revision));
    }
    Ok(())
}

fn validate_entry(entry: &KnowledgeEntry) -> Result<(), KnowledgeEnvelopeError> {
    if !is_identifier(entry.knowledge_entry_id.as_str()) {
        return Err(KnowledgeEnvelopeError::FormatError(
            "knowledge_entry_id must be a lowercase ASCII identifier".to_string(),
        ));
    }
    validate_selector(&entry.selector)?;

    match entry.state.as_str() {
        "tombstone" => {
            if entry.option_id.is_some()
                || entry.action_id.is_some()
                || entry.policy.is_some()
                || entry.parameter_bindings.is_some()
                || entry.content_keys.is_some()
            {
                return Err(KnowledgeEnvelopeError::FormatError(
                    "tombstone cannot contain option content".to_string(),
                ));
            }
        }
        "active" => validate_active_entry(entry)?,
        other => return Err(KnowledgeEnvelopeError::InvalidState(other.to_string())),
    }
    Ok(())
}

fn validate_active_entry(entry: &KnowledgeEntry) -> Result<(), KnowledgeEnvelopeError> {
    let option_id = required_identifier(entry.option_id.as_deref(), "option_id")?;
    let action_id = required_identifier(entry.action_id.as_deref(), "action_id")?;
    let policy = entry.policy.as_ref().ok_or_else(|| {
        KnowledgeEnvelopeError::FormatError("active entry requires policy".to_string())
    })?;
    let content = entry.content_keys.as_ref().ok_or_else(|| {
        KnowledgeEnvelopeError::FormatError("active entry requires content_keys".to_string())
    })?;

    let action = validate_knowledge_binding(option_id, action_id).map_err(|error| {
        KnowledgeEnvelopeError::FormatError(format!("invalid action binding: {error:?}"))
    })?;
    let definition = repair_action_definition(action);
    if entry
        .selector
        .finding_code
        .as_deref()
        .is_some_and(|value| value != definition.finding_code)
        || entry
            .selector
            .issue_code
            .as_deref()
            .is_some_and(|value| value != definition.issue_code)
    {
        return Err(KnowledgeEnvelopeError::FormatError(
            "selector is incompatible with the registered action".to_string(),
        ));
    }
    let automation = policy.automation_level().ok_or_else(|| {
        KnowledgeEnvelopeError::FormatError("unknown automation_level".to_string())
    })?;
    if (definition.requires_confirmation && !policy.requires_confirmation)
        || automation_rank(automation) < automation_rank(definition.minimum_automation_level)
    {
        return Err(KnowledgeEnvelopeError::FormatError(
            "knowledge policy weakens the action registry".to_string(),
        ));
    }
    for key in [
        content.title.as_str(),
        content.description.as_str(),
        content.impact.as_deref().unwrap_or("repair.none"),
        content.guidance.as_deref().unwrap_or("repair.none"),
    ] {
        if !is_localization_key(key) {
            return Err(KnowledgeEnvelopeError::FormatError(
                "content_keys must contain localization keys".to_string(),
            ));
        }
    }
    if let Some(bindings) = entry.parameter_bindings.as_ref() {
        for (name, reference) in bindings {
            let allowed = matches!(
                name.as_str(),
                "manager_id"
                    | "source_manager_id"
                    | "package_name"
                    | "subject_kind"
                    | "subject_value"
            ) && reference == format!("finding.{name}").as_str();
            if !allowed {
                return Err(KnowledgeEnvelopeError::FormatError(
                    "parameter_bindings contains a non-allowlisted reference".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_selector(selector: &KnowledgeSelector) -> Result<(), KnowledgeEnvelopeError> {
    let has_fingerprint = selector.fingerprint.is_some();
    let has_fields = selector.finding_code.is_some()
        || selector.issue_code.is_some()
        || selector.manager_id.is_some()
        || selector.source_manager_id.is_some()
        || selector.subject_kind.is_some()
        || selector.subject_value.is_some()
        || selector.package_name.is_some();
    if has_fingerprint == has_fields {
        return Err(KnowledgeEnvelopeError::FormatError(
            "selector must contain either one v2 fingerprint or normalized fields".to_string(),
        ));
    }
    if let Some(fingerprint) = selector.fingerprint.as_deref()
        && !fingerprint
            .strip_prefix("helm-doctor:v2:sha256:")
            .is_some_and(is_lower_hex_64)
    {
        return Err(KnowledgeEnvelopeError::FormatError(
            "selector fingerprint is malformed".to_string(),
        ));
    }
    if !has_fingerprint && selector.finding_code.is_none() {
        return Err(KnowledgeEnvelopeError::FormatError(
            "field selector requires finding_code".to_string(),
        ));
    }
    for value in [
        selector.finding_code.as_deref(),
        selector.issue_code.as_deref(),
        selector.subject_kind.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if !is_identifier(value) {
            return Err(KnowledgeEnvelopeError::FormatError(
                "selector contains an invalid identifier".to_string(),
            ));
        }
    }
    for manager in [
        selector.manager_id.as_deref(),
        selector.source_manager_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        manager.parse::<ManagerId>().map_err(|_| {
            KnowledgeEnvelopeError::FormatError("selector contains an unknown manager".to_string())
        })?;
    }
    for value in [
        selector.subject_value.as_deref(),
        selector.package_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(KnowledgeEnvelopeError::FormatError(
                "selector contains an invalid value".to_string(),
            ));
        }
    }
    Ok(())
}

fn required_identifier<'a>(
    value: Option<&'a str>,
    field: &str,
) -> Result<&'a str, KnowledgeEnvelopeError> {
    value.filter(|value| is_identifier(value)).ok_or_else(|| {
        KnowledgeEnvelopeError::FormatError(format!("active entry requires valid {field}"))
    })
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn is_localization_key(value: &str) -> bool {
    is_identifier(value) && value.contains('.')
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn automation_rank(level: RepairAutomationLevel) -> u8 {
    match level {
        RepairAutomationLevel::Automatic => 0,
        RepairAutomationLevel::NeedsConfirmation => 1,
        RepairAutomationLevel::ReadOnly => 2,
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::KnowledgeEnvelope;

    #[test]
    fn bundled_knowledge_is_strictly_valid() {
        KnowledgeEnvelope::parse_and_validate(include_str!(
            "../../resources/bundled_knowledge.json"
        ))
        .expect("bundled repair knowledge must pass strict envelope validation");
    }
}
