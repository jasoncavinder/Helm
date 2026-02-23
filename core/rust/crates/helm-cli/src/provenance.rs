use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

const DEFAULT_MARKER_RELATIVE_PATH: &str = ".config/helm/install.json";
const DEFAULT_BREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];
const MACPORTS_PREFIX: &str = "/opt/local";
const APP_BUNDLE_SEGMENT: &str = ".app/Contents/";

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

#[allow(dead_code)]
pub fn write_install_marker(path: &Path, marker: &InstallMarker) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create marker directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    let payload = serde_json::to_string_pretty(marker)
        .map_err(|error| format!("failed to serialize install marker: {error}"))?;
    fs::write(path, payload).map_err(|error| {
        format!(
            "failed to write install marker '{}': {error}",
            path.display()
        )
    })
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
