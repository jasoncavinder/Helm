use crate::models::{
    AutomationLevel, InstallProvenance, ManagerId, ManagerInstallInstance, StrategyKind,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiInstanceState {
    None,
    AttentionNeeded,
    Acknowledged,
}

impl MultiInstanceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::AttentionNeeded => "attention_needed",
            Self::Acknowledged => "acknowledged",
        }
    }
}

pub fn install_instance_fingerprint(instances: &[ManagerInstallInstance]) -> Option<String> {
    let ids = instances
        .iter()
        .map(|instance| instance.instance_id.as_str());
    instance_ids_fingerprint(ids)
}

pub fn normalize_manager_install_instances(
    instances: &[ManagerInstallInstance],
) -> Vec<ManagerInstallInstance> {
    if instances.len() <= 1 {
        return instances
            .iter()
            .cloned()
            .map(canonicalize_system_manager_instance)
            .collect();
    }

    let manager = instances[0].manager;
    let Some(preferred_display_path) =
        preferred_system_manager_display_path(manager).map(PathBuf::from)
    else {
        return instances
            .iter()
            .cloned()
            .map(canonicalize_system_manager_instance)
            .collect();
    };

    let mut trusted = Vec::new();
    let mut passthrough = Vec::new();
    for instance in instances
        .iter()
        .cloned()
        .map(canonicalize_system_manager_instance)
    {
        if is_trusted_system_manager_instance(manager, &instance) {
            trusted.push(instance);
        } else {
            passthrough.push(instance);
        }
    }

    if trusted.len() <= 1 {
        let mut normalized = trusted;
        normalized.extend(passthrough);
        return normalized;
    }

    let mut merged = trusted
        .iter()
        .max_by(|left, right| {
            system_manager_instance_rank(manager, left)
                .cmp(&system_manager_instance_rank(manager, right))
                .then_with(|| {
                    left.confidence
                        .partial_cmp(&right.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| left.instance_id.cmp(&right.instance_id))
        })
        .cloned()
        .expect("trusted system manager group is non-empty");

    let mut alias_paths = BTreeSet::new();
    for instance in &trusted {
        alias_paths.insert(instance.display_path.clone());
        if let Some(path) = instance.canonical_path.clone() {
            alias_paths.insert(path);
        }
        for path in &instance.alias_paths {
            alias_paths.insert(path.clone());
        }
    }
    alias_paths.insert(preferred_display_path.clone());

    merged.display_path = preferred_display_path.clone();
    if merged.canonical_path.is_none() {
        merged.canonical_path = Some(preferred_display_path.clone());
    }
    merged.alias_paths = alias_paths.into_iter().collect();
    merged.is_active = trusted.iter().any(|instance| instance.is_active);

    passthrough.push(merged);
    passthrough.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    passthrough
}

pub fn instance_ids_fingerprint<'a>(
    instance_ids: impl IntoIterator<Item = &'a str>,
) -> Option<String> {
    let mut sorted = instance_ids
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if sorted.len() <= 1 {
        return None;
    }
    sorted.sort_unstable();
    sorted.dedup();
    if sorted.len() <= 1 {
        return None;
    }
    let canonical = sorted.join("\n");
    Some(format!("{:016x}", stable_hash64(canonical.as_str())))
}

pub fn resolve_multi_instance_state<'a>(
    instance_ids: impl IntoIterator<Item = &'a str>,
    acknowledged_fingerprint: Option<&str>,
) -> (MultiInstanceState, Option<String>, bool) {
    let fingerprint = instance_ids_fingerprint(instance_ids);
    match fingerprint {
        None => (MultiInstanceState::None, None, false),
        Some(value) => {
            let acknowledged = acknowledged_fingerprint
                .map(str::trim)
                .filter(|stored| !stored.is_empty())
                .is_some_and(|stored| stored == value);
            if acknowledged {
                (MultiInstanceState::Acknowledged, Some(value), true)
            } else {
                (MultiInstanceState::AttentionNeeded, Some(value), false)
            }
        }
    }
}

fn stable_hash64(input: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub fn preferred_system_manager_display_path(manager: ManagerId) -> Option<&'static str> {
    match manager {
        ManagerId::SoftwareUpdate => Some("/usr/sbin/softwareupdate"),
        ManagerId::XcodeCommandLineTools => {
            Some("/Library/Developer/CommandLineTools/usr/bin/clang")
        }
        ManagerId::Rosetta2 | ManagerId::FirmwareUpdates => Some("/usr/sbin/softwareupdate"),
        _ => None,
    }
}

