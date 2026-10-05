//! Explicit per-app permission, separate from both provenance and update consent.
//! The authenticated coordinator supplies fresh native observations. A saved
//! receipt or a client-provided authority field must never substitute for them.

use super::*;
use crate::external_update::durable::DurableUpdateError as Error;
use crate::sqlite::SqliteStore;

/// Point-in-time history only. Even Recorded is not resolved authority, complete
/// exclusion coverage, candidate consent, or an install permit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsentStatus {
    NotRecorded,
    Recorded,
    Revoked,
    IdentityChanged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AdoptionRequest {
    pub schema_version: u32,
    pub consent_id: String,
    pub target_path: PathBuf,
    pub expected_bundle_identifier: String,
    pub expected_installed_build: String,
}

impl AdoptionRequest {
    pub fn decode(bytes: &[u8]) -> Result<Self, Rejection> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(Rejection::MalformedRequest);
        }
        let request: Self =
            serde_json::from_slice(bytes).map_err(|_| Rejection::MalformedRequest)?;
        request.validate()?;
        Ok(request)
    }

    fn validate(&self) -> Result<(), Rejection> {
        if self.schema_version != 1
            || !operation_identifier(&self.consent_id)
            || !normal_app_path(&self.target_path)
            || !bundle_identifier(&self.expected_bundle_identifier)
            || !bounded_text(&self.expected_installed_build, 128)
        {
            return Err(Rejection::MalformedRequest);
        }
        Ok(())
    }
}

/// Store-issued revision. The random epoch prevents reset/downgrade from making
/// an old in-memory review current again when sequence numbers restart.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AdoptionToken {
    pub(crate) epoch: [u8; 32],
    pub(crate) sequence: i64,
}

/// Output only: this is not installation history, an update approval or a token
/// that the helper accepts over its wire interface.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AdoptionReceipt {
    pub token: AdoptionToken,
    pub target_path: PathBuf,
    pub consent_id: Option<String>,
    pub identity_fingerprint: Option<String>,
    pub review_fingerprint: Option<String>,
}

impl AdoptionReceipt {
    pub fn is_revoked(&self) -> bool {
        self.consent_id.is_none()
    }
}

#[derive(Debug, Serialize)]
pub struct ReviewedAdoption {
    request: AdoptionRequest,
    target: TargetObservation,
    boundary: BoundaryObservation,
    prior: AdoptionToken,
    fingerprint: String,
    reviewed_at: u64,
}

impl ReviewedAdoption {
    /// Prepare without mutating consent. Unknown origin is allowed only here:
    /// successful fresh native collection must already have ruled out all known
    /// competing claims. Unreadable or incomplete collection is an error, never
    /// an observation whose authority can be filled in by the client.
    pub fn prepare(
        store: &SqliteStore,
        request: AdoptionRequest,
        target: TargetObservation,
        boundary: BoundaryObservation,
        application_roots: &[PathBuf],
        now: u64,
    ) -> Result<Self, Error> {
        Self::validate(&request, &target, &boundary, application_roots).map_err(Error::Policy)?;
        let prior = store.external_adoption_cursor(&target.canonical_path)?;
        Self::from_validated(request, target, boundary, prior, now)
    }

