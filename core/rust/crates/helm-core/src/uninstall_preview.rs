use crate::adapters::AdapterRequest;
use crate::manager_lifecycle::{
    HomebrewUninstallCleanupMode, parse_homebrew_manager_uninstall_package_name,
    strip_shell_setup_cleanup_suffix,
};
use crate::models::{
    AutomationLevel, InstallProvenance, ManagerId, ManagerInstallInstance, ManagerUninstallPreview,
    PackageRef, PackageRuntimeState, PackageUninstallPreview, StrategyKind, UninstallImpactPath,
};
use crate::persistence::PackageStore;
use crate::sqlite::SqliteStore;
use std::collections::HashSet;
use std::path::PathBuf;

pub const DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD: u32 = 4;

#[derive(Debug, Clone, Copy)]
pub struct ManagerUninstallPreviewContext<'a> {
    pub requested_manager: ManagerId,
    pub target_manager: ManagerId,
    pub request: &'a AdapterRequest,
    pub strategy: StrategyKind,
    pub active_instance: Option<&'a ManagerInstallInstance>,
    pub unknown_override_required: bool,
    pub used_unknown_override: bool,
    pub legacy_fallback_used: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PackageUninstallPreviewContext<'a> {
    pub package: &'a PackageRef,
    pub active_instance: Option<&'a ManagerInstallInstance>,
    pub package_runtime_state: Option<&'a PackageRuntimeState>,
    pub rustup_override_paths: &'a [String],
}

struct UninstallSummaryContext<'a> {
    target_manager: ManagerId,
    strategy: StrategyKind,
    active_instance: Option<&'a ManagerInstallInstance>,
    blast_radius_score: u32,
    unknown_override_required: bool,
    used_unknown_override: bool,
    read_only_blocked: bool,
    files_count: usize,
    directories_count: usize,
    secondary_effect_count: usize,
}

struct PackageUninstallSummaryContext<'a> {
    package: &'a PackageRef,
    blast_radius_score: u32,
    files_count: usize,
    directories_count: usize,
    secondary_effect_count: usize,
    package_runtime_state: Option<&'a PackageRuntimeState>,
    rustup_override_count: usize,
}

