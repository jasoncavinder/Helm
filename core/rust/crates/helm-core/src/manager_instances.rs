use crate::models::{
    AutomationLevel, InstallProvenance, ManagerId, ManagerInstallInstance, StrategyKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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
        let normalized = instances
            .iter()
            .cloned()
            .map(canonicalize_system_manager_instance)
            .collect::<Vec<_>>();
        return normalize_runtime_dependency_instances(normalized);
    }

    let manager = instances[0].manager;
    let Some(preferred_display_path) =
        preferred_system_manager_display_path(manager).map(PathBuf::from)
    else {
        let normalized = instances
            .iter()
            .cloned()
            .map(canonicalize_system_manager_instance)
            .collect::<Vec<_>>();
        return normalize_runtime_dependency_instances(normalized);
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
        return normalize_runtime_dependency_instances(normalized);
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
    normalize_runtime_dependency_instances(passthrough)
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

fn normalize_runtime_dependency_instances(
    instances: Vec<ManagerInstallInstance>,
) -> Vec<ManagerInstallInstance> {
    collapse_runtime_dependency_entrypoints(collapse_runtime_dependency_shims(instances))
}

fn collapse_runtime_dependency_shims(
    instances: Vec<ManagerInstallInstance>,
) -> Vec<ManagerInstallInstance> {
    let Some(executable_names) = runtime_dependency_shim_executable_names(
        instances.first().map(|instance| instance.manager),
    ) else {
        return instances;
    };

    let mut concrete = Vec::new();
    let mut shims = Vec::new();
    for instance in instances {
        if is_runtime_dependency_shim_instance(&instance, executable_names) {
            shims.push(instance);
        } else {
            concrete.push(instance);
        }
    }

    if shims.is_empty() || concrete.is_empty() {
        let mut combined = concrete;
        combined.extend(shims);
        return combined;
    }

    for shim in shims {
        let target_index = concrete
            .iter()
            .position(|instance| instance.is_active && instance.provenance == shim.provenance)
            .or_else(|| {
                concrete
                    .iter()
                    .position(|instance| instance.provenance == shim.provenance)
            })
            .or_else(|| (concrete.len() == 1).then_some(0));

        let Some(target_index) = target_index else {
            concrete.push(shim);
            continue;
        };

        let target = &mut concrete[target_index];
        let mut alias_paths = BTreeSet::new();
        alias_paths.insert(target.display_path.clone());
        if let Some(path) = target.canonical_path.clone() {
            alias_paths.insert(path);
        }
        for path in &target.alias_paths {
            alias_paths.insert(path.clone());
        }
        alias_paths.insert(shim.display_path.clone());
        for path in &shim.alias_paths {
            alias_paths.insert(path.clone());
        }

        target.alias_paths = alias_paths.into_iter().collect();
        target.is_active = target.is_active || shim.is_active;
    }

    concrete.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    concrete
}

fn collapse_runtime_dependency_entrypoints(
    instances: Vec<ManagerInstallInstance>,
) -> Vec<ManagerInstallInstance> {
    let Some(executable_names) =
        runtime_dependency_entrypoint_names(instances.first().map(|instance| instance.manager))
    else {
        return instances;
    };

    let mut grouped: BTreeMap<String, Vec<ManagerInstallInstance>> = BTreeMap::new();
    let mut passthrough = Vec::new();
    for instance in instances {
        let Some(key) = runtime_dependency_install_root_key(&instance, executable_names) else {
            passthrough.push(instance);
            continue;
        };
        grouped.entry(key).or_default().push(instance);
    }

    for group in grouped.into_values() {
        if group.len() == 1 {
            passthrough.extend(group);
            continue;
        }
        passthrough.push(merge_runtime_dependency_entrypoint_group(
            group,
            executable_names,
        ));
    }

    passthrough.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    passthrough
}

fn merge_runtime_dependency_entrypoint_group(
    mut group: Vec<ManagerInstallInstance>,
    executable_names: &[&str],
) -> ManagerInstallInstance {
    group.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| {
                runtime_dependency_entrypoint_rank(left, executable_names)
                    .cmp(&runtime_dependency_entrypoint_rank(right, executable_names))
            })
            .then_with(|| {
                right
                    .confidence
                    .partial_cmp(&left.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });

    let mut merged = group
        .first()
        .cloned()
        .expect("runtime dependency entrypoint group is non-empty");
    let mut alias_paths = BTreeSet::new();
    for instance in &group {
        alias_paths.insert(instance.display_path.clone());
        if let Some(path) = instance.canonical_path.clone() {
            alias_paths.insert(path);
        }
        for path in &instance.alias_paths {
            alias_paths.insert(path.clone());
        }
    }

    merged.alias_paths = alias_paths.into_iter().collect();
    merged.is_active = group.iter().any(|instance| instance.is_active);
    if merged.canonical_path.is_none() {
        merged.canonical_path = group
            .iter()
            .find_map(|instance| instance.canonical_path.clone());
    }
    if merged.version.is_none() {
        merged.version = group.iter().find_map(|instance| instance.version.clone());
    }
    merged
}

fn runtime_dependency_shim_executable_names(
    manager: Option<ManagerId>,
) -> Option<&'static [&'static str]> {
    match manager? {
        ManagerId::Rustup => Some(&["rustup"]),
        ManagerId::Npm => Some(&["npm"]),
        ManagerId::Pnpm => Some(&["pnpm"]),
        ManagerId::Yarn => Some(&["yarn"]),
        ManagerId::Pip => Some(&["python3", "pip3", "pip"]),
        ManagerId::Pipx => Some(&["pipx"]),
        ManagerId::Poetry => Some(&["poetry"]),
        ManagerId::RubyGems => Some(&["gem"]),
        ManagerId::Bundler => Some(&["bundle"]),
        ManagerId::Cargo => Some(&["cargo"]),
        ManagerId::CargoBinstall => Some(&["cargo-binstall"]),
        _ => None,
    }
}

