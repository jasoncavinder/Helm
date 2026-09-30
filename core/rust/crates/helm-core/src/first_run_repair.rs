//! One finite, consented first-run improvement. No discovery command or mutation
//! runs while proposing a repair; applying it requires the exact reviewed plan.
use std::collections::BTreeSet;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::execution::{CommandSpec, ProcessExecutor, ProcessExitStatus, ProcessSpawnRequest};
use crate::first_run::{
    CandidateScanStatus, LocalObservationContext, observe_first_run_environment,
};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskType};
use crate::persistence::{DetectionStore, ManagerPreference, PersistenceResult};

pub const ACTION_ID: &str = "manager.clear_selected_executable_override";
const POLICY_REVISION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FirstRunRepairPlan {
    pub(crate) fingerprint: String,
    policy_revision: u32,
    action_id: &'static str,
    mutation_class: &'static str,
    requires_network: bool,
    requires_privilege: bool,
    rollback_eligible: bool,
    verification_method_id: &'static str,
    pub(crate) before: ManagerPreference,
    executable: ExecutableEvidence,
}

impl FirstRunRepairPlan {
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub fn executable_path(&self) -> &Path {
        &self.executable.path
    }
    pub fn previous_path(&self) -> &str {
        self.before
            .selected_executable_path
            .as_deref()
            .unwrap_or_default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct ExecutableEvidence {
    path: PathBuf,
    device: u64,
    inode: u64,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
    mode: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairVerification {
    /// Also the durable result after interruption. Never means still running.
    Unverified,
    Verified,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstRunRepairReceipt {
    pub receipt_id: i64,
    pub plan_fingerprint: String,
    pub action_id: String,
    pub previous_path: String,
    pub checked_path: PathBuf,
    pub applied: bool,
    pub verification: RepairVerification,
    pub observed_version: Option<String>,
    /// Fixed diagnostic code, never subprocess output or a claim of rollback.
    pub reason: Option<String>,
}

pub trait FirstRunRepairStore: DetectionStore {
    /// Atomically compare every reviewed preference, clear only the selected
    /// path, and record an unverified receipt. None means the plan went stale.
    fn apply_first_run_repair(
        &self,
        plan: &FirstRunRepairPlan,
    ) -> PersistenceResult<Option<FirstRunRepairReceipt>>;
    /// Finalization compares the current preference to the exact expected after
    /// state in the same transaction as the durable verification result.
    fn finish_first_run_repair(
        &self,
        plan: &FirstRunRepairPlan,
        receipt: &FirstRunRepairReceipt,
    ) -> PersistenceResult<FirstRunRepairReceipt>;
    fn first_run_repair_receipts(&self) -> PersistenceResult<Vec<FirstRunRepairReceipt>>;
}

/// Metadata-only proposal. Disabled/unconfigured managers, partial scans,
/// uncertain absence, scripts/shims and multiple distinct binaries are excluded.
pub fn propose_first_run_repair(
    store: &dyn DetectionStore,
    context: &LocalObservationContext,
) -> PersistenceResult<Option<FirstRunRepairPlan>> {
    if store.safe_mode()? {
        return Ok(None);
    }
    let preferences = store.list_manager_preferences()?;
    let Some(before) = preferences
        .into_iter()
        .find(|p| p.manager == ManagerId::Mise && p.enabled)
    else {
        return Ok(None);
    };
    let Some(previous) = before.selected_executable_path.as_deref() else {
        return Ok(None);
    };
    if !Path::new(previous).is_absolute() || !definitely_absent(Path::new(previous)) {
        return Ok(None);
    }
    let observation = observe_first_run_environment(store, context)?;
    let Some(manager) = observation
        .managers
        .iter()
        .find(|m| m.manager_id == ManagerId::Mise)
    else {
        return Ok(None);
    };
    if manager.candidate_scan_status != CandidateScanStatus::Complete
        || manager.selected_executable_path != before.selected_executable_path
        || manager.configured_enabled != Some(true)
    {
        return Ok(None);
    }
    let mut paths = BTreeSet::new();
    for candidate in &manager.candidate_paths {
        // One unverifiable candidate makes the entire alternative ambiguous.
        let Some(evidence) = executable_evidence(candidate) else {
            return Ok(None);
        };
        paths.insert(evidence.path);
    }
    if paths.len() != 1 {
        return Ok(None);
    }
    let Some(executable) = executable_evidence(paths.first().expect("one candidate")) else {
        return Ok(None);
    };
    let mut plan = FirstRunRepairPlan {
        fingerprint: String::new(),
        policy_revision: POLICY_REVISION,
        action_id: ACTION_ID,
        mutation_class: "helm_preference",
        requires_network: false,
        requires_privilege: false,
        rollback_eligible: false,
        verification_method_id: "manager.detect_bound_executable",
        before,
        executable,
    };
    let bytes = serde_json::to_vec(&plan).map_err(|_| repair_error("plan_encoding_failed"))?;
    plan.fingerprint = format!("{:x}", Sha256::digest(bytes));
    Ok(Some(plan))
}

fn definitely_absent(path: &Path) -> bool {
    matches!(std::fs::metadata(path), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

fn executable_evidence(path: &Path) -> Option<ExecutableEvidence> {
    if !path.is_absolute() || path.file_name() != Some(std::ffi::OsStr::new("mise")) {
        return None;
    }
    let path = path.canonicalize().ok()?;
    let mut file = std::fs::File::open(&path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return None;
    }
    let mut magic = [0; 4];
    file.read_exact(&mut magic).ok()?;
    // A native executable only: never run a discovered shell wrapper or shim.
    if !matches!(
        magic,
        [0xcf, 0xfa, 0xed, 0xfe]
            | [0xce, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xcf]
            | [0xfe, 0xed, 0xfa, 0xce]
            | [0xca, 0xfe, 0xba, 0xbe]
            | [0xbe, 0xba, 0xfe, 0xca]
            | [0xca, 0xfe, 0xba, 0xbf]
            | [0xbf, 0xba, 0xfe, 0xca]
            | [0x7f, b'E', b'L', b'F']
    ) {
        return None;
    }
    Some(ExecutableEvidence {
        path,
        device: metadata.dev(),
        inode: metadata.ino(),
        length: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
        mode: metadata.mode(),
    })
}

/// Caller must obtain separate explicit mutation consent for this reviewed plan.
/// Only the compiled --version check runs, with no shell/network/elevation.
/// This does not certify manager readiness, package inventory or PATH precedence.
pub async fn apply_approved_first_run_repair(
    store: &dyn FirstRunRepairStore,
    context: &LocalObservationContext,
    plan: &FirstRunRepairPlan,
    executor: &dyn ProcessExecutor,
) -> PersistenceResult<FirstRunRepairReceipt> {
    if propose_first_run_repair(store, context)?.as_ref() != Some(plan) {
        return Err(repair_error("reviewed_plan_changed"));
    }
    let mut receipt = store
        .apply_first_run_repair(plan)?
        .ok_or_else(|| repair_error("reviewed_plan_changed"))?;
    // Nothing above acknowledges first run, accepts terms or activates a runtime.
    // If this future is interrupted, the atomically persisted unverified receipt
    // remains and startup never resumes this mutation automatically.
    if executable_evidence(&plan.executable.path).as_ref() != Some(&plan.executable) {
        receipt.verification = RepairVerification::Failed;
        receipt.reason = Some("executable_changed".into());
        return store.finish_first_run_repair(plan, &receipt);
    }
    let request = verification_request(&plan.executable.path);
    request.validate()?;
    // Bypass global selected-executable/timeout overrides: the reviewed absolute
    // binary and fixed bound are authoritative for this storage-only operation.
    let output = match executor.spawn(request) {
        Ok(process) => process.wait().await,
        Err(error) => Err(error),
    };
    let version = output
        .ok()
        .filter(|o| o.status == ProcessExitStatus::ExitCode(0))
        .and_then(|o| parse_version(&o.stdout, &o.stderr));
    let unchanged = definitely_absent(Path::new(plan.previous_path()))
        && executable_evidence(&plan.executable.path).as_ref() == Some(&plan.executable);
    if let Some(version) = version.filter(|_| unchanged) {
        receipt.verification = RepairVerification::Verified;
        receipt.observed_version = Some(version);
    } else {
        receipt.verification = RepairVerification::Failed;
        receipt.reason = Some(
            if unchanged {
                "version_check_failed"
            } else {
                "evidence_changed"
            }
            .into(),
        );
    }
    store.finish_first_run_repair(plan, &receipt)
}

fn verification_request(path: &Path) -> ProcessSpawnRequest {
    let command = CommandSpec::new(path)
        .arg("--version")
        .working_dir("/")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("MISE_OFFLINE", "1")
        .env("MISE_AUTO_UPDATE", "0")
        .env("NO_COLOR", "1");
    let mut request = ProcessSpawnRequest::new(
        ManagerId::Mise,
        TaskType::Detection,
        ManagerAction::Detect,
        command,
    )
    .timeout(Duration::from_secs(10));
    request.private_output_limit = Some(4096);
    request
}

fn parse_version(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    if stdout.len().checked_add(stderr.len())? > 4096 {
        return None;
    }
    let output = std::str::from_utf8(if stdout.is_empty() { stderr } else { stdout }).ok()?;
    let output = output.trim();
    if output.lines().count() != 1 {
        return None;
    }
    let version = output
        .strip_prefix("mise ")
        .unwrap_or(output)
        .split_whitespace()
        .next()?;
    let parsed = semver::Version::parse(version).ok()?;
    if !(2020..=2100).contains(&parsed.major) {
        return None;
    }
    Some(parsed.to_string())
}

fn repair_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Mise),
        task: Some(TaskType::Configure),
        action: None,
        kind: CoreErrorKind::InvalidInput,
        message: message.into(),
    }
}
