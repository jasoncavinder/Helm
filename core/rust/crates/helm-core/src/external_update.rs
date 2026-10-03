//! Pure policy for the proposed external Sparkle helper. No process, network or
//! installation API is exposed here. Native observations must come from an
//! authenticated service, never be deserialized from the client's request.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

pub mod adoption;
pub mod durable;

pub const HELPER_IDENTIFIER: &str = "com.jasoncavinder.Helm.SparkleExternalUpdater";
const HELM_IDENTIFIER: &str = "com.jasoncavinder.Helm";
const HELM_TEAM: &str = "V73WPJR9M4";
const REVIEW_SECONDS: u64 = 120;
const MAX_REQUEST_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReviewRequest {
    pub schema_version: u32,
    pub operation_id: String,
    pub target_path: PathBuf,
    pub expected_bundle_identifier: String,
    pub expected_installed_build: String,
    pub expected_candidate_build: String,
}

impl ReviewRequest {
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
            || !operation_identifier(&self.operation_id)
            || !bundle_identifier(&self.expected_bundle_identifier)
            || !bounded_text(&self.expected_installed_build, 128)
            || !bounded_text(&self.expected_candidate_build, 128)
            || !normal_app_path(&self.target_path)
        {
            return Err(Rejection::MalformedRequest);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Authority {
    Standalone,
    /// Explicit local consent, not proof of standalone installation history.
    UserAdopted(adoption::AdoptionToken),
    OtherManager,
    Unknown,
}

impl Authority {
    fn permits_review(self) -> bool {
        matches!(self, Self::Standalone | Self::UserAdopted(_))
    }
}

/// Security.framework, filesystem and bundle observations, collected locally by
/// the future helper. This deliberately does not implement Deserialize.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TargetObservation {
    pub canonical_path: PathBuf,
    pub device: u64,
    pub inode: u64,
    pub bundle_identifier: String,
    pub build: String,
    pub team_identifier: String,
    pub code_directory_hash: Vec<u8>,
    pub signature_valid: bool,
    pub ed25519_public_key: Vec<u8>,
    pub feed_url: String,
    pub framework_major: u32,
    pub authority: Authority,
    pub has_store_receipt: bool,
    pub translocated: bool,
    pub writable_by_others: bool,
}

/// Sparkle must still perform its own compatibility, archive signature and
/// installation checks. These facts are not obtained from Helm's abbreviated
/// read-only appcast inventory, which is insufficient to authorize installation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateObservation {
    pub build: String,
    pub feed_url: String,
    pub archive_url: String,
    pub archive_length: u64,
    pub ed25519_signature: Vec<u8>,
    pub channel: Option<String>,
    pub is_full_zip_application: bool,
    pub sparkle_accepts_upgrade: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BoundaryObservation {
    pub helper_identifier: String,
    pub helper_team_identifier: String,
    pub helper_code_directory_hash: Vec<u8>,
    pub caller_identifier: String,
    pub caller_team_identifier: String,
    pub authenticated_live_caller: bool,
    pub developer_id_signature_valid: bool,
    pub notarization_accepted: bool,
    pub helm_sandbox_preserved: bool,
    pub external_helper_unsandboxed: bool,
    pub direct_consumer_channel: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rejection {
    MalformedRequest,
    BoundaryUnavailable,
    TargetOutsideRoots,
    HelmSelfUpdate,
    TargetChanged,
    UnsupportedAuthority,
    UnsupportedTarget,
    UnsupportedCandidate,
    ReviewExpired,
    ReviewChanged,
    WrongOperation,
    InvalidTransition,
}

#[derive(Debug, Serialize)]
pub struct ReviewedUpdate {
    request: ReviewRequest,
    target: TargetObservation,
    candidate: CandidateObservation,
    boundary: BoundaryObservation,
    fingerprint: String,
    reviewed_at: u64,
}

impl ReviewedUpdate {
    /// `application_roots` and all observations are trusted local inputs, not
    /// caller-controlled request fields. Time is supplied by a monotonic clock.
    pub fn prepare(
        request: ReviewRequest,
        target: TargetObservation,
        candidate: CandidateObservation,
        boundary: BoundaryObservation,
        application_roots: &[PathBuf],
        now: u64,
    ) -> Result<Self, Rejection> {
        request.validate()?;
        validate_boundary(&boundary)?;
        validate_target(&target, application_roots)?;
        if request.target_path != target.canonical_path
            || request.expected_bundle_identifier != target.bundle_identifier
            || request.expected_installed_build != target.build
        {
            return Err(Rejection::TargetChanged);
        }
        if !target.authority.permits_review() {
            return Err(Rejection::UnsupportedAuthority);
        }
        if candidate.build != request.expected_candidate_build
            || candidate.build == target.build
            || candidate.feed_url != target.feed_url
            || !secure_url(&candidate.archive_url)
            || candidate.archive_length == 0
            || candidate.archive_length > 4 * 1024 * 1024 * 1024
            || candidate.ed25519_signature.len() != 64
            || candidate.channel.is_some()
            || !candidate.is_full_zip_application
            || !candidate.sparkle_accepts_upgrade
        {
            return Err(Rejection::UnsupportedCandidate);
        }
        let encoded = serde_json::to_vec(&(&request, &target, &candidate, &boundary))
            .map_err(|_| Rejection::MalformedRequest)?;
        Ok(Self {
            request,
            target,
            candidate,
            boundary,
            fingerprint: format!("{:x}", Sha256::digest(encoded)),
            reviewed_at: now,
        })
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Consumes this in-memory review. The external runtime must persist the
    /// returned session through `durable` before starting any side effect.
    pub fn confirm(
        self,
        target: TargetObservation,
        candidate: CandidateObservation,
        boundary: BoundaryObservation,
        application_roots: &[PathBuf],
        now: u64,
    ) -> Result<UpdateSession, Rejection> {
        if now < self.reviewed_at || now - self.reviewed_at > REVIEW_SECONDS {
            return Err(Rejection::ReviewExpired);
        }
        let current = Self::prepare(
            self.request.clone(),
            target,
            candidate,
            boundary,
            application_roots,
            now,
        )?;
        if current.fingerprint != self.fingerprint {
            return Err(Rejection::ReviewChanged);
        }
        Ok(UpdateSession {
            review: self,
            state: UpdateState::Downloading,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    Downloading,
    ReadyToInstall,
    Installing,
    AwaitingVerification,
    CancelledBeforeInstall,
    FailedBeforeInstall,
    Unverified,
    VersionVerified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateEvent {
    Downloaded,
    InstallationWillBegin,
    InstallerFinished,
    Cancelled,
    Failed,
    ConnectionLost,
}

#[derive(Debug, Serialize)]
pub struct UpdateSession {
    review: ReviewedUpdate,
    state: UpdateState,
}

impl UpdateSession {
    pub fn state(&self) -> UpdateState {
        self.state
    }

    pub fn event(&mut self, operation_id: &str, event: UpdateEvent) -> Result<(), Rejection> {
        if operation_id != self.review.request.operation_id {
            return Err(Rejection::WrongOperation);
        }
        use UpdateEvent as Event;
        use UpdateState as State;
        self.state = match (self.state, event) {
            (State::Downloading, Event::Downloaded) => State::ReadyToInstall,
            (State::ReadyToInstall, Event::InstallationWillBegin) => State::Installing,
            (State::Installing, Event::InstallerFinished) => State::AwaitingVerification,
            (State::Downloading | State::ReadyToInstall, Event::Cancelled) => {
                State::CancelledBeforeInstall
            }
            (State::Downloading | State::ReadyToInstall, Event::Failed | Event::ConnectionLost) => {
                State::FailedBeforeInstall
            }
            (
                State::Installing | State::AwaitingVerification,
                Event::Cancelled | Event::Failed | Event::ConnectionLost,
            ) => State::Unverified,
            _ => return Err(Rejection::InvalidTransition),
        };
        Ok(())
    }

    /// Fresh native observation is mandatory. An installer exit/callback alone
    /// cannot prove replacement. This verifies version, not process relaunch or
    /// attribution of every external filesystem change to Helm.
    pub fn reconcile(
        &mut self,
        operation_id: &str,
        observed: &TargetObservation,
    ) -> Result<UpdateState, Rejection> {
        if operation_id != self.review.request.operation_id {
            return Err(Rejection::WrongOperation);
        }
        if !matches!(
            self.state,
            UpdateState::AwaitingVerification | UpdateState::Unverified
        ) {
            return Err(Rejection::InvalidTransition);
        }
        let original = &self.review.target;
        let verified = observed.canonical_path == original.canonical_path
            && observed.bundle_identifier == original.bundle_identifier
            && observed.build == self.review.candidate.build
            && observed.signature_valid
            && observed.team_identifier == original.team_identifier
            && valid_cdhash(&observed.code_directory_hash)
            && observed.ed25519_public_key == original.ed25519_public_key
            && observed.feed_url == original.feed_url
            && observed.framework_major == 2
            && observed.authority.permits_review()
            && observed.authority == original.authority
            && !observed.has_store_receipt
            && !observed.translocated
            && !observed.writable_by_others;
        self.state = if verified {
            UpdateState::VersionVerified
        } else {
            UpdateState::Unverified
        };
        Ok(self.state)
    }
}

fn validate_boundary(boundary: &BoundaryObservation) -> Result<(), Rejection> {
    if boundary.helper_identifier != HELPER_IDENTIFIER
        || boundary.helper_team_identifier != HELM_TEAM
        || boundary.caller_identifier != HELM_IDENTIFIER
        || boundary.caller_team_identifier != HELM_TEAM
        || !boundary.authenticated_live_caller
        || !boundary.developer_id_signature_valid
        || !boundary.notarization_accepted
        || !boundary.helm_sandbox_preserved
        || !boundary.external_helper_unsandboxed
        || !boundary.direct_consumer_channel
        || !valid_cdhash(&boundary.helper_code_directory_hash)
    {
        return Err(Rejection::BoundaryUnavailable);
    }
    Ok(())
}

fn validate_target(target: &TargetObservation, roots: &[PathBuf]) -> Result<(), Rejection> {
    if target
        .bundle_identifier
        .to_ascii_lowercase()
        .starts_with(&HELM_IDENTIFIER.to_ascii_lowercase())
    {
        return Err(Rejection::HelmSelfUpdate);
    }
    if !normal_app_path(&target.canonical_path)
        || !roots.iter().any(|root| {
            allowed_application_root(root)
                && target.canonical_path.starts_with(root)
                && target.canonical_path != *root
        })
    {
        return Err(Rejection::TargetOutsideRoots);
    }
    if target.has_store_receipt || target.authority == Authority::OtherManager {
        return Err(Rejection::UnsupportedAuthority);
    }
    if !target.signature_valid
        || !bundle_identifier(&target.bundle_identifier)
        || !bounded_text(&target.build, 128)
        || !team_identifier(&target.team_identifier)
        || !valid_cdhash(&target.code_directory_hash)
        || target.ed25519_public_key.len() != 32
        || target.framework_major != 2
        || target.translocated
        || target.writable_by_others
        || !secure_url(&target.feed_url)
    {
        return Err(Rejection::UnsupportedTarget);
    }
    Ok(())
}

fn operation_identifier(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
            }
        })
}

fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn team_identifier(value: &str) -> bool {
    value.len() == 10
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn bundle_identifier(value: &str) -> bool {
    bounded_text(value, 255)
        && value.contains('.')
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn allowed_application_root(root: &Path) -> bool {
    if root == Path::new("/Applications") {
        return true;
    }
    let components: Vec<_> = root.components().collect();
    matches!(components.as_slice(), [Component::RootDir, Component::Normal(users), Component::Normal(user), Component::Normal(apps)]
        if *users == "Users" && *apps == "Applications" && !user.is_empty())
        && root
            .to_str()
            .is_some_and(|value| !value.contains("//") && !value.contains("/./"))
}

fn valid_cdhash(value: &[u8]) -> bool {
    matches!(value.len(), 20 | 32) && value.iter().any(|byte| *byte != 0)
}

pub(crate) fn normal_app_path(path: &Path) -> bool {
    let Some(text) = path.to_str() else {
        return false;
    };
    path.is_absolute()
        && bounded_text(text, 4096)
        && !text.contains("//")
        && !text.ends_with('/')
        && !text.split('/').any(|part| part == "." || part == "..")
        && path.extension().is_some_and(|extension| extension == "app")
        && path
            .components()
            .all(|component| matches!(component, Component::RootDir | Component::Normal(_)))
        && !path.ancestors().skip(1).any(|parent| {
            parent
                .extension()
                .is_some_and(|extension| extension == "app")
        })
}

fn secure_url(value: &str) -> bool {
    if !bounded_text(value, 4096) || !value.starts_with("https://") || value.contains('\\') {
        return false;
    }
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && url.port_or_known_default() == Some(443)
    })
}

#[cfg(test)]
mod tests;
