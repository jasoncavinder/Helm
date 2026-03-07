use std::collections::{BTreeMap, HashMap};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use helm_core::persistence::{DetectionStore, PackageStore, PinStore, TaskStore};
use helm_core::versioning::PackageCoordinate;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use serde_json::json;

use super::provenance::{InstallChannel, UpdatePolicy};
use super::{
    CachedSearchResult, Capability, CliDiagnosticsSummary, CliManagerInstallInstance,
    CliManagerStatus, CliTaskLogRecord, CliTaskRecord, CoordinatorSubmitRequest,
    CoordinatorWorkflowRequest, ExecutionMode, InstalledPackage, ManagerId, OutdatedPackage,
    PackageRef, PinKind, PinRecord, SELF_UPDATE_ALLOW_ROOT_ENV, SqliteStore, TASK_FETCH_LIMIT,
    TaskId, acknowledge_manager_multi_instance_state, adapter_request_to_coordinator_submit,
    build_diagnostics_summary, build_manager_mutation_request,
    build_manager_uninstall_plan_with_options, build_package_uninstall_preview_for_package,
    cancel_inflight_tasks_for_manager, channel_managed_check_status,
    clear_manager_multi_instance_ack, coordinator_cancel_task, coordinator_start_workflow,
    coordinator_submit_request, current_cli_version, database_path, detect_install_provenance,
    direct_update_apply, direct_update_check_status, enabled_dependents_for_manager,
    env_flag_enabled, is_running_as_root, list_installed_for_enabled,
    list_manager_install_instances, list_managers, list_outdated_for_enabled,
    list_tasks_for_enabled, manager_enabled_map, manager_enablement_eligibility_for_store,
    manager_executable_status, manager_install_methods_status, manager_priority_entries,
    provenance_can_self_update, provenance_recommended_action, registry,
    resolve_install_method_override_for_tui, search_local_for_enabled,
    set_manager_active_install_instance, set_manager_priority_rank, task_log_to_cli_record,
    task_to_cli_task, write_setting,
};
use helm_core::models::HomebrewKegPolicy;
use helm_core::models::PackageRuntimeState;

const SPLASH_AUTO_DISMISS_MS: Option<u64> = Some(900);
const DATA_REFRESH_INTERVAL_MS: u64 = 1200;
const REMOTE_SEARCH_DEBOUNCE_MS: u64 = 350;

const SPLASH_LARGE: &str = include_str!("assets/splash_large.txt");
const SPLASH_COMPACT: &str = include_str!("assets/splash_compact.txt");

fn package_manager_preference_key(package_name: &str, version: Option<&str>) -> String {
    let normalized_name = package_name.trim().to_ascii_lowercase();
    if normalized_name.is_empty() {
        return String::new();
    }

    let normalized_version = version.map(str::trim).filter(|value| !value.is_empty());
    let Some(normalized_version) = normalized_version else {
        return normalized_name;
    };

    let coordinate_raw = format!("{}@{}", package_name.trim(), normalized_version);
    let qualifier_key = PackageCoordinate::parse(coordinate_raw.as_str())
        .and_then(|coordinate| coordinate.version_selector)
        .map(|selector| selector.qualifier_atoms())
        .filter(|atoms| !atoms.is_empty())
        .map(|atoms| atoms.join("-").trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    if let Some(qualifier_key) = qualifier_key {
        format!("{}@{}", normalized_name, qualifier_key)
    } else {
        normalized_name
    }
}

fn preferred_manager_for_package(
    package_manager_preferences: &HashMap<String, ManagerId>,
    package_name: &str,
    version: Option<&str>,
) -> Option<ManagerId> {
    let preference_key = package_manager_preference_key(package_name, version);
    if !preference_key.is_empty()
        && let Some(preferred) = package_manager_preferences.get(preference_key.as_str())
    {
        return Some(*preferred);
    }

    let fallback_key = package_name.trim().to_ascii_lowercase();
    if fallback_key.is_empty() || fallback_key == preference_key {
        return None;
    }
    package_manager_preferences
        .get(fallback_key.as_str())
        .copied()
}

fn mise_uninstall_options_label(
    options: &helm_core::manager_lifecycle::ManagerUninstallOptions,
) -> Option<&'static str> {
    use helm_core::manager_lifecycle::{MiseUninstallCleanupMode, MiseUninstallConfigRemoval};

    match (options.mise_cleanup_mode, options.mise_config_removal) {
        (
            Some(MiseUninstallCleanupMode::FullCleanup),
            Some(MiseUninstallConfigRemoval::KeepConfig),
        ) => Some("mode=full_cleanup config=keep"),
        (
            Some(MiseUninstallCleanupMode::FullCleanup),
            Some(MiseUninstallConfigRemoval::RemoveConfig),
        ) => Some("mode=full_cleanup config=remove"),
        (Some(MiseUninstallCleanupMode::ManagerOnly), _) | (None, None) => {
            Some("mode=manager_only")
        }
        _ => None,
    }
}

fn homebrew_uninstall_options_label(
    options: &helm_core::manager_lifecycle::ManagerUninstallOptions,
) -> Option<&'static str> {
    use helm_core::manager_lifecycle::HomebrewUninstallCleanupMode;

    match options.homebrew_cleanup_mode {
        Some(HomebrewUninstallCleanupMode::ManagerOnly) => Some("homebrew_mode=manager_only"),
        Some(HomebrewUninstallCleanupMode::FullCleanup) => Some("homebrew_mode=full_cleanup"),
        None => None,
    }
}

fn mise_uninstall_options_full_cleanup(
    config_removal: helm_core::manager_lifecycle::MiseUninstallConfigRemoval,
) -> helm_core::manager_lifecycle::ManagerUninstallOptions {
    helm_core::manager_lifecycle::ManagerUninstallOptions {
        homebrew_cleanup_mode: None,
        mise_cleanup_mode: Some(
            helm_core::manager_lifecycle::MiseUninstallCleanupMode::FullCleanup,
        ),
        mise_config_removal: Some(config_removal),
        remove_helm_managed_shell_setup: None,
    }
}

