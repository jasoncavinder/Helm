use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_MARKER_RELATIVE_PATH: &str = ".config/helm/install.json";
const DEFAULT_BREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];
const MACPORTS_PREFIX: &str = "/opt/local";
const APP_BUNDLE_SEGMENT: &str = ".app/Contents/";
const INSTALL_MARKER_SCHEMA_JSON: &str =
    include_str!("../../../../../docs/contracts/install-marker.schema.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallChannel {
    DirectScript,
    AppBundleShim,
    Brew,
    Macports,
    Cargo,
    Unknown,
    Managed,
}

impl InstallChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DirectScript => "direct-script",
            Self::AppBundleShim => "app-bundle-shim",
            Self::Brew => "brew",
            Self::Macports => "macports",
            Self::Cargo => "cargo",
            Self::Unknown => "unknown",
            Self::Managed => "managed",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "direct-script" => Some(Self::DirectScript),
            "app-bundle-shim" => Some(Self::AppBundleShim),
            "brew" => Some(Self::Brew),
            "macports" => Some(Self::Macports),
            "cargo" => Some(Self::Cargo),
            "unknown" => Some(Self::Unknown),
            "managed" => Some(Self::Managed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePolicy {
    SelfManaged,
    ChannelManaged,
    Managed,
    None,
}

impl UpdatePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelfManaged => "self",
            Self::ChannelManaged => "channel",
            Self::Managed => "managed",
            Self::None => "none",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "self" => Some(Self::SelfManaged),
            "channel" => Some(Self::ChannelManaged),
            "managed" => Some(Self::Managed),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvenanceSource {
    Marker,
    Heuristic,
}

impl ProvenanceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Marker => "marker",
            Self::Heuristic => "heuristic",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallProvenance {
    pub channel: InstallChannel,
    pub artifact: String,
    pub installed_at: Option<String>,
    pub update_policy: UpdatePolicy,
    pub version: Option<String>,
    pub source: ProvenanceSource,
    pub marker_path: PathBuf,
    pub executable_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallMarker {
    pub channel: String,
    pub artifact: String,
    pub installed_at: String,
    pub update_policy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawInstallMarker {
    channel: String,
    artifact: String,
    installed_at: String,
    update_policy: String,
    #[serde(default)]
    version: Option<String>,
}

pub fn install_marker_path() -> Result<PathBuf, String> {
    if let Ok(explicit) = env::var("HELM_INSTALL_MARKER_PATH")
        && !explicit.trim().is_empty()
    {
        return Ok(PathBuf::from(explicit));
    }

    let home = env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
    Ok(PathBuf::from(home).join(DEFAULT_MARKER_RELATIVE_PATH))
}

pub fn read_install_marker(path: &Path) -> Option<InstallMarker> {
    let raw = fs::read_to_string(path).ok()?;
    let parsed: RawInstallMarker = serde_json::from_str(&raw).ok()?;
    let channel = InstallChannel::parse(&parsed.channel)?;
    let policy = UpdatePolicy::parse(&parsed.update_policy)?;
    if parsed.artifact.trim().is_empty() || parsed.installed_at.trim().is_empty() {
        return None;
    }

    Some(InstallMarker {
        channel: channel.as_str().to_string(),
        artifact: parsed.artifact.trim().to_string(),
        installed_at: parsed.installed_at.trim().to_string(),
        update_policy: policy.as_str().to_string(),
        version: parsed.version.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }),
    })
}

pub fn write_install_marker(path: &Path, marker: &InstallMarker) -> Result<(), String> {
    validate_install_marker(marker)?;
    reject_symlink_marker_path(path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create marker directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    reject_symlink_marker_path(path)?;

    let payload = serde_json::to_string_pretty(marker)
        .map_err(|error| format!("failed to serialize install marker: {error}"))?;
    let temp_path = marker_temp_path(path);
    let mut temp_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temp_path)
        .map_err(|error| {
            format!(
                "failed to create temporary install marker '{}': {error}",
                temp_path.display()
            )
        })?;
    temp_file.write_all(payload.as_bytes()).map_err(|error| {
        format!(
            "failed to write temporary install marker '{}': {error}",
            temp_path.display()
        )
    })?;
    temp_file.sync_all().map_err(|error| {
        format!(
            "failed to flush temporary install marker '{}': {error}",
            temp_path.display()
        )
    })?;

    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "failed to atomically replace install marker '{}': {error}",
            path.display()
        ));
    }

    sync_parent_directory(path)?;
    Ok(())
}

fn marker_temp_path(path: &Path) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "install.json".to_string());
    path.with_file_name(format!(".{file_name}.tmp-{suffix}"))
}