fn runtime_dependency_entrypoint_names(
    manager: Option<ManagerId>,
) -> Option<&'static [&'static str]> {
    match manager? {
        ManagerId::Pip => Some(&["pip3", "pip", "python3"]),
        _ => None,
    }
}

fn is_runtime_dependency_shim_instance(
    instance: &ManagerInstallInstance,
    executable_names: &[&str],
) -> bool {
    matches!(
        instance.provenance,
        InstallProvenance::Asdf | InstallProvenance::Mise
    ) && std::iter::once(instance.display_path.as_path())
        .chain(instance.canonical_path.iter().map(PathBuf::as_path))
        .any(|path| {
            executable_names
                .iter()
                .any(|name| is_runtime_dependency_shim_path(path, name))
        })
}

fn is_runtime_dependency_shim_path(path: &Path, executable_name: &str) -> bool {
    let rendered = path.to_string_lossy().to_ascii_lowercase();
    let suffix = format!("/shims/{}", executable_name.to_ascii_lowercase());
    rendered.ends_with(suffix.as_str())
        && (rendered.contains("/.local/share/mise/shims/")
            || rendered.contains("/.local/share/rtx/shims/")
            || rendered.contains("/.asdf/shims/"))
}

fn runtime_dependency_install_root_key(
    instance: &ManagerInstallInstance,
    executable_names: &[&str],
) -> Option<String> {
    instance
        .canonical_path
        .as_deref()
        .and_then(|path| runtime_dependency_install_root(path, executable_names))
        .or_else(|| {
            runtime_dependency_install_root(instance.display_path.as_path(), executable_names)
        })
        .or_else(|| {
            instance
                .alias_paths
                .iter()
                .find_map(|path| runtime_dependency_install_root(path.as_path(), executable_names))
        })
        .map(|path| path.to_string_lossy().to_string())
}

fn runtime_dependency_install_root(path: &Path, executable_names: &[&str]) -> Option<PathBuf> {
    let basename = path.file_name().and_then(|value| value.to_str())?;
    if !executable_names
        .iter()
        .any(|name| runtime_dependency_entrypoint_name_matches(basename, name))
    {
        return None;
    }

    let bin_dir = path.parent()?;
    if !bin_dir
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("bin"))
    {
        return None;
    }

    Some(
        bin_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| bin_dir.to_path_buf()),
    )
}