    fn from_validated(
        request: AdoptionRequest,
        target: TargetObservation,
        boundary: BoundaryObservation,
        prior: AdoptionToken,
        now: u64,
    ) -> Result<Self, Error> {
        let fingerprint = Self::fingerprint_for(&request, &target, &boundary, prior)?;
        Ok(Self {
            request,
            target,
            boundary,
            prior,
            fingerprint,
            reviewed_at: now,
        })
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Consumes the review; commit errors are not retry permission. Confirmation
    /// persists only adoption, never an update session or an installation permit.
    pub fn confirm(
        self,
        store: &SqliteStore,
        target: TargetObservation,
        boundary: BoundaryObservation,
        application_roots: &[PathBuf],
        now: u64,
    ) -> Result<AdoptionReceipt, Error> {
        self.validate_confirmation(&target, &boundary, application_roots, now)?;
        store
            .commit_external_adoption(
                &self.request.consent_id,
                &target,
                &self.fingerprint,
                self.prior,
            )?
            .ok_or(Error::Conflict)
    }

    fn validate_confirmation(
        &self,
        target: &TargetObservation,
        boundary: &BoundaryObservation,
        application_roots: &[PathBuf],
        now: u64,
    ) -> Result<(), Error> {
        if now < self.reviewed_at || now - self.reviewed_at >= REVIEW_SECONDS {
            return Err(Error::Policy(Rejection::ReviewExpired));
        }
        Self::validate(&self.request, target, boundary, application_roots)
            .map_err(Error::Policy)?;
        if Self::fingerprint_for(&self.request, target, boundary, self.prior)? != self.fingerprint {
            return Err(Error::Policy(Rejection::ReviewChanged));
        }
        Ok(())
    }

    fn validate(
        request: &AdoptionRequest,
        target: &TargetObservation,
        boundary: &BoundaryObservation,
        roots: &[PathBuf],
    ) -> Result<(), Rejection> {
        request.validate()?;
        validate_boundary(boundary)?;
        validate_unresolved_target(target, roots)?;
        if target.canonical_path != request.target_path
            || target.bundle_identifier != request.expected_bundle_identifier
            || target.build != request.expected_installed_build
        {
            return Err(Rejection::TargetChanged);
        }
        Ok(())
    }

    fn fingerprint_for(
        request: &AdoptionRequest,
        target: &TargetObservation,
        boundary: &BoundaryObservation,
        prior: AdoptionToken,
    ) -> Result<String, Error> {
        let bytes = serde_json::to_vec(&(request, target, boundary, prior))
            .map_err(|_| Error::Policy(Rejection::MalformedRequest))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

/// Existing-only helper ledger review. Neither the review nor its path crosses
/// IPC. The authenticated native coordinator must supply fresh observations and
/// hold its private filesystem lease around both preparation and confirmation.
/// This type does not collect those facts, authenticate a peer or start an update.
pub struct ReviewedLedgerAdoption {
    database_path: PathBuf,
    review: ReviewedAdoption,
}

impl ReviewedLedgerAdoption {
    pub fn prepare(
        database_path: &Path,
        request: AdoptionRequest,
        target: TargetObservation,
        boundary: BoundaryObservation,
        application_roots: &[PathBuf],
        now: u64,
    ) -> Result<Self, Error> {
        if !database_path.is_absolute() {
            return Err(Error::Policy(Rejection::MalformedRequest));
        }
        ReviewedAdoption::validate(&request, &target, &boundary, application_roots)
            .map_err(Error::Policy)?;
        let prior =
            SqliteStore::read_external_adoption_cursor(database_path, &target.canonical_path)?;
        Ok(Self {
            database_path: database_path.into(),
            review: ReviewedAdoption::from_validated(request, target, boundary, prior, now)?,
        })
    }

    /// Consumes the review regardless of outcome. The caller must admit this
    /// explicit confirmation while its authenticated session is live. A storage
    /// error or lost reply is not retry permission or proof of an unchanged ledger.
    pub fn confirm(
        self,
        database_path: &Path,
        target: TargetObservation,
        boundary: BoundaryObservation,
        application_roots: &[PathBuf],
        now: u64,
    ) -> Result<AdoptionReceipt, Error> {
        if database_path != self.database_path {
            return Err(Error::Policy(Rejection::ReviewChanged));
        }
        self.review
            .validate_confirmation(&target, &boundary, application_roots, now)?;
        SqliteStore::commit_reviewed_external_adoption(
            database_path,
            &self.review.request.consent_id,
            &target,
            &self.review.fingerprint,
            self.review.prior,
        )?
        .ok_or(Error::Conflict)
    }
}

/// Resolve only a new, successful native observation. Cached resolved authority
/// cannot be fed back in; competing claims and unsafe targets always win.
pub fn resolve(
    store: &SqliteStore,
    mut target: TargetObservation,
    roots: &[PathBuf],
) -> Result<TargetObservation, Error> {
    validate_unresolved_target(&target, roots).map_err(Error::Policy)?;
    if let Some(receipt) = store.external_update_adoption(&target.canonical_path)?
        && !receipt.is_revoked()
        && receipt.identity_fingerprint.as_deref() == Some(&identity_fingerprint(&target))
    {
        target.authority = Authority::UserAdopted(receipt.token);
    }
    Ok(target)
}

/// Read-only preflight for freshly collected local evidence. Success is NOT
/// provenance, complete exclusion coverage, adoption consent or update consent.
/// No database, boundary observation or candidate is consulted here.
pub fn validate_unresolved_target(
    target: &TargetObservation,
    roots: &[PathBuf],
) -> Result<(), Rejection> {
    validate_target(target, roots)?;
    if target.authority != Authority::Unknown {
        return Err(Rejection::UnsupportedAuthority);
    }
    Ok(())
}

/// Version/inode/cdhash are deliberately omitted from ongoing per-app consent:
/// vendor updates replace them. They are still bound exactly during adoption
/// review and every individual update's review/confirmation/handoff.
pub(crate) fn identity_fingerprint(target: &TargetObservation) -> String {
    let bytes = serde_json::to_vec(&(
        &target.canonical_path,
        &target.bundle_identifier,
        &target.team_identifier,
        &target.ed25519_public_key,
        &target.feed_url,
        target.framework_major,
    ))
    .expect("serializing an already validated target identity");
    format!("{:x}", Sha256::digest(bytes))
}