pub fn build_manager_uninstall_preview(
    store: &SqliteStore,
    context: ManagerUninstallPreviewContext<'_>,
    safe_blast_radius_threshold: u32,
) -> ManagerUninstallPreview {
    let mut files_removed = Vec::new();
    let mut directories_removed = Vec::new();
    let mut secondary_effects = Vec::new();
    let mut seen_files = HashSet::new();
    let mut seen_directories = HashSet::new();
    let homebrew_uninstall_spec = uninstall_request_package_name(context.request)
        .and_then(parse_homebrew_manager_uninstall_package_name);
    let self_request_shell_cleanup_requested = uninstall_request_package_name(context.request)
        .map(strip_shell_setup_cleanup_suffix)
        .map(|(_, remove)| remove)
        .unwrap_or(false);

    if context.target_manager == ManagerId::Rustup || context.strategy == StrategyKind::RustupSelf {
        append_rustup_uninstall_impact(
            store,
            &mut files_removed,
            &mut directories_removed,
            &mut secondary_effects,
            &mut seen_files,
            &mut seen_directories,
            context.active_instance,
            self_request_shell_cleanup_requested,
        );
    } else if context.target_manager == ManagerId::Mise {
        append_mise_uninstall_impact(
            &mut files_removed,
            &mut directories_removed,
            &mut secondary_effects,
            &mut seen_files,
            &mut seen_directories,
            context.active_instance,
            uninstall_request_package_name(context.request),
        );
    } else if context.target_manager == ManagerId::Asdf
        || context.strategy == StrategyKind::AsdfSelf
    {
        append_asdf_uninstall_impact(
            &mut files_removed,
            &mut directories_removed,
            &mut secondary_effects,
            &mut seen_files,
            &mut seen_directories,
            context.active_instance,
            self_request_shell_cleanup_requested,
        );
    } else if context.target_manager == ManagerId::HomebrewFormula {
        let formula_name = homebrew_uninstall_spec
            .as_ref()
            .map(|spec| spec.formula_name.as_str())
            .or_else(|| uninstall_request_package_name(context.request))
            .unwrap_or("unknown");
        if let Some(path) = context
            .active_instance
            .map(|instance| instance.display_path.as_path())
            .filter(|path| path.exists())
        {
            push_impact_path(&mut files_removed, &mut seen_files, path.to_path_buf());
        }
        secondary_effects.push(format!(
            "Homebrew formula '{}' will be uninstalled.",
            formula_name
        ));
        if context.requested_manager == ManagerId::Rustup
            && homebrew_uninstall_spec
                .as_ref()
                .map(|spec| spec.cleanup_mode)
                .unwrap_or(HomebrewUninstallCleanupMode::ManagerOnly)
                == HomebrewUninstallCleanupMode::ManagerOnly
        {
            secondary_effects.push(format!(
                "Manager-only uninstall keeps rustup state/toolchains under '{}'.",
                resolve_rustup_home().display()
            ));
        }
        if let Some(spec) = homebrew_uninstall_spec.as_ref()
            && spec.cleanup_mode == HomebrewUninstallCleanupMode::FullCleanup
        {
            append_homebrew_full_cleanup_impact(
                &mut files_removed,
                &mut directories_removed,
                &mut secondary_effects,
                &mut seen_files,
                &mut seen_directories,
                spec.requested_manager,
            );
        }
        if let Some(spec) = homebrew_uninstall_spec.as_ref()
            && spec.remove_helm_managed_shell_setup
        {
            append_shell_setup_cleanup_impact(
                spec.requested_manager,
                &mut files_removed,
                &mut secondary_effects,
                &mut seen_files,
            );
        }
    } else if context.target_manager == ManagerId::MacPorts {
        secondary_effects.push(format!(
            "MacPorts port '{}' will be uninstalled.",
            uninstall_request_package_name(context.request).unwrap_or("unknown")
        ));
    }

    if files_removed.is_empty() && directories_removed.is_empty() && secondary_effects.is_empty() {
        secondary_effects.push("No blast-radius details were discovered for this strategy.".into());
    }

    let automation_level = context
        .active_instance
        .map(|instance| instance.automation_level);
    let unknown_provenance = context
        .active_instance
        .map(|instance| instance.provenance == InstallProvenance::Unknown)
        .unwrap_or(true);
    let read_only_blocked = context.strategy == StrategyKind::ReadOnly
        || matches!(automation_level, Some(AutomationLevel::ReadOnly));
    let confidence_requires_confirmation = matches!(
        automation_level,
        Some(AutomationLevel::NeedsConfirmation | AutomationLevel::ReadOnly)
    ) || unknown_provenance;
    let blast_radius_score = compute_uninstall_blast_radius_score(
        files_removed.len(),
        directories_removed.len(),
        secondary_effects.len(),
        confidence_requires_confirmation,
        context.used_unknown_override,
        context.legacy_fallback_used,
    );
    let requires_yes = blast_radius_score >= safe_blast_radius_threshold
        || confidence_requires_confirmation
        || context.unknown_override_required
        || context.used_unknown_override
        || read_only_blocked;
    let summary_lines = build_manager_uninstall_summary_lines(UninstallSummaryContext {
        target_manager: context.target_manager,
        strategy: context.strategy,
        active_instance: context.active_instance,
        blast_radius_score,
        unknown_override_required: context.unknown_override_required,
        used_unknown_override: context.used_unknown_override,
        read_only_blocked,
        files_count: files_removed.len(),
        directories_count: directories_removed.len(),
        secondary_effect_count: secondary_effects.len(),
    });

    ManagerUninstallPreview {
        requested_manager_id: context.requested_manager.as_str().to_string(),
        target_manager_id: context.target_manager.as_str().to_string(),
        package_name: uninstall_request_package_name(context.request)
            .unwrap_or("unknown")
            .to_string(),
        strategy: context.strategy.as_str().to_string(),
        provenance: context
            .active_instance
            .map(|instance| instance.provenance.as_str().to_string()),
        automation_level: context
            .active_instance
            .map(|instance| instance.automation_level.as_str().to_string()),
        confidence: context.active_instance.map(|instance| instance.confidence),
        decision_margin: context
            .active_instance
            .and_then(|instance| instance.decision_margin),
        explanation_primary: context
            .active_instance
            .and_then(|instance| normalize_nonempty(instance.explanation_primary.clone())),
        explanation_secondary: context
            .active_instance
            .and_then(|instance| normalize_nonempty(instance.explanation_secondary.clone())),
        competing_provenance: context.active_instance.and_then(|instance| {
            instance
                .competing_provenance
                .map(|value| value.as_str().to_string())
        }),
        competing_confidence: context
            .active_instance
            .and_then(|instance| instance.competing_confidence),
        files_removed,
        directories_removed,
        secondary_effects,
        summary_lines,
        blast_radius_score,
        requires_yes,
        confidence_requires_confirmation,
        unknown_provenance,
        unknown_override_required: context.unknown_override_required,
        used_unknown_override: context.used_unknown_override,
        legacy_fallback_used: context.legacy_fallback_used,
        read_only_blocked,
    }
}