fn homebrew_uninstall_options_full_cleanup() -> helm_core::manager_lifecycle::ManagerUninstallOptions
{
    helm_core::manager_lifecycle::ManagerUninstallOptions {
        homebrew_cleanup_mode: Some(
            helm_core::manager_lifecycle::HomebrewUninstallCleanupMode::FullCleanup,
        ),
        ..helm_core::manager_lifecycle::ManagerUninstallOptions::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    Updates,
    Packages,
    Tasks,
    Managers,
    Settings,
    Diagnostics,
}

impl Section {
    const ALL: [Section; 6] = [
        Section::Updates,
        Section::Packages,
        Section::Tasks,
        Section::Managers,
        Section::Settings,
        Section::Diagnostics,
    ];

    fn title(self) -> &'static str {
        match self {
            Section::Updates => "Updates",
            Section::Packages => "Packages",
            Section::Tasks => "Tasks",
            Section::Managers => "Managers",
            Section::Settings => "Settings",
            Section::Diagnostics => "Diagnostics",
        }
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Normal,
    Search,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteAction {
    RefreshAll,
    DetectAll,
    Switch(Section),
    UpgradeAll,
    Quit,
}

impl PaletteAction {
    fn title(self) -> &'static str {
        match self {
            PaletteAction::RefreshAll => "Refresh all managers",
            PaletteAction::DetectAll => "Detect all managers",
            PaletteAction::Switch(section) => match section {
                Section::Updates => "Switch: Updates",
                Section::Packages => "Switch: Packages",
                Section::Tasks => "Switch: Tasks",
                Section::Managers => "Switch: Managers",
                Section::Settings => "Switch: Settings",
                Section::Diagnostics => "Switch: Diagnostics",
            },
            PaletteAction::UpgradeAll => "Run Upgrade All",
            PaletteAction::Quit => "Quit",
        }
    }
}

const PALETTE_ACTIONS: [PaletteAction; 10] = [
    PaletteAction::RefreshAll,
    PaletteAction::DetectAll,
    PaletteAction::UpgradeAll,
    PaletteAction::Switch(Section::Updates),
    PaletteAction::Switch(Section::Packages),
    PaletteAction::Switch(Section::Tasks),
    PaletteAction::Switch(Section::Managers),
    PaletteAction::Switch(Section::Settings),
    PaletteAction::Switch(Section::Diagnostics),
    PaletteAction::Quit,
];

#[derive(Clone, Debug)]
enum ConfirmAction {
    InstallPackage {
        manager: ManagerId,
        package_name: String,
    },
    UpgradePackage {
        manager: ManagerId,
        package_name: String,
    },
    UninstallPackage {
        manager: ManagerId,
        package_name: String,
        uninstall_preview_summary: Option<String>,
    },
    TogglePin {
        manager: ManagerId,
        package_name: String,
        pinned: bool,
    },
    SetPackageKegPolicy {
        package_name: String,
        policy: Option<HomebrewKegPolicy>,
    },
    ManagerMutation {
        manager: ManagerId,
        subcommand: &'static str,
        allow_unknown_provenance: bool,
        uninstall_options: helm_core::manager_lifecycle::ManagerUninstallOptions,
        uninstall_preview_summary: Option<String>,
    },
    CancelTask {
        task_id: u64,
    },
    ToggleManager {
        manager: ManagerId,
        enable: bool,
    },
    UpgradeAllWithOptions {
        include_pinned: bool,
        allow_os_updates: bool,
        manager_scope: Option<ManagerId>,
    },
    SelfUpdate {
        force: bool,
    },
    RustupToolchainMutation {
        request: CoordinatorSubmitRequest,
        prompt: String,
        success_message: String,
    },
}

impl ConfirmAction {
    fn prompt(&self) -> String {
        match self {
            Self::InstallPackage {
                manager,
                package_name,
            } => format!(
                "Install '{}@{}'? [Enter confirm / Esc cancel]",
                package_name,
                manager.as_str()
            ),
            Self::UpgradePackage {
                manager,
                package_name,
            } => format!(
                "Upgrade '{}@{}'? [Enter confirm / Esc cancel]",
                package_name,
                manager.as_str()
            ),
            Self::UninstallPackage {
                manager,
                package_name,
                uninstall_preview_summary,
            } => {
                let mut prompt = format!("Uninstall '{}@{}'?", package_name, manager.as_str());
                if let Some(summary) = uninstall_preview_summary.as_deref() {
                    prompt.push(' ');
                    prompt.push_str(summary);
                }
                prompt.push_str(" [Enter confirm / Esc cancel]");
                prompt
            }
            Self::TogglePin {
                manager,
                package_name,
                pinned,
            } => {
                let action = if *pinned { "Unpin" } else { "Pin" };
                format!(
                    "{} '{}@{}'? [Enter confirm / Esc cancel]",
                    action,
                    package_name,
                    manager.as_str()
                )
            }
            Self::SetPackageKegPolicy {
                package_name,
                policy,
            } => {
                let rendered = policy
                    .map(|value| value.as_str().to_string())
                    .unwrap_or_else(|| "default".to_string());
                format!(
                    "Set Homebrew keg policy for '{}' to '{}' ? [Enter confirm / Esc cancel]",
                    package_name, rendered
                )
            }
            Self::ManagerMutation {
                manager,
                subcommand,
                allow_unknown_provenance,
                uninstall_options,
                uninstall_preview_summary,
            } => {
                let mut prompt = format!("{} manager '{}' ?", subcommand, manager.as_str());
                if let Some(summary) = uninstall_preview_summary.as_deref() {
                    prompt.push(' ');
                    prompt.push_str(summary);
                }
                if *subcommand == "uninstall" {
                    if let Some(summary) = homebrew_uninstall_options_label(uninstall_options) {
                        prompt.push(' ');
                        prompt.push('[');
                        prompt.push_str(summary);
                        prompt.push(']');
                    }
                    if *manager == ManagerId::Mise
                        && let Some(summary) = mise_uninstall_options_label(uninstall_options)
                    {
                        prompt.push(' ');
                        prompt.push('[');
                        prompt.push_str(summary);
                        prompt.push(']');
                    }
                }
                if *allow_unknown_provenance {
                    prompt.push_str(" [unknown-provenance override]");
                }
                prompt.push_str(" [Enter confirm / Esc cancel]");
                prompt
            }
            Self::CancelTask { task_id } => {
                format!("Cancel task #{}? [Enter confirm / Esc cancel]", task_id)
            }
            Self::ToggleManager { manager, enable } => {
                let action = if *enable { "Enable" } else { "Disable" };
                format!(
                    "{} manager '{}' ? [Enter confirm / Esc cancel]",
                    action,
                    manager.as_str()
                )
            }
            Self::UpgradeAllWithOptions {
                include_pinned,
                allow_os_updates,
                manager_scope,
            } => format!(
                "Run Upgrade All workflow? include_pinned={} allow_os_updates={} manager_scope={} [Enter confirm / Esc cancel]",
                include_pinned,
                allow_os_updates,
                manager_scope
                    .map(|value| value.as_str().to_string())
                    .unwrap_or_else(|| "all".to_string())
            ),
            Self::SelfUpdate { force } => {
                if *force {
                    "Apply Helm CLI self-update in force mode? [Enter confirm / Esc cancel]"
                        .to_string()
                } else {
                    "Apply Helm CLI self-update now? [Enter confirm / Esc cancel]".to_string()
                }
            }
            Self::RustupToolchainMutation { prompt, .. } => prompt.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PackageRowKind {
    Installed,
    Available,
}

#[derive(Clone, Debug)]
struct PackageRow {
    kind: PackageRowKind,
    manager: ManagerId,
    package_name: String,
    installed_version: Option<String>,
    candidate_version: Option<String>,
    pinned: bool,
    summary: Option<String>,
    homebrew_keg_policy: Option<String>,
    preferred_manager: Option<ManagerId>,
    runtime_state: PackageRuntimeState,
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

#[derive(Clone, Debug, Default)]
struct StatusSnapshot {
    installed_packages: usize,
    update_packages: usize,
    running_or_queued: usize,
    enabled_managers: usize,
    detected_enabled_managers: usize,
}

#[derive(Clone, Debug, Default)]
struct SettingsSnapshot {
    safe_mode: bool,
    homebrew_keg_auto_cleanup: bool,
    database_path: String,
    auto_check_for_updates: bool,
    auto_check_frequency_minutes: u32,
    auto_check_last_checked_unix: Option<i64>,
}

#[derive(Clone, Debug, Default)]
struct SelfUpdateCheckSnapshot {
    checked_at_unix: i64,
    checked: bool,
    update_available: Option<bool>,
    latest_version: Option<String>,
    published_at: Option<String>,
    source: String,
    reason: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct SelfUpdateSnapshot {
    current_version: String,
    channel: String,
    update_policy: String,
    source: String,
    executable_path: String,
    marker_path: String,
    can_self_update: bool,
    recommended_action: String,
    last_check: Option<SelfUpdateCheckSnapshot>,
    last_error: Option<String>,
}

#[derive(Clone, Debug)]
struct ToastMessage {
    message: String,
    error: bool,
    expires_at: Instant,
}

#[derive(Clone, Debug)]
struct TuiTheme {
    heading: Style,
    accent: Style,
    selected: Style,
    text: Style,
    subtle: Style,
    error: Style,
    warning: Style,
    success: Style,
}

impl TuiTheme {
    fn new(no_color: bool) -> Self {
        if no_color {
            return Self {
                heading: Style::default().add_modifier(Modifier::BOLD),
                accent: Style::default().add_modifier(Modifier::BOLD),
                selected: Style::default().add_modifier(Modifier::REVERSED),
                text: Style::default(),
                subtle: Style::default(),
                error: Style::default().add_modifier(Modifier::BOLD),
                warning: Style::default().add_modifier(Modifier::BOLD),
                success: Style::default().add_modifier(Modifier::BOLD),
            };
        }

        Self {
            heading: Style::default()
                .fg(Color::Rgb(27, 58, 102))
                .add_modifier(Modifier::BOLD),
            accent: Style::default()
                .fg(Color::Rgb(42, 93, 168))
                .add_modifier(Modifier::BOLD),
            selected: Style::default()
                .bg(Color::Rgb(20, 30, 47))
                .fg(Color::Rgb(108, 166, 232))
                .add_modifier(Modifier::BOLD),
            text: Style::default().fg(Color::Rgb(230, 237, 246)),
            subtle: Style::default().fg(Color::Rgb(159, 176, 199)),
            error: Style::default()
                .fg(Color::Rgb(223, 77, 96))
                .add_modifier(Modifier::BOLD),
            warning: Style::default()
                .fg(Color::Rgb(200, 156, 61))
                .add_modifier(Modifier::BOLD),
            success: Style::default()
                .fg(Color::Rgb(95, 174, 113))
                .add_modifier(Modifier::BOLD),
        }
    }
}

struct AppState {
    section: Section,
    input_mode: InputMode,
    filter_query: String,
    show_help: bool,
    show_palette: bool,
    palette_index: usize,
    confirm_action: Option<ConfirmAction>,
    updates_include_pinned: bool,
    updates_allow_os_updates: bool,
    updates_manager_scope: Option<ManagerId>,
    splash_started_at: Instant,
    splash_visible: bool,
    should_quit: bool,
    last_reload_at: Instant,
    updates_cursor: usize,
    packages_cursor: usize,
    tasks_cursor: usize,
    managers_cursor: usize,
    settings_cursor: usize,
    status: StatusSnapshot,
    packages: Vec<InstalledPackage>,
    package_search_results: Vec<CachedSearchResult>,
    package_rows: Vec<PackageRow>,
    all_updates: Vec<OutdatedPackage>,
    updates: Vec<OutdatedPackage>,
    tasks: Vec<CliTaskRecord>,
    managers: Vec<CliManagerStatus>,
    diagnostics: CliDiagnosticsSummary,
    settings: SettingsSnapshot,
    self_update: SelfUpdateSnapshot,
    selected_manager_executable_paths: Vec<String>,
    selected_manager_install_methods: Vec<String>,
    selected_manager_install_instances: Vec<CliManagerInstallInstance>,
    selected_manager_priority_label: Option<String>,
    selected_rustup_toolchain_key: Option<String>,
    selected_rustup_toolchain_detail: Option<helm_core::adapters::rustup::RustupToolchainDetail>,
    rustup_component_cursor: usize,
    rustup_target_cursor: usize,
    task_logs: Vec<CliTaskLogRecord>,
    pending_remote_search_query: Option<String>,
    pending_remote_search_at: Option<Instant>,
    last_remote_search_query: Option<String>,
    remote_search_task_ids: Vec<u64>,
    toast: Option<ToastMessage>,
    theme: TuiTheme,
    color_enabled: bool,
}

impl AppState {
    fn new(no_color: bool) -> Self {
        Self {
            section: Section::Updates,
            input_mode: InputMode::Normal,
            filter_query: String::new(),
            show_help: false,
            show_palette: false,
            palette_index: 0,
            confirm_action: None,
            updates_include_pinned: false,
            updates_allow_os_updates: false,
            updates_manager_scope: None,
            splash_started_at: Instant::now(),
            splash_visible: true,
            should_quit: false,
            last_reload_at: Instant::now(),
            updates_cursor: 0,
            packages_cursor: 0,
            tasks_cursor: 0,
            managers_cursor: 0,
            settings_cursor: 0,
            status: StatusSnapshot::default(),
            packages: Vec::new(),
            package_search_results: Vec::new(),
            package_rows: Vec::new(),
            all_updates: Vec::new(),
            updates: Vec::new(),
            tasks: Vec::new(),
            managers: Vec::new(),
            diagnostics: CliDiagnosticsSummary {
                installed_packages: 0,
                updatable_packages: 0,
                enabled_managers: 0,
                detected_enabled_managers: 0,
                queued_tasks: 0,
                running_tasks: 0,
                completed_tasks: 0,
                failed_tasks: 0,
                cancelled_tasks: 0,
                failed_task_ids: Vec::new(),
                undetected_enabled_managers: Vec::new(),
                failure_classes: BTreeMap::new(),
                failure_class_hints: BTreeMap::new(),
                coordinator: super::CliCoordinatorHealthSummary {
                    state_dir: String::new(),
                    ready_file_present: false,
                    pid: None,
                    pid_alive: None,
                    executable_path: None,
                    executable_exists: None,
                    last_heartbeat_unix: None,
                    stale_reasons: Vec::new(),
                },
            },
            settings: SettingsSnapshot::default(),
            self_update: SelfUpdateSnapshot::default(),
            selected_manager_executable_paths: Vec::new(),
            selected_manager_install_methods: Vec::new(),
            selected_manager_install_instances: Vec::new(),
            selected_manager_priority_label: None,
            selected_rustup_toolchain_key: None,
            selected_rustup_toolchain_detail: None,
            rustup_component_cursor: 0,
            rustup_target_cursor: 0,
            task_logs: Vec::new(),
            pending_remote_search_query: None,
            pending_remote_search_at: None,
            last_remote_search_query: None,
            remote_search_task_ids: Vec::new(),
            toast: None,
            theme: TuiTheme::new(no_color),
            color_enabled: !no_color,
        }
    }

    fn note_success(&mut self, message: impl Into<String>) {
        self.toast = Some(ToastMessage {
            message: message.into(),
            error: false,
            expires_at: Instant::now() + Duration::from_secs(4),
        });
    }

    fn note_error(&mut self, message: impl Into<String>) {
        self.toast = Some(ToastMessage {
            message: message.into(),
            error: true,
            expires_at: Instant::now() + Duration::from_secs(6),
        });
    }

    fn prune_toast(&mut self) {
        if self
            .toast
            .as_ref()
            .is_some_and(|toast| Instant::now() >= toast.expires_at)
        {
            self.toast = None;
        }
    }

    fn is_filter_active(&self) -> bool {
        !self.filter_query.trim().is_empty()
    }

    fn reload(&mut self, store: &SqliteStore) -> Result<(), String> {
        let enabled_map = manager_enabled_map(store)?;
        self.packages = list_installed_for_enabled(store, &enabled_map)?;
        self.all_updates = list_outdated_for_enabled(store, &enabled_map)?;
        let mut updates = self.all_updates.clone();
        if let Some(manager_scope) = self.updates_manager_scope {
            updates.retain(|row| row.package.manager == manager_scope);
        }
        self.updates = updates;
        if self.is_filter_active() {
            self.package_search_results =
                search_local_for_enabled(store, &enabled_map, self.filter_query.as_str())?;
        } else {
            self.package_search_results.clear();
        }
        self.package_rows =
            build_package_rows(store, &self.packages, &self.package_search_results)?;
        self.tasks = list_tasks_for_enabled(store, &enabled_map)?;
        self.managers = list_managers(store)?;
        self.refresh_selected_rustup_toolchain_detail(false);
        if let Some(scope) = self.updates_manager_scope {
            let has_scope = self
                .managers
                .iter()
                .any(|manager| manager.manager_id == scope.as_str() && manager.enabled);
            if !has_scope {
                self.updates_manager_scope = None;
            }
        }
        self.diagnostics = build_diagnostics_summary(store)?;

        self.status = StatusSnapshot {
            installed_packages: self.packages.len(),
            update_packages: self.all_updates.len(),
            running_or_queued: self
                .tasks
                .iter()
                .filter(|task| task.status == "running" || task.status == "queued")
                .count(),
            enabled_managers: self
                .managers
                .iter()
                .filter(|manager| manager.enabled)
                .count(),
            detected_enabled_managers: self
                .managers
                .iter()
                .filter(|manager| manager.enabled && manager.detected)
                .count(),
        };

        let safe_mode = store
            .safe_mode()
            .map_err(|error| format!("failed to read safe_mode: {error}"))?;
        let homebrew_policy = store
            .homebrew_keg_policy()
            .map_err(|error| format!("failed to read homebrew_keg_policy: {error}"))?;
        let auto_check_for_updates = store
            .auto_check_for_updates()
            .map_err(|error| format!("failed to read auto_check_for_updates: {error}"))?;
        let auto_check_frequency_minutes = store
            .auto_check_frequency_minutes()
            .map_err(|error| format!("failed to read auto_check_frequency_minutes: {error}"))?;
        let auto_check_last_checked_unix = store
            .auto_check_last_checked_unix()
            .map_err(|error| format!("failed to read auto_check_last_checked_unix: {error}"))?;
        self.settings = SettingsSnapshot {
            safe_mode,
            homebrew_keg_auto_cleanup: homebrew_policy
                == helm_core::models::HomebrewKegPolicy::Cleanup,
            database_path: database_path()?,
            auto_check_for_updates,
            auto_check_frequency_minutes,
            auto_check_last_checked_unix,
        };

        self.refresh_self_update_snapshot()?;
        self.refresh_task_logs(store)?;
        if self.section == Section::Managers {
            self.refresh_selected_manager_controls(store)?;
        }
        self.clamp_cursors();
        self.last_reload_at = Instant::now();
        Ok(())
    }

    fn refresh_self_update_snapshot(&mut self) -> Result<(), String> {
        let executable_path = env::current_exe()
            .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
        let provenance = detect_install_provenance(&executable_path);
        let current_version = current_cli_version();
        let can_self_update = provenance_can_self_update(provenance.update_policy);
        let recommended_action = provenance_recommended_action(provenance.channel).to_string();
        let prior_check = self.self_update.last_check.clone();
        let prior_error = self.self_update.last_error.clone();

        self.self_update = SelfUpdateSnapshot {
            current_version,
            channel: provenance.channel.as_str().to_string(),
            update_policy: provenance.update_policy.as_str().to_string(),
            source: provenance.source.as_str().to_string(),
            executable_path: provenance.executable_path.to_string_lossy().to_string(),
            marker_path: provenance.marker_path.to_string_lossy().to_string(),
            can_self_update,
            recommended_action,
            last_check: prior_check,
            last_error: prior_error,
        };
        Ok(())
    }

    fn run_self_update_check(&mut self) -> Result<String, String> {
        let executable_path = env::current_exe()
            .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
        let provenance = detect_install_provenance(&executable_path);
        let checked_at = chrono_like_unix_now();
        let status = if provenance_can_self_update(provenance.update_policy) {
            direct_update_check_status(self.self_update.current_version.as_str())
                .map_err(|error| error.to_string())?
        } else {
            channel_managed_check_status(format!(
                "self-update check is channel-managed for '{}' installs; {}",
                provenance.channel.as_str(),
                self.self_update.recommended_action
            ))
        };

        self.self_update.last_error = None;
        self.self_update.last_check = Some(SelfUpdateCheckSnapshot {
            checked_at_unix: checked_at,
            checked: status.checked,
            update_available: status.update_available,
            latest_version: status.latest_version.clone(),
            published_at: status.published_at.clone(),
            source: status.source.clone(),
            reason: status.reason.clone(),
        });

        let availability = status
            .update_available
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Ok(format!(
            "Self update check completed (available: {}, latest: {}).",
            availability,
            status.latest_version.unwrap_or_else(|| "-".to_string())
        ))
    }

    fn refresh_task_logs(&mut self, store: &SqliteStore) -> Result<(), String> {
        let Some(task_id) = self.selected_task().map(|task| task.id) else {
            self.task_logs.clear();
            return Ok(());
        };
        self.task_logs = store
            .list_task_logs(TaskId(task_id), 120)
            .map_err(|error| format!("failed to list task logs: {error}"))?
            .into_iter()
            .map(task_log_to_cli_record)
            .collect();
        self.task_logs.sort_by_key(|record| record.id);
        Ok(())
    }

    fn should_reload(&self) -> bool {
        Instant::now().duration_since(self.last_reload_at)
            >= Duration::from_millis(DATA_REFRESH_INTERVAL_MS)
    }

    fn refresh_selected_manager_controls(&mut self, store: &SqliteStore) -> Result<(), String> {
        self.selected_manager_executable_paths.clear();
        self.selected_manager_install_methods.clear();
        self.selected_manager_install_instances.clear();
        self.selected_manager_priority_label = None;

        let Some(row) = self.selected_manager() else {
            return Ok(());
        };
        let manager = row
            .manager_id
            .parse::<ManagerId>()
            .map_err(|_| format!("unknown manager id '{}'", row.manager_id))?;
        let executable = manager_executable_status(store, manager)?;
        let install_methods = manager_install_methods_status(store, manager)?;
        let install_instances = list_manager_install_instances(store, Some(manager))?;
        let priorities = manager_priority_entries(store)?;

        self.selected_manager_executable_paths = executable.executable_paths;
        self.selected_manager_install_methods = install_methods.install_methods;
        self.selected_manager_install_instances = install_instances;

        if let Some(entry) = priorities
            .iter()
            .find(|entry| entry.manager_id == manager.as_str())
        {
            let total_in_authority = priorities
                .iter()
                .filter(|candidate| candidate.authority == entry.authority)
                .count();
            self.selected_manager_priority_label = Some(format!(
                "{} / {} ({})",
                entry.rank + 1,
                total_in_authority,
                entry.authority
            ));
        }
        Ok(())
    }

    fn visible_update_indices(&self) -> Vec<usize> {
        self.updates
            .iter()
            .enumerate()
            .filter(|(_, pkg)| {
                if self.is_filter_active()
                    && !manager_participates_in_package_search(pkg.package.manager)
                {
                    return false;
                }
                if self.updates_manager_scope.is_some()
                    && self.updates_manager_scope != Some(pkg.package.manager)
                {
                    return false;
                }
                matches_query(
                    self.filter_query.as_str(),
                    &[
                        pkg.package.name.as_str(),
                        pkg.package.manager.as_str(),
                        pkg.candidate_version.as_str(),
                    ],
                )
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn visible_package_indices(&self) -> Vec<usize> {
        self.package_rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                if self.is_filter_active() && !manager_participates_in_package_search(row.manager) {
                    return false;
                }
                matches_query(
                    self.filter_query.as_str(),
                    &[
                        row.package_name.as_str(),
                        row.manager.as_str(),
                        row.installed_version.as_deref().unwrap_or(""),
                        row.candidate_version.as_deref().unwrap_or(""),
                        row.summary.as_deref().unwrap_or(""),
                    ],
                )
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn visible_task_indices(&self) -> Vec<usize> {
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| {
                matches_query(
                    self.filter_query.as_str(),
                    &[
                        task.manager.as_str(),
                        task.task_type.as_str(),
                        task.status.as_str(),
                        &task.id.to_string(),
                    ],
                )
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn visible_manager_indices(&self) -> Vec<usize> {
        self.managers
            .iter()
            .enumerate()
            .filter(|(_, manager)| {
                matches_query(
                    self.filter_query.as_str(),
                    &[
                        manager.manager_id.as_str(),
                        manager.display_name.as_str(),
                        manager.authority.as_str(),
                        manager.version.as_deref().unwrap_or(""),
                    ],
                )
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn settings_entries(&self) -> Vec<(String, String)> {
        vec![
            ("safe_mode".to_string(), self.settings.safe_mode.to_string()),
            (
                "homebrew_keg_auto_cleanup".to_string(),
                self.settings.homebrew_keg_auto_cleanup.to_string(),
            ),
            (
                "database_path".to_string(),
                self.settings.database_path.clone(),
            ),
            (
                "auto_check_for_updates".to_string(),
                self.settings.auto_check_for_updates.to_string(),
            ),
            (
                "auto_check_frequency_minutes".to_string(),
                self.settings.auto_check_frequency_minutes.to_string(),
            ),
            (
                "auto_check_last_checked_unix".to_string(),
                self.settings
                    .auto_check_last_checked_unix
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string()),
            ),
        ]
    }

    fn selected_update(&self) -> Option<&OutdatedPackage> {
        let indices = self.visible_update_indices();
        let selected = indices.get(self.updates_cursor.min(indices.len().saturating_sub(1)))?;
        self.updates.get(*selected)
    }

    fn selected_package(&self) -> Option<&PackageRow> {
        let indices = self.visible_package_indices();
        let selected = indices.get(self.packages_cursor.min(indices.len().saturating_sub(1)))?;
        self.package_rows.get(*selected)
    }

    fn selected_rustup_component(
        &self,
    ) -> Option<&helm_core::adapters::rustup::RustupToolchainDetailEntry> {
        let detail = self.selected_rustup_toolchain_detail.as_ref()?;
        let index = self
            .rustup_component_cursor
            .min(detail.components.len().saturating_sub(1));
        detail.components.get(index)
    }

    fn selected_rustup_target(
        &self,
    ) -> Option<&helm_core::adapters::rustup::RustupToolchainDetailEntry> {
        let detail = self.selected_rustup_toolchain_detail.as_ref()?;
        let index = self
            .rustup_target_cursor
            .min(detail.targets.len().saturating_sub(1));
        detail.targets.get(index)
    }

    fn selected_task(&self) -> Option<&CliTaskRecord> {
        let indices = self.visible_task_indices();
        let selected = indices.get(self.tasks_cursor.min(indices.len().saturating_sub(1)))?;
        self.tasks.get(*selected)
    }

    fn selected_manager(&self) -> Option<&CliManagerStatus> {
        let indices = self.visible_manager_indices();
        let selected = indices.get(self.managers_cursor.min(indices.len().saturating_sub(1)))?;
        self.managers.get(*selected)
    }

    fn selected_settings_entry(&self) -> Option<(String, String)> {
        let entries = self.settings_entries();
        entries
            .get(self.settings_cursor.min(entries.len().saturating_sub(1)))
            .cloned()
    }

    fn clamp_cursors(&mut self) {
        self.updates_cursor =
            clamp_cursor(self.updates_cursor, self.visible_update_indices().len());
        self.packages_cursor =
            clamp_cursor(self.packages_cursor, self.visible_package_indices().len());
        self.tasks_cursor = clamp_cursor(self.tasks_cursor, self.visible_task_indices().len());
        self.managers_cursor =
            clamp_cursor(self.managers_cursor, self.visible_manager_indices().len());
        self.settings_cursor = clamp_cursor(self.settings_cursor, self.settings_entries().len());
    }

    fn refresh_selected_rustup_toolchain_detail(&mut self, force: bool) {
        let (toolchain, detail_key) = match self.selected_package() {
            Some(package)
                if package.manager == ManagerId::Rustup
                    && package.kind != PackageRowKind::Available =>
            {
                let toolchain = package.package_name.clone();
                let key = format!("rustup|{}", toolchain.to_lowercase());
                (toolchain, key)
            }
            _ => {
                self.selected_rustup_toolchain_key = None;
                self.selected_rustup_toolchain_detail = None;
                self.rustup_component_cursor = 0;
                self.rustup_target_cursor = 0;
                return;
            }
        };

        if !force
            && self.selected_rustup_toolchain_key.as_deref() == Some(detail_key.as_str())
            && self.selected_rustup_toolchain_detail.is_some()
        {
            return;
        }

        self.selected_rustup_toolchain_detail =
            super::load_rustup_toolchain_detail_for_cli(toolchain.as_str(), "tui rustup detail");
        self.selected_rustup_toolchain_key = Some(detail_key);

        let component_len = self
            .selected_rustup_toolchain_detail
            .as_ref()
            .map(|detail| detail.components.len())
            .unwrap_or(0);
        let target_len = self
            .selected_rustup_toolchain_detail
            .as_ref()
            .map(|detail| detail.targets.len())
            .unwrap_or(0);
        self.rustup_component_cursor = clamp_cursor(self.rustup_component_cursor, component_len);
        self.rustup_target_cursor = clamp_cursor(self.rustup_target_cursor, target_len);
    }

    fn move_rustup_component_cursor(&mut self, delta: i32) {
        let len = self
            .selected_rustup_toolchain_detail
            .as_ref()
            .map(|detail| detail.components.len())
            .unwrap_or(0);
        self.rustup_component_cursor = wrap_cursor(self.rustup_component_cursor, len, delta);
    }

    fn move_rustup_target_cursor(&mut self, delta: i32) {
        let len = self
            .selected_rustup_toolchain_detail
            .as_ref()
            .map(|detail| detail.targets.len())
            .unwrap_or(0);
        self.rustup_target_cursor = wrap_cursor(self.rustup_target_cursor, len, delta);
    }

    fn list_len_for_section(&self, section: Section) -> usize {
        match section {
            Section::Updates => self.visible_update_indices().len(),
            Section::Packages => self.visible_package_indices().len(),
            Section::Tasks => self.visible_task_indices().len(),
            Section::Managers => self.visible_manager_indices().len(),
            Section::Settings => self.settings_entries().len(),
            Section::Diagnostics => 1,
        }
    }

    fn move_cursor(&mut self, delta: i32) {
        let len = self.list_len_for_section(self.section);
        if len == 0 {
            return;
        }
        let apply = |cursor: &mut usize| {
            if delta > 0 {
                *cursor = (*cursor + delta as usize).min(len.saturating_sub(1));
            } else if delta < 0 {
                *cursor = cursor.saturating_sub((-delta) as usize);
            }
        };
        match self.section {
            Section::Updates => apply(&mut self.updates_cursor),
            Section::Packages => apply(&mut self.packages_cursor),
            Section::Tasks => apply(&mut self.tasks_cursor),
            Section::Managers => apply(&mut self.managers_cursor),
            Section::Settings => apply(&mut self.settings_cursor),
            Section::Diagnostics => {}
        }
    }

    fn switch_section(&mut self, section: Section) {
        self.section = section;
        if section != Section::Packages {
            for task_id in self.remote_search_task_ids.drain(..) {
                let _ = coordinator_cancel_task(task_id);
            }
            self.pending_remote_search_query = None;
            self.pending_remote_search_at = None;
        } else if self.is_filter_active() {
            self.mark_filter_changed();
        }
        self.clamp_cursors();
    }

    fn mark_filter_changed(&mut self) {
        if self.section != Section::Packages {
            return;
        }
        let query = self.filter_query.trim();
        if query.is_empty() {
            for task_id in self.remote_search_task_ids.drain(..) {
                let _ = coordinator_cancel_task(task_id);
            }
            self.pending_remote_search_query = None;
            self.pending_remote_search_at = None;
            self.last_remote_search_query = None;
            return;
        }
        if self.last_remote_search_query.as_deref() != Some(query) {
            for task_id in self.remote_search_task_ids.drain(..) {
                let _ = coordinator_cancel_task(task_id);
            }
        }
        self.pending_remote_search_query = Some(query.to_string());
        self.pending_remote_search_at = Some(Instant::now());
    }

    fn cycle_updates_manager_scope(&mut self, direction: i32) -> Option<ManagerId> {
        let mut options: Vec<Option<ManagerId>> = vec![None];
        let mut managers = self
            .all_updates
            .iter()
            .map(|row| row.package.manager)
            .collect::<Vec<_>>();
        managers.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        managers.dedup();
        options.extend(managers.into_iter().map(Some));
        let current = self.updates_manager_scope;
        let current_index = options
            .iter()
            .position(|candidate| *candidate == current)
            .unwrap_or(0);
        let next = options[next_choice_index(current_index, options.len(), direction)];
        self.updates_manager_scope = next;
        next
    }

    fn maybe_dispatch_remote_search(&mut self, store: &SqliteStore) -> Result<(), String> {
        if self.section != Section::Packages {
            return Ok(());
        }
        let Some(query) = self.pending_remote_search_query.clone() else {
            return Ok(());
        };
        let Some(queued_at) = self.pending_remote_search_at else {
            return Ok(());
        };
        if Instant::now().duration_since(queued_at)
            < Duration::from_millis(REMOTE_SEARCH_DEBOUNCE_MS)
        {
            return Ok(());
        }
        if self.last_remote_search_query.as_deref() == Some(query.as_str()) {
            self.pending_remote_search_at = None;
            return Ok(());
        }

        for task_id in self.remote_search_task_ids.drain(..) {
            let _ = coordinator_cancel_task(task_id);
        }

        let mut target_managers = self
            .managers
            .iter()
            .filter_map(|row| {
                if !row.enabled || !row.detected || !row.supports_remote_search {
                    return None;
                }
                row.manager_id.parse::<ManagerId>().ok()
            })
            .collect::<Vec<_>>();
        target_managers.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        target_managers.dedup();

        for manager in target_managers {
            let response = coordinator_submit_request(
                store,
                manager,
                CoordinatorSubmitRequest::Search {
                    query: query.clone(),
                },
                ExecutionMode::Detach,
            )?;
            if let Some(task_id) = response.task_id {
                self.remote_search_task_ids.push(task_id);
            }
        }

        self.last_remote_search_query = Some(query);
        self.pending_remote_search_at = None;
        Ok(())
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(std::io::stdout(), LeaveAlternateScreen);
    }
}

pub(crate) fn run(store: Arc<SqliteStore>, no_color: bool, _quiet: bool) -> Result<(), String> {
    enable_raw_mode().map_err(|error| format!("failed to enable raw mode: {error}"))?;
    execute!(std::io::stdout(), EnterAlternateScreen)
        .map_err(|error| format!("failed to enter alternate screen: {error}"))?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)
        .map_err(|error| format!("failed to initialize TUI terminal: {error}"))?;
    terminal
        .clear()
        .map_err(|error| format!("failed to clear TUI terminal: {error}"))?;

    let mut app = AppState::new(no_color);
    app.reload(store.as_ref())?;

    while !app.should_quit {
        app.prune_toast();
        if app.splash_visible
            && let Some(timeout_ms) = SPLASH_AUTO_DISMISS_MS
            && Instant::now().duration_since(app.splash_started_at)
                >= Duration::from_millis(timeout_ms)
        {
            app.splash_visible = false;
        }
        if !app.splash_visible && app.should_reload() {
            let _ = app.reload(store.as_ref());
        }
        if !app.splash_visible {
            let _ = app.maybe_dispatch_remote_search(store.as_ref());
        }

        terminal
            .draw(|frame| render(frame, &app))
            .map_err(|error| format!("failed to draw TUI frame: {error}"))?;

        let poll_timeout = if app.splash_visible {
            Duration::from_millis(20)
        } else {
            Duration::from_millis(120)
        };
        if !event::poll(poll_timeout)
            .map_err(|error| format!("failed to poll TUI input: {error}"))?
        {
            continue;
        }
        let next_event =
            event::read().map_err(|error| format!("failed to read TUI input: {error}"))?;
        if let Event::Key(key) = next_event {
            if app.splash_visible {
                app.splash_visible = false;
                continue;
            }
            handle_key_event(&mut app, store.as_ref(), key)?;
        }
    }

    Ok(())
}

fn refresh_selected_section_context(app: &mut AppState, store: &SqliteStore) {
    if app.section == Section::Tasks {
        let _ = app.refresh_task_logs(store);
    }
    if app.section == Section::Managers {
        let _ = app.refresh_selected_manager_controls(store);
    }
    if app.section == Section::Packages {
        app.refresh_selected_rustup_toolchain_detail(false);
    }
}

fn handle_key_event(app: &mut AppState, store: &SqliteStore, key: KeyEvent) -> Result<(), String> {
    if app.show_help {
        if matches!(
            key.code,
            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
        ) {
            app.show_help = false;
        }
        return Ok(());
    }

    if app.show_palette {
        match key.code {
            KeyCode::Esc => app.show_palette = false,
            KeyCode::Up | KeyCode::Char('k') => {
                app.palette_index = app.palette_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.palette_index =
                    (app.palette_index + 1).min(PALETTE_ACTIONS.len().saturating_sub(1));
            }
            KeyCode::Enter => {
                execute_palette_action(app, store)?;
                app.show_palette = false;
            }
            _ => {}
        }
        return Ok(());
    }

    if let Some(action) = app.confirm_action.clone() {
        match key.code {
            KeyCode::Esc => app.confirm_action = None,
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let should_force_rustup_detail_refresh =
                    matches!(action, ConfirmAction::RustupToolchainMutation { .. });
                app.confirm_action = None;
                match execute_confirmed_action(store, action) {
                    Ok(message) => {
                        app.note_success(message);
                        if app.section == Section::Settings {
                            let _ = app.run_self_update_check();
                        }
                    }
                    Err(error) => app.note_error(error),
                }
                app.reload(store)?;
                if should_force_rustup_detail_refresh {
                    app.refresh_selected_rustup_toolchain_detail(true);
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => app.confirm_action = None,
            _ => {}
        }
        return Ok(());
    }

    if app.input_mode == InputMode::Search {
        match key.code {
            KeyCode::Esc => app.input_mode = InputMode::Normal,
            KeyCode::Enter => app.input_mode = InputMode::Normal,
            KeyCode::Backspace => {
                apply_filter_backspace(app);
                app.mark_filter_changed();
                let _ = app.reload(store);
            }
            KeyCode::Char(ch) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(());
                }
                app.filter_query.push(ch);
                app.mark_filter_changed();
                app.clamp_cursors();
                let _ = app.reload(store);
            }
            _ => {}
        }
        return Ok(());
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('k')) {
        app.show_palette = true;
        app.palette_index = 0;
        return Ok(());
    }

    match key.code {
        KeyCode::Char('?') => app.show_help = true,
        KeyCode::Tab => {
            let next = (app.section.index() + 1) % Section::ALL.len();
            app.switch_section(Section::ALL[next]);
            refresh_selected_section_context(app, store);
        }
        KeyCode::BackTab => {
            let current = app.section.index();
            let previous = if current == 0 {
                Section::ALL.len().saturating_sub(1)
            } else {
                current - 1
            };
            app.switch_section(Section::ALL[previous]);
            refresh_selected_section_context(app, store);
        }
        KeyCode::Char('/') => {
            app.input_mode = InputMode::Search;
        }
        KeyCode::Char('1') => app.switch_section(Section::Updates),
        KeyCode::Char('2') => {
            app.switch_section(Section::Packages);
            refresh_selected_section_context(app, store);
        }
        KeyCode::Char('3') => {
            app.switch_section(Section::Tasks);
            let _ = app.refresh_task_logs(store);
        }
        KeyCode::Char('4') => {
            app.switch_section(Section::Managers);
            let _ = app.refresh_selected_manager_controls(store);
        }
        KeyCode::Char('5') => app.switch_section(Section::Settings),
        KeyCode::Char('6') => app.switch_section(Section::Diagnostics),
        KeyCode::Down | KeyCode::Char('j') => {
            app.move_cursor(1);
            refresh_selected_section_context(app, store);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.move_cursor(-1);
            refresh_selected_section_context(app, store);
        }
        KeyCode::PageDown => {
            app.move_cursor(10);
            refresh_selected_section_context(app, store);
        }
        KeyCode::PageUp => {
            app.move_cursor(-10);
            refresh_selected_section_context(app, store);
        }
        KeyCode::Char('g') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
            {
                let preference_key = package_manager_preference_key(
                    package.package_name.as_str(),
                    package
                        .installed_version
                        .as_deref()
                        .or(package.candidate_version.as_deref()),
                );
                match store
                    .set_package_manager_preference(preference_key.as_str(), Some(package.manager))
                {
                    Ok(()) => {
                        app.note_success(format!(
                            "Preferred manager for '{}' set to {}.",
                            package.package_name,
                            package.manager.as_str()
                        ));
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(format!(
                        "failed to set preferred manager for '{}': {}",
                        package.package_name, error
                    )),
                }
            } else {
                app.move_cursor(-(usize::MAX as i32 / 2));
                refresh_selected_section_context(app, store);
            }
        }
        KeyCode::Char('G') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
            {
                let preference_key = package_manager_preference_key(
                    package.package_name.as_str(),
                    package
                        .installed_version
                        .as_deref()
                        .or(package.candidate_version.as_deref()),
                );
                match store.set_package_manager_preference(preference_key.as_str(), None) {
                    Ok(()) => {
                        app.note_success(format!(
                            "Preferred manager for '{}' cleared.",
                            package.package_name
                        ));
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(format!(
                        "failed to clear preferred manager for '{}': {}",
                        package.package_name, error
                    )),
                }
            } else {
                app.move_cursor(i32::MAX / 2);
                refresh_selected_section_context(app, store);
            }
        }
        KeyCode::Home => {
            app.move_cursor(-(usize::MAX as i32 / 2));
            refresh_selected_section_context(app, store);
        }
        KeyCode::End => {
            app.move_cursor(i32::MAX / 2);
            refresh_selected_section_context(app, store);
        }
        KeyCode::Char('r') => {
            match coordinator_start_workflow(
                store,
                CoordinatorWorkflowRequest::RefreshAll,
                ExecutionMode::Detach,
            ) {
                Ok(response) => {
                    let message = response
                        .job_id
                        .map(|job| format!("Refresh workflow submitted (job {}).", job))
                        .unwrap_or_else(|| "Refresh workflow submitted.".to_string());
                    app.note_success(message);
                    app.reload(store)?;
                }
                Err(error) => app.note_error(error),
            }
        }
        KeyCode::Char('d') => {
            match coordinator_start_workflow(
                store,
                CoordinatorWorkflowRequest::DetectAll,
                ExecutionMode::Detach,
            ) {
                Ok(response) => {
                    let message = response
                        .job_id
                        .map(|job| format!("Detection workflow submitted (job {}).", job))
                        .unwrap_or_else(|| "Detection workflow submitted.".to_string());
                    app.note_success(message);
                    app.reload(store)?;
                }
                Err(error) => app.note_error(error),
            }
        }
        KeyCode::Char('u') => match app.section {
            Section::Updates => {
                if let Some(update) = app.selected_update() {
                    app.confirm_action = Some(ConfirmAction::UpgradePackage {
                        manager: update.package.manager,
                        package_name: update.package.name.clone(),
                    });
                }
            }
            Section::Packages => {
                if let Some(package) = app.selected_package()
                    && package.kind == PackageRowKind::Installed
                {
                    app.confirm_action = Some(ConfirmAction::UpgradePackage {
                        manager: package.manager,
                        package_name: package.package_name.clone(),
                    });
                }
            }
            Section::Managers => {
                if let Some(manager) = app.selected_manager()
                    && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
                {
                    app.confirm_action = Some(ConfirmAction::ManagerMutation {
                        manager: manager_id,
                        subcommand: "update",
                        allow_unknown_provenance: false,
                        uninstall_options:
                            helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
                        uninstall_preview_summary: None,
                    });
                }
            }
            Section::Settings => {
                app.confirm_action = Some(ConfirmAction::SelfUpdate { force: false });
            }
            _ => {}
        },
        KeyCode::Char('x') => match app.section {
            Section::Packages => {
                if let Some(package) = app.selected_package()
                    && package.kind == PackageRowKind::Installed
                {
                    match prepare_package_uninstall_confirm_action(
                        store,
                        package.manager,
                        package.package_name.clone(),
                    ) {
                        Ok(action) => app.confirm_action = Some(action),
                        Err(error) => app.note_error(error),
                    }
                }
            }
            Section::Managers => {
                if let Some(manager) = app.selected_manager()
                    && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
                {
                    match prepare_manager_uninstall_confirm_action(
                        store,
                        manager_id,
                        false,
                        helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
                    ) {
                        Ok(action) => app.confirm_action = Some(action),
                        Err(error) => app.note_error(error),
                    }
                }
            }
            _ => {}
        },
        KeyCode::Char('X') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match prepare_manager_uninstall_confirm_action(
                    store,
                    manager_id,
                    true,
                    helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
                ) {
                    Ok(action) => app.confirm_action = Some(action),
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('z') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                if manager_id == ManagerId::Mise {
                    let options = mise_uninstall_options_full_cleanup(
                        helm_core::manager_lifecycle::MiseUninstallConfigRemoval::KeepConfig,
                    );
                    match prepare_manager_uninstall_confirm_action(
                        store, manager_id, false, options,
                    ) {
                        Ok(action) => app.confirm_action = Some(action),
                        Err(error) => app.note_error(error),
                    }
                } else {
                    let options = homebrew_uninstall_options_full_cleanup();
                    match prepare_manager_uninstall_confirm_action(
                        store, manager_id, false, options,
                    ) {
                        Ok(action) => app.confirm_action = Some(action),
                        Err(error) => app.note_error(error),
                    }
                }
            }
        }
        KeyCode::Char('Z') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                if manager_id == ManagerId::Mise {
                    let options = mise_uninstall_options_full_cleanup(
                        helm_core::manager_lifecycle::MiseUninstallConfigRemoval::RemoveConfig,
                    );
                    match prepare_manager_uninstall_confirm_action(
                        store, manager_id, false, options,
                    ) {
                        Ok(action) => app.confirm_action = Some(action),
                        Err(error) => app.note_error(error),
                    }
                } else {
                    let options = homebrew_uninstall_options_full_cleanup();
                    match prepare_manager_uninstall_confirm_action(
                        store, manager_id, false, options,
                    ) {
                        Ok(action) => app.confirm_action = Some(action),
                        Err(error) => app.note_error(error),
                    }
                }
            }
        }
        KeyCode::Char('p') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.kind == PackageRowKind::Installed
            {
                app.confirm_action = Some(ConfirmAction::TogglePin {
                    manager: package.manager,
                    package_name: package.package_name.clone(),
                    pinned: package.pinned,
                });
            }
        }
        KeyCode::Char('c') => {
            if app.section == Section::Tasks
                && let Some(task) = app.selected_task()
            {
                app.confirm_action = Some(ConfirmAction::CancelTask { task_id: task.id });
            } else if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
            {
                app.move_rustup_component_cursor(1);
            }
        }
        KeyCode::Char('C') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
            {
                app.move_rustup_component_cursor(-1);
            }
        }
        KeyCode::Char('t') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
            {
                app.move_rustup_target_cursor(1);
            }
        }
        KeyCode::Char('T') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
            {
                app.move_rustup_target_cursor(-1);
            }
        }
        KeyCode::Char('b') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
                && let Some(component) = app.selected_rustup_component()
            {
                let toolchain = package.package_name.clone();
                let component_name = component.name.clone();
                let (request, prompt, success_message) = if component.installed {
                    (
                        CoordinatorSubmitRequest::RustupRemoveComponent {
                            toolchain: toolchain.clone(),
                            component: component_name.clone(),
                        },
                        format!(
                            "Remove rustup component '{}' from '{}' ? [Enter confirm / Esc cancel]",
                            component_name, toolchain
                        ),
                        format!(
                            "Rustup component '{}' removal requested for '{}'",
                            component_name, toolchain
                        ),
                    )
                } else {
                    (
                        CoordinatorSubmitRequest::RustupAddComponent {
                            toolchain: toolchain.clone(),
                            component: component_name.clone(),
                        },
                        format!(
                            "Add rustup component '{}' to '{}' ? [Enter confirm / Esc cancel]",
                            component_name, toolchain
                        ),
                        format!(
                            "Rustup component '{}' add requested for '{}'",
                            component_name, toolchain
                        ),
                    )
                };
                app.confirm_action = Some(ConfirmAction::RustupToolchainMutation {
                    request,
                    prompt,
                    success_message,
                });
            }
        }
        KeyCode::Char('B') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
                && let Some(target) = app.selected_rustup_target()
            {
                let toolchain = package.package_name.clone();
                let target_name = target.name.clone();
                let (request, prompt, success_message) = if target.installed {
                    (
                        CoordinatorSubmitRequest::RustupRemoveTarget {
                            toolchain: toolchain.clone(),
                            target: target_name.clone(),
                        },
                        format!(
                            "Remove rustup target '{}' from '{}' ? [Enter confirm / Esc cancel]",
                            target_name, toolchain
                        ),
                        format!(
                            "Rustup target '{}' removal requested for '{}'",
                            target_name, toolchain
                        ),
                    )
                } else {
                    (
                        CoordinatorSubmitRequest::RustupAddTarget {
                            toolchain: toolchain.clone(),
                            target: target_name.clone(),
                        },
                        format!(
                            "Add rustup target '{}' to '{}' ? [Enter confirm / Esc cancel]",
                            target_name, toolchain
                        ),
                        format!(
                            "Rustup target '{}' add requested for '{}'",
                            target_name, toolchain
                        ),
                    )
                };
                app.confirm_action = Some(ConfirmAction::RustupToolchainMutation {
                    request,
                    prompt,
                    success_message,
                });
            }
        }
        KeyCode::Char('e') => match app.section {
            Section::Managers => {
                if let Some(manager) = app.selected_manager()
                    && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
                {
                    app.confirm_action = Some(ConfirmAction::ToggleManager {
                        manager: manager_id,
                        enable: !manager.enabled,
                    });
                }
            }
            Section::Settings => {
                if let Some((key, value)) = app.selected_settings_entry()
                    && (key == "safe_mode"
                        || key == "homebrew_keg_auto_cleanup"
                        || key == "auto_check_for_updates")
                {
                    let next = value != "true";
                    let rendered = if next { "true" } else { "false" };
                    match write_setting(store, key.as_str(), rendered) {
                        Ok(()) => {
                            app.note_success(format!("{key} set to {rendered}"));
                            app.reload(store)?;
                        }
                        Err(error) => app.note_error(error),
                    }
                }
            }
            _ => {}
        },
        KeyCode::Char('i') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.kind == PackageRowKind::Available
            {
                app.confirm_action = Some(ConfirmAction::InstallPackage {
                    manager: package.manager,
                    package_name: package.package_name.clone(),
                });
            } else if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                app.confirm_action = Some(ConfirmAction::ManagerMutation {
                    manager: manager_id,
                    subcommand: "install",
                    allow_unknown_provenance: false,
                    uninstall_options:
                        helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
                    uninstall_preview_summary: None,
                });
            }
        }
        KeyCode::Char('D') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match coordinator_submit_request(
                    store,
                    manager_id,
                    CoordinatorSubmitRequest::Detect,
                    ExecutionMode::Wait,
                ) {
                    Ok(response) => {
                        app.note_success(format!(
                            "Detection requested for '{}' (task #{}).",
                            manager_id.as_str(),
                            response.task_id.unwrap_or(0)
                        ));
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('o') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match cycle_manager_executable_selection(store, manager_id, 1) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('O') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match cycle_manager_executable_selection(store, manager_id, -1) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('m') => {
            if app.section == Section::Updates {
                let scope = app.cycle_updates_manager_scope(1);
                let rendered = scope
                    .map(|manager| manager.as_str().to_string())
                    .unwrap_or_else(|| "all".to_string());
                app.note_success(format!("Upgrade-all manager scope set to {}.", rendered));
                app.reload(store)?;
            } else if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match cycle_manager_install_method_selection(store, manager_id, 1) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('M') => {
            if app.section == Section::Updates {
                let scope = app.cycle_updates_manager_scope(-1);
                let rendered = scope
                    .map(|manager| manager.as_str().to_string())
                    .unwrap_or_else(|| "all".to_string());
                app.note_success(format!("Upgrade-all manager scope set to {}.", rendered));
                app.reload(store)?;
            } else if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match cycle_manager_install_method_selection(store, manager_id, -1) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('[') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match shift_manager_priority(store, manager_id, -1) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char(']') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match shift_manager_priority(store, manager_id, 1) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            if app.section == Section::Settings
                && let Some((key, value)) = app.selected_settings_entry()
                && key == "auto_check_frequency_minutes"
            {
                let current = value.parse::<u32>().unwrap_or(1440);
                let next = (current.saturating_add(60)).max(1);
                match write_setting(store, "auto_check_frequency_minutes", &next.to_string()) {
                    Ok(()) => {
                        app.note_success(format!("auto_check_frequency_minutes set to {}", next));
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('-') => {
            if app.section == Section::Settings
                && let Some((key, value)) = app.selected_settings_entry()
                && key == "auto_check_frequency_minutes"
            {
                let current = value.parse::<u32>().unwrap_or(1440);
                let next = current.saturating_sub(60).max(1);
                match write_setting(store, "auto_check_frequency_minutes", &next.to_string()) {
                    Ok(()) => {
                        app.note_success(format!("auto_check_frequency_minutes set to {}", next));
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('K') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::HomebrewFormula
            {
                match cycle_package_keg_policy(store, package) {
                    Ok((next_policy, _message)) => {
                        app.confirm_action = Some(ConfirmAction::SetPackageKegPolicy {
                            package_name: package.package_name.clone(),
                            policy: next_policy,
                        });
                    }
                    Err(error) => app.note_error(error),
                }
            } else if app.section == Section::Settings {
                match app.run_self_update_check() {
                    Ok(message) => {
                        app.note_success(message);
                    }
                    Err(error) => {
                        app.self_update.last_error = Some(error.clone());
                        app.note_error(error);
                    }
                }
            }
        }
        KeyCode::Char('U') => {
            if app.section == Section::Settings {
                app.confirm_action = Some(ConfirmAction::SelfUpdate { force: true });
            }
        }
        KeyCode::Char('I') => {
            if app.section == Section::Updates {
                app.updates_include_pinned = !app.updates_include_pinned;
                app.note_success(format!(
                    "Upgrade-all include_pinned set to {}.",
                    app.updates_include_pinned
                ));
            }
        }
        KeyCode::Char('S') => {
            if app.section == Section::Updates {
                app.updates_allow_os_updates = !app.updates_allow_os_updates;
                app.note_success(format!(
                    "Upgrade-all allow_os_updates set to {}.",
                    app.updates_allow_os_updates
                ));
            }
        }
        KeyCode::Char('s') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
                && !package.runtime_state.is_default
            {
                let toolchain = package.package_name.clone();
                app.confirm_action = Some(ConfirmAction::RustupToolchainMutation {
                    request: CoordinatorSubmitRequest::RustupSetDefaultToolchain {
                        toolchain: toolchain.clone(),
                    },
                    prompt: format!(
                        "Set '{}' as the rustup default toolchain? [Enter confirm / Esc cancel]",
                        toolchain
                    ),
                    success_message: format!(
                        "Rustup default toolchain change requested for '{}'",
                        toolchain
                    ),
                });
            }
        }
        KeyCode::Char('w') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
            {
                match std::env::current_dir() {
                    Ok(path) => {
                        let rendered_path = path.to_string_lossy().to_string();
                        let toolchain = package.package_name.clone();
                        app.confirm_action = Some(ConfirmAction::RustupToolchainMutation {
                            request: CoordinatorSubmitRequest::RustupSetOverride {
                                toolchain: toolchain.clone(),
                                path: rendered_path.clone(),
                            },
                            prompt: format!(
                                "Set rustup override at '{}' to '{}' ? [Enter confirm / Esc cancel]",
                                rendered_path, toolchain
                            ),
                            success_message: format!(
                                "Rustup override set requested for '{}' at '{}'",
                                toolchain, rendered_path
                            ),
                        });
                    }
                    Err(error) => app.note_error(format!(
                        "failed to resolve current working directory: {}",
                        error
                    )),
                }
            }
        }
        KeyCode::Char('W') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
            {
                match std::env::current_dir() {
                    Ok(path) => {
                        let rendered_path = path.to_string_lossy().to_string();
                        let has_override = app
                            .selected_rustup_toolchain_detail
                            .as_ref()
                            .is_some_and(|detail| {
                                detail
                                    .override_paths
                                    .iter()
                                    .any(|entry| entry == &rendered_path)
                            });
                        if has_override {
                            let toolchain = package.package_name.clone();
                            app.confirm_action = Some(ConfirmAction::RustupToolchainMutation {
                                request: CoordinatorSubmitRequest::RustupUnsetOverride {
                                    toolchain: toolchain.clone(),
                                    path: rendered_path.clone(),
                                },
                                prompt: format!(
                                    "Clear rustup override at '{}' for '{}' ? [Enter confirm / Esc cancel]",
                                    rendered_path, toolchain
                                ),
                                success_message: format!(
                                    "Rustup override clear requested for '{}' at '{}'",
                                    toolchain, rendered_path
                                ),
                            });
                        } else {
                            app.note_error(format!(
                                "no rustup override for the current directory ('{}')",
                                rendered_path
                            ));
                        }
                    }
                    Err(error) => app.note_error(format!(
                        "failed to resolve current working directory: {}",
                        error
                    )),
                }
            }
        }
        KeyCode::Char('P') => {
            if app.section == Section::Packages
                && let Some(package) = app.selected_package()
                && package.manager == ManagerId::Rustup
                && package.kind != PackageRowKind::Available
            {
                let current_profile = app
                    .selected_rustup_toolchain_detail
                    .as_ref()
                    .and_then(|detail| detail.current_profile.clone())
                    .unwrap_or_else(|| "default".to_string());
                let profiles = ["minimal", "default", "complete"];
                let current_index = profiles
                    .iter()
                    .position(|candidate| candidate.eq_ignore_ascii_case(current_profile.as_str()))
                    .unwrap_or(1);
                let next_profile = profiles[(current_index + 1) % profiles.len()].to_string();
                app.confirm_action = Some(ConfirmAction::RustupToolchainMutation {
                    request: CoordinatorSubmitRequest::RustupSetProfile {
                        profile: next_profile.clone(),
                    },
                    prompt: format!(
                        "Set rustup profile to '{}' ? [Enter confirm / Esc cancel]",
                        next_profile
                    ),
                    success_message: format!(
                        "Rustup profile change requested to '{}'",
                        next_profile
                    ),
                });
            }
        }
        KeyCode::Char('E') => {
            if app.section == Section::Diagnostics {
                match export_diagnostics_snapshot(store) {
                    Ok(message) => app.note_success(message),
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('a') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match acknowledge_manager_multi_instance_state(store, manager_id) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            } else if app.section == Section::Updates {
                app.confirm_action = Some(ConfirmAction::UpgradeAllWithOptions {
                    include_pinned: app.updates_include_pinned,
                    allow_os_updates: app.updates_allow_os_updates,
                    manager_scope: app.updates_manager_scope,
                });
            }
        }
        KeyCode::Char('A') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match clear_manager_multi_instance_ack(store, manager_id) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('v') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match cycle_manager_active_install_instance(store, manager_id, 1) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Char('V') => {
            if app.section == Section::Managers
                && let Some(manager) = app.selected_manager()
                && let Ok(manager_id) = manager.manager_id.parse::<ManagerId>()
            {
                match cycle_manager_active_install_instance(store, manager_id, -1) {
                    Ok(message) => {
                        app.note_success(message);
                        app.reload(store)?;
                    }
                    Err(error) => app.note_error(error),
                }
            }
        }
        KeyCode::Esc => {
            if app.is_filter_active() {
                app.filter_query.clear();
                app.mark_filter_changed();
                let _ = app.reload(store);
                app.clamp_cursors();
            }
        }
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
    Ok(())
}

fn execute_confirmed_action(store: &SqliteStore, action: ConfirmAction) -> Result<String, String> {
    match action {
        ConfirmAction::InstallPackage {
            manager,
            package_name,
        } => {
            let response = coordinator_submit_request(
                store,
                manager,
                CoordinatorSubmitRequest::Install {
                    package_name: package_name.clone(),
                    version: None,
                },
                ExecutionMode::Wait,
            )?;
            Ok(format!(
                "Install requested for '{}@{}' (task #{}).",
                package_name,
                manager.as_str(),
                response.task_id.unwrap_or(0)
            ))
        }
        ConfirmAction::UpgradePackage {
            manager,
            package_name,
        } => {
            let response = coordinator_submit_request(
                store,
                manager,
                CoordinatorSubmitRequest::Upgrade {
                    package_name: Some(package_name.clone()),
                },
                ExecutionMode::Wait,
            )?;
            Ok(format!(
                "Upgrade requested for '{}@{}' (task #{}).",
                package_name,
                manager.as_str(),
                response.task_id.unwrap_or(0)
            ))
        }
        ConfirmAction::UninstallPackage {
            manager,
            package_name,
            uninstall_preview_summary: _,
        } => {
            let response = coordinator_submit_request(
                store,
                manager,
                CoordinatorSubmitRequest::Uninstall {
                    package_name: package_name.clone(),
                },
                ExecutionMode::Wait,
            )?;
            Ok(format!(
                "Uninstall requested for '{}@{}' (task #{}).",
                package_name,
                manager.as_str(),
                response.task_id.unwrap_or(0)
            ))
        }
        ConfirmAction::TogglePin {
            manager,
            package_name,
            pinned,
        } => {
            let supports_native_pin = registry::manager(manager)
                .map(|descriptor| descriptor.capabilities.contains(&Capability::Pin))
                .unwrap_or(false);
            let supports_native_unpin = registry::manager(manager)
                .map(|descriptor| descriptor.capabilities.contains(&Capability::Unpin))
                .unwrap_or(false);

            if pinned {
                if supports_native_unpin {
                    let response = coordinator_submit_request(
                        store,
                        manager,
                        CoordinatorSubmitRequest::Unpin {
                            package_name: package_name.clone(),
                        },
                        ExecutionMode::Wait,
                    )?;
                    Ok(format!(
                        "Unpin requested for '{}@{}' (task #{}).",
                        package_name,
                        manager.as_str(),
                        response.task_id.unwrap_or(0)
                    ))
                } else {
                    let package = PackageRef {
                        manager,
                        name: package_name.clone(),
                    };
                    let package_key = format!("{}:{}", manager.as_str(), package_name);
                    store
                        .remove_pin(&package_key)
                        .map_err(|error| format!("failed to remove pin record: {error}"))?;
                    store
                        .set_snapshot_pinned(&package, false)
                        .map_err(|error| {
                            format!("failed to unmark package pinned in snapshot: {error}")
                        })?;
                    Ok(format!(
                        "Virtual unpin applied for '{}@{}'.",
                        package_name,
                        manager.as_str()
                    ))
                }
            } else if supports_native_pin {
                let response = coordinator_submit_request(
                    store,
                    manager,
                    CoordinatorSubmitRequest::Pin {
                        package_name: package_name.clone(),
                        version: None,
                    },
                    ExecutionMode::Wait,
                )?;
                Ok(format!(
                    "Pin requested for '{}@{}' (task #{}).",
                    package_name,
                    manager.as_str(),
                    response.task_id.unwrap_or(0)
                ))
            } else {
                let package = PackageRef {
                    manager,
                    name: package_name.clone(),
                };
                store
                    .upsert_pin(&PinRecord {
                        package: package.clone(),
                        kind: PinKind::Virtual,
                        pinned_version: None,
                        created_at: SystemTime::now(),
                    })
                    .map_err(|error| format!("failed to persist pin record: {error}"))?;
                store.set_snapshot_pinned(&package, true).map_err(|error| {
                    format!("failed to mark package pinned in snapshot: {error}")
                })?;
                Ok(format!(
                    "Virtual pin applied for '{}@{}'.",
                    package_name,
                    manager.as_str()
                ))
            }
        }
        ConfirmAction::SetPackageKegPolicy {
            package_name,
            policy,
        } => {
            let package = PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: package_name.clone(),
            };
            store
                .set_package_keg_policy(&package, policy)
                .map_err(|error| format!("failed to persist package keg policy: {error}"))?;
            let rendered = policy
                .map(|value| value.as_str().to_string())
                .unwrap_or_else(|| "default".to_string());
            Ok(format!(
                "Homebrew keg policy for '{}' set to '{}'.",
                package_name, rendered
            ))
        }
        ConfirmAction::ManagerMutation {
            manager,
            subcommand,
            allow_unknown_provenance,
            uninstall_options,
            uninstall_preview_summary: _,
        } => {
            if subcommand == "uninstall" {
                let plan = build_manager_uninstall_plan_with_options(
                    store,
                    manager,
                    allow_unknown_provenance,
                    false,
                    uninstall_options,
                )?;
                let submit_request = adapter_request_to_coordinator_submit(plan.request)?;
                let response = coordinator_submit_request(
                    store,
                    plan.target_manager,
                    submit_request,
                    ExecutionMode::Wait,
                )?;
                return Ok(format!(
                    "Manager '{}' uninstall submitted via '{}' (task #{}). strategy={} blast_radius={} requires_confirmation={}",
                    manager.as_str(),
                    plan.target_manager.as_str(),
                    response.task_id.unwrap_or(0),
                    plan.preview.strategy,
                    plan.preview.blast_radius_score,
                    plan.preview.requires_yes
                ));
            }

            let install_method_override = if subcommand == "install" {
                resolve_install_method_override_for_tui(store, manager)?
            } else {
                None
            };
            let (target_manager, request) = build_manager_mutation_request(
                store,
                manager,
                subcommand,
                install_method_override,
            )?;
            let submit_request = adapter_request_to_coordinator_submit(request)?;
            let response = coordinator_submit_request(
                store,
                target_manager,
                submit_request,
                ExecutionMode::Wait,
            )?;
            Ok(format!(
                "Manager '{}' {} submitted via '{}' (task #{}).",
                manager.as_str(),
                subcommand,
                target_manager.as_str(),
                response.task_id.unwrap_or(0)
            ))
        }
        ConfirmAction::CancelTask { task_id } => {
            coordinator_cancel_task(task_id)?;
            Ok(format!("Cancellation requested for task #{}.", task_id))
        }
        ConfirmAction::ToggleManager { manager, enable } => {
            if enable {
                let eligibility = manager_enablement_eligibility_for_store(store, manager)?;
                if !eligibility.is_eligible {
                    let reason = eligibility.reason_message.unwrap_or(
                        "manager is not eligible to be enabled with the current executable selection",
                    );
                    let code = eligibility.reason_code.unwrap_or("manager.ineligible");
                    return Err(format!("{reason} (reason_code={code})"));
                }
            } else {
                let enabled_map = manager_enabled_map(store)?;
                let dependents = enabled_dependents_for_manager(store, &enabled_map, manager)?;
                if !dependents.is_empty() {
                    return Err(format!(
                        "cannot disable manager '{}': enabled managers depend on it ({})",
                        manager.as_str(),
                        dependents
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
            store
                .set_manager_enabled(manager, enable)
                .map_err(|error| format!("failed to set manager enabled state: {error}"))?;
            let mut message = format!(
                "Manager '{}' is now {}.",
                manager.as_str(),
                if enable { "enabled" } else { "disabled" }
            );
            if !enable {
                let (cancelled, warnings) = cancel_inflight_tasks_for_manager(store, manager);
                if !cancelled.is_empty() {
                    message.push_str(format!(" cancelled tasks: {:?}", cancelled).as_str());
                }
                if !warnings.is_empty() {
                    message.push_str(format!(" warnings: {}", warnings.join("; ")).as_str());
                }
            }
            Ok(message)
        }
        ConfirmAction::UpgradeAllWithOptions {
            include_pinned,
            allow_os_updates,
            manager_scope,
        } => {
            let response = coordinator_start_workflow(
                store,
                CoordinatorWorkflowRequest::UpdatesRun {
                    include_pinned,
                    allow_os_updates,
                    manager_id: manager_scope.map(|manager| manager.as_str().to_string()),
                },
                ExecutionMode::Detach,
            )?;
            Ok(response
                .job_id
                .map(|job| format!("Upgrade workflow submitted (job {}).", job))
                .unwrap_or_else(|| "Upgrade workflow submitted.".to_string()))
        }
        ConfirmAction::RustupToolchainMutation {
            request,
            success_message,
            ..
        } => {
            let response =
                coordinator_submit_request(store, ManagerId::Rustup, request, ExecutionMode::Wait)?;
            Ok(format!(
                "{} (task #{}).",
                success_message,
                response.task_id.unwrap_or(0)
            ))
        }
        ConfirmAction::SelfUpdate { force } => apply_self_update(force),
    }
}

fn prepare_manager_uninstall_confirm_action(
    store: &SqliteStore,
    manager: ManagerId,
    allow_unknown_provenance: bool,
    uninstall_options: helm_core::manager_lifecycle::ManagerUninstallOptions,
) -> Result<ConfirmAction, String> {
    let plan = build_manager_uninstall_plan_with_options(
        store,
        manager,
        allow_unknown_provenance,
        true,
        uninstall_options.clone(),
    )?;
    let summary = format!(
        "[strategy={} provenance={} confidence={} blast_radius={}]",
        plan.preview.strategy,
        plan.preview.provenance.as_deref().unwrap_or("-"),
        plan.preview
            .confidence
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string()),
        plan.preview.blast_radius_score
    );
    Ok(ConfirmAction::ManagerMutation {
        manager,
        subcommand: "uninstall",
        allow_unknown_provenance,
        uninstall_options,
        uninstall_preview_summary: Some(summary),
    })
}

fn prepare_package_uninstall_confirm_action(
    store: &SqliteStore,
    manager: ManagerId,
    package_name: String,
) -> Result<ConfirmAction, String> {
    let package = PackageRef {
        manager,
        name: package_name.clone(),
    };
    let preview = build_package_uninstall_preview_for_package(store, &package)?;

    if preview.manager_automation_level.as_deref() == Some("read_only") {
        return Err(format!(
            "package uninstall is blocked because manager '{}' automation is read-only",
            manager.as_str()
        ));
    }

    let summary = format!(
        "[manager_strategy={} manager_provenance={} blast_radius={} requires_confirmation={}]",
        preview.manager_uninstall_strategy.as_deref().unwrap_or("-"),
        preview.manager_provenance.as_deref().unwrap_or("-"),
        preview.blast_radius_score,
        preview.requires_yes
    );
    Ok(ConfirmAction::UninstallPackage {
        manager,
        package_name,
        uninstall_preview_summary: Some(summary),
    })
}

fn execute_palette_action(app: &mut AppState, store: &SqliteStore) -> Result<(), String> {
    let action = PALETTE_ACTIONS
        .get(app.palette_index)
        .copied()
        .unwrap_or(PaletteAction::RefreshAll);
    match action {
        PaletteAction::RefreshAll => {
            let response = coordinator_start_workflow(
                store,
                CoordinatorWorkflowRequest::RefreshAll,
                ExecutionMode::Detach,
            )?;
            app.note_success(
                response
                    .job_id
                    .map(|job| format!("Refresh workflow submitted (job {}).", job))
                    .unwrap_or_else(|| "Refresh workflow submitted.".to_string()),
            );
            app.reload(store)?;
        }
        PaletteAction::DetectAll => {
            let response = coordinator_start_workflow(
                store,
                CoordinatorWorkflowRequest::DetectAll,
                ExecutionMode::Detach,
            )?;
            app.note_success(
                response
                    .job_id
                    .map(|job| format!("Detection workflow submitted (job {}).", job))
                    .unwrap_or_else(|| "Detection workflow submitted.".to_string()),
            );
            app.reload(store)?;
        }
        PaletteAction::Switch(section) => app.switch_section(section),
        PaletteAction::UpgradeAll => {
            app.confirm_action = Some(ConfirmAction::UpgradeAllWithOptions {
                include_pinned: app.updates_include_pinned,
                allow_os_updates: app.updates_allow_os_updates,
                manager_scope: app.updates_manager_scope,
            })
        }
        PaletteAction::Quit => app.should_quit = true,
    }
    Ok(())
}

fn chrono_like_unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

fn normalized_nonempty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn next_choice_index(current: usize, len: usize, direction: i32) -> usize {
    if len == 0 {
        return 0;
    }
    if direction >= 0 {
        (current + (direction as usize % len)) % len
    } else {
        let delta = ((-direction) as usize) % len;
        (current + len - delta) % len
    }
}

fn cycle_manager_executable_selection(
    store: &SqliteStore,
    manager: ManagerId,
    direction: i32,
) -> Result<String, String> {
    let details = manager_executable_status(store, manager)?;
    let mut choices: Vec<Option<String>> = vec![None];
    for path in details.executable_paths {
        if path.trim().is_empty() {
            continue;
        }
        if choices
            .iter()
            .any(|candidate| candidate.as_deref() == Some(path.as_str()))
        {
            continue;
        }
        choices.push(Some(path));
    }
    if choices.len() <= 1 {
        return Err(format!(
            "manager '{}' has no alternate executable paths to select",
            manager.as_str()
        ));
    }

    let selected_preference = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?
        .into_iter()
        .find(|preference| preference.manager == manager)
        .and_then(|preference| normalized_nonempty(preference.selected_executable_path));
    let current_index = choices
        .iter()
        .position(|candidate| candidate.as_ref() == selected_preference.as_ref())
        .unwrap_or(0);
    let next_index = next_choice_index(current_index, choices.len(), direction);
    let next = choices[next_index].clone();
    store
        .set_manager_selected_executable_path(manager, next.as_deref())
        .map_err(|error| format!("failed to set manager executable path: {error}"))?;

    if let Some(path) = next {
        Ok(format!(
            "Manager '{}' executable path set to '{}'.",
            manager.as_str(),
            path
        ))
    } else {
        Ok(format!(
            "Manager '{}' executable path reset to default resolution.",
            manager.as_str()
        ))
    }
}

fn cycle_manager_active_install_instance(
    store: &SqliteStore,
    manager: ManagerId,
    direction: i32,
) -> Result<String, String> {
    let instances = list_manager_install_instances(store, Some(manager))?;
    if instances.len() <= 1 {
        return Err(format!(
            "manager '{}' does not have multiple install instances",
            manager.as_str()
        ));
    }
    let current_index = instances
        .iter()
        .position(|instance| instance.is_active)
        .unwrap_or(0);
    let next_index = next_choice_index(current_index, instances.len(), direction);
    let target = instances
        .get(next_index)
        .ok_or_else(|| "failed to resolve next manager install instance".to_string())?;
    set_manager_active_install_instance(store, manager, target.instance_id.as_str())
}

fn cycle_manager_install_method_selection(
    store: &SqliteStore,
    manager: ManagerId,
    direction: i32,
) -> Result<String, String> {
    let details = manager_install_methods_status(store, manager)?;
    let mut choices: Vec<Option<String>> = vec![None];
    for method in details.install_methods {
        if method.trim().is_empty() {
            continue;
        }
        if choices
            .iter()
            .any(|candidate| candidate.as_deref() == Some(method.as_str()))
        {
            continue;
        }
        choices.push(Some(method));
    }
    if choices.len() <= 1 {
        return Err(format!(
            "manager '{}' has no install methods available",
            manager.as_str()
        ));
    }

    let selected_preference = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?
        .into_iter()
        .find(|preference| preference.manager == manager)
        .and_then(|preference| normalized_nonempty(preference.selected_install_method));
    let current_index = choices
        .iter()
        .position(|candidate| candidate.as_ref() == selected_preference.as_ref())
        .unwrap_or(0);
    let next_index = next_choice_index(current_index, choices.len(), direction);
    let next = choices[next_index].clone();
    store
        .set_manager_selected_install_method(manager, next.as_deref())
        .map_err(|error| format!("failed to set manager install method: {error}"))?;

    if let Some(method) = next {
        Ok(format!(
            "Manager '{}' install method set to '{}'.",
            manager.as_str(),
            method
        ))
    } else {
        Ok(format!(
            "Manager '{}' install method reset to default resolution.",
            manager.as_str()
        ))
    }
}

fn shift_manager_priority(
    store: &SqliteStore,
    manager: ManagerId,
    direction: i32,
) -> Result<String, String> {
    if direction == 0 {
        return Ok("Manager priority unchanged.".to_string());
    }

    let entries = manager_priority_entries(store)?;
    let current_id = manager.as_str().to_string();
    let current_entry = entries
        .iter()
        .find(|entry| entry.manager_id == current_id)
        .ok_or_else(|| {
            format!(
                "manager '{}' is unavailable in priority table",
                manager.as_str()
            )
        })?;
    let authority = current_entry.authority.clone();
    let mut authority_entries = entries
        .iter()
        .filter(|entry| entry.authority == authority)
        .collect::<Vec<_>>();
    authority_entries.sort_by(|left, right| {
        if left.rank != right.rank {
            return left.rank.cmp(&right.rank);
        }
        left.manager_id.cmp(&right.manager_id)
    });

    let current_pos = authority_entries
        .iter()
        .position(|entry| entry.manager_id == current_id)
        .ok_or_else(|| {
            format!(
                "manager '{}' is unavailable in '{}' authority ordering",
                manager.as_str(),
                authority
            )
        })?;
    let target_pos = if direction < 0 {
        if current_pos == 0 {
            return Ok(format!(
                "Manager '{}' is already the highest priority in '{}'.",
                manager.as_str(),
                authority
            ));
        }
        current_pos - 1
    } else {
        if current_pos + 1 >= authority_entries.len() {
            return Ok(format!(
                "Manager '{}' is already the lowest priority in '{}'.",
                manager.as_str(),
                authority
            ));
        }
        current_pos + 1
    };

    let target_rank = authority_entries[target_pos].rank;
    let new_rank = set_manager_priority_rank(store, manager, target_rank)?;
    Ok(format!(
        "Manager '{}' priority moved to rank {} in '{}'.",
        manager.as_str(),
        new_rank + 1,
        authority
    ))
}

fn cycle_package_keg_policy(
    store: &SqliteStore,
    package: &PackageRow,
) -> Result<(Option<HomebrewKegPolicy>, String), String> {
    if package.manager != ManagerId::HomebrewFormula {
        return Err(
            "keg policy overrides are only available for homebrew_formula packages".to_string(),
        );
    }

    let package_ref = PackageRef {
        manager: ManagerId::HomebrewFormula,
        name: package.package_name.clone(),
    };
    let current = store
        .package_keg_policy(&package_ref)
        .map_err(|error| format!("failed to read package keg policy: {error}"))?;
    let next = match current {
        None => Some(HomebrewKegPolicy::Cleanup),
        Some(HomebrewKegPolicy::Cleanup) => Some(HomebrewKegPolicy::Keep),
        Some(HomebrewKegPolicy::Keep) => None,
    };
    let rendered = next
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "default".to_string());
    Ok((
        next,
        format!(
            "Homebrew keg policy for '{}' queued: {}",
            package.package_name, rendered
        ),
    ))
}

fn apply_self_update(force: bool) -> Result<String, String> {
    let current_version = current_cli_version();
    let executable_path = env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    let provenance = detect_install_provenance(&executable_path);
    let can_self_update = provenance_can_self_update(provenance.update_policy);
    let recommended_action = provenance_recommended_action(provenance.channel).to_string();
    let force_direct_override = force && provenance.channel == InstallChannel::DirectScript;

    if is_running_as_root() && !env_flag_enabled(SELF_UPDATE_ALLOW_ROOT_ENV) {
        return Err(format!(
            "self update unavailable: running as root is blocked by default (set {SELF_UPDATE_ALLOW_ROOT_ENV}=1 to override)"
        ));
    }

    if provenance.channel == InstallChannel::AppBundleShim {
        return Err("self update unavailable: app-bundled CLI is channel-managed".to_string());
    }

    if provenance.update_policy == UpdatePolicy::Managed {
        return Err(
            "self update unavailable: managed policy denies direct self-update".to_string(),
        );
    }

    if !can_self_update && !force_direct_override {
        return Err(format!(
            "self update unavailable: installation is channel-managed ({})",
            recommended_action
        ));
    }

    let applied = direct_update_apply(&current_version, &provenance.executable_path)
        .map_err(|error| error.to_string())?;
    if applied.updated {
        Ok(format!(
            "Self-update completed (latest: {}).",
            applied.latest_version.unwrap_or_else(|| "-".to_string())
        ))
    } else {
        Ok(format!(
            "Already up to date (latest: {}).",
            applied.latest_version.unwrap_or_else(|| "-".to_string())
        ))
    }
}

fn diagnostics_export_payload(store: &SqliteStore) -> Result<serde_json::Value, String> {
    let summary = build_diagnostics_summary(store)?;
    let managers = list_managers(store)?;
    let tasks = store
        .list_recent_tasks(TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list recent tasks: {error}"))?
        .into_iter()
        .map(task_to_cli_task)
        .collect::<Vec<_>>();

    let failed_task_logs = tasks
        .iter()
        .filter(|task| task.status == "failed")
        .take(25)
        .map(|task| {
            let logs = store
                .list_task_logs(TaskId(task.id), 128)
                .map_err(|error| format!("failed to list task logs: {error}"))?
                .into_iter()
                .map(task_log_to_cli_record)
                .collect::<Vec<_>>();
            Ok(json!({
                "taskId": task.id,
                "logs": logs
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(json!({
        "schema": "helm.cli.v1.diagnostics.export",
        "schema_version": 1,
        "generated_at": chrono_like_unix_now(),
        "data": {
            "summary": summary,
            "managers": managers,
            "tasks": tasks,
            "failedTaskLogs": failed_task_logs
        }
    }))
}

fn default_diagnostics_export_path() -> PathBuf {
    let timestamp = chrono_like_unix_now();
    let mut path = env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    path.push(format!("helm-diagnostics-{}.json", timestamp));
    path
}

fn export_diagnostics_snapshot(store: &SqliteStore) -> Result<String, String> {
    let payload = diagnostics_export_payload(store)?;
    let rendered = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("failed to serialize diagnostics export payload: {error}"))?;
    let path = default_diagnostics_export_path();
    std::fs::write(path.as_path(), rendered).map_err(|error| {
        format!(
            "failed to write diagnostics export to '{}': {error}",
            path.display()
        )
    })?;
    Ok(format!(
        "Diagnostics export written to '{}'.",
        path.display()
    ))
}

fn render(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    if app.splash_visible {
        render_splash(frame, app);
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, layout[0], app);
    render_tabs(frame, layout[1], app);
    render_body(frame, layout[2], app);
    render_footer(frame, layout[3], app);

    if app.show_help {
        render_help_overlay(frame, app);
    }
    if app.show_palette {
        render_palette_overlay(frame, app);
    }
    if let Some(confirm_action) = app.confirm_action.as_ref() {
        render_confirm_overlay(frame, app, confirm_action.prompt().as_str());
    }
}

fn render_header(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let left = format!("Helm  v{}  |  Take the helm.", env!("CARGO_PKG_VERSION"));
    let right = format!(
        "Pkgs {}  Updates {}  Tasks {}",
        app.status.installed_packages, app.status.update_packages, app.status.running_or_queued
    );

    let header = Paragraph::new(Line::from(vec![
        Span::styled(left, app.theme.heading),
        Span::raw("  "),
        Span::styled(
            format!(
                "Enabled {}/{}",
                app.status.detected_enabled_managers, app.status.enabled_managers
            ),
            app.theme.subtle,
        ),
        Span::raw(" ".repeat(2)),
        Span::styled(right, app.theme.subtle),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Helm Control Center (TUI)"),
    );
    frame.render_widget(header, area);
}

fn render_tabs(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let titles = Section::ALL
        .iter()
        .map(|section| Line::from(Span::raw(section.title())))
        .collect::<Vec<_>>();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Sections"))
        .select(app.section.index())
        .style(app.theme.text)
        .highlight_style(app.theme.selected)
        .divider(" | ");
    frame.render_widget(tabs, area);
}

fn render_body(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(44), Constraint::Percentage(56)])
        .split(area);
    render_list_pane(frame, split[0], app);
    render_detail_pane(frame, split[1], app);
}

fn render_list_pane(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let title = if app.is_filter_active() {
        format!("{}  (filter: {})", app.section.title(), app.filter_query)
    } else {
        app.section.title().to_string()
    };

    match app.section {
        Section::Updates => {
            let indices = app.visible_update_indices();
            let items = indices
                .iter()
                .filter_map(|index| app.updates.get(*index))
                .map(|pkg| {
                    let pinned = if pkg.pinned { " [pinned]" } else { "" };
                    let restart = if pkg.restart_required {
                        " [restart]"
                    } else {
                        ""
                    };
                    ListItem::new(format!(
                        "{}@{}  {} -> {}{}{}",
                        pkg.package.name,
                        pkg.package.manager.as_str(),
                        pkg.installed_version.as_deref().unwrap_or("-"),
                        pkg.candidate_version,
                        pinned,
                        restart
                    ))
                })
                .collect::<Vec<_>>();
            render_list(
                frame,
                area,
                title.as_str(),
                items,
                app.updates_cursor.min(indices.len().saturating_sub(1)),
                app,
            );
        }
        Section::Packages => {
            let indices = app.visible_package_indices();
            let items = indices
                .iter()
                .filter_map(|index| app.package_rows.get(*index))
                .map(|row| match row.kind {
                    PackageRowKind::Installed => {
                        let pinned = if row.pinned { " [pinned]" } else { "" };
                        let preferred = if row.preferred_manager == Some(row.manager) {
                            " [preferred]"
                        } else {
                            ""
                        };
                        ListItem::new(format!(
                            "{}@{}  {}{}{}",
                            row.package_name,
                            row.manager.as_str(),
                            row.installed_version.as_deref().unwrap_or("-"),
                            pinned,
                            preferred
                        ))
                    }
                    PackageRowKind::Available => {
                        let preferred = if row.preferred_manager == Some(row.manager) {
                            " [preferred]"
                        } else {
                            ""
                        };
                        ListItem::new(format!(
                            "{}@{}  available {}{}",
                            row.package_name,
                            row.manager.as_str(),
                            row.candidate_version.as_deref().unwrap_or("-"),
                            preferred
                        ))
                    }
                })
                .collect::<Vec<_>>();
            render_list(
                frame,
                area,
                title.as_str(),
                items,
                app.packages_cursor.min(indices.len().saturating_sub(1)),
                app,
            );
        }
        Section::Tasks => {
            let indices = app.visible_task_indices();
            let items = indices
                .iter()
                .filter_map(|index| app.tasks.get(*index))
                .map(|task| {
                    ListItem::new(format!(
                        "#{}  [{}]  {}  {}",
                        task.id, task.status, task.manager, task.task_type
                    ))
                })
                .collect::<Vec<_>>();
            render_list(
                frame,
                area,
                title.as_str(),
                items,
                app.tasks_cursor.min(indices.len().saturating_sub(1)),
                app,
            );
        }
        Section::Managers => {
            let indices = app.visible_manager_indices();
            let items = indices
                .iter()
                .filter_map(|index| app.managers.get(*index))
                .map(|manager| {
                    let enabled = if manager.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    let eligibility = if manager.is_eligible {
                        "eligible"
                    } else {
                        "blocked"
                    };
                    let detected = if manager.detected {
                        "detected"
                    } else {
                        "not-detected"
                    };
                    ListItem::new(format!(
                        "{}  [{}|{}|{}]  {}",
                        manager.manager_id,
                        enabled,
                        eligibility,
                        detected,
                        manager.version.as_deref().unwrap_or("-")
                    ))
                })
                .collect::<Vec<_>>();
            render_list(
                frame,
                area,
                title.as_str(),
                items,
                app.managers_cursor.min(indices.len().saturating_sub(1)),
                app,
            );
        }
        Section::Settings => {
            let entries = app.settings_entries();
            let items = entries
                .iter()
                .map(|(key, value)| ListItem::new(format!("{key} = {value}")))
                .collect::<Vec<_>>();
            render_list(
                frame,
                area,
                title.as_str(),
                items,
                app.settings_cursor.min(entries.len().saturating_sub(1)),
                app,
            );
        }
        Section::Diagnostics => {
            let items = vec![ListItem::new(format!(
                "failed tasks: {} | running: {} | undetected managers: {}",
                app.diagnostics.failed_tasks,
                app.diagnostics.running_tasks,
                app.diagnostics.undetected_enabled_managers.len()
            ))];
            render_list(frame, area, title.as_str(), items, 0, app);
        }
    }
}

fn render_list(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    title: &str,
    items: Vec<ListItem<'_>>,
    selected: usize,
    app: &AppState,
) {
    let block = Block::default().borders(Borders::ALL).title(title);
    if items.is_empty() {
        let empty = Paragraph::new("No rows.")
            .block(block)
            .style(app.theme.subtle)
            .alignment(Alignment::Left);
        frame.render_widget(empty, area);
        return;
    }
    let list = List::new(items)
        .block(block)
        .highlight_style(app.theme.selected)
        .highlight_symbol("▶ ");
    let mut state = ListState::default().with_selected(Some(selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_detail_pane(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let mut lines: Vec<Line<'_>> = Vec::new();
    match app.section {
        Section::Updates => {
            if let Some(update) = app.selected_update() {
                lines.push(Line::from(Span::styled(
                    "Selected Update",
                    app.theme.accent,
                )));
                lines.push(Line::from(format!("Package: {}", update.package.name)));
                lines.push(Line::from(format!(
                    "Manager: {}",
                    update.package.manager.as_str()
                )));
                lines.push(Line::from(format!(
                    "Installed: {}",
                    update.installed_version.as_deref().unwrap_or("-")
                )));
                lines.push(Line::from(format!(
                    "Candidate: {}",
                    update.candidate_version
                )));
                lines.push(Line::from(format!("Pinned: {}", update.pinned)));
                lines.push(Line::from(format!(
                    "Restart required: {}",
                    update.restart_required
                )));
                lines.push(Line::from(""));
                lines.push(Line::from("Actions: [u] upgrade selected  [a] upgrade all"));
                lines.push(Line::from(format!(
                    "Upgrade-all options: [I] include_pinned={}  [S] allow_os_updates={}  [m/M] manager_scope={}",
                    app.updates_include_pinned,
                    app.updates_allow_os_updates,
                    app.updates_manager_scope
                        .map(|manager| manager.as_str().to_string())
                        .unwrap_or_else(|| "all".to_string())
                )));
            } else {
                lines.push(Line::from("No update selected."));
            }
        }
        Section::Packages => {
            if let Some(package) = app.selected_package() {
                lines.push(Line::from(Span::styled(
                    "Selected Package",
                    app.theme.accent,
                )));
                lines.push(Line::from(format!("Package: {}", package.package_name)));
                lines.push(Line::from(format!("Manager: {}", package.manager.as_str())));
                lines.push(Line::from(format!(
                    "Installed: {}",
                    package.installed_version.as_deref().unwrap_or("-")
                )));
                lines.push(Line::from(format!(
                    "Candidate: {}",
                    package.candidate_version.as_deref().unwrap_or("-")
                )));
                lines.push(Line::from(format!("Pinned: {}", package.pinned)));
                if !package.runtime_state.is_empty() {
                    lines.push(Line::from(format!(
                        "Runtime state: {}",
                        render_package_runtime_state(&package.runtime_state)
                    )));
                }
                if let Some(summary) = package.summary.as_deref() {
                    lines.push(Line::from(format!("Summary: {}", summary)));
                }
                if package.manager == ManagerId::Rustup
                    && let Some(detail) = app.selected_rustup_toolchain_detail.as_ref()
                {
                    lines.push(Line::from(format!(
                        "Rustup profile: {}",
                        detail.current_profile.as_deref().unwrap_or("-")
                    )));
                    lines.push(Line::from(format!(
                        "Rustup overrides: {}",
                        if detail.override_paths.is_empty() {
                            "none".to_string()
                        } else {
                            detail.override_paths.join(", ")
                        }
                    )));
                    let installed_components = detail
                        .components
                        .iter()
                        .filter(|entry| entry.installed)
                        .count();
                    let installed_targets = detail
                        .targets
                        .iter()
                        .filter(|entry| entry.installed)
                        .count();
                    lines.push(Line::from(format!(
                        "Components: {} installed of {} available",
                        installed_components,
                        detail.components.len()
                    )));
                    if let Some(component) = app.selected_rustup_component() {
                        lines.push(Line::from(format!(
                            "Selected component: {} [{}]",
                            component.name,
                            if component.installed {
                                "installed"
                            } else {
                                "available"
                            }
                        )));
                    }
                    lines.push(Line::from(format!(
                        "Targets: {} installed of {} available",
                        installed_targets,
                        detail.targets.len()
                    )));
                    if let Some(target) = app.selected_rustup_target() {
                        lines.push(Line::from(format!(
                            "Selected target: {} [{}]",
                            target.name,
                            if target.installed {
                                "installed"
                            } else {
                                "available"
                            }
                        )));
                    }
                }
                lines.push(Line::from(format!(
                    "State: {}",
                    match package.kind {
                        PackageRowKind::Installed => "installed",
                        PackageRowKind::Available => "available",
                    }
                )));
                if let Some(preferred_manager) = package.preferred_manager {
                    lines.push(Line::from(format!(
                        "Preferred manager: {}",
                        preferred_manager.as_str()
                    )));
                    lines.push(Line::from(format!(
                        "Selected manager preferred: {}",
                        package.manager == preferred_manager
                    )));
                }
                if let Some(policy) = package.homebrew_keg_policy.as_deref() {
                    lines.push(Line::from(format!("Homebrew keg policy: {}", policy)));
                }
                lines.push(Line::from(""));
                match package.kind {
                    PackageRowKind::Installed => {
                        lines.push(Line::from(
                            "Actions: [u] upgrade  [x] uninstall  [p] pin/unpin",
                        ));
                    }
                    PackageRowKind::Available => {
                        lines.push(Line::from("Actions: [i] install"));
                    }
                }
                lines.push(Line::from(
                    "         [g] set preferred manager  [G] clear preferred manager",
                ));
                if package.manager == ManagerId::HomebrewFormula {
                    lines.push(Line::from(
                        "         [K] cycle Homebrew keg policy (default/cleanup/keep)",
                    ));
                }
                if package.manager == ManagerId::Rustup && package.kind != PackageRowKind::Available
                {
                    lines.push(Line::from(
                        "         [c/C] cycle component  [b] toggle component",
                    ));
                    lines.push(Line::from("         [t/T] cycle target  [B] toggle target"));
                    lines.push(Line::from(
                        "         [s] make default  [w] set cwd override  [W] clear cwd override  [P] cycle profile",
                    ));
                }
            } else {
                lines.push(Line::from("No package selected."));
            }
        }
        Section::Tasks => {
            if let Some(task) = app.selected_task() {
                lines.push(Line::from(Span::styled("Selected Task", app.theme.accent)));
                lines.push(Line::from(format!("Task ID: {}", task.id)));
                lines.push(Line::from(format!("Manager: {}", task.manager)));
                lines.push(Line::from(format!("Type: {}", task.task_type)));
                lines.push(Line::from(format!("Status: {}", task.status)));
                lines.push(Line::from(format!("Created: {}", task.created_at_unix)));
                lines.push(Line::from(""));
                lines.push(Line::from("Recent logs:"));
                if app.task_logs.is_empty() {
                    lines.push(Line::from(Span::styled("  (none)", app.theme.subtle)));
                } else {
                    for entry in app.task_logs.iter().rev().take(12) {
                        lines.push(Line::from(format!(
                            "  [{}] [{}] {}",
                            entry.created_at_unix, entry.level, entry.message
                        )));
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from("Process output:"));
                if let Some(output) = helm_core::execution::task_output(TaskId(task.id)) {
                    lines.push(Line::from(format!(
                        "  command: {}",
                        output.command.as_deref().unwrap_or("-")
                    )));
                    let stdout_preview = output
                        .stdout
                        .as_deref()
                        .unwrap_or("")
                        .lines()
                        .rev()
                        .take(4)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>();
                    let stderr_preview = output
                        .stderr
                        .as_deref()
                        .unwrap_or("")
                        .lines()
                        .rev()
                        .take(4)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>();
                    if stdout_preview.is_empty() {
                        lines.push(Line::from("  stdout: (empty)"));
                    } else {
                        lines.push(Line::from("  stdout:"));
                        for line in stdout_preview {
                            lines.push(Line::from(format!("    {}", line)));
                        }
                    }
                    if stderr_preview.is_empty() {
                        lines.push(Line::from("  stderr: (empty)"));
                    } else {
                        lines.push(Line::from("  stderr:"));
                        for line in stderr_preview {
                            lines.push(Line::from(format!("    {}", line)));
                        }
                    }
                } else {
                    lines.push(Line::from(Span::styled(
                        "  output unavailable in this process session",
                        app.theme.subtle,
                    )));
                }
                lines.push(Line::from(""));
                lines.push(Line::from("Actions: [c] cancel task"));
            } else {
                lines.push(Line::from("No task selected."));
            }
        }
        Section::Managers => {
            if let Some(manager) = app.selected_manager() {
                lines.push(Line::from(Span::styled(
                    "Selected Manager",
                    app.theme.accent,
                )));
                lines.push(Line::from(format!("Manager ID: {}", manager.manager_id)));
                lines.push(Line::from(format!("Name: {}", manager.display_name)));
                lines.push(Line::from(format!("Authority: {}", manager.authority)));
                lines.push(Line::from(format!("Enabled: {}", manager.enabled)));
                lines.push(Line::from(format!("Eligible: {}", manager.is_eligible)));
                lines.push(Line::from(format!("Detected: {}", manager.detected)));
                lines.push(Line::from(format!(
                    "Version: {}",
                    manager.version.as_deref().unwrap_or("-")
                )));
                if !manager.is_eligible {
                    lines.push(Line::from(format!(
                        "Ineligible reason: {}",
                        manager
                            .ineligible_reason_message
                            .as_deref()
                            .unwrap_or("manager policy blocked")
                    )));
                }
                lines.push(Line::from(format!(
                    "Executable: {}",
                    manager
                        .selected_executable_path
                        .as_deref()
                        .or(manager.executable_path.as_deref())
                        .unwrap_or("-")
                )));
                lines.push(Line::from(format!(
                    "Install method: {}",
                    manager.selected_install_method.as_deref().unwrap_or("-")
                )));
                let active_instance_id = app
                    .selected_manager_install_instances
                    .iter()
                    .find(|instance| instance.is_active)
                    .or_else(|| app.selected_manager_install_instances.first())
                    .map(|instance| instance.instance_id.as_str())
                    .unwrap_or("-");
                lines.push(Line::from(format!(
                    "Install instances: {}",
                    manager.install_instance_count
                )));
                lines.push(Line::from(format!(
                    "Active instance: {}",
                    active_instance_id
                )));
                lines.push(Line::from(format!(
                    "Multi-instance state: {}",
                    manager.multi_instance_state
                )));
                lines.push(Line::from(format!(
                    "Multi-instance acknowledged: {}",
                    manager.multi_instance_acknowledged
                )));
                if let Some(priority) = app.selected_manager_priority_label.as_deref() {
                    lines.push(Line::from(format!("Priority: {}", priority)));
                }
                if app.selected_manager_executable_paths.is_empty() {
                    lines.push(Line::from("Executable candidates: -"));
                } else {
                    lines.push(Line::from("Executable candidates:"));
                    for path in app.selected_manager_executable_paths.iter().take(6) {
                        lines.push(Line::from(format!("  - {}", path)));
                    }
                    if app.selected_manager_executable_paths.len() > 6 {
                        lines.push(Line::from(format!(
                            "  ... ({} more)",
                            app.selected_manager_executable_paths.len() - 6
                        )));
                    }
                }
                if app.selected_manager_install_methods.is_empty() {
                    lines.push(Line::from("Install methods: -"));
                } else {
                    lines.push(Line::from(format!(
                        "Install methods: {}",
                        app.selected_manager_install_methods.join(", ")
                    )));
                }
                if app.selected_manager_install_instances.is_empty() {
                    lines.push(Line::from("Install instance IDs: -"));
                } else {
                    let ids = app
                        .selected_manager_install_instances
                        .iter()
                        .take(6)
                        .map(|instance| {
                            if instance.is_active {
                                format!("{}*", instance.instance_id)
                            } else {
                                instance.instance_id.clone()
                            }
                        })
                        .collect::<Vec<_>>();
                    lines.push(Line::from(format!(
                        "Install instance IDs: {}",
                        ids.join(", ")
                    )));
                    if app.selected_manager_install_instances.len() > 6 {
                        lines.push(Line::from(format!(
                            "  ... ({} more)",
                            app.selected_manager_install_instances.len() - 6
                        )));
                    }
                }
                lines.push(Line::from(""));
                lines.push(Line::from(
                    "Actions: [e] enable/disable  [i] install  [u] update  [x] uninstall  [X] uninstall+override",
                ));
                lines.push(Line::from(
                    "         [z] full-cleanup (homebrew / mise keep config)  [Z] mise full-cleanup (remove config)",
                ));
                lines.push(Line::from(
                    "         [D] detect  [o/O] cycle executable  [m/M] cycle install method  [v/V] cycle active instance",
                ));
                lines.push(Line::from(
                    "         [a] acknowledge multi-instance  [A] clear multi-instance acknowledgement",
                ));
                lines.push(Line::from(
                    "         [[] / ]] move priority up/down within authority",
                ));
            } else {
                lines.push(Line::from("No manager selected."));
            }
        }
        Section::Settings => {
            lines.push(Line::from(Span::styled(
                "Settings Snapshot",
                app.theme.accent,
            )));
            if let Some((key, value)) = app.selected_settings_entry() {
                lines.push(Line::from(format!("Selected key: {}", key)));
                lines.push(Line::from(format!("Value: {}", value)));
                lines.push(Line::from(""));
            }
            lines.push(Line::from("Current values:"));
            for (key, value) in app.settings_entries() {
                lines.push(Line::from(format!("  {key}: {value}")));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(
                "Actions: [e] toggle selected bool  [+/-] auto-check frequency",
            ));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Self Update", app.theme.accent)));
            lines.push(Line::from(format!(
                "Current version: {}",
                app.self_update.current_version
            )));
            lines.push(Line::from(format!("Channel: {}", app.self_update.channel)));
            lines.push(Line::from(format!(
                "Update policy: {}",
                app.self_update.update_policy
            )));
            lines.push(Line::from(format!(
                "Provenance source: {}",
                app.self_update.source
            )));
            lines.push(Line::from(format!(
                "Can self-update: {}",
                app.self_update.can_self_update
            )));
            lines.push(Line::from(format!(
                "Recommended action: {}",
                app.self_update.recommended_action
            )));
            lines.push(Line::from(format!(
                "Marker path: {}",
                app.self_update.marker_path
            )));
            lines.push(Line::from(format!(
                "Executable path: {}",
                app.self_update.executable_path
            )));
            if let Some(check) = app.self_update.last_check.as_ref() {
                lines.push(Line::from(""));
                lines.push(Line::from(format!(
                    "Last check: checked={} at {} source={}",
                    check.checked, check.checked_at_unix, check.source
                )));
                lines.push(Line::from(format!(
                    "Update available: {}",
                    check
                        .update_available
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                )));
                lines.push(Line::from(format!(
                    "Latest version: {}",
                    check.latest_version.as_deref().unwrap_or("-")
                )));
                if let Some(published_at) = check.published_at.as_deref() {
                    lines.push(Line::from(format!("Published at: {}", published_at)));
                }
                if let Some(reason) = check.reason.as_deref() {
                    lines.push(Line::from(format!("Reason: {}", reason)));
                }
            }
            if let Some(error) = app.self_update.last_error.as_deref() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("Last check/update error: {}", error),
                    app.theme.error,
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(
                "Self actions: [K] check  [u] update  [U] force update (direct-script only)",
            ));
        }
        Section::Diagnostics => {
            lines.push(Line::from(Span::styled(
                "Diagnostics Summary",
                app.theme.accent,
            )));
            lines.push(Line::from(format!(
                "Installed packages: {}",
                app.diagnostics.installed_packages
            )));
            lines.push(Line::from(format!(
                "Updatable packages: {}",
                app.diagnostics.updatable_packages
            )));
            lines.push(Line::from(format!(
                "Enabled managers: {}",
                app.diagnostics.enabled_managers
            )));
            lines.push(Line::from(format!(
                "Detected enabled managers: {}",
                app.diagnostics.detected_enabled_managers
            )));
            lines.push(Line::from(format!(
                "Queued/running/completed: {}/{}/{}",
                app.diagnostics.queued_tasks,
                app.diagnostics.running_tasks,
                app.diagnostics.completed_tasks
            )));
            lines.push(Line::from(format!(
                "Failed/cancelled: {}/{}",
                app.diagnostics.failed_tasks, app.diagnostics.cancelled_tasks
            )));
            if app.diagnostics.failed_task_ids.is_empty() {
                lines.push(Line::from("Failed task IDs: -"));
            } else {
                lines.push(Line::from(format!(
                    "Failed task IDs: {}",
                    app.diagnostics
                        .failed_task_ids
                        .iter()
                        .map(|task_id| task_id.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            if app.diagnostics.undetected_enabled_managers.is_empty() {
                lines.push(Line::from("Undetected enabled managers: -"));
            } else {
                lines.push(Line::from(format!(
                    "Undetected enabled managers: {}",
                    app.diagnostics.undetected_enabled_managers.join(", ")
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from("Actions: [E] export diagnostics snapshot JSON"));
        }
    }

    let details = Paragraph::new(Text::from(lines))
        .block(Block::default().borders(Borders::ALL).title("Details"))
        .style(app.theme.text)
        .wrap(Wrap { trim: false });
    frame.render_widget(details, area);
}

fn render_footer(frame: &mut ratatui::Frame<'_>, area: Rect, app: &AppState) {
    let base_help =
        "1-6 switch | / filter | r refresh | d detect | Ctrl+K palette | ? help | q quit";
    let section_help = match app.section {
        Section::Updates => {
            "u upgrade selected | a upgrade all | I include pinned | S allow OS updates | m/M manager scope"
        }
        Section::Packages => {
            "i install | u upgrade | x uninstall | p pin/unpin | K keg policy | c/C+b rustup components | t/T+B rustup targets | s/w/W/P rustup toolchain"
        }
        Section::Tasks => "c cancel task",
        Section::Managers => {
            "e toggle | i/u/x/X lifecycle | z/Z full-cleanup | D detect | o/O exec | m/M method | v/V active | a/A ack"
        }
        Section::Settings => {
            "e toggle selected bool | +/- auto-check frequency | K check self update | u update"
        }
        Section::Diagnostics => "E export diagnostics snapshot",
    };
    let filter_hint = if app.input_mode == InputMode::Search {
        format!(" [filter input] {}", app.filter_query)
    } else {
        String::new()
    };

    let mut line = base_help.to_string();
    if !section_help.is_empty() {
        line.push_str(" | ");
        line.push_str(section_help);
    }
    if !filter_hint.is_empty() {
        line.push_str(" |");
        line.push_str(filter_hint.as_str());
    }

    let mut styled = vec![Span::styled(line, app.theme.subtle)];
    if let Some(toast) = app.toast.as_ref() {
        styled.push(Span::raw("   "));
        styled.push(Span::styled(
            toast.message.as_str(),
            if toast.error {
                app.theme.error
            } else {
                app.theme.success
            },
        ));
    }

    let footer = Paragraph::new(Line::from(styled))
        .style(app.theme.subtle)
        .alignment(Alignment::Left);
    frame.render_widget(footer, area);
}

fn render_splash(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = frame.area();
    let art = if area.width >= 90 && area.height >= 30 {
        SPLASH_LARGE
    } else {
        SPLASH_COMPACT
    };
    let lines = art.lines().collect::<Vec<_>>();
    let width = lines
        .iter()
        .map(|line| line.chars().count() as u16)
        .max()
        .unwrap_or(0)
        + 4;
    let height = lines.len() as u16 + 3;
    let splash_area = centered_rect(width.min(area.width), height.min(area.height), area);
    frame.render_widget(Clear, splash_area);

    let splash_style = Style::default().fg(Color::White);
    let rendered_art = parse_ansi_splash_text(art, splash_style, app.color_enabled);
    let paragraph = Paragraph::new(rendered_art)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(splash_style)
                .title("Helm")
                .title_style(splash_style),
        );
    frame.render_widget(paragraph, splash_area);
}

fn render_help_overlay(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = centered_rect(
        92.min(frame.area().width),
        22.min(frame.area().height),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    let help_text = vec![
        Line::from(Span::styled("Helm TUI Keymap", app.theme.accent)),
        Line::from(Span::styled(
            "Copyright (c) 2026 Jason Cavinder",
            app.theme.subtle,
        )),
        Line::from(""),
        Line::from("Navigation"),
        Line::from("  1..6       switch sections"),
        Line::from("  Tab        next section"),
        Line::from("  Up/Down    move selection"),
        Line::from("  PgUp/PgDn  page move"),
        Line::from("  /          filter query"),
        Line::from(""),
        Line::from("Global"),
        Line::from("  r          refresh all managers"),
        Line::from("  d          detect all managers"),
        Line::from("  Ctrl+K     command palette"),
        Line::from("  ?          toggle help"),
        Line::from("  q          quit"),
        Line::from(""),
        Line::from("Section actions"),
        Line::from("  Updates:   u upgrade selected, a upgrade all"),
        Line::from("             I toggle include pinned, S toggle allow OS updates"),
        Line::from("             m/M cycle manager scope (all or selected manager)"),
        Line::from("  Packages:  i install selected available package"),
        Line::from("             u upgrade selected installed package"),
        Line::from("             x uninstall selected installed package, p pin/unpin"),
        Line::from("             g set preferred manager, G clear preferred manager"),
        Line::from("             K cycle Homebrew keg policy override"),
        Line::from("             c/C cycle rustup component, b toggle selected component"),
        Line::from("             t/T cycle rustup target, B toggle selected target"),
        Line::from("             s set rustup default, w/W set or clear cwd rustup override"),
        Line::from("             P cycle rustup profile"),
        Line::from("  Tasks:     c cancel task"),
        Line::from("  Managers:  e enable/disable, i install, u update, x uninstall"),
        Line::from("             X uninstall with unknown-provenance override"),
        Line::from(
            "             z full-cleanup (homebrew / mise keep config), Z mise full-cleanup (remove config)",
        ),
        Line::from("             D detect selected, o/O executable cycle, m/M method cycle"),
        Line::from("             v/V cycle active install instance"),
        Line::from("             a acknowledge multi-instance, A clear acknowledgement"),
        Line::from("             [ and ] priority shift within authority"),
        Line::from("  Settings:  e toggle selected bool, +/- frequency"),
        Line::from("             K self check, u self update, U force self update"),
        Line::from("  Diagnostics: E export snapshot JSON"),
    ];
    let widget = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, area);
}

fn render_palette_overlay(frame: &mut ratatui::Frame<'_>, app: &AppState) {
    let area = centered_rect(
        72.min(frame.area().width),
        18.min(frame.area().height),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    let items = PALETTE_ACTIONS
        .iter()
        .map(|action| ListItem::new(action.title()))
        .collect::<Vec<_>>();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Command Palette"),
        )
        .highlight_style(app.theme.selected)
        .highlight_symbol("▶ ");
    let mut state = ListState::default().with_selected(Some(
        app.palette_index
            .min(PALETTE_ACTIONS.len().saturating_sub(1)),
    ));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_confirm_overlay(frame: &mut ratatui::Frame<'_>, app: &AppState, prompt: &str) {
    let area = centered_rect(
        92.min(frame.area().width),
        7.min(frame.area().height),
        frame.area(),
    );
    frame.render_widget(Clear, area);
    let content = vec![
        Line::from(Span::styled(prompt, app.theme.warning)),
        Line::from(""),
        Line::from("[Enter/Y] Confirm    [Esc/N] Cancel"),
    ];
    let widget = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("Confirm"))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(widget, area);
}

fn parse_ansi_splash_text(raw: &str, fallback_style: Style, allow_colors: bool) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for raw_line in raw.split('\n') {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut active_style = fallback_style;
        let mut segment = String::new();
        let mut chars = raw_line.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' && chars.peek().copied() == Some('[') {
                let _ = chars.next();
                if !segment.is_empty() {
                    spans.push(Span::styled(segment.clone(), active_style));
                    segment.clear();
                }
                let mut sgr = String::new();
                for next in chars.by_ref() {
                    if next == 'm' {
                        break;
                    }
                    sgr.push(next);
                }
                active_style = apply_sgr_style(active_style, fallback_style, &sgr, allow_colors);
                continue;
            }
            segment.push(ch);
        }

        if !segment.is_empty() {
            spans.push(Span::styled(segment, active_style));
        }
        if spans.is_empty() {
            spans.push(Span::styled(String::new(), fallback_style));
        }
        lines.push(Line::from(spans));
    }
    Text::from(lines)
}

fn apply_sgr_style(current: Style, fallback_style: Style, sgr: &str, allow_colors: bool) -> Style {
    let mut style = current;
    let mut codes = if sgr.trim().is_empty() {
        vec![0u16]
    } else {
        sgr.split(';')
            .filter_map(|token| token.parse::<u16>().ok())
            .collect::<Vec<_>>()
    };
    if codes.is_empty() {
        codes.push(0);
    }

    let mut index = 0usize;
    while index < codes.len() {
        match codes[index] {
            0 => {
                style = fallback_style;
                index += 1;
            }
            39 => {
                style = style.fg(fallback_style.fg.unwrap_or(Color::White));
                index += 1;
            }
            30..=37 => {
                if allow_colors {
                    let color = basic_ansi_color(codes[index] - 30, false);
                    style = style.fg(color);
                }
                index += 1;
            }
            90..=97 => {
                if allow_colors {
                    let color = basic_ansi_color(codes[index] - 90, true);
                    style = style.fg(color);
                }
                index += 1;
            }
            38 => {
                if index + 1 < codes.len() {
                    match codes[index + 1] {
                        2 if index + 4 < codes.len() => {
                            if allow_colors {
                                let r = codes[index + 2].min(255) as u8;
                                let g = codes[index + 3].min(255) as u8;
                                let b = codes[index + 4].min(255) as u8;
                                style = style.fg(Color::Rgb(r, g, b));
                            }
                            index += 5;
                        }
                        5 if index + 2 < codes.len() => {
                            if allow_colors {
                                style = style.fg(Color::Indexed(codes[index + 2].min(255) as u8));
                            }
                            index += 3;
                        }
                        _ => {
                            index += 1;
                        }
                    }
                } else {
                    index += 1;
                }
            }
            _ => {
                index += 1;
            }
        }
    }
    style
}

fn basic_ansi_color(index: u16, bright: bool) -> Color {
    let normal = [
        Color::Black,
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::White,
    ];
    let bright_colors = [
        Color::DarkGray,
        Color::LightRed,
        Color::LightGreen,
        Color::LightYellow,
        Color::LightBlue,
        Color::LightMagenta,
        Color::LightCyan,
        Color::White,
    ];
    let idx = (index as usize).min(7);
    if bright {
        bright_colors[idx]
    } else {
        normal[idx]
    }
}

fn build_package_rows(
    store: &SqliteStore,
    installed: &[InstalledPackage],
    search_results: &[CachedSearchResult],
) -> Result<Vec<PackageRow>, String> {
    let default_homebrew_policy = store
        .homebrew_keg_policy()
        .map_err(|error| format!("failed to read homebrew_keg_policy: {error}"))?;
    let homebrew_overrides: HashMap<String, HomebrewKegPolicy> = store
        .list_package_keg_policies()
        .map_err(|error| format!("failed to list package keg policies: {error}"))?
        .into_iter()
        .filter(|entry| entry.package.manager == ManagerId::HomebrewFormula)
        .map(|entry| (entry.package.name, entry.policy))
        .collect();
    let package_manager_preferences: HashMap<String, ManagerId> = store
        .list_package_manager_preferences()
        .map_err(|error| format!("failed to list package manager preferences: {error}"))?
        .into_iter()
        .map(|entry| (entry.package_name, entry.manager))
        .collect();

    let mut rows_by_key: HashMap<(ManagerId, String), PackageRow> = HashMap::new();
    for package in installed {
        let preferred_manager = preferred_manager_for_package(
            &package_manager_preferences,
            package.package.name.as_str(),
            package.installed_version.as_deref(),
        );
        let homebrew_keg_policy = if package.package.manager == ManagerId::HomebrewFormula {
            let policy = homebrew_overrides
                .get(package.package.name.as_str())
                .copied()
                .unwrap_or(default_homebrew_policy);
            let source = if homebrew_overrides.contains_key(package.package.name.as_str()) {
                "override"
            } else {
                "default"
            };
            Some(format!("{} ({})", policy.as_str(), source))
        } else {
            None
        };
        rows_by_key.insert(
            (package.package.manager, package.package.name.clone()),
            PackageRow {
                kind: PackageRowKind::Installed,
                manager: package.package.manager,
                package_name: package.package.name.clone(),
                installed_version: package.installed_version.clone(),
                candidate_version: None,
                pinned: package.pinned,
                summary: None,
                homebrew_keg_policy,
                preferred_manager,
                runtime_state: package.runtime_state.clone(),
            },
        );
    }

    for result in search_results {
        let manager = result.result.package.manager;
        let package_name = result.result.package.name.clone();
        let key = (manager, package_name.clone());
        if let Some(existing) = rows_by_key.get_mut(&key) {
            if existing.candidate_version.is_none() {
                existing.candidate_version = result.result.version.clone();
            }
            if existing.summary.is_none() {
                existing.summary = result.result.summary.clone();
            }
            continue;
        }

        let preferred_manager = preferred_manager_for_package(
            &package_manager_preferences,
            package_name.as_str(),
            result.result.version.as_deref(),
        );
        let homebrew_keg_policy = if manager == ManagerId::HomebrewFormula {
            let policy = homebrew_overrides
                .get(package_name.as_str())
                .copied()
                .unwrap_or(default_homebrew_policy);
            let source = if homebrew_overrides.contains_key(package_name.as_str()) {
                "override"
            } else {
                "default"
            };
            Some(format!("{} ({})", policy.as_str(), source))
        } else {
            None
        };

        rows_by_key.insert(
            key,
            PackageRow {
                kind: PackageRowKind::Available,
                manager,
                package_name,
                installed_version: None,
                candidate_version: result.result.version.clone(),
                pinned: false,
                summary: result.result.summary.clone(),
                homebrew_keg_policy,
                preferred_manager,
                runtime_state: PackageRuntimeState::default(),
            },
        );
    }

    let mut rows = rows_by_key.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        if left.kind != right.kind {
            return match (left.kind, right.kind) {
                (PackageRowKind::Installed, PackageRowKind::Available) => std::cmp::Ordering::Less,
                (PackageRowKind::Available, PackageRowKind::Installed) => {
                    std::cmp::Ordering::Greater
                }
                _ => std::cmp::Ordering::Equal,
            };
        }
        let name_cmp = left
            .package_name
            .to_ascii_lowercase()
            .cmp(&right.package_name.to_ascii_lowercase());
        if name_cmp != std::cmp::Ordering::Equal {
            return name_cmp;
        }
        if let Some(preferred_manager) = left.preferred_manager.or(right.preferred_manager) {
            let left_is_preferred = left.manager == preferred_manager;
            let right_is_preferred = right.manager == preferred_manager;
            if left_is_preferred != right_is_preferred {
                return if left_is_preferred {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
        }
        left.manager.as_str().cmp(right.manager.as_str())
    });
    Ok(rows)
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area
        .x
        .saturating_add(area.width.saturating_sub(width).saturating_div(2));
    let y = area
        .y
        .saturating_add(area.height.saturating_sub(height).saturating_div(2));
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn clamp_cursor(cursor: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        cursor.min(len.saturating_sub(1))
    }
}

fn wrap_cursor(cursor: usize, len: usize, delta: i32) -> usize {
    if len == 0 {
        return 0;
    }
    if delta == 0 {
        return clamp_cursor(cursor, len);
    }
    let len = len as i32;
    let current = clamp_cursor(cursor, len as usize) as i32;
    let next = (current + delta).rem_euclid(len);
    next as usize
}

fn matches_query(query: &str, fields: &[&str]) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return true;
    }
    let needle = trimmed.to_ascii_lowercase();
    fields
        .iter()
        .any(|field| field.to_ascii_lowercase().contains(&needle))
}

fn manager_participates_in_package_search(manager: ManagerId) -> bool {
    helm_core::registry::manager_participates_in_package_search(manager)
}

fn apply_filter_backspace(app: &mut AppState) {
    if app.filter_query.is_empty() {
        app.input_mode = InputMode::Normal;
        return;
    }
    app.filter_query.pop();
    app.clamp_cursors();
    if app.filter_query.is_empty() {
        app.input_mode = InputMode::Normal;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppState, ConfirmAction, InputMode, apply_filter_backspace, execute_confirmed_action,
        manager_participates_in_package_search, next_choice_index, normalized_nonempty,
    };
    use crate::{ManagerId, PackageRef, PinKind, SqliteStore};
    use helm_core::models::InstalledPackage;
    use helm_core::persistence::{PackageStore, PinStore};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_store_path(test_name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "helm-cli-tui-{test_name}-{}-{nanos}.db",
            std::process::id()
        ))
    }

    #[test]
    fn next_choice_index_wraps_forward() {
        assert_eq!(next_choice_index(0, 3, 1), 1);
        assert_eq!(next_choice_index(2, 3, 1), 0);
        assert_eq!(next_choice_index(2, 3, 4), 0);
    }

    #[test]
    fn next_choice_index_wraps_backward() {
        assert_eq!(next_choice_index(0, 3, -1), 2);
        assert_eq!(next_choice_index(1, 3, -1), 0);
        assert_eq!(next_choice_index(0, 3, -4), 2);
    }

    #[test]
    fn normalized_nonempty_filters_blank_values() {
        assert_eq!(normalized_nonempty(None), None);
        assert_eq!(normalized_nonempty(Some("".to_string())), None);
        assert_eq!(normalized_nonempty(Some("   ".to_string())), None);
        assert_eq!(
            normalized_nonempty(Some("  value  ".to_string())),
            Some("value".to_string())
        );
    }

    #[test]
    fn filter_backspace_exits_search_mode_when_empty() {
        let mut app = AppState::new(true);
        app.input_mode = InputMode::Search;
        app.filter_query = "ab".to_string();
        apply_filter_backspace(&mut app);
        assert_eq!(app.input_mode, InputMode::Search);
        assert_eq!(app.filter_query, "a");
        apply_filter_backspace(&mut app);
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.filter_query.is_empty());
    }

    #[test]
    fn filter_backspace_on_empty_exits_search_mode() {
        let mut app = AppState::new(true);
        app.input_mode = InputMode::Search;
        app.filter_query.clear();
        apply_filter_backspace(&mut app);
        assert_eq!(app.input_mode, InputMode::Normal);
    }

    #[test]
    fn manager_uninstall_confirm_prompt_includes_preview_summary() {
        let prompt = ConfirmAction::ManagerMutation {
            manager: ManagerId::Rustup,
            subcommand: "uninstall",
            allow_unknown_provenance: false,
            uninstall_options: helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
            uninstall_preview_summary: Some(
                "[strategy=rustup_self provenance=rustup_init confidence=0.92 blast_radius=8]"
                    .to_string(),
            ),
        }
        .prompt();
        assert!(prompt.contains("uninstall manager 'rustup' ?"));
        assert!(prompt.contains("strategy=rustup_self"));
        assert!(prompt.contains("blast_radius=8"));
    }

    #[test]
    fn manager_uninstall_confirm_prompt_marks_unknown_override() {
        let prompt = ConfirmAction::ManagerMutation {
            manager: ManagerId::Rustup,
            subcommand: "uninstall",
            allow_unknown_provenance: true,
            uninstall_options: helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
            uninstall_preview_summary: Some(
                "[strategy=homebrew_formula provenance=unknown confidence=0.49 blast_radius=9]"
                    .to_string(),
            ),
        }
        .prompt();
        assert!(prompt.contains("unknown-provenance override"));
    }

    #[test]
    fn manager_uninstall_confirm_prompt_shows_mise_cleanup_mode() {
        let prompt = ConfirmAction::ManagerMutation {
            manager: ManagerId::Mise,
            subcommand: "uninstall",
            allow_unknown_provenance: false,
            uninstall_options: super::mise_uninstall_options_full_cleanup(
                helm_core::manager_lifecycle::MiseUninstallConfigRemoval::RemoveConfig,
            ),
            uninstall_preview_summary: Some(
                "[strategy=mise_self provenance=source_build confidence=0.93 blast_radius=10]"
                    .to_string(),
            ),
        }
        .prompt();
        assert!(prompt.contains("mode=full_cleanup"));
        assert!(prompt.contains("config=remove"));
    }

    #[test]
    fn manager_uninstall_confirm_prompt_shows_homebrew_cleanup_mode() {
        let prompt = ConfirmAction::ManagerMutation {
            manager: ManagerId::Rustup,
            subcommand: "uninstall",
            allow_unknown_provenance: false,
            uninstall_options: super::homebrew_uninstall_options_full_cleanup(),
            uninstall_preview_summary: Some(
                "[strategy=homebrew_formula provenance=homebrew confidence=0.99 blast_radius=11]"
                    .to_string(),
            ),
        }
        .prompt();
        assert!(prompt.contains("homebrew_mode=full_cleanup"));
    }

    #[test]
    fn package_uninstall_confirm_prompt_includes_preview_summary() {
        let prompt = ConfirmAction::UninstallPackage {
            manager: ManagerId::Rustup,
            package_name: "stable".to_string(),
            uninstall_preview_summary: Some(
                "[manager_strategy=rustup_self manager_provenance=rustup_init blast_radius=5 requires_confirmation=true]"
                    .to_string(),
            ),
        }
        .prompt();
        assert!(prompt.contains("Uninstall 'stable@rustup'?"));
        assert!(prompt.contains("manager_strategy=rustup_self"));
        assert!(prompt.contains("blast_radius=5"));
    }

    #[test]
    fn package_search_includes_rustup_manager() {
        assert!(manager_participates_in_package_search(ManagerId::Rustup));
        assert!(manager_participates_in_package_search(
            ManagerId::HomebrewFormula
        ));
    }

    #[test]
    fn toggle_pin_for_virtual_manager_persists_pin_record_and_snapshot_state() {
        let db_path = test_store_path("toggle-pin-virtual");
        let store = SqliteStore::new(db_path.clone());
        store
            .migrate_to_latest()
            .expect("failed to migrate test sqlite store");

        let package = PackageRef {
            manager: ManagerId::Pip,
            name: "certifi".to_string(),
        };
        store
            .upsert_installed(&[InstalledPackage {
                package: package.clone(),
                installed_version: Some("2026.1.4".to_string()),
                pinned: false,
                runtime_state: Default::default(),
            }])
            .expect("failed to seed installed package");

        let pin_message = execute_confirmed_action(
            &store,
            ConfirmAction::TogglePin {
                manager: ManagerId::Pip,
                package_name: "certifi".to_string(),
                pinned: false,
            },
        )
        .expect("virtual pin action should succeed");
        assert!(pin_message.contains("Virtual pin applied"));

        let pins = store
            .list_pins()
            .expect("failed to list pins after virtual pin");
        assert_eq!(pins.len(), 1);
        assert_eq!(pins[0].package.manager, ManagerId::Pip);
        assert_eq!(pins[0].package.name, "certifi");
        assert_eq!(pins[0].kind, PinKind::Virtual);

        let installed_after_pin = store
            .list_installed()
            .expect("failed to list installed packages after virtual pin");
        let certifi_after_pin = installed_after_pin
            .iter()
            .find(|entry| {
                entry.package.manager == ManagerId::Pip && entry.package.name == "certifi"
            })
            .expect("expected pip certifi after virtual pin");
        assert!(certifi_after_pin.pinned);

        let unpin_message = execute_confirmed_action(
            &store,
            ConfirmAction::TogglePin {
                manager: ManagerId::Pip,
                package_name: "certifi".to_string(),
                pinned: true,
            },
        )
        .expect("virtual unpin action should succeed");
        assert!(unpin_message.contains("Virtual unpin applied"));

        let pins_after_unpin = store
            .list_pins()
            .expect("failed to list pins after virtual unpin");
        assert!(pins_after_unpin.is_empty());

        let installed_after_unpin = store
            .list_installed()
            .expect("failed to list installed packages after virtual unpin");
        let certifi_after_unpin = installed_after_unpin
            .iter()
            .find(|entry| {
                entry.package.manager == ManagerId::Pip && entry.package.name == "certifi"
            })
            .expect("expected pip certifi after virtual unpin");
        assert!(!certifi_after_unpin.pinned);

        let _ = std::fs::remove_file(db_path);
    }
}
