//! Denial-only consent removal. Missing or newly ineligible apps remain revocable.
use super::*;
use crate::external_update::adoption::{AdoptionReceipt, AdoptionToken};
use crate::external_update::durable::DurableUpdateError as Error;
use crate::sqlite::SqliteStore;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RevocationRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub target_path: PathBuf,
}

impl RevocationRequest {
    pub fn decode(bytes: &[u8], roots: &[PathBuf]) -> Result<Self, Rejection> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(Rejection::MalformedRequest);
        }
        let request: Self =
            serde_json::from_slice(bytes).map_err(|_| Rejection::MalformedRequest)?;
        if request.schema_version != 1
            || !operation_identifier(&request.request_id)
            || !normal_app_path(&request.target_path)
        {
            return Err(Rejection::MalformedRequest);
        }
        if !roots
            .iter()
            .any(|root| allowed_application_root(root) && request.target_path.starts_with(root))
        {
            return Err(Rejection::TargetOutsideRoots);
        }
        Ok(request)
    }
}

/// Helper-owned, nonserializable and consumed by confirmation. The wire review
/// handle must be connection-bound; callers cannot supply an epoch or revision.
pub struct ReviewedRevocation {
    database_path: PathBuf,
    target_path: PathBuf,
    prior: AdoptionToken,
    reviewed_at: u64,
}

impl ReviewedRevocation {
    pub fn prepare(
        database_path: &Path,
        request: RevocationRequest,
        roots: &[PathBuf],
        now: u64,
    ) -> Result<Self, Error> {
        let bytes =
            serde_json::to_vec(&request).map_err(|_| Error::Policy(Rejection::MalformedRequest))?;
        RevocationRequest::decode(&bytes, roots).map_err(Error::Policy)?;
        let prior =
            SqliteStore::read_external_revocation_cursor(database_path, &request.target_path)?;
        Ok(Self {
            database_path: database_path.into(),
            target_path: request.target_path,
            prior,
            reviewed_at: now,
        })
    }

    /// The coordinator must admit this explicit confirmation while its session
    /// is live. Losing the reply after admission is NOT permission to replay it.
    pub fn confirm(self, database_path: &Path, now: u64) -> Result<AdoptionReceipt, Error> {
        if now < self.reviewed_at || now - self.reviewed_at >= REVIEW_SECONDS {
            return Err(Error::Policy(Rejection::ReviewExpired));
        }
        if database_path != self.database_path {
            return Err(Error::Policy(Rejection::ReviewChanged));
        }
        SqliteStore::commit_reviewed_external_revocation(
            database_path,
            &self.target_path,
            self.prior,
        )?
        .ok_or(Error::Conflict)
    }
}
