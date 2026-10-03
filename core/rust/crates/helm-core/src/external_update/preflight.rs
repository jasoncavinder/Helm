//! Read-only requests are not adoption consent or update authorization.
use super::*;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PreflightRequest {
    pub schema_version: u32,
    pub request_id: String,
    pub target_path: PathBuf,
    pub expected_bundle_identifier: String,
    pub expected_installed_build: String,
}

impl PreflightRequest {
    /// Validate before native filesystem collection, including permitted roots.
    pub fn decode(bytes: &[u8], roots: &[PathBuf]) -> Result<Self, Rejection> {
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(Rejection::MalformedRequest);
        }
        let request: Self =
            serde_json::from_slice(bytes).map_err(|_| Rejection::MalformedRequest)?;
        if request.schema_version != 1
            || !operation_identifier(&request.request_id)
            || !normal_app_path(&request.target_path)
            || !bundle_identifier(&request.expected_bundle_identifier)
            || !bounded_text(&request.expected_installed_build, 128)
        {
            return Err(Rejection::MalformedRequest);
        }
        if !roots
            .iter()
            .any(|root| allowed_application_root(root) && request.target_path.starts_with(root))
        {
            return Err(Rejection::TargetOutsideRoots);
        }
        if request
            .expected_bundle_identifier
            .to_ascii_lowercase()
            .starts_with(&HELM_IDENTIFIER.to_ascii_lowercase())
        {
            return Err(Rejection::HelmSelfUpdate);
        }
        Ok(request)
    }

    pub fn assess(&self, target: &TargetObservation, roots: &[PathBuf]) -> Result<(), Rejection> {
        // Revalidate even for locally constructed requests. Evidence is never
        // deserialized; a matching request does not authenticate the observation.
        let encoded = serde_json::to_vec(self).map_err(|_| Rejection::MalformedRequest)?;
        Self::decode(&encoded, roots)?;
        if self.target_path != target.canonical_path
            || self.expected_bundle_identifier != target.bundle_identifier
            || self.expected_installed_build != target.build
        {
            return Err(Rejection::TargetChanged);
        }
        adoption::validate_unresolved_target(target, roots)
    }
}
