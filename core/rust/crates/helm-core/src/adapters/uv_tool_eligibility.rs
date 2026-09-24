//! Pure, receipt-based candidate filtering, not update authorization or a resolver.
//!
//! The caller must bind receipt bytes, inventory, interpreter facts, and registry
//! metadata to the same selected tool/store. This module does no I/O. In particular,
//! `tool list --outdated` alone cannot provide its compatibility inputs.

use std::collections::BTreeSet;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use uv_pep440::{Version, VersionSpecifiers};

use super::uv_tool::{UvToolObservation, normalize_name};

const MAX_RECEIPT_BYTES: usize = 1024 * 1024;
const MAX_REQUIREMENTS: usize = 256;
const MAX_FIELD_BYTES: usize = 4096;

/// Errors are intentionally payload-free: receipt/parser diagnostics can contain
/// private index credentials, direct URLs, and local paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum UvEligibilityError {
    #[error("uv eligibility input exceeded its limit")]
    InputTooLarge,
    #[error("uv tool receipt is malformed or has unsupported fields")]
    UnsupportedReceipt,
    #[error("uv tool receipt and installed observation disagree")]
    IdentityMismatch,
    #[error("uv tool source is not a registry requirement")]
    NonRegistrySource,
    #[error("uv tool source configuration requires resolver integration")]
    SourceConfigurationRequiresResolution,
    #[error("uv tool markers or dependency groups require resolver integration")]
    ConditionalRequirement,
    #[error("uv tool uses an unsupported legacy requirement receipt")]
    LegacyRequirement,
    #[error("uv tool version or constraint is invalid")]
    InvalidVersion,
}

