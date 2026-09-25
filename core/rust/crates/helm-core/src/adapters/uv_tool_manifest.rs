//! Loss-intolerant translation of modern tool receipts into uv resolver inputs.
//! No URLs, credentials, or receipt bytes appear in Debug or parser errors.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use uv_pep440::{Version, VersionSpecifiers};

use super::uv_tool::{UvToolObservation, normalize_name};
use super::uv_tool_eligibility::UvEligibilityError;

const MAX_BYTES: usize = 1024 * 1024;
const MAX_ITEMS: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    tool: Tool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Tool {
    requirements: Vec<Requirement>,
    #[serde(default)]
    constraints: Vec<Requirement>,
    #[serde(default)]
    overrides: Vec<Requirement>,
    #[serde(default)]
    build_constraints: Vec<Requirement>,
    #[serde(default)]
    excludes: Vec<toml::Value>,
    entrypoints: Vec<Entrypoint>,
    python: Option<String>,
    #[serde(default)]
    options: toml::Table,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Requirement {
    name: String,
    #[serde(default)]
    specifier: String,
    #[serde(default)]
    extras: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Entrypoint {
    name: String,
    install_path: PathBuf,
    from: Option<String>,
}

pub(crate) struct UvToolManifest {
    pub requirements: String,
    pub constraints: String,
    pub options: toml::Table,
    pub entrypoints: Vec<PathBuf>,
    /// Requirements and interpreter policy; effective source options are checked separately.
    pub policy: toml::Value,
}

impl UvToolManifest {
    pub fn parse(bytes: &[u8], installed: &UvToolObservation) -> Result<Self, UvEligibilityError> {
        if bytes.len() > MAX_BYTES {
            return Err(UvEligibilityError::InputTooLarge);
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| UvEligibilityError::UnsupportedReceipt)?;
        let receipt: Receipt =
            toml::from_str(text).map_err(|_| UvEligibilityError::UnsupportedReceipt)?;
        let tool = receipt.tool;
        if tool.requirements.len() + tool.constraints.len() > MAX_ITEMS
            || tool.entrypoints.len() > MAX_ITEMS
            || !tool.overrides.is_empty()
            || !tool.build_constraints.is_empty()
            || !tool.excludes.is_empty()
        {
            return Err(UvEligibilityError::UnsupportedReceipt);
        }
        if tool
            .requirements
            .first()
            .and_then(|r| normalize_name(&r.name))
            .as_deref()
            != Some(installed.name.as_str())
        {
            return Err(UvEligibilityError::IdentityMismatch);
        }
        let requirements = render_requirements(&tool.requirements)?;
        let constraints = render_requirements(&tool.constraints)?;
        validate_options(&tool.options)?;
        let expected: BTreeSet<_> = installed.executables.iter().map(String::as_str).collect();
        let mut actual = BTreeSet::new();
        for entrypoint in &tool.entrypoints {
            if !valid_text(&entrypoint.name)
                || !actual.insert(entrypoint.name.as_str())
                || entrypoint.install_path.file_name().and_then(|s| s.to_str())
                    != Some(&entrypoint.name)
                || !super::uv_tool_scope::valid_path(&entrypoint.install_path)
                || entrypoint
                    .from
                    .as_ref()
                    .is_some_and(|name| normalize_name(name).is_none())
            {
                return Err(UvEligibilityError::UnsupportedReceipt);
            }
        }
        if actual.is_empty() || actual != expected || actual.len() != installed.executables.len() {
            return Err(UvEligibilityError::IdentityMismatch);
        }
        if tool.python.as_ref().is_some_and(|s| !valid_text(s)) {
            return Err(UvEligibilityError::UnsupportedReceipt);
        }
        let mut policy: toml::Value =
            toml::from_str(text).map_err(|_| UvEligibilityError::UnsupportedReceipt)?;
        let policy_tool = policy
            .get_mut("tool")
            .and_then(toml::Value::as_table_mut)
            .ok_or(UvEligibilityError::UnsupportedReceipt)?;
        policy_tool.remove("entrypoints");
        policy_tool.remove("options");
        Ok(Self {
            requirements,
            constraints,
            options: tool.options,
            entrypoints: tool
                .entrypoints
                .into_iter()
                .map(|e| e.install_path)
                .collect(),
            policy,
        })
    }

    pub fn validate_entrypoints(&self, environment: &Path) -> Result<(), UvEligibilityError> {
        for path in &self.entrypoints {
            let target = path
                .canonicalize()
                .map_err(|_| UvEligibilityError::IdentityMismatch)?;
            if !target.starts_with(environment.join("bin")) {
                return Err(UvEligibilityError::IdentityMismatch);
            }
        }
        Ok(())
    }
}

fn valid_text(text: &str) -> bool {
    !text.is_empty() && text.len() <= 4096 && !text.chars().any(char::is_control)
}

fn render_requirements(requirements: &[Requirement]) -> Result<String, UvEligibilityError> {
    let mut lines = Vec::new();
    for requirement in requirements {
        let name = normalize_name(&requirement.name).ok_or(UvEligibilityError::IdentityMismatch)?;
        if !requirement.specifier.is_empty() && !valid_text(&requirement.specifier) {
            return Err(UvEligibilityError::InvalidVersion);
        }
        let specifiers: VersionSpecifiers = requirement
            .specifier
            .parse()
            .map_err(|_| UvEligibilityError::InvalidVersion)?;
        if requirement.extras.len() > MAX_ITEMS {
            return Err(UvEligibilityError::InputTooLarge);
        }
        let extras = requirement
            .extras
            .iter()
            .map(|extra| normalize_name(extra).ok_or(UvEligibilityError::UnsupportedReceipt))
            .collect::<Result<Vec<_>, _>>()?;
        let extras = if extras.is_empty() {
            String::new()
        } else {
            format!("[{}]", extras.join(","))
        };
        lines.push(format!("{name}{extras}{specifiers}\n"));
    }
    Ok(lines.concat())
}

/// Only options whose resolver semantics are shared by `tool upgrade` and `pip compile`.
/// Named per-requirement indexes, source overrides, and future fields fail closed.
pub(crate) fn validate_options(options: &toml::Table) -> Result<(), UvEligibilityError> {
    for (key, value) in options {
        let valid = match key.as_str() {
            "index-url" => value.as_str().is_some_and(valid_source),
            "index-strategy" | "prerelease" | "resolution" | "exclude-newer"
            | "keyring-provider" => value.as_str().is_some_and(valid_text),
            "extra-index-url" | "find-links" => value.as_array().is_some_and(|list| {
                list.len() <= MAX_ITEMS && list.iter().all(|v| v.as_str().is_some_and(valid_source))
            }),
            "no-index" | "no-build" | "no-sources" | "offline" | "native-tls" => value.is_bool(),
            _ => false,
        };
        if !valid {
            return Err(UvEligibilityError::SourceConfigurationRequiresResolution);
        }
    }
    Ok(())
}

fn valid_source(text: &str) -> bool {
    if !valid_text(text) {
        return false;
    }
    if super::uv_tool_scope::valid_path(Path::new(text)) {
        return true;
    }
    url::Url::parse(text).is_ok_and(|url| match url.scheme() {
        "https" | "http" => url.host_str().is_some() && url.fragment().is_none(),
        "file" => url
            .to_file_path()
            .is_ok_and(|path| super::uv_tool_scope::valid_path(&path)),
        _ => false,
    })
}

/// The compile output is authoritative only when the complete bounded capture is valid.
pub(crate) fn resolved_version(output: &[u8], name: &str) -> Result<String, UvEligibilityError> {
    if output.len() > 4 * MAX_BYTES {
        return Err(UvEligibilityError::InputTooLarge);
    }
    let text = std::str::from_utf8(output).map_err(|_| UvEligibilityError::InvalidVersion)?;
    let mut found = None;
    let mut names = BTreeSet::new();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let (package, version) = line
            .split_once("==")
            .ok_or(UvEligibilityError::InvalidVersion)?;
        let package = normalize_name(package).ok_or(UvEligibilityError::IdentityMismatch)?;
        let _: Version = version
            .parse()
            .map_err(|_| UvEligibilityError::InvalidVersion)?;
        if !names.insert(package.clone()) {
            return Err(UvEligibilityError::IdentityMismatch);
        }
        if package == name {
            found = Some(version.to_owned());
        }
    }
    found.ok_or(UvEligibilityError::IdentityMismatch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> UvToolObservation {
        UvToolObservation {
            name: "black".into(),
            installed_version: "24.0".into(),
            executables: vec!["black".into()],
            requirement: None,
            latest_version: None,
        }
    }
    fn receipt(extra: &str) -> String {
        format!(
            r#"[tool]
requirements = [{{name = "black", specifier = ">=24,<26", extras = ["colorama"]}}, {{name = "click", specifier = "<9"}}]
constraints = [{{name = "black", specifier = "!=25.1"}}]
entrypoints = [{{name = "black", install-path = "/home/test/bin/black"}}]
{extra}
"#
        )
    }

    #[test]
    fn retains_all_requirements_extras_constraints_and_sources() {
        let bytes = receipt("[tool.options]\nno-index = true\nfind-links = [\"/private/wheels\"]");
        let manifest = UvToolManifest::parse(bytes.as_bytes(), &observation()).unwrap();
        assert_eq!(manifest.requirements, "black[colorama]>=24, <26\nclick<9\n");
        assert_eq!(manifest.constraints, "black!=25.1\n");
        assert_eq!(
            manifest.options["find-links"][0].as_str(),
            Some("/private/wheels")
        );
    }

    #[test]
    fn unknown_receipt_fields_and_nonregistry_sources_fail_closed() {
        for bytes in [
            receipt("new-field = true"),
            receipt("").replace(
                "specifier = \">=24,<26\"",
                "url = \"https://private.example/secret.whl\"",
            ),
            receipt("").replace("specifier = \">=24,<26\"", "index = \"private\""),
            receipt("").replace(
                "specifier = \">=24,<26\"",
                "marker = \"python_version < '3.13'\"",
            ),
            receipt("[tool.options]\nunknown = true"),
        ] {
            let err = UvToolManifest::parse(bytes.as_bytes(), &observation())
                .err()
                .unwrap();
            assert!(!err.to_string().contains("private.example"));
        }
    }

    #[test]
    fn rejects_duplicate_or_mismatched_entrypoint_identity() {
        assert!(
            UvToolManifest::parse(
                receipt("")
                    .replace("/home/test/bin/black", "/home/test/bin/other")
                    .as_bytes(),
                &observation()
            )
            .is_err()
        );
        let mut observation = observation();
        observation.executables.push("black".into());
        assert!(UvToolManifest::parse(receipt("").as_bytes(), &observation).is_err());
    }

    #[test]
    fn resolution_requires_a_complete_unambiguous_pep440_pin_set() {
        assert_eq!(
            resolved_version(b"black==25.0rc1\nclick==8.2\n", "black").unwrap(),
            "25.0rc1"
        );
        for bytes in [
            b"black==25\nblack==24\n".as_slice(),
            b"black>=25\n",
            b"black @ https://example.org\n",
            b"click==8.2\n",
            b"black==25\nwarning: incomplete\n",
            b"black==25; python_version < '3.13'\n",
        ] {
            assert!(resolved_version(bytes, "black").is_err());
        }
    }

    #[test]
    fn verification_policy_preserves_source_and_constraints() {
        let before = UvToolManifest::parse(
            receipt("[tool.options]\nno-index = true").as_bytes(),
            &observation(),
        )
        .unwrap();
        let after = UvToolManifest::parse(
            receipt("[tool.options]\nno-index = true\nno-build = true").as_bytes(),
            &observation(),
        )
        .unwrap();
        assert_eq!(before.policy, after.policy);
        let drift = UvToolManifest::parse(
            receipt("[tool.options]\nno-index = false").as_bytes(),
            &observation(),
        )
        .unwrap();
        assert_ne!(before.options, drift.options);
    }
}