fn runtime_dependency_entrypoint_name_matches(basename: &str, executable_name: &str) -> bool {
    basename == executable_name
        || basename
            .strip_prefix(executable_name)
            .is_some_and(|suffix| {
                suffix.starts_with('.')
                    || suffix
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_digit())
            })
}

fn runtime_dependency_entrypoint_rank(
    instance: &ManagerInstallInstance,
    executable_names: &[&str],
) -> usize {
    let basename = instance
        .display_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    executable_names
        .iter()
        .position(|name| runtime_dependency_entrypoint_name_matches(basename, name))
        .unwrap_or(executable_names.len())
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

    #[test]
    fn normalizes_runtime_dependency_shims_out_of_visible_instances() {
        let mut shim = sample_instance(
            ManagerId::Npm,
            "npm-shim",
            "/Users/test/.local/share/mise/shims/npm",
            true,
            0.92,
        );
        shim.provenance = InstallProvenance::Mise;
        shim.automation_level = AutomationLevel::NeedsConfirmation;
        shim.uninstall_strategy = StrategyKind::InteractivePrompt;
        shim.update_strategy = StrategyKind::InteractivePrompt;
        shim.remediation_strategy = StrategyKind::InteractivePrompt;

        let mut node26 = sample_instance(
            ManagerId::Npm,
            "npm-node26",
            "/Users/test/.local/share/mise/installs/node/26/bin/npm",
            false,
            0.92,
        );
        node26.provenance = InstallProvenance::Mise;
        node26.alias_paths.push(PathBuf::from(
            "/Users/test/.local/share/mise/installs/node/latest/bin/npm",
        ));

        let mut node24 = sample_instance(
            ManagerId::Npm,
            "npm-node24",
            "/Users/test/.local/share/mise/installs/node/lts-krypton/bin/npm",
            false,
            0.92,
        );
        node24.provenance = InstallProvenance::Mise;
        node24.alias_paths.push(PathBuf::from(
            "/Users/test/.local/share/mise/installs/node/24/bin/npm",
        ));

        let normalized = normalize_manager_install_instances(&[shim, node26, node24]);
        assert_eq!(normalized.len(), 2);
        assert!(
            normalized
                .iter()
                .all(|instance| { !instance.display_path.to_string_lossy().contains("/shims/") })
        );

        let active = normalized
            .iter()
            .find(|instance| instance.instance_id == "npm-node26")
            .expect("node 26 install should remain visible");
        assert!(active.is_active);
        assert!(
            active
                .alias_paths
                .iter()
                .any(|path| { path == &PathBuf::from("/Users/test/.local/share/mise/shims/npm") })
        );
    }

    #[test]
    fn normalizes_pip_runtime_entrypoints_into_single_visible_instance() {
        let mut python = sample_instance(
            ManagerId::Pip,
            "pip-python",
            "/Users/test/.local/share/mise/installs/python/3.13.5/bin/python3",
            true,
            0.92,
        );
        python.provenance = InstallProvenance::Mise;

        let mut pip3 = sample_instance(
            ManagerId::Pip,
            "pip-pip3",
            "/Users/test/.local/share/mise/installs/python/3.13.5/bin/pip3",
            false,
            0.92,
        );
        pip3.provenance = InstallProvenance::Mise;

        let mut pip = sample_instance(
            ManagerId::Pip,
            "pip-pip",
            "/Users/test/.local/share/mise/installs/python/3.13.5/bin/pip",
            false,
            0.92,
        );
        pip.provenance = InstallProvenance::Mise;

        let normalized = normalize_manager_install_instances(&[python, pip3, pip]);
        assert_eq!(normalized.len(), 1);
        assert_eq!(
            normalized[0].display_path,
            PathBuf::from("/Users/test/.local/share/mise/installs/python/3.13.5/bin/python3")
        );
        assert!(normalized[0].is_active);
        assert!(normalized[0].alias_paths.iter().any(|path| path
            == &PathBuf::from("/Users/test/.local/share/mise/installs/python/3.13.5/bin/pip3")));
        assert!(normalized[0].alias_paths.iter().any(|path| path
            == &PathBuf::from("/Users/test/.local/share/mise/installs/python/3.13.5/bin/pip")));
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
