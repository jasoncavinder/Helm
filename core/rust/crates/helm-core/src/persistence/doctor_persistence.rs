use crate::models::{ManagerId, TaskId};
use crate::persistence::PersistenceResult;
use serde_json::Value;

pub struct PersistedDoctorFinding {
    pub fingerprint: String,
    pub finding_code: String,
    pub issue_code: String,
    pub manager_id: ManagerId,
    pub source_manager_id: Option<ManagerId>,
    pub subject_kind: String,
    pub subject_value: String,
    pub severity: String,
    pub evidence_json: String,
    pub detector_id: String,
    pub detector_version: String,
    pub first_seen_unix: i64,
    pub last_seen_unix: i64,
    pub latest_observation_generation: i64,
    pub resolution_state: String,
}

pub struct EffectiveKnowledge {
    pub knowledge_entry_id: String,
    pub option_id: String,
    pub action_id: String,
    pub policy_json: Option<String>,
    pub parameter_bindings_json: Option<String>,
    pub content_keys_json: Option<String>,
}

pub struct RepairHistoryRecord {
    pub fingerprint: String,
    pub option_id: String,
    pub action_id: String,
    pub task_id: Option<TaskId>,
    pub result: String,
    pub verified_outcome: Option<String>,
    pub executed_at_unix: i64,
}

pub trait DoctorStore: Send + Sync {
    fn start_scan(
        &self,
        scan_id: &str,
        generation: i64,
        started_at_unix: i64,
    ) -> PersistenceResult<()>;
    fn upsert_findings(
        &self,
        generation: i64,
        findings: &[PersistedDoctorFinding],
    ) -> PersistenceResult<()>;
    fn mark_resolved_for_scopes(
        &self,
        generation: i64,
        scopes: &[(String, ManagerId)],
    ) -> PersistenceResult<()>;
    fn get_active_findings(&self) -> PersistenceResult<Vec<PersistedDoctorFinding>>;
    fn import_knowledge(
        &self,
        source_key: &str,
        envelope: &crate::persistence::repair_knowledge::KnowledgeEnvelope,
        trust_level: &str,
    ) -> PersistenceResult<()>;
    fn export_knowledge(
        &self,
        source_key: &str,
    ) -> PersistenceResult<crate::persistence::repair_knowledge::KnowledgeEnvelope>;
    fn get_effective_knowledge(
        &self,
        fingerprint: &str,
    ) -> PersistenceResult<Vec<EffectiveKnowledge>>;
    fn record_repair_history(&self, record: &RepairHistoryRecord) -> PersistenceResult<()>;
}
