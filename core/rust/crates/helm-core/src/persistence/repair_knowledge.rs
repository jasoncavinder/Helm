use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

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
pub struct KnowledgeEntry {
    pub knowledge_entry_id: String,
    pub revision: u64,
    pub state: String,
    pub selector: Value,
    pub option_id: Option<String>,
    pub action_id: Option<String>,
    pub policy: Option<Value>,
    pub parameter_bindings: Option<Value>,
    pub content_keys: Option<Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeIntegrity {
    pub algorithm: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<Value>,
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

        let mut seen = std::collections::HashSet::new();
        for entry in &envelope.entries {
            if entry.state != "active" && entry.state != "tombstone" {
                return Err(KnowledgeEnvelopeError::InvalidState(entry.state.clone()));
            }
            if !seen.insert((entry.knowledge_entry_id.clone(), entry.revision)) {
                return Err(KnowledgeEnvelopeError::DuplicateEntryRevision);
            }
        }

        let mut json_val: Value = serde_json::from_str(json)
            .map_err(|e| KnowledgeEnvelopeError::FormatError(e.to_string()))?;
        if let Some(obj) = json_val.as_object_mut() {
            obj.remove("integrity");
        } else {
            return Err(KnowledgeEnvelopeError::FormatError(
                "Expected object".into(),
            ));
        }

        let canonical_str = serde_jcs::to_string(&json_val)
            .map_err(|e| KnowledgeEnvelopeError::CanonicalizationError(e.to_string()))?;

        let mut hasher = Sha256::new();
        hasher.update(canonical_str.as_bytes());
        let calculated = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();

        if calculated != envelope.integrity.value {
            return Err(KnowledgeEnvelopeError::IntegrityMismatch {
                expected: envelope.integrity.value.clone(),
                calculated,
            });
        }

        Ok(envelope)
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
