use crate::doctor::{DoctorFinding, DoctorReport};
use crate::models::{CoreError, CoreErrorKind, ManagerId, TaskId};
use crate::persistence::PersistenceResult;
use crate::persistence::repair_knowledge::{KnowledgeContentKeys, KnowledgePolicy};
use std::sync::atomic::{AtomicU64, Ordering};

pub const LOCAL_DOCTOR_DETECTOR_ID: &str = "helm.local_package_state";
pub const LOCAL_DOCTOR_DETECTOR_VERSION: &str = "1";
static NEXT_SCAN_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorScanScope {
    pub detector_id: String,
    pub manager_id: ManagerId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DoctorScanHandle {
    pub scan_id: String,
    pub generation: i64,
    pub scopes: Vec<DoctorScanScope>,
    pub started_at_unix: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnowledgeTrustLevel {
    UserImported,
    Bundled,
    VerifiedSigned,
}

impl KnowledgeTrustLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserImported => "user_imported",
            Self::Bundled => "bundled",
            Self::VerifiedSigned => "verified_signed",
        }
    }

    pub fn authority_rank(self) -> u8 {
        match self {
            Self::UserImported => 0,
            Self::Bundled => 1,
            Self::VerifiedSigned => 2,
        }
    }
}

impl std::str::FromStr for KnowledgeTrustLevel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "user_imported" => Ok(Self::UserImported),
            "bundled" => Ok(Self::Bundled),
            "verified_signed" => Ok(Self::VerifiedSigned),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveKnowledge {
    pub source_key: String,
    pub trust_level: KnowledgeTrustLevel,
    pub knowledge_entry_id: String,
    pub revision: u64,
    pub option_id: String,
    pub action_id: String,
    pub recommendation_rank: Option<u32>,
    pub policy: KnowledgePolicy,
    pub content_keys: KnowledgeContentKeys,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
        started_at_unix: i64,
        scopes: &[DoctorScanScope],
    ) -> PersistenceResult<i64>;
    fn complete_scan(
        &self,
        scan_id: &str,
        generation: i64,
        findings: &[PersistedDoctorFinding],
        successful_scopes: &[DoctorScanScope],
        completed_at_unix: i64,
    ) -> PersistenceResult<()>;
    fn fail_scan(&self, scan_id: &str, completed_at_unix: i64) -> PersistenceResult<()>;
    fn cancel_scan(&self, scan_id: &str, completed_at_unix: i64) -> PersistenceResult<()>;
    fn get_active_findings(&self) -> PersistenceResult<Vec<PersistedDoctorFinding>>;
    fn import_knowledge(
        &self,
        source_key: &str,
        envelope: &crate::persistence::repair_knowledge::KnowledgeEnvelope,
        trust_level: KnowledgeTrustLevel,
    ) -> PersistenceResult<()>;
    fn export_knowledge(
        &self,
        source_key: &str,
    ) -> PersistenceResult<crate::persistence::repair_knowledge::KnowledgeEnvelope>;
    fn get_effective_knowledge(
        &self,
        finding: &DoctorFinding,
    ) -> PersistenceResult<Vec<EffectiveKnowledge>>;
    fn record_repair_history(&self, record: &RepairHistoryRecord) -> PersistenceResult<()>;
}

pub fn begin_local_doctor_scan(
    store: &dyn DoctorStore,
    started_at_unix: i64,
) -> PersistenceResult<DoctorScanHandle> {
    let sequence = NEXT_SCAN_ID.fetch_add(1, Ordering::Relaxed);
    let scan_id = format!("local-{}-{started_at_unix}-{sequence}", std::process::id());
    let scopes = ManagerId::ALL
        .into_iter()
        .map(|manager_id| DoctorScanScope {
            detector_id: LOCAL_DOCTOR_DETECTOR_ID.to_string(),
            manager_id,
        })
        .collect::<Vec<_>>();
    let generation = store.start_scan(scan_id.as_str(), started_at_unix, scopes.as_slice())?;
    Ok(DoctorScanHandle {
        scan_id,
        generation,
        scopes,
        started_at_unix,
    })
}

pub fn complete_local_doctor_scan(
    store: &dyn DoctorStore,
    handle: &DoctorScanHandle,
    report: &DoctorReport,
    completed_at_unix: i64,
) -> PersistenceResult<()> {
    let findings = report
        .findings
        .iter()
        .map(|finding| persisted_finding(finding, handle.generation, completed_at_unix))
        .collect::<PersistenceResult<Vec<_>>>()?;
    store.complete_scan(
        handle.scan_id.as_str(),
        handle.generation,
        findings.as_slice(),
        handle.scopes.as_slice(),
        completed_at_unix,
    )
}

fn persisted_finding(
    finding: &DoctorFinding,
    generation: i64,
    observed_at_unix: i64,
) -> PersistenceResult<PersistedDoctorFinding> {
    let manager_id = finding
        .manager_id
        .parse::<ManagerId>()
        .map_err(|_| persistence_contract_error("doctor finding has an unknown manager"))?;
    let source_manager_id = finding
        .source_manager_id
        .as_deref()
        .map(str::parse::<ManagerId>)
        .transpose()
        .map_err(|_| persistence_contract_error("doctor finding has an unknown source manager"))?;
    let (subject_kind, subject_value) = finding
        .package_name
        .as_ref()
        .map(|package| ("package", package.as_str()))
        .unwrap_or(("manager", finding.manager_id.as_str()));
    let evidence_json = serde_json::to_string(&serde_json::json!({
        "summary": finding.summary,
        "primary": finding.evidence_primary,
        "secondary": finding.evidence_secondary,
    }))
    .map_err(|error| persistence_contract_error(error.to_string().as_str()))?;
    Ok(PersistedDoctorFinding {
        fingerprint: finding.fingerprint.clone(),
        finding_code: finding.finding_code.clone(),
        issue_code: finding.issue_code.clone(),
        manager_id,
        source_manager_id,
        subject_kind: subject_kind.to_string(),
        subject_value: subject_value.to_string(),
        severity: finding.severity.as_str().to_string(),
        evidence_json,
        detector_id: LOCAL_DOCTOR_DETECTOR_ID.to_string(),
        detector_version: LOCAL_DOCTOR_DETECTOR_VERSION.to_string(),
        first_seen_unix: observed_at_unix,
        last_seen_unix: observed_at_unix,
        latest_observation_generation: generation,
        resolution_state: "active".to_string(),
    })
}

fn persistence_contract_error(message: &str) -> CoreError {
    CoreError {
        manager: None,
        task: None,
        action: None,
        kind: CoreErrorKind::StorageFailure,
        message: message.to_string(),
    }
}
