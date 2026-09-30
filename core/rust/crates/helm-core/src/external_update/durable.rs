//! Durable, one-shot bookkeeping for the proposed external updater. This is not
//! an installer or an authenticated transport. No record can recreate authority.

use super::*;
use crate::models::CoreError;
use crate::sqlite::SqliteStore;

#[derive(Debug, thiserror::Error)]
pub enum DurableUpdateError {
    #[error("external updater policy rejected the transition: {0:?}")]
    Policy(Rejection),
    #[error("external updater session conflicts with persisted state")]
    Conflict,
    #[error("external updater session requires recovery")]
    RecoveryRequired,
    #[error("external updater persistence failed: {0}")]
    Storage(CoreError),
}

impl From<CoreError> for DurableUpdateError {
    fn from(error: CoreError) -> Self {
        Self::Storage(error)
    }
}

/// Read-only local receipt. It cannot be submitted as consent or resumed work.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExternalUpdateReceipt {
    pub operation_id: String,
    pub fingerprint: String,
    pub target_path: PathBuf,
    pub target_device: u64,
    pub target_inode: u64,
    pub bundle_identifier: String,
    pub installed_build: String,
    pub candidate_build: String,
    pub state: UpdateState,
    pub revision: i64,
}

/// Non-cloneable proof that fresh observation and the durable handoff succeeded.
/// A future authenticated helper must consume it exactly once to invoke Sparkle.
#[derive(Debug)]
pub struct InstallationPermit {
    operation_id: String,
    fingerprint: String,
}

impl InstallationPermit {
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
}

pub struct DurableUpdateSession<'a> {
    store: &'a SqliteStore,
    session: UpdateSession,
    receipt: ExternalUpdateReceipt,
    usable: bool,
}

impl<'a> DurableUpdateSession<'a> {
    /// Call only after authenticated live confirmation. A failed/uncertain claim
    /// consumes the in-memory authorization; inspect the ledger, never resubmit.
    pub fn claim(
        store: &'a SqliteStore,
        session: UpdateSession,
        now: u64,
    ) -> Result<Self, DurableUpdateError> {
        if session.state != UpdateState::Downloading {
            return Err(DurableUpdateError::Policy(Rejection::InvalidTransition));
        }
        let review = &session.review;
        if now < review.reviewed_at || now - review.reviewed_at > REVIEW_SECONDS {
            return Err(DurableUpdateError::Policy(Rejection::ReviewExpired));
        }
        let receipt = ExternalUpdateReceipt {
            operation_id: review.request.operation_id.clone(),
            fingerprint: review.fingerprint.clone(),
            target_path: review.target.canonical_path.clone(),
            target_device: review.target.device,
            target_inode: review.target.inode,
            bundle_identifier: review.target.bundle_identifier.clone(),
            installed_build: review.target.build.clone(),
            candidate_build: review.candidate.build.clone(),
            state: UpdateState::Downloading,
            revision: 0,
        };
        if !store.claim_external_update(&receipt)? {
            return Err(DurableUpdateError::Conflict);
        }
        Ok(Self {
            store,
            session,
            receipt,
            usable: true,
        })
    }

    pub fn receipt(&self) -> &ExternalUpdateReceipt {
        &self.receipt
    }

    /// Persist callback state before publishing it. Handoff requires fresh native
    /// observations via `begin_install`, not a caller-supplied generic event.
    pub fn event(
        &mut self,
        operation_id: &str,
        event: UpdateEvent,
    ) -> Result<(), DurableUpdateError> {
        self.ensure_usable()?;
        if event == UpdateEvent::InstallationWillBegin {
            return Err(DurableUpdateError::Policy(Rejection::InvalidTransition));
        }
        self.session
            .event(operation_id, event)
            .map_err(DurableUpdateError::Policy)?;
        self.persist_transition(false)
    }