fn system_manager_instance_rank(manager: ManagerId, instance: &ManagerInstallInstance) -> (u8, u8) {
    let has_preferred = preferred_system_manager_display_path(manager)
        .map(Path::new)
        .is_some_and(|preferred| instance_contains_path(instance, preferred));
    (u8::from(has_preferred), u8::from(instance.is_active))
}

fn is_trusted_system_manager_instance(
    manager: ManagerId,
    instance: &ManagerInstallInstance,
) -> bool {
    instance_paths(instance).any(|path| trusted_system_manager_path(manager, path))
}

fn instance_contains_path(instance: &ManagerInstallInstance, expected: &Path) -> bool {
    instance_paths(instance).any(|path| path == expected)
}

fn instance_paths(instance: &ManagerInstallInstance) -> impl Iterator<Item = &Path> {
    std::iter::once(instance.display_path.as_path())
        .chain(instance.canonical_path.iter().map(PathBuf::as_path))
        .chain(instance.alias_paths.iter().map(PathBuf::as_path))
}

pub fn trusted_system_manager_path(manager: ManagerId, path: &Path) -> bool {
    let rendered = path.to_string_lossy().to_ascii_lowercase();
    match manager {
        ManagerId::SoftwareUpdate => {
            rendered == "/usr/sbin/softwareupdate" || rendered == "/usr/bin/sw_vers"
        }
        ManagerId::XcodeCommandLineTools => {
            rendered == "/usr/bin/xcode-select"
                || rendered == "/library/developer/commandlinetools/usr/bin/clang"
        }
        ManagerId::Rosetta2 | ManagerId::FirmwareUpdates => rendered == "/usr/sbin/softwareupdate",
        _ => false,
    }
}

pub fn canonicalize_system_manager_instance(
    instance: ManagerInstallInstance,
) -> ManagerInstallInstance {
    let Some(preferred_path) = preferred_system_manager_display_path(instance.manager) else {
        return instance;
    };
    if !is_trusted_system_manager_instance(instance.manager, &instance) {
        return instance;
    }

    let mut normalized = instance;
    let preferred_path = PathBuf::from(preferred_path);
    let mut alias_paths = BTreeSet::new();
    alias_paths.insert(normalized.display_path.clone());
    if let Some(path) = normalized.canonical_path.clone() {
        alias_paths.insert(path);
    }
    for path in &normalized.alias_paths {
        alias_paths.insert(path.clone());
    }
    alias_paths.insert(preferred_path.clone());

    normalized.display_path = preferred_path.clone();
    normalized.canonical_path = Some(preferred_path);
    normalized.alias_paths = alias_paths.into_iter().collect();
    normalized.provenance = InstallProvenance::System;
    normalized.confidence = 0.99;
    normalized.decision_margin = Some(0.60);
    normalized.automation_level = AutomationLevel::ReadOnly;
    normalized.uninstall_strategy = StrategyKind::ReadOnly;
    normalized.update_strategy = StrategyKind::ReadOnly;
    normalized.remediation_strategy = StrategyKind::ReadOnly;
    normalized.explanation_primary = Some(system_manager_explanation_primary(normalized.manager));
    normalized.explanation_secondary = None;
    normalized.competing_provenance = Some(InstallProvenance::Unknown);
    normalized.competing_confidence = Some(0.30);
    normalized
}