/// Metadata for one observed distribution from the owning source. Unknown data
/// is not equivalent to an unrestricted or non-yanked distribution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UvRegistryCandidate {
    pub name: String,
    pub version: String,
    /// `Some("")` means metadata was read and declares no Python restriction.
    pub requires_python: Option<String>,
    pub yanked: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UvCandidateRejection {
    NotNewer,
    OutsideConstraints,
    PythonIncompatible,
    PrereleaseDisallowed,
    Yanked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UvCandidateEligibility {
    Rejected(UvCandidateRejection),
    NeedsDistributionMetadata,
    NeedsPythonEvidence,
    /// Default/explicit/fallback prerelease preference is uv resolver policy,
    /// not merely PEP 440 membership or the absence of a newer stable version.
    NeedsPrereleaseResolution,
    /// Only top-level version/constraint/Python checks passed. Dependencies,
    /// extras, wheel/platform support, source configuration, ownership, receipt
    /// freshness, and reviewed execution still need independent verification.
    NeedsResolution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
enum PrereleasePolicy {
    Allow,
    Disallow,
    Explicit,
    IfNecessary,
    #[default]
    IfNecessaryOrExplicit,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    tool: ReceiptTool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReceiptTool {
    requirements: Vec<toml::Value>,
    #[serde(default)]
    constraints: Vec<toml::Value>,
    entrypoints: Vec<Entrypoint>,
    // This is a selection request, NOT the actual environment's Python version.
    python: Option<String>,
    #[serde(default)]
    options: toml::Table,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Entrypoint {
    name: String,
    install_path: String,
    from: Option<String>,
}

/// No raw receipt, source URL, or path is retained in this debug-printable type.
#[derive(Clone, Debug)]
pub struct UvToolEligibilityPolicy {
    name: String,
    installed_version: Version,
    constraints: Vec<VersionSpecifiers>,
    prerelease: PrereleasePolicy,
    receipt_digest: String,
}

impl UvToolEligibilityPolicy {
    /// Parse a bounded modern receipt and cross-check its primary distribution
    /// and entrypoint names against the separately accepted installed inventory.
    /// Display `[required: ...]` and `[latest: ...]` annotations are NOT authority.
    pub fn from_receipt(
        installed: &UvToolObservation,
        bytes: &[u8],
    ) -> Result<Self, UvEligibilityError> {
        if bytes.len() > MAX_RECEIPT_BYTES {
            return Err(UvEligibilityError::InputTooLarge);
        }
        let raw = std::str::from_utf8(bytes).map_err(|_| UvEligibilityError::UnsupportedReceipt)?;
        let receipt: Receipt =
            toml::from_str(raw).map_err(|_| UvEligibilityError::UnsupportedReceipt)?;
        let tool = receipt.tool;
        if tool
            .requirements
            .len()
            .saturating_add(tool.constraints.len())
            > MAX_REQUIREMENTS
            || tool.entrypoints.len() > MAX_REQUIREMENTS
            || installed.executables.len() > MAX_REQUIREMENTS
        {
            return Err(UvEligibilityError::InputTooLarge);
        }
        let name = checked_name(&installed.name)?;
        let installed_version = checked_version(&installed.installed_version)?;
        let first = tool
            .requirements
            .first()
            .ok_or(UvEligibilityError::UnsupportedReceipt)?;
        if registry_requirement(first)?.0 != name {
            return Err(UvEligibilityError::IdentityMismatch);
        }
        let mut constraints = Vec::new();
        for requirement in tool.requirements.iter().chain(&tool.constraints) {
            let (requirement_name, specifiers) = registry_requirement(requirement)?;
            if requirement_name == name {
                constraints.push(specifiers);
            }
        }
        // Validate other dependency requirements too; never discard an unknown
        // source/marker and accidentally reinterpret it as a registry dependency.
        let expected: BTreeSet<_> = installed.executables.iter().map(String::as_str).collect();
        let mut actual = BTreeSet::new();
        for entrypoint in &tool.entrypoints {
            checked_text(&entrypoint.name)?;
            checked_text(&entrypoint.install_path)?;
            if let Some(from) = &entrypoint.from {
                checked_name(from)?;
            }
            if entrypoint.name.is_empty()
                || entrypoint.install_path.is_empty()
                || !actual.insert(entrypoint.name.as_str())
            {
                return Err(UvEligibilityError::UnsupportedReceipt);
            }
        }
        if actual.is_empty() || actual != expected || expected.len() != installed.executables.len()
        {
            return Err(UvEligibilityError::IdentityMismatch);
        }
        if let Some(python) = &tool.python {
            checked_text(python)?;
            if python.is_empty() {
                return Err(UvEligibilityError::UnsupportedReceipt);
            }
        }
        let prerelease = parse_options(tool.options)?;
        Ok(Self {
            name,
            installed_version,
            constraints,
            prerelease,
            receipt_digest: format!("{:x}", Sha256::digest(bytes)),
        })
    }

    /// An evidence fingerprint, not filesystem freshness or source authentication.
    pub fn receipt_digest(&self) -> &str {
        &self.receipt_digest
    }

    /// Assess a candidate without turning it into an `OutdatedPackage` or action.
    /// Rejection of all observed candidates does NOT prove the tool is current:
    /// a latest-only/incomplete listing may omit a newer in-range version.
    pub fn assess(
        &self,
        candidate: &UvRegistryCandidate,
        observed_python: Option<&str>,
    ) -> Result<UvCandidateEligibility, UvEligibilityError> {
        use UvCandidateEligibility::*;
        use UvCandidateRejection::*;

        if checked_name(&candidate.name)? != self.name {
            return Err(UvEligibilityError::IdentityMismatch);
        }
        let version = checked_version(&candidate.version)?;
        if version <= self.installed_version {
            return Ok(Rejected(NotNewer));
        }
        if !self.constraints.iter().all(|spec| spec.contains(&version)) {
            return Ok(Rejected(OutsideConstraints));
        }
        if candidate.yanked == Some(true) {
            return Ok(Rejected(Yanked));
        }
        if version.any_prerelease() && self.prerelease == PrereleasePolicy::Disallow {
            return Ok(Rejected(PrereleaseDisallowed));
        }
        let Some(requires_python) = &candidate.requires_python else {
            return Ok(NeedsDistributionMetadata);
        };
        if candidate.yanked.is_none() {
            return Ok(NeedsDistributionMetadata);
        }
        let python_constraints = checked_specifiers(requires_python)?;
        let Some(python) = observed_python else {
            return Ok(NeedsPythonEvidence);
        };
        let python = checked_version(python)?;
        // Do not treat a receipt request such as `3.12` as interpreter evidence.
        if python.release().len() != 3
            || python.epoch() != 0
            || python.any_prerelease()
            || python.is_post()
            || python.is_local()
        {
            return Ok(NeedsPythonEvidence);
        }
        if !python_constraints.contains(&python) {
            return Ok(Rejected(PythonIncompatible));
        }
        if version.any_prerelease() && self.prerelease != PrereleasePolicy::Allow {
            return Ok(NeedsPrereleaseResolution);
        }
        Ok(NeedsResolution)
    }
}

fn checked_text(text: &str) -> Result<(), UvEligibilityError> {
    if text.len() > MAX_FIELD_BYTES {
        return Err(UvEligibilityError::InputTooLarge);
    }
    if text.chars().any(char::is_control) {
        return Err(UvEligibilityError::UnsupportedReceipt);
    }
    Ok(())
}

fn checked_name(text: &str) -> Result<String, UvEligibilityError> {
    checked_text(text)?;
    normalize_name(text).ok_or(UvEligibilityError::IdentityMismatch)
}

fn checked_version(text: &str) -> Result<Version, UvEligibilityError> {
    checked_text(text)?;
    text.parse().map_err(|_| UvEligibilityError::InvalidVersion)
}

fn checked_specifiers(text: &str) -> Result<VersionSpecifiers, UvEligibilityError> {
    checked_text(text)?;
    text.parse().map_err(|_| UvEligibilityError::InvalidVersion)
}

fn registry_requirement(
    value: &toml::Value,
) -> Result<(String, VersionSpecifiers), UvEligibilityError> {
    if value.is_str() {
        return Err(UvEligibilityError::LegacyRequirement);
    }
    let table = value
        .as_table()
        .ok_or(UvEligibilityError::UnsupportedReceipt)?;
    if ["git", "url", "path", "directory", "editable", "virtual"]
        .iter()
        .any(|key| table.contains_key(*key))
    {
        return Err(UvEligibilityError::NonRegistrySource);
    }
    if table.contains_key("index") {
        return Err(UvEligibilityError::SourceConfigurationRequiresResolution);
    }
    if table.contains_key("marker") || table.contains_key("groups") {
        return Err(UvEligibilityError::ConditionalRequirement);
    }
    if table
        .keys()
        .any(|key| !matches!(key.as_str(), "name" | "specifier" | "extras"))
    {
        return Err(UvEligibilityError::UnsupportedReceipt);
    }
    let name = table
        .get("name")
        .and_then(toml::Value::as_str)
        .ok_or(UvEligibilityError::UnsupportedReceipt)?;
    if let Some(extras) = table.get("extras") {
        let extras = extras
            .as_array()
            .ok_or(UvEligibilityError::UnsupportedReceipt)?;
        if extras.len() > MAX_REQUIREMENTS {
            return Err(UvEligibilityError::InputTooLarge);
        }
        for extra in extras {
            checked_name(
                extra
                    .as_str()
                    .ok_or(UvEligibilityError::UnsupportedReceipt)?,
            )?;
        }
    }
    let specifier = match table.get("specifier") {
        None => "",
        Some(value) => value
            .as_str()
            .ok_or(UvEligibilityError::UnsupportedReceipt)?,
    };
    Ok((checked_name(name)?, checked_specifiers(specifier)?))
}

fn parse_options(mut options: toml::Table) -> Result<PrereleasePolicy, UvEligibilityError> {
    if [
        "index",
        "index-url",
        "extra-index-url",
        "default-index",
        "find-links",
        "no-index",
    ]
    .iter()
    .any(|key| options.contains_key(*key))
    {
        return Err(UvEligibilityError::SourceConfigurationRequiresResolution);
    }
    let prerelease = options
        .remove("prerelease")
        .map(toml::Value::try_into)
        .transpose()
        .map_err(|_| UvEligibilityError::UnsupportedReceipt)?
        .unwrap_or_default();
    if !options.is_empty() {
        return Err(UvEligibilityError::UnsupportedReceipt);
    }
    Ok(prerelease)
}