pub fn build_package_uninstall_preview(
    context: PackageUninstallPreviewContext<'_>,
    safe_blast_radius_threshold: u32,
) -> PackageUninstallPreview {
    let files_removed = Vec::new();
    let mut directories_removed = Vec::new();
    let mut secondary_effects = Vec::new();
    let mut seen_directories = HashSet::new();

    if context.package.manager == ManagerId::Rustup {
        let rustup_home = resolve_rustup_home();
        let toolchain_dir = rustup_home.join("toolchains").join(&context.package.name);
        push_impact_path(
            &mut directories_removed,
            &mut seen_directories,
            toolchain_dir,
        );
        secondary_effects.push(format!(
            "Rustup toolchain '{}' will be removed.",
            context.package.name
        ));
        if context
            .package_runtime_state
            .is_some_and(|state| state.is_active)
        {
            secondary_effects.push(format!(
                "Rustup toolchain '{}' is currently active for at least one shell context.",
                context.package.name
            ));
        }
        if context
            .package_runtime_state
            .is_some_and(|state| state.is_default)
        {
            secondary_effects.push(format!(
                "Rustup toolchain '{}' is configured as the default toolchain.",
                context.package.name
            ));
        }
        if !context.rustup_override_paths.is_empty() {
            let sample = context
                .rustup_override_paths
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>();
            let remaining = context
                .rustup_override_paths
                .len()
                .saturating_sub(sample.len());
            let suffix = if remaining == 0 {
                String::new()
            } else {
                format!(" (+{} more)", remaining)
            };
            secondary_effects.push(format!(
                "{} directory override(s) currently select this toolchain: {}{}",
                context.rustup_override_paths.len(),
                sample.join(", "),
                suffix
            ));
        } else if context
            .package_runtime_state
            .is_some_and(|state| state.has_override)
        {
            secondary_effects.push(format!(
                "One or more directory overrides currently select rustup toolchain '{}'.",
                context.package.name
            ));
        }
    } else if context.package.manager == ManagerId::HomebrewFormula {
        secondary_effects.push(format!(
            "Homebrew formula '{}' will be uninstalled.",
            context.package.name
        ));
        secondary_effects
            .push("Homebrew may report dependency and cleanup follow-up actions.".to_string());
    } else {
        secondary_effects.push(format!(
            "Package '{}' will be uninstalled via manager '{}'.",
            context.package.name,
            context.package.manager.as_str()
        ));
    }

    let confidence_requires_confirmation = context.package.manager == ManagerId::Rustup
        && (context
            .package_runtime_state
            .is_some_and(|state| state.is_active || state.is_default || state.has_override)
            || !context.rustup_override_paths.is_empty());
    let blast_radius_score = compute_uninstall_blast_radius_score_with_base_risk(
        files_removed.len(),
        directories_removed.len(),
        secondary_effects.len(),
        confidence_requires_confirmation,
        false,
        false,
        package_uninstall_base_risk(context.package.manager),
    );
    let requires_yes =
        blast_radius_score >= safe_blast_radius_threshold || confidence_requires_confirmation;
    let summary_lines = build_package_uninstall_summary_lines(PackageUninstallSummaryContext {
        package: context.package,
        blast_radius_score,
        files_count: files_removed.len(),
        directories_count: directories_removed.len(),
        secondary_effect_count: secondary_effects.len(),
        package_runtime_state: context.package_runtime_state,
        rustup_override_count: context.rustup_override_paths.len(),
    });

    PackageUninstallPreview {
        manager_id: context.package.manager.as_str().to_string(),
        package_name: context.package.name.clone(),
        files_removed,
        directories_removed,
        secondary_effects,
        summary_lines,
        blast_radius_score,
        requires_yes,
        confidence_requires_confirmation,
        manager_provenance: context
            .active_instance
            .map(|instance| instance.provenance.as_str().to_string()),
        manager_automation_level: context
            .active_instance
            .map(|instance| instance.automation_level.as_str().to_string()),
        manager_uninstall_strategy: context
            .active_instance
            .map(|instance| instance.uninstall_strategy.as_str().to_string()),
        explanation_primary: context
            .active_instance
            .and_then(|instance| normalize_nonempty(instance.explanation_primary.clone())),
        explanation_secondary: context
            .active_instance
            .and_then(|instance| normalize_nonempty(instance.explanation_secondary.clone())),
        competing_provenance: context.active_instance.and_then(|instance| {
            instance
                .competing_provenance
                .map(|value| value.as_str().to_string())
        }),
        competing_confidence: context
            .active_instance
            .and_then(|instance| instance.competing_confidence),
    }
}