fn reject_symlink_marker_path(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "failed to read install marker metadata '{}': {error}",
                path.display()
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to write install marker to symlink path '{}'",
            path.display()
        ));
    }
    Ok(())
}

fn sync_parent_directory(path: &Path) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let directory = OpenOptions::new()
        .read(true)
        .open(parent)
        .map_err(|error| {
            format!(
                "failed to open install marker parent directory '{}': {error}",
                parent.display()
            )
        })?;
    directory.sync_all().map_err(|error| {
        format!(
            "failed to sync install marker parent directory '{}': {error}",
            parent.display()
        )
    })
}

#[cfg(test)]
pub fn install_marker_schema_json() -> &'static str {
    INSTALL_MARKER_SCHEMA_JSON
}

pub fn validate_install_marker(marker: &InstallMarker) -> Result<(), String> {
    let value = serde_json::to_value(marker)
        .map_err(|error| format!("failed to encode install marker for validation: {error}"))?;
    validate_install_marker_value(&value)
}

pub fn validate_install_marker_value(value: &Value) -> Result<(), String> {
    let schema: Value = serde_json::from_str(INSTALL_MARKER_SCHEMA_JSON)
        .map_err(|error| format!("invalid install marker schema JSON: {error}"))?;
    let schema_object = schema
        .as_object()
        .ok_or_else(|| "install marker schema must be a JSON object".to_string())?;
    let marker = value
        .as_object()
        .ok_or_else(|| "install marker payload must be a JSON object".to_string())?;
    let properties = schema_object
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| "install marker schema is missing object 'properties'".to_string())?;

    let additional_properties = schema_object
        .get("additionalProperties")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if !additional_properties {
        for key in marker.keys() {
            if !properties.contains_key(key) {
                return Err(format!("install marker has unexpected property '{key}'"));
            }
        }
    }

    if let Some(required) = schema_object.get("required").and_then(Value::as_array) {
        for key in required {
            let Some(key) = key.as_str() else {
                return Err("install marker schema has non-string required key".to_string());
            };
            if !marker.contains_key(key) {
                return Err(format!(
                    "install marker is missing required property '{key}'"
                ));
            }
        }
    }

    for (name, property_schema) in properties {
        let Some(property_schema) = property_schema.as_object() else {
            continue;
        };
        let Some(value) = marker.get(name) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        let property_type = property_schema.get("type").and_then(Value::as_str);
        if property_type == Some("string") {
            let string_value = value
                .as_str()
                .ok_or_else(|| format!("install marker property '{name}' must be a string"))?;
            if let Some(min_length) = property_schema.get("minLength").and_then(Value::as_u64)
                && (string_value.chars().count() as u64) < min_length
            {
                return Err(format!(
                    "install marker property '{name}' must be at least {min_length} characters"
                ));
            }
            if let Some(enum_values) = property_schema.get("enum").and_then(Value::as_array)
                && !enum_values
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|candidate| candidate == string_value)
            {
                return Err(format!(
                    "install marker property '{name}' has unsupported value '{}'",
                    string_value
                ));
            }
        }
    }

    Ok(())
}

pub fn recommended_action(channel: InstallChannel) -> &'static str {
    match channel {
        InstallChannel::DirectScript => "helm self update",
        InstallChannel::Brew => "brew upgrade helm-cli",
        InstallChannel::Macports => "sudo port selfupdate && sudo port upgrade helm-cli",
        InstallChannel::Cargo => "cargo install --locked helm-cli",
        InstallChannel::AppBundleShim => {
            "update Helm GUI via Sparkle/App Store/Setapp; app-bundled shim cannot self-update"
        }
        InstallChannel::Managed => "follow managed organizational update policy",
        InstallChannel::Unknown => "reinstall Helm CLI via a supported channel",
    }
}

pub fn can_self_update(policy: UpdatePolicy) -> bool {
    matches!(policy, UpdatePolicy::SelfManaged)
}

pub fn detect_install_provenance(executable_path: &Path) -> InstallProvenance {
    let marker_path =
        install_marker_path().unwrap_or_else(|_| PathBuf::from(DEFAULT_MARKER_RELATIVE_PATH));
    let resolved_executable = normalize_path(executable_path);

    if let Some(marker) = read_install_marker(&marker_path)
        && let (Some(channel), Some(update_policy)) = (
            InstallChannel::parse(&marker.channel),
            UpdatePolicy::parse(&marker.update_policy),
        )
    {
        return InstallProvenance {
            channel,
            artifact: marker.artifact,
            installed_at: Some(marker.installed_at),
            update_policy,
            version: marker.version,
            source: ProvenanceSource::Marker,
            marker_path,
            executable_path: resolved_executable,
        };
    }

    let home_dir = home_dir();
    let brew_prefixes = brew_prefixes();
    let channel =
        detect_channel_from_path(&resolved_executable, home_dir.as_deref(), &brew_prefixes);

    InstallProvenance {
        channel,
        artifact: "helm-cli".to_string(),
        installed_at: None,
        update_policy: default_policy_for_channel(channel),
        version: None,
        source: ProvenanceSource::Heuristic,
        marker_path,
        executable_path: resolved_executable,
    }
}