fn system_manager_explanation_primary(manager: ManagerId) -> String {
    match manager {
        ManagerId::XcodeCommandLineTools => {
            "xcode_command_line_tools executable path is in an OS-managed system location"
                .to_string()
        }
        ManagerId::SoftwareUpdate => {
            "softwareupdate executable path is in an OS-managed system prefix".to_string()
        }
        ManagerId::Rosetta2 => {
            "rosetta2 executable path is in an OS-managed system prefix".to_string()
        }
        ManagerId::FirmwareUpdates => {
            "firmware_updates executable path is in an OS-managed system prefix".to_string()
        }
        _ => format!(
            "{} executable path is in an OS-managed system prefix",
            manager.as_str()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MultiInstanceState, instance_ids_fingerprint, normalize_manager_install_instances,
        resolve_multi_instance_state,
    };
    use crate::models::{
        AutomationLevel, InstallInstanceIdentityKind, InstallProvenance, ManagerId,
        ManagerInstallInstance, StrategyKind,
    };
    use std::path::PathBuf;

    #[test]
    fn fingerprint_is_order_independent() {
        let first =
            instance_ids_fingerprint(["rustup-homebrew", "rustup-user"]).expect("fingerprint");
        let second =
            instance_ids_fingerprint(["rustup-user", "rustup-homebrew"]).expect("fingerprint");
        assert_eq!(first, second);
    }

    #[test]
    fn fingerprint_requires_multiple_unique_instances() {
        assert_eq!(instance_ids_fingerprint(["only-one"]), None);
        assert_eq!(instance_ids_fingerprint(["same", "same"]), None);
    }

    #[test]
    fn resolve_state_defaults_to_attention_when_unacknowledged() {
        let (state, fingerprint, acknowledged) = resolve_multi_instance_state(["a", "b"], None);
        assert_eq!(state, MultiInstanceState::AttentionNeeded);
        assert!(fingerprint.is_some());
        assert!(!acknowledged);
    }

    #[test]
    fn resolve_state_is_acknowledged_when_fingerprint_matches() {
        let fingerprint = instance_ids_fingerprint(["a", "b"]).expect("fingerprint");
        let (state, resolved, acknowledged) =
            resolve_multi_instance_state(["b", "a"], Some(fingerprint.as_str()));
        assert_eq!(state, MultiInstanceState::Acknowledged);
        assert_eq!(resolved.as_deref(), Some(fingerprint.as_str()));
        assert!(acknowledged);
    }

    #[test]
    fn resolve_state_returns_none_for_single_instance() {
        let (state, fingerprint, acknowledged) =
            resolve_multi_instance_state(["only"], Some("abc"));
        assert_eq!(state, MultiInstanceState::None);
        assert!(fingerprint.is_none());
        assert!(!acknowledged);
    }

    #[test]
    fn normalizes_softwareupdate_helper_paths_into_single_instance() {
        let normalized = normalize_manager_install_instances(&[
            sample_instance(
                ManagerId::SoftwareUpdate,
                "sw-vers",
                "/usr/bin/sw_vers",
                true,
                0.30,
            ),
            sample_instance(
                ManagerId::SoftwareUpdate,
                "softwareupdate",
                "/usr/sbin/softwareupdate",
                false,
                0.99,
            ),
        ]);

        assert_eq!(normalized.len(), 1);
        assert_eq!(
            normalized[0].display_path,
            PathBuf::from("/usr/sbin/softwareupdate")
        );
        assert!(normalized[0].is_active);
        assert_eq!(normalized[0].provenance, InstallProvenance::System);
        assert_eq!(normalized[0].automation_level, AutomationLevel::ReadOnly);
        assert!(
            normalized[0]
                .alias_paths
                .iter()
                .any(|path| path == &PathBuf::from("/usr/bin/sw_vers"))
        );
    }

    #[test]
    fn normalizes_xcode_clt_helper_paths_into_single_instance() {
        let normalized = normalize_manager_install_instances(&[
            sample_instance(
                ManagerId::XcodeCommandLineTools,
                "clang",
                "/Library/Developer/CommandLineTools/usr/bin/clang",
                true,
                0.30,
            ),
            sample_instance(
                ManagerId::XcodeCommandLineTools,
                "xcode-select",
                "/usr/bin/xcode-select",
                false,
                0.99,
            ),
        ]);

        assert_eq!(normalized.len(), 1);
        assert_eq!(
            normalized[0].display_path,
            PathBuf::from("/Library/Developer/CommandLineTools/usr/bin/clang")
        );
        assert!(normalized[0].is_active);
        assert_eq!(normalized[0].provenance, InstallProvenance::System);
        assert_eq!(
            normalized[0].explanation_primary.as_deref(),
            Some("xcode_command_line_tools executable path is in an OS-managed system location")
        );
        assert!(
            normalized[0]
                .alias_paths
                .iter()
                .any(|path| path == &PathBuf::from("/usr/bin/xcode-select"))
        );
    }

    fn sample_instance(
        manager: ManagerId,
        instance_id: &str,
        path: &str,
        is_active: bool,
        confidence: f64,
    ) -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager,
            instance_id: instance_id.to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: path.to_string(),
            display_path: PathBuf::from(path),
            canonical_path: Some(PathBuf::from(path)),
            alias_paths: vec![PathBuf::from(path)],
            is_active,
            version: None,
            provenance: InstallProvenance::Unknown,
            confidence,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::ReadOnly,
            update_strategy: StrategyKind::ReadOnly,
            remediation_strategy: StrategyKind::ReadOnly,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        }
    }
}
