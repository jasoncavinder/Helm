//! Explicit per-app permission, separate from both provenance and update consent.
//! The authenticated coordinator supplies fresh native observations. A saved
//! receipt or a client-provided authority field must never substitute for them.

use super::*;
use crate::external_update::durable::DurableUpdateError as Error;
use crate::sqlite::SqliteStore;

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
        if now < self.reviewed_at || now - self.reviewed_at > REVIEW_SECONDS {
            return Err(Error::Policy(Rejection::ReviewExpired));
        }
        Self::validate(&self.request, &target, &boundary, application_roots)
            .map_err(Error::Policy)?;
        if Self::fingerprint_for(&self.request, &target, &boundary, self.prior)? != self.fingerprint
        {
            return Err(Error::Policy(Rejection::ReviewChanged));
        }
        store
            .commit_external_adoption(
                &self.request.consent_id,
                &target,
                &self.fingerprint,
                self.prior,
            )?
            .ok_or(Error::Conflict)
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