fn home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

fn brew_prefixes() -> Vec<PathBuf> {
    let mut prefixes = DEFAULT_BREW_PREFIXES
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    let output = Command::new("brew").arg("--prefix").output();
    if let Ok(output) = output
        && output.status.success()
    {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !value.is_empty() {
            let discovered = PathBuf::from(value);
            if !prefixes.iter().any(|existing| existing == &discovered) {
                prefixes.push(discovered);
            }
        }
    }

    prefixes
}

fn detect_channel_from_path(
    executable_path: &Path,
    home_dir: Option<&Path>,
    brew_prefixes: &[PathBuf],
) -> InstallChannel {
    if is_app_bundle_path(executable_path) {
        return InstallChannel::AppBundleShim;
    }

    if path_starts_with(executable_path, Path::new(MACPORTS_PREFIX)) {
        return InstallChannel::Macports;
    }

    if brew_prefixes
        .iter()
        .any(|prefix| path_starts_with(executable_path, prefix))
    {
        return InstallChannel::Brew;
    }

    if let Some(home_dir) = home_dir {
        let cargo_bin = home_dir.join(".cargo").join("bin");
        if path_starts_with(executable_path, &cargo_bin) {
            return InstallChannel::Cargo;
        }
    }

    InstallChannel::Unknown
}