fn uninstall_request_package_name(request: &AdapterRequest) -> Option<&str> {
    match request {
        AdapterRequest::Uninstall(uninstall) => Some(uninstall.package.name.as_str()),
        _ => None,
    }
}

fn append_asdf_uninstall_impact(
    files_removed: &mut Vec<UninstallImpactPath>,
    directories_removed: &mut Vec<UninstallImpactPath>,
    secondary_effects: &mut Vec<String>,
    seen_files: &mut HashSet<String>,
    seen_directories: &mut HashSet<String>,
    active_instance: Option<&ManagerInstallInstance>,
    remove_shell_setup: bool,
) {
    if let Some(instance) = active_instance {
        push_impact_path(files_removed, seen_files, instance.display_path.clone());
        if let Some(path) = instance.canonical_path.clone() {
            push_impact_path(files_removed, seen_files, path);
        }
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"));
    let asdf_root = home.join(".asdf");
    push_impact_path(directories_removed, seen_directories, asdf_root.clone());

    secondary_effects.push(format!(
        "asdf install root '{}' may be removed.",
        asdf_root.display()
    ));
    secondary_effects.push(
        "asdf-managed plugins and tool versions under .asdf may also be removed.".to_string(),
    );
    if remove_shell_setup {
        append_shell_setup_cleanup_impact(
            ManagerId::Asdf,
            files_removed,
            secondary_effects,
            seen_files,
        );
    } else {
        secondary_effects
            .push("Shell profile initialization lines are not automatically removed.".to_string());
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MiseUninstallPreviewMode {
    ManagerOnlyKeep,
    FullCleanupKeep,
    FullCleanupRemove,
}

fn parse_mise_uninstall_preview_mode(
    package_name: Option<&str>,
) -> (MiseUninstallPreviewMode, bool) {
    let (base, remove_shell_setup) =
        strip_shell_setup_cleanup_suffix(package_name.unwrap_or("__self__"));
    let mode = match base.trim() {
        "__self__:fullCleanup:keepConfig" => MiseUninstallPreviewMode::FullCleanupKeep,
        "__self__:fullCleanup:removeConfig" => MiseUninstallPreviewMode::FullCleanupRemove,
        _ => MiseUninstallPreviewMode::ManagerOnlyKeep,
    };
    (mode, remove_shell_setup)
}

fn append_mise_uninstall_impact(
    files_removed: &mut Vec<UninstallImpactPath>,
    directories_removed: &mut Vec<UninstallImpactPath>,
    secondary_effects: &mut Vec<String>,
    seen_files: &mut HashSet<String>,
    seen_directories: &mut HashSet<String>,
    active_instance: Option<&ManagerInstallInstance>,
    package_name: Option<&str>,
) {
    if let Some(instance) = active_instance {
        push_impact_path(files_removed, seen_files, instance.display_path.clone());
        if let Some(path) = instance.canonical_path.clone() {
            push_impact_path(files_removed, seen_files, path);
        }
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"));
    let mise_state_dir = home.join(".local/share/mise");
    let mise_cache_dir = home.join(".cache/mise");
    let mise_config_dir = home.join(".config/mise");

    let (mode, remove_shell_setup) = parse_mise_uninstall_preview_mode(package_name);
    match mode {
        MiseUninstallPreviewMode::ManagerOnlyKeep => {
            secondary_effects.push(
                "Manager-only uninstall keeps mise tool installs, cache, and config.".to_string(),
            );
        }
        MiseUninstallPreviewMode::FullCleanupKeep => {
            push_impact_path(
                directories_removed,
                seen_directories,
                mise_state_dir.clone(),
            );
            push_impact_path(
                directories_removed,
                seen_directories,
                mise_cache_dir.clone(),
            );
            secondary_effects.push(format!(
                "Mise state and cache directories under '{}' may be removed.",
                home.display()
            ));
            secondary_effects
                .push("Mise config files are preserved in this uninstall mode.".to_string());
        }
        MiseUninstallPreviewMode::FullCleanupRemove => {
            push_impact_path(
                directories_removed,
                seen_directories,
                mise_state_dir.clone(),
            );
            push_impact_path(
                directories_removed,
                seen_directories,
                mise_cache_dir.clone(),
            );
            push_impact_path(
                directories_removed,
                seen_directories,
                mise_config_dir.clone(),
            );
            secondary_effects.push(format!(
                "Mise state, cache, and config directories under '{}' may be removed.",
                home.display()
            ));
        }
    }
    if remove_shell_setup {
        append_shell_setup_cleanup_impact(
            ManagerId::Mise,
            files_removed,
            secondary_effects,
            seen_files,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn append_rustup_uninstall_impact(
    store: &SqliteStore,
    files_removed: &mut Vec<UninstallImpactPath>,
    directories_removed: &mut Vec<UninstallImpactPath>,
    secondary_effects: &mut Vec<String>,
    seen_files: &mut HashSet<String>,
    seen_directories: &mut HashSet<String>,
    active_instance: Option<&ManagerInstallInstance>,
    remove_shell_setup: bool,
) {
    if let Some(instance) = active_instance {
        push_impact_path(files_removed, seen_files, instance.display_path.clone());
        if let Some(path) = instance.canonical_path.clone() {
            push_impact_path(files_removed, seen_files, path);
        }
    }

    let cargo_home = resolve_cargo_home();
    let rustup_home = resolve_rustup_home();
    let cargo_bin = cargo_home.join("bin");

    for binary in [
        "rustup",
        "cargo",
        "rustc",
        "rustdoc",
        "rustfmt",
        "clippy-driver",
        "rust-gdb",
        "rust-gdbgui",
        "rust-lldb",
    ] {
        push_impact_path(files_removed, seen_files, cargo_bin.join(binary));
    }
    push_impact_path(directories_removed, seen_directories, rustup_home.clone());

    let toolchain_names = store
        .list_installed()
        .ok()
        .map(|packages| {
            packages
                .into_iter()
                .filter(|package| package.package.manager == ManagerId::Rustup)
                .map(|package| package.package.name)
                .filter(|name| !name.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if toolchain_names.is_empty() {
        secondary_effects.push("No cached rustup toolchain inventory found.".to_string());
    } else {
        let sample = toolchain_names.iter().take(6).cloned().collect::<Vec<_>>();
        secondary_effects.push(format!(
            "{} rustup toolchains may be removed: {}",
            toolchain_names.len(),
            sample.join(", ")
        ));
    }

    secondary_effects.push(format!(
        "Rustup metadata under '{}' may be removed.",
        rustup_home.display()
    ));
    secondary_effects.push(format!(
        "Cargo proxy binaries under '{}' may be removed or relinked.",
        cargo_bin.display()
    ));
    if remove_shell_setup {
        append_shell_setup_cleanup_impact(
            ManagerId::Rustup,
            files_removed,
            secondary_effects,
            seen_files,
        );
    }
}

fn append_shell_setup_cleanup_impact(
    manager: ManagerId,
    files_removed: &mut Vec<UninstallImpactPath>,
    secondary_effects: &mut Vec<String>,
    seen_files: &mut HashSet<String>,
) {
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        for path in [
            home.join(".zshrc"),
            home.join(".zprofile"),
            home.join(".bashrc"),
            home.join(".bash_profile"),
            home.join(".profile"),
            home.join(".config/fish/config.fish"),
        ] {
            push_impact_path(files_removed, seen_files, path);
        }
    }
    secondary_effects.push(format!(
        "Helm-managed {} setup blocks in shell startup files will be removed when bounded markers are present.",
        manager.as_str()
    ));
}

fn append_homebrew_full_cleanup_impact(
    files_removed: &mut Vec<UninstallImpactPath>,
    directories_removed: &mut Vec<UninstallImpactPath>,
    secondary_effects: &mut Vec<String>,
    seen_files: &mut HashSet<String>,
    seen_directories: &mut HashSet<String>,
    requested_manager: ManagerId,
) {
    match requested_manager {
        ManagerId::Rustup => {
            let rustup_home = resolve_rustup_home();
            let cargo_bin = resolve_cargo_home().join("bin");
            push_impact_path(directories_removed, seen_directories, rustup_home.clone());
            for binary in [
                "rustup",
                "cargo",
                "rustc",
                "rustdoc",
                "rustfmt",
                "clippy-driver",
                "rust-gdb",
                "rust-gdbgui",
                "rust-lldb",
            ] {
                push_impact_path(files_removed, seen_files, cargo_bin.join(binary));
            }
            secondary_effects.push(format!(
                "Full cleanup removes rustup state/toolchains under '{}'.",
                rustup_home.display()
            ));
            secondary_effects.push(format!(
                "Full cleanup removes rustup proxy binaries under '{}'.",
                cargo_bin.display()
            ));
        }
        ManagerId::Mise => {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("~"));
            let mise_state = home.join(".local/share/mise");
            let mise_cache = home.join(".cache/mise");
            push_impact_path(directories_removed, seen_directories, mise_state.clone());
            push_impact_path(directories_removed, seen_directories, mise_cache.clone());
            secondary_effects.push(format!(
                "Full cleanup removes mise state/cache under '{}' and '{}'.",
                mise_state.display(),
                mise_cache.display()
            ));
        }
        _ => {
            secondary_effects.push(format!(
                "Full cleanup selected; no additional manager-owned cleanup paths are currently defined for '{}'.",
                requested_manager.as_str()
            ));
        }
    }
}

fn resolve_cargo_home() -> PathBuf {
    if let Some(raw) = std::env::var_os("CARGO_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(raw);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".cargo"))
        .unwrap_or_else(|| PathBuf::from("~/.cargo"))
}

fn resolve_rustup_home() -> PathBuf {
    if let Some(raw) = std::env::var_os("RUSTUP_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(raw);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".rustup"))
        .unwrap_or_else(|| PathBuf::from("~/.rustup"))
}

fn push_impact_path(
    entries: &mut Vec<UninstallImpactPath>,
    seen: &mut HashSet<String>,
    path: PathBuf,
) {
    let rendered = path.to_string_lossy().to_string();
    if seen.insert(rendered.clone()) {
        entries.push(UninstallImpactPath {
            path: rendered,
            exists: path.exists(),
        });
    }
}

fn compute_uninstall_blast_radius_score(
    file_count: usize,
    directory_count: usize,
    secondary_effect_count: usize,
    confidence_requires_confirmation: bool,
    used_unknown_override: bool,
    legacy_fallback_used: bool,
) -> u32 {
    compute_uninstall_blast_radius_score_with_base_risk(
        file_count,
        directory_count,
        secondary_effect_count,
        confidence_requires_confirmation,
        used_unknown_override,
        legacy_fallback_used,
        0,
    )
}

fn compute_uninstall_blast_radius_score_with_base_risk(
    file_count: usize,
    directory_count: usize,
    secondary_effect_count: usize,
    confidence_requires_confirmation: bool,
    used_unknown_override: bool,
    legacy_fallback_used: bool,
    base_risk: u32,
) -> u32 {
    let mut score =
        file_count as u32 + (directory_count as u32 * 2) + secondary_effect_count as u32;
    score += base_risk;
    if confidence_requires_confirmation {
        score += 2;
    }
    if used_unknown_override {
        score += 2;
    }
    if legacy_fallback_used {
        score += 1;
    }
    score
}

fn package_uninstall_base_risk(manager: ManagerId) -> u32 {
    match manager {
        ManagerId::HomebrewFormula => 4,
        ManagerId::Rustup => 3,
        _ => 1,
    }
}

fn build_manager_uninstall_summary_lines(context: UninstallSummaryContext<'_>) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "Target manager: {}",
        context.target_manager.as_str()
    ));
    lines.push(format!("Uninstall strategy: {}", context.strategy.as_str()));
    lines.push(format!(
        "Blast radius score: {}",
        context.blast_radius_score
    ));
    lines.push(format!(
        "Impacts: {} files, {} directories, {} secondary effects",
        context.files_count, context.directories_count, context.secondary_effect_count
    ));

    if let Some(instance) = context.active_instance {
        lines.push(format!(
            "Detected provenance: {}",
            instance.provenance.as_str()
        ));
        lines.push(format!("Confidence: {:.2}", instance.confidence));
        if let Some(margin) = instance.decision_margin {
            lines.push(format!("Decision margin: {:.2}", margin));
        }
        lines.push(format!(
            "Automation level: {}",
            instance.automation_level.as_str()
        ));
    }

    if context.unknown_override_required {
        lines.push("Unknown provenance requires explicit override.".to_string());
    }
    if context.used_unknown_override {
        lines.push("Unknown-provenance override will be applied.".to_string());
    }
    if context.read_only_blocked {
        lines.push("Read-only automation policy blocks uninstall execution.".to_string());
    }

    lines
}

fn build_package_uninstall_summary_lines(
    context: PackageUninstallSummaryContext<'_>,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "Target package: {}@{}",
        context.package.name,
        context.package.manager.as_str()
    ));
    lines.push(format!(
        "Blast radius score: {}",
        context.blast_radius_score
    ));
    lines.push(format!(
        "Impacts: {} files, {} directories, {} secondary effects",
        context.files_count, context.directories_count, context.secondary_effect_count
    ));
    if let Some(state) = context
        .package_runtime_state
        .filter(|state| !state.is_empty())
    {
        lines.push(format!(
            "Runtime state: {}",
            render_package_runtime_state(state)
        ));
    }
    if context.rustup_override_count > 0 {
        lines.push(format!(
            "Directory overrides: {}",
            context.rustup_override_count
        ));
    }
    lines
}

fn render_package_runtime_state(state: &PackageRuntimeState) -> String {
    let mut parts = Vec::new();
    if state.is_active {
        parts.push("active");
    }
    if state.is_default {
        parts.push("default");
    }
    if state.has_override {
        parts.push("override");
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(", ")
    }
}

fn normalize_nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else if trimmed.len() == raw.len() {
            Some(raw)
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_instance() -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager: ManagerId::HomebrewFormula,
            instance_id: "sample".to_string(),
            identity_kind: crate::models::InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/bin/brew".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/brew"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/brew")],
            is_active: true,
            version: Some("4.3.0".to_string()),
            provenance: InstallProvenance::Homebrew,
            confidence: 0.99,
            decision_margin: Some(0.80),
            automation_level: AutomationLevel::Automatic,
            uninstall_strategy: StrategyKind::HomebrewFormula,
            update_strategy: StrategyKind::HomebrewFormula,
            remediation_strategy: StrategyKind::ManualRemediation,
            explanation_primary: Some("sample".to_string()),
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        }
    }

    #[test]
    fn package_uninstall_preview_requires_confirmation_for_homebrew_formula() {
        let package = PackageRef {
            manager: ManagerId::HomebrewFormula,
            name: "git".to_string(),
        };
        let preview = build_package_uninstall_preview(
            PackageUninstallPreviewContext {
                package: &package,
                active_instance: Some(&sample_instance()),
                package_runtime_state: None,
                rustup_override_paths: &[],
            },
            DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
        );

        assert!(preview.requires_yes);
        assert_eq!(preview.manager_provenance.as_deref(), Some("homebrew"));
        assert!(
            preview
                .secondary_effects
                .iter()
                .any(|line| line.contains("Homebrew formula"))
        );
    }

    #[test]
    fn package_uninstall_preview_remains_non_blocking_for_low_risk_manager() {
        let package = PackageRef {
            manager: ManagerId::Npm,
            name: "eslint".to_string(),
        };
        let preview = build_package_uninstall_preview(
            PackageUninstallPreviewContext {
                package: &package,
                active_instance: None,
                package_runtime_state: None,
                rustup_override_paths: &[],
            },
            DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
        );

        assert!(!preview.requires_yes);
        assert_eq!(preview.manager_provenance, None);
        assert_eq!(preview.files_removed.len(), 0);
        assert_eq!(preview.directories_removed.len(), 0);
    }

    #[test]
    fn package_uninstall_preview_warns_for_active_default_rustup_toolchain() {
        let package = PackageRef {
            manager: ManagerId::Rustup,
            name: "stable-aarch64-apple-darwin".to_string(),
        };
        let runtime_state = PackageRuntimeState {
            is_active: true,
            is_default: true,
            has_override: true,
        };
        let override_paths = vec![
            "/Users/test/project-a".to_string(),
            "/Users/test/project-b".to_string(),
        ];
        let preview = build_package_uninstall_preview(
            PackageUninstallPreviewContext {
                package: &package,
                active_instance: None,
                package_runtime_state: Some(&runtime_state),
                rustup_override_paths: override_paths.as_slice(),
            },
            DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
        );

        assert!(preview.requires_yes);
        assert!(preview.confidence_requires_confirmation);
        assert!(
            preview
                .summary_lines
                .iter()
                .any(|line| line.contains("Runtime state: active, default, override"))
        );
        assert!(
            preview
                .summary_lines
                .iter()
                .any(|line| line.contains("Directory overrides: 2"))
        );
        assert!(
            preview
                .secondary_effects
                .iter()
                .any(|line| line.contains("currently active"))
        );
        assert!(
            preview
                .secondary_effects
                .iter()
                .any(|line| line.contains("default toolchain"))
        );
        assert!(
            preview
                .secondary_effects
                .iter()
                .any(|line| line.contains("directory override"))
        );
    }
}