    /// Re-observe immediately before installation. Download time may exceed the
    /// original review window, but target, candidate and authenticated boundary
    /// must still match the exact originally accepted fingerprint.
    pub fn begin_install(
        &mut self,
        target: TargetObservation,
        candidate: CandidateObservation,
        boundary: BoundaryObservation,
        application_roots: &[PathBuf],
    ) -> Result<InstallationPermit, DurableUpdateError> {
        self.ensure_usable()?;
        let current = ReviewedUpdate::prepare(
            self.session.review.request.clone(),
            target,
            candidate,
            boundary,
            application_roots,
            0,
        )
        .map_err(DurableUpdateError::Policy)?;
        if current.fingerprint != self.receipt.fingerprint {
            return Err(DurableUpdateError::Policy(Rejection::ReviewChanged));
        }
        self.session
            .event(
                &self.receipt.operation_id,
                UpdateEvent::InstallationWillBegin,
            )
            .map_err(DurableUpdateError::Policy)?;
        self.persist_transition(true)?;
        Ok(InstallationPermit {
            operation_id: self.receipt.operation_id.clone(),
            fingerprint: self.receipt.fingerprint.clone(),
        })
    }

    /// Only an uninterrupted authoritative installer-finished callback can enter
    /// this path. Restart/loss receipts remain locked and cannot be resumed here.
    pub fn verify(
        &mut self,
        operation_id: &str,
        observed: &TargetObservation,
    ) -> Result<(), DurableUpdateError> {
        self.ensure_usable()?;
        if self.session.state != UpdateState::AwaitingVerification {
            return Err(DurableUpdateError::RecoveryRequired);
        }
        self.session
            .reconcile(operation_id, observed)
            .map_err(DurableUpdateError::Policy)?;
        self.persist_transition(false)
    }

    fn ensure_usable(&self) -> Result<(), DurableUpdateError> {
        if self.usable {
            Ok(())
        } else {
            Err(DurableUpdateError::RecoveryRequired)
        }
    }

    fn persist_transition(&mut self, check_safe_mode: bool) -> Result<(), DurableUpdateError> {
        // A commit error may be ambiguous. Do not issue a permit, restore the
        // old in-memory state, or allow the caller to retry this authorization.
        self.usable = false;
        let next = self
            .receipt
            .revision
            .checked_add(1)
            .ok_or(DurableUpdateError::RecoveryRequired)?;
        if !self.store.transition_external_update(
            &self.receipt,
            self.session.state,
            check_safe_mode,
        )? {
            return Err(DurableUpdateError::Conflict);
        }
        self.receipt.state = self.session.state;
        self.receipt.revision = next;
        self.usable = !matches!(self.session.state, UpdateState::Unverified);
        Ok(())
    }
}

pub(crate) fn state_name(state: UpdateState) -> &'static str {
    match state {
        UpdateState::Downloading => "downloading",
        UpdateState::ReadyToInstall => "ready_to_install",
        UpdateState::Installing => "installing",
        UpdateState::AwaitingVerification => "awaiting_verification",
        UpdateState::CancelledBeforeInstall => "cancelled_before_install",
        UpdateState::FailedBeforeInstall => "failed_before_install",
        UpdateState::Unverified => "unverified",
        UpdateState::VersionVerified => "version_verified",
    }
}

pub(crate) fn parse_state(value: &str) -> Option<UpdateState> {
    [
        UpdateState::Downloading,
        UpdateState::ReadyToInstall,
        UpdateState::Installing,
        UpdateState::AwaitingVerification,
        UpdateState::CancelledBeforeInstall,
        UpdateState::FailedBeforeInstall,
        UpdateState::Unverified,
        UpdateState::VersionVerified,
    ]
    .into_iter()
    .find(|state| state_name(*state) == value)
}

pub(crate) fn holds_target(state: UpdateState) -> bool {
    !matches!(
        state,
        UpdateState::CancelledBeforeInstall
            | UpdateState::FailedBeforeInstall
            | UpdateState::VersionVerified
    )
}