fn default_policy_for_channel(channel: InstallChannel) -> UpdatePolicy {
    match channel {
        InstallChannel::DirectScript => UpdatePolicy::SelfManaged,
        InstallChannel::Managed => UpdatePolicy::Managed,
        InstallChannel::Brew
        | InstallChannel::Macports
        | InstallChannel::Cargo
        | InstallChannel::AppBundleShim => UpdatePolicy::ChannelManaged,
        InstallChannel::Unknown => UpdatePolicy::None,
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn is_app_bundle_path(path: &Path) -> bool {
    let raw = path.to_string_lossy();
    raw.contains(APP_BUNDLE_SEGMENT)
}

fn path_starts_with(path: &Path, prefix: &Path) -> bool {
    if let (Ok(path), Ok(prefix)) = (path.canonicalize(), prefix.canonicalize()) {
        return path.starts_with(prefix);
    }
    path.starts_with(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time monotonic")
            .as_nanos();
        std::env::temp_dir().join(format!("helm-cli-{name}-{nanos}.json"))
    }

    #[test]
    fn marker_channel_and_policy_parse() {
        let marker = InstallMarker {
            channel: "direct-script".to_string(),
            artifact: "helm-cli".to_string(),
            installed_at: "2026-02-23T00:00:00Z".to_string(),
            update_policy: "self".to_string(),
            version: Some("0.17.2".to_string()),
        };
        assert_eq!(
            InstallChannel::parse(&marker.channel),
            Some(InstallChannel::DirectScript)
        );
        assert_eq!(
            UpdatePolicy::parse(&marker.update_policy),
            Some(UpdatePolicy::SelfManaged)
        );
    }

    #[test]
    fn install_marker_schema_is_valid_json() {
        let schema: Value =
            serde_json::from_str(install_marker_schema_json()).expect("schema json parses");
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .expect("schema required array");
        assert!(required.iter().any(|value| value == "channel"));
        assert!(required.iter().any(|value| value == "artifact"));
        assert!(required.iter().any(|value| value == "installed_at"));
        assert!(required.iter().any(|value| value == "update_policy"));
    }

    #[test]
    fn read_install_marker_accepts_valid_json() {
        let path = temp_file_path("provenance-valid");
        let payload = r#"{
  "channel": "direct-script",
  "artifact": "helm-cli",
  "installed_at": "2026-02-23T00:00:00Z",
  "update_policy": "self",
  "version": "0.17.2"
}"#;
        fs::write(&path, payload).expect("writes marker fixture");

        let marker = read_install_marker(&path).expect("reads valid marker");
        assert_eq!(marker.channel, "direct-script");
        assert_eq!(marker.update_policy, "self");
        assert_eq!(marker.version.as_deref(), Some("0.17.2"));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn read_install_marker_rejects_invalid_enum_values() {
        let path = temp_file_path("provenance-invalid");
        let payload = r#"{
  "channel": "somewhere-else",
  "artifact": "helm-cli",
  "installed_at": "2026-02-23T00:00:00Z",
  "update_policy": "self"
}"#;
        fs::write(&path, payload).expect("writes marker fixture");

        let marker = read_install_marker(&path);
        assert!(marker.is_none());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn write_install_marker_persists_schema_compliant_payload() {
        let path = temp_file_path("provenance-write-valid");
        let marker = InstallMarker {
            channel: "direct-script".to_string(),
            artifact: "helm-cli".to_string(),
            installed_at: "2026-02-23T00:00:00Z".to_string(),
            update_policy: "self".to_string(),
            version: Some("0.17.2".to_string()),
        };

        write_install_marker(&path, &marker).expect("writes schema-valid marker");
        let payload = fs::read_to_string(&path).expect("reads marker");
        let value: Value = serde_json::from_str(&payload).expect("marker json parses");
        validate_install_marker_value(&value).expect("marker validates against schema");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn write_install_marker_rejects_symlink_target() {
        let dir = temp_file_path("provenance-symlink-dir");
        fs::create_dir_all(&dir).expect("creates temp dir");
        let target = dir.join("target-install.json");
        fs::write(&target, "{}").expect("writes target marker");
        let symlink_path = dir.join("install.json");
        std::os::unix::fs::symlink(&target, &symlink_path).expect("creates symlink marker path");

        let marker = InstallMarker {
            channel: "direct-script".to_string(),
            artifact: "helm-cli".to_string(),
            installed_at: "2026-02-23T00:00:00Z".to_string(),
            update_policy: "self".to_string(),
            version: Some("0.17.2".to_string()),
        };

        let error = write_install_marker(&symlink_path, &marker)
            .expect_err("symlink marker path must be rejected");
        assert!(error.contains("symlink"));

        let _ = fs::remove_file(symlink_path);
        let _ = fs::remove_file(target);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn heuristic_detects_macports() {
        let path = PathBuf::from("/opt/local/bin/helm");
        let detected = detect_channel_from_path(&path, None, &[]);
        assert_eq!(detected, InstallChannel::Macports);
    }

    #[test]
    fn heuristic_detects_brew_with_default_prefix() {
        let path = PathBuf::from("/opt/homebrew/bin/helm");
        let detected = detect_channel_from_path(
            &path,
            Some(Path::new("/Users/tester")),
            &[PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")],
        );
        assert_eq!(detected, InstallChannel::Brew);
    }

    #[test]
    fn heuristic_detects_brew_with_discovered_prefix() {
        let path = PathBuf::from("/custom/homebrew/bin/helm");
        let detected = detect_channel_from_path(
            &path,
            Some(Path::new("/Users/tester")),
            &[PathBuf::from("/custom/homebrew")],
        );
        assert_eq!(detected, InstallChannel::Brew);
    }

    #[test]
    fn heuristic_detects_cargo_install() {
        let path = PathBuf::from("/Users/tester/.cargo/bin/helm");
        let detected = detect_channel_from_path(&path, Some(Path::new("/Users/tester")), &[]);
        assert_eq!(detected, InstallChannel::Cargo);
    }

    #[test]
    fn heuristic_detects_app_bundle_shim() {
        let path = PathBuf::from("/Applications/Helm.app/Contents/MacOS/helm");
        let detected = detect_channel_from_path(
            &path,
            Some(Path::new("/Users/tester")),
            &[PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")],
        );
        assert_eq!(detected, InstallChannel::AppBundleShim);
    }

    #[test]
    fn heuristic_defaults_to_unknown() {
        let path = PathBuf::from("/usr/bin/helm");
        let detected = detect_channel_from_path(&path, Some(Path::new("/Users/tester")), &[]);
        assert_eq!(detected, InstallChannel::Unknown);
    }

    #[test]
    fn default_policy_mapping_matches_contract() {
        assert_eq!(
            default_policy_for_channel(InstallChannel::DirectScript),
            UpdatePolicy::SelfManaged
        );
        assert_eq!(
            default_policy_for_channel(InstallChannel::Managed),
            UpdatePolicy::Managed
        );
        assert_eq!(
            default_policy_for_channel(InstallChannel::Brew),
            UpdatePolicy::ChannelManaged
        );
        assert_eq!(
            default_policy_for_channel(InstallChannel::Unknown),
            UpdatePolicy::None
        );
    }
}
