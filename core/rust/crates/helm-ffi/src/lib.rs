//! # Helm FFI Layer
//!
//! This module exposes the Helm core engine to the macOS XPC service via a C ABI
//! FFI boundary.
//!
//! ## Lifecycle
//!
//! - **Initialization**: [`helm_init`] must be called once with a valid SQLite
//!   database path. It creates a Tokio runtime, initializes the SQLite store with
//!   migrations, registers all implemented manager adapters, and stores the engine state in
//!   a process-global `Mutex<Option<HelmState>>`.
//!
//! - **No explicit shutdown**: There is no `helm_shutdown()` function. The Tokio
//!   runtime, SQLite connections, and adapter state live for the entire process
//!   lifetime. Cleanup occurs when the XPC service process exits.
//!
//! - **Thread safety**: All FFI functions acquire the global state mutex before
//!   accessing the engine. Poisoned-lock recovery is implemented via
//!   [`lock_or_recover`] to prevent lock-poison panics at the FFI boundary.
//!
//! ## FFI Exports (service surface)
//!
//! | Function | Category |
//! |----------|----------|
//! | `helm_init` | Lifecycle |
//! | `helm_list_installed_packages` | Package queries |
//! | `helm_list_outdated_packages` | Package queries |
//! | `helm_list_tasks` | Task management |
//! | `helm_get_task_output` | Task management |
//! | `helm_list_task_logs` | Task management |
//! | `helm_trigger_refresh` | Task management |
//! | `helm_trigger_detection` | Task management |
//! | `helm_trigger_detection_for_manager` | Task management |
//! | `helm_cancel_task` | Task management |
//! | `helm_dismiss_task` | Task management |
//! | `helm_search_local` | Search |
//! | `helm_trigger_remote_search` | Search |
//! | `helm_list_manager_status` | Manager control |
//! | `helm_doctor_scan` | Diagnostics |
//! | `helm_set_manager_enabled` | Manager control |
//! | `helm_install_manager` | Manager control |
//! | `helm_update_manager` | Manager control |
//! | `helm_uninstall_manager` | Manager control |
//! | `helm_preview_manager_uninstall` | Manager control |
//! | `helm_uninstall_manager_with_options` | Manager control |
//! | `helm_apply_manager_package_state_issue_repair` | Manager control |
//! | `helm_get_safe_mode` | Settings |
//! | `helm_set_safe_mode` | Settings |
//! | `helm_get_homebrew_keg_auto_cleanup` | Settings |
//! | `helm_set_homebrew_keg_auto_cleanup` | Settings |
//! | `helm_list_package_keg_policies` | Keg policies |
//! | `helm_set_package_keg_policy` | Keg policies |
//! | `helm_preview_upgrade_plan` | Upgrade |
//! | `helm_upgrade_all` | Upgrade |
//! | `helm_upgrade_package` | Upgrade |
//! | `helm_list_pins` | Pinning |
//! | `helm_pin_package` | Pinning |
//! | `helm_unpin_package` | Pinning |
//! | `helm_reset_database` | Database |
//! | `helm_take_last_error_key` | Error |
//! | `helm_free_string` | Memory management |
//!
//! All data exchange uses JSON-encoded UTF-8 `*mut c_char` strings. The caller
//! must free returned strings via [`helm_free_string`].

use std::ffi::{CStr, CString};
use std::fs::OpenOptions;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::os::raw::c_char;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use helm_core::adapters::asdf::AsdfAdapter;
use helm_core::adapters::asdf_process::ProcessAsdfSource;
use helm_core::adapters::bundler::BundlerAdapter;
use helm_core::adapters::bundler_process::ProcessBundlerSource;
use helm_core::adapters::cargo::CargoAdapter;
use helm_core::adapters::cargo_binstall::CargoBinstallAdapter;
use helm_core::adapters::cargo_binstall_process::ProcessCargoBinstallSource;
use helm_core::adapters::cargo_process::ProcessCargoSource;
use helm_core::adapters::colima::ColimaAdapter;
use helm_core::adapters::colima_process::ProcessColimaSource;
use helm_core::adapters::docker_desktop::DockerDesktopAdapter;
use helm_core::adapters::docker_desktop_process::ProcessDockerDesktopSource;
use helm_core::adapters::firmware_updates::FirmwareUpdatesAdapter;
use helm_core::adapters::firmware_updates_process::ProcessFirmwareUpdatesSource;
use helm_core::adapters::homebrew::HomebrewAdapter;
use helm_core::adapters::homebrew_cask::HomebrewCaskAdapter;
use helm_core::adapters::homebrew_cask_process::ProcessHomebrewCaskSource;
use helm_core::adapters::homebrew_process::ProcessHomebrewSource;
use helm_core::adapters::macports::MacPortsAdapter;
use helm_core::adapters::macports_process::ProcessMacPortsSource;
use helm_core::adapters::mas::MasAdapter;
use helm_core::adapters::mas_process::ProcessMasSource;
use helm_core::adapters::mise::MiseAdapter;
use helm_core::adapters::mise_process::ProcessMiseSource;
use helm_core::adapters::nix_darwin::NixDarwinAdapter;
use helm_core::adapters::nix_darwin_process::ProcessNixDarwinSource;
use helm_core::adapters::npm::NpmAdapter;
use helm_core::adapters::npm_process::ProcessNpmSource;
use helm_core::adapters::parallels_desktop::ParallelsDesktopAdapter;
use helm_core::adapters::parallels_desktop_process::ProcessParallelsDesktopSource;
use helm_core::adapters::pip::PipAdapter;
use helm_core::adapters::pip_process::ProcessPipSource;
use helm_core::adapters::pipx::PipxAdapter;
use helm_core::adapters::pipx_process::ProcessPipxSource;
use helm_core::adapters::pnpm::PnpmAdapter;
use helm_core::adapters::pnpm_process::ProcessPnpmSource;
use helm_core::adapters::podman::PodmanAdapter;
use helm_core::adapters::podman_process::ProcessPodmanSource;
use helm_core::adapters::poetry::PoetryAdapter;
use helm_core::adapters::poetry_process::ProcessPoetrySource;
use helm_core::adapters::rosetta2::Rosetta2Adapter;
use helm_core::adapters::rosetta2_process::ProcessRosetta2Source;
use helm_core::adapters::rubygems::RubyGemsAdapter;
use helm_core::adapters::rubygems_process::ProcessRubyGemsSource;
use helm_core::adapters::rustup::RustupAdapter;
use helm_core::adapters::rustup_process::ProcessRustupSource;
use helm_core::adapters::setapp::SetappAdapter;
use helm_core::adapters::setapp_process::ProcessSetappSource;
use helm_core::adapters::softwareupdate::SoftwareUpdateAdapter;
use helm_core::adapters::softwareupdate_process::ProcessSoftwareUpdateSource;
use helm_core::adapters::sparkle::SparkleAdapter;
use helm_core::adapters::sparkle_process::ProcessSparkleSource;
use helm_core::adapters::xcode_command_line_tools::XcodeCommandLineToolsAdapter;
use helm_core::adapters::xcode_command_line_tools_process::ProcessXcodeCommandLineToolsSource;
use helm_core::adapters::yarn::YarnAdapter;
use helm_core::adapters::yarn_process::ProcessYarnSource;
use helm_core::adapters::{
    AdapterRequest, InstallRequest, PinRequest, SearchRequest, UninstallRequest, UnpinRequest,
    UpgradeRequest,
};
use helm_core::execution::tokio_process::TokioProcessExecutor;
use helm_core::execution::{
    ManagerTimeoutProfile, clear_manager_selected_executables,
    replace_manager_execution_preferences,
};
use helm_core::managed_automation_policy::{
    ManagedAutomationPolicyMode, apply_managed_automation_policy,
};
use helm_core::manager_dependencies::provenance_dependency_manager;
use helm_core::manager_instances::{install_instance_fingerprint, resolve_multi_instance_state};
use helm_core::manager_policy::manager_enablement_eligibility;
use helm_core::models::{
    Capability, DetectionInfo, HomebrewKegPolicy, ManagerAction, ManagerAuthority, ManagerId,
    ManagerInstallInstance, ManagerUninstallPreview, OutdatedPackage, PackageRef, PinKind,
    PinRecord, SearchQuery, StrategyKind, TaskId, TaskLogLevel, TaskLogRecord, TaskStatus,
    TaskType,
};
use helm_core::orchestration::adapter_runtime::AdapterRuntime;
use helm_core::orchestration::{AdapterTaskTerminalState, CancellationMode};
use helm_core::persistence::{
    DetectionStore, ManagerPreference, MigrationStore, PackageStore, PinStore, SearchCacheStore,
    TaskStore,
};
use helm_core::sqlite::SqliteStore;
use helm_core::uninstall_preview::{
    DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD, ManagerUninstallPreviewContext,
    PackageUninstallPreviewContext, build_manager_uninstall_preview,
    build_package_uninstall_preview,
};
use lazy_static::lazy_static;

struct HelmState {
    store: Arc<SqliteStore>,
    runtime: Arc<AdapterRuntime>,
    rt_handle: tokio::runtime::Handle,
    _tokio_rt: tokio::runtime::Runtime,
}

#[derive(Clone, Debug, Default)]
struct TaskLabel {
    key: String,
    args: std::collections::BTreeMap<String, String>,
}

lazy_static! {
    static ref STATE: Mutex<Option<HelmState>> = Mutex::new(None);
    static ref TASK_LABELS: Mutex<std::collections::HashMap<u64, TaskLabel>> =
        Mutex::new(std::collections::HashMap::new());
    static ref LAST_ERROR_KEY: Mutex<Option<String>> = Mutex::new(None);
}

const LOCK_POISONED_ERROR_KEY: &str = "error.ffi.lock_poisoned";
const SERVICE_ERROR_INVALID_INPUT: &str = "service.error.invalid_input";
const SERVICE_ERROR_INTERNAL: &str = "service.error.internal";
const SERVICE_ERROR_PROCESS_FAILURE: &str = "service.error.process_failure";
const SERVICE_ERROR_STORAGE_FAILURE: &str = "service.error.storage_failure";
const SERVICE_ERROR_UNSUPPORTED_CAPABILITY: &str = "service.error.unsupported_capability";
const SERVICE_ERROR_MANAGER_DEPENDENCY_BLOCKED: &str = "service.error.manager_dependency_blocked";
const SERVICE_ERROR_MANAGER_SETUP_REQUIRED: &str = "service.error.manager_setup_required";

fn note_lock_poisoned(context: &str) {
    eprintln!("helm-ffi: recovering from poisoned mutex: {context}");
    if let Ok(mut key) = LAST_ERROR_KEY.try_lock() {
        *key = Some(LOCK_POISONED_ERROR_KEY.to_string());
    }
}

fn lock_or_recover<'a, T>(mutex: &'a Mutex<T>, context: &str) -> MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            note_lock_poisoned(context);
            poisoned.into_inner()
        }
    }
}

fn clear_last_error_key() {
    lock_or_recover(&LAST_ERROR_KEY, "last_error_key").take();
}

fn set_last_error_key(error_key: &str) {
    *lock_or_recover(&LAST_ERROR_KEY, "last_error_key") = Some(error_key.to_string());
}

fn return_error_bool(error_key: &str) -> bool {
    set_last_error_key(error_key);
    false
}

fn return_error_i64(error_key: &str) -> i64 {
    set_last_error_key(error_key);
    -1
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum RustupInstallSourcePayload {
    #[default]
    OfficialDownload,
    ExistingBinaryPath,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum MiseInstallSourcePayload {
    #[default]
    OfficialDownload,
    ExistingBinaryPath,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagerInstallOptionsPayload {
    install_method_override: Option<String>,
    rustup_install_source: Option<RustupInstallSourcePayload>,
    rustup_binary_path: Option<String>,
    mise_install_source: Option<MiseInstallSourcePayload>,
    mise_binary_path: Option<String>,
    complete_post_install_setup_automatically: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum MiseUninstallCleanupModePayload {
    #[default]
    ManagerOnly,
    FullCleanup,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum MiseUninstallConfigRemovalPayload {
    KeepConfig,
    RemoveConfig,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
enum HomebrewUninstallCleanupModePayload {
    #[default]
    ManagerOnly,
    FullCleanup,
}

#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManagerUninstallOptionsPayload {
    allow_unknown_provenance: Option<bool>,
    homebrew_cleanup_mode: Option<HomebrewUninstallCleanupModePayload>,
    mise_cleanup_mode: Option<MiseUninstallCleanupModePayload>,
    mise_config_removal: Option<MiseUninstallConfigRemovalPayload>,
}

fn parse_install_options_payload(
    options_json: *const c_char,
) -> Result<helm_core::manager_lifecycle::ManagerInstallOptions, &'static str> {
    if options_json.is_null() {
        return Ok(helm_core::manager_lifecycle::ManagerInstallOptions::default());
    }
    let raw = unsafe { CStr::from_ptr(options_json) };
    let raw = raw.to_str().map_err(|_| SERVICE_ERROR_INVALID_INPUT)?;
    if raw.trim().is_empty() {
        return Ok(helm_core::manager_lifecycle::ManagerInstallOptions::default());
    }
    let payload: ManagerInstallOptionsPayload =
        serde_json::from_str(raw).map_err(|_| SERVICE_ERROR_INVALID_INPUT)?;
    let rustup_install_source = payload.rustup_install_source.map(|source| match source {
        RustupInstallSourcePayload::OfficialDownload => {
            helm_core::manager_lifecycle::RustupInstallSource::OfficialDownload
        }
        RustupInstallSourcePayload::ExistingBinaryPath => {
            helm_core::manager_lifecycle::RustupInstallSource::ExistingBinaryPath
        }
    });
    let mise_install_source = payload.mise_install_source.map(|source| match source {
        MiseInstallSourcePayload::OfficialDownload => {
            helm_core::manager_lifecycle::MiseInstallSource::OfficialDownload
        }
        MiseInstallSourcePayload::ExistingBinaryPath => {
            helm_core::manager_lifecycle::MiseInstallSource::ExistingBinaryPath
        }
    });
    Ok(helm_core::manager_lifecycle::ManagerInstallOptions {
        install_method_override: payload
            .install_method_override
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        rustup_install_source,
        rustup_binary_path: payload.rustup_binary_path,
        mise_install_source,
        mise_binary_path: payload.mise_binary_path,
        complete_post_install_setup_automatically: payload
            .complete_post_install_setup_automatically
            .unwrap_or(false),
    })
}

fn parse_uninstall_options_payload(
    options_json: *const c_char,
    fallback_allow_unknown_provenance: bool,
) -> Result<(bool, helm_core::manager_lifecycle::ManagerUninstallOptions), &'static str> {
    if options_json.is_null() {
        return Ok((
            fallback_allow_unknown_provenance,
            helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
        ));
    }
    let raw = unsafe { CStr::from_ptr(options_json) };
    let raw = raw.to_str().map_err(|_| SERVICE_ERROR_INVALID_INPUT)?;
    if raw.trim().is_empty() {
        return Ok((
            fallback_allow_unknown_provenance,
            helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
        ));
    }

    let payload: ManagerUninstallOptionsPayload =
        serde_json::from_str(raw).map_err(|_| SERVICE_ERROR_INVALID_INPUT)?;
    let allow_unknown_provenance = payload
        .allow_unknown_provenance
        .unwrap_or(fallback_allow_unknown_provenance);
    let homebrew_cleanup_mode = payload.homebrew_cleanup_mode.map(|value| match value {
        HomebrewUninstallCleanupModePayload::ManagerOnly => {
            helm_core::manager_lifecycle::HomebrewUninstallCleanupMode::ManagerOnly
        }
        HomebrewUninstallCleanupModePayload::FullCleanup => {
            helm_core::manager_lifecycle::HomebrewUninstallCleanupMode::FullCleanup
        }
    });
    let mise_cleanup_mode = payload.mise_cleanup_mode.map(|value| match value {
        MiseUninstallCleanupModePayload::ManagerOnly => {
            helm_core::manager_lifecycle::MiseUninstallCleanupMode::ManagerOnly
        }
        MiseUninstallCleanupModePayload::FullCleanup => {
            helm_core::manager_lifecycle::MiseUninstallCleanupMode::FullCleanup
        }
    });
    let mise_config_removal = payload.mise_config_removal.map(|value| match value {
        MiseUninstallConfigRemovalPayload::KeepConfig => {
            helm_core::manager_lifecycle::MiseUninstallConfigRemoval::KeepConfig
        }
        MiseUninstallConfigRemovalPayload::RemoveConfig => {
            helm_core::manager_lifecycle::MiseUninstallConfigRemoval::RemoveConfig
        }
    });

    Ok((
        allow_unknown_provenance,
        helm_core::manager_lifecycle::ManagerUninstallOptions {
            homebrew_cleanup_mode,
            mise_cleanup_mode,
            mise_config_removal,
        },
    ))
}

unsafe fn parse_manager_id_arg(manager_id: *const c_char) -> Result<ManagerId, &'static str> {
    if manager_id.is_null() {
        return Err(SERVICE_ERROR_INVALID_INPUT);
    }
    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = c_str.to_str().map_err(|_| SERVICE_ERROR_INVALID_INPUT)?;
    id_str
        .parse::<ManagerId>()
        .map_err(|_| SERVICE_ERROR_INVALID_INPUT)
}

unsafe fn parse_required_cstr_arg(raw: *const c_char) -> Result<String, &'static str> {
    if raw.is_null() {
        return Err(SERVICE_ERROR_INVALID_INPUT);
    }
    let c_str = unsafe { CStr::from_ptr(raw) };
    let value = c_str.to_str().map_err(|_| SERVICE_ERROR_INVALID_INPUT)?;
    let value = value.trim();
    if value.is_empty() {
        return Err(SERVICE_ERROR_INVALID_INPUT);
    }
    Ok(value.to_string())
}

fn manager_install_plan_error_key(
    error: helm_core::manager_lifecycle::ManagerInstallPlanError,
) -> &'static str {
    match error {
        helm_core::manager_lifecycle::ManagerInstallPlanError::UnsupportedManager
        | helm_core::manager_lifecycle::ManagerInstallPlanError::UnsupportedMethod => {
            SERVICE_ERROR_UNSUPPORTED_CAPABILITY
        }
        helm_core::manager_lifecycle::ManagerInstallPlanError::InvalidRustupBinaryPath => {
            SERVICE_ERROR_INVALID_INPUT
        }
        helm_core::manager_lifecycle::ManagerInstallPlanError::InvalidMiseBinaryPath => {
            SERVICE_ERROR_INVALID_INPUT
        }
    }
}

fn manager_update_plan_error_key(
    _error: helm_core::manager_lifecycle::ManagerUpdatePlanError,
) -> &'static str {
    SERVICE_ERROR_UNSUPPORTED_CAPABILITY
}

fn log_manager_operation_failure(
    operation: &'static str,
    manager: ManagerId,
    error: &(impl std::fmt::Display + ?Sized),
) {
    eprintln!(
        "helm-ffi: operation={operation} manager={} error={error}",
        manager.as_str()
    );
}

fn manager_display_name(id: ManagerId) -> &'static str {
    match id {
        ManagerId::HomebrewFormula => "Homebrew (formulae)",
        ManagerId::Npm => "npm",
        ManagerId::Pnpm => "pnpm",
        ManagerId::Yarn => "Yarn",
        ManagerId::Cargo => "Cargo",
        ManagerId::CargoBinstall => "cargo-binstall",
        ManagerId::Pip => "pip",
        ManagerId::Pipx => "pipx",
        ManagerId::Poetry => "Poetry",
        ManagerId::RubyGems => "RubyGems",
        ManagerId::Bundler => "Bundler",
        ManagerId::Mise => "mise",
        ManagerId::Rustup => "rustup",
        ManagerId::SoftwareUpdate => "Software Update",
        ManagerId::Mas => "App Store",
        _ => id.as_str(),
    }
}

fn is_optional_manager(id: ManagerId) -> bool {
    matches!(
        id,
        ManagerId::Asdf | ManagerId::MacPorts | ManagerId::NixDarwin
    )
}

fn is_detection_only_manager(id: ManagerId) -> bool {
    matches!(
        helm_core::registry::manager(id).map(|descriptor| descriptor.authority),
        Some(ManagerAuthority::DetectionOnly)
    )
}

fn default_enabled_for_manager(id: ManagerId) -> bool {
    !is_optional_manager(id)
}

fn is_implemented_manager(id: ManagerId) -> bool {
    matches!(
        id,
        ManagerId::HomebrewFormula
            | ManagerId::HomebrewCask
            | ManagerId::Mise
            | ManagerId::Asdf
            | ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::Rustup
            | ManagerId::SoftwareUpdate
            | ManagerId::MacPorts
            | ManagerId::NixDarwin
            | ManagerId::Mas
            | ManagerId::DockerDesktop
            | ManagerId::Podman
            | ManagerId::Colima
            | ManagerId::Sparkle
            | ManagerId::Setapp
            | ManagerId::ParallelsDesktop
            | ManagerId::XcodeCommandLineTools
            | ManagerId::Rosetta2
            | ManagerId::FirmwareUpdates
    )
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FfiManagerStatus {
    manager_id: String,
    detected: bool,
    version: Option<String>,
    executable_path: Option<String>,
    executable_paths: Vec<String>,
    default_executable_path: Option<String>,
    selected_executable_path: Option<String>,
    selected_executable_differs_from_default: bool,
    executable_path_diagnostic: String,
    selected_install_method: Option<String>,
    install_method_options: Vec<FfiManagerInstallMethodOption>,
    timeout_hard_seconds: Option<u64>,
    timeout_idle_seconds: Option<u64>,
    enabled: bool,
    is_implemented: bool,
    is_optional: bool,
    is_detection_only: bool,
    supports_remote_search: bool,
    supports_package_install: bool,
    supports_package_uninstall: bool,
    supports_package_upgrade: bool,
    package_state_issues: Vec<FfiManagerPackageStateIssue>,
    is_eligible: bool,
    ineligible_reason_code: Option<String>,
    ineligible_reason_message: Option<String>,
    ineligible_service_error_key: Option<String>,
    install_instances: Vec<FfiManagerInstallInstanceSummary>,
    install_instance_count: usize,
    multi_instance_state: String,
    multi_instance_acknowledged: bool,
    multi_instance_fingerprint: Option<String>,
    active_provenance: Option<String>,
    active_confidence: Option<f64>,
    active_decision_margin: Option<f64>,
    active_automation_level: Option<String>,
    active_uninstall_strategy: Option<String>,
    active_update_strategy: Option<String>,
    active_remediation_strategy: Option<String>,
    active_explanation_primary: Option<String>,
    active_explanation_secondary: Option<String>,
    competing_provenance: Option<String>,
    competing_confidence: Option<f64>,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FfiManagerPackageStateIssue {
    source_manager_id: String,
    package_name: String,
    issue_code: String,
    finding_code: String,
    fingerprint: String,
    severity: String,
    summary: Option<String>,
    evidence_primary: Option<String>,
    evidence_secondary: Option<String>,
    knowledge_source: Option<String>,
    knowledge_version: Option<String>,
    repair_options: Vec<FfiManagerIssueRepairOption>,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FfiManagerIssueRepairOption {
    option_id: String,
    action: String,
    title: String,
    description: String,
    recommended: bool,
    requires_confirmation: bool,
    automation_level: String,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FfiManagerInstallMethodOption {
    method_id: String,
    recommendation_rank: u8,
    recommendation_reason: Option<String>,
    policy_tag: String,
    executable_path_hints: Vec<String>,
    package_hints: Vec<String>,
}

#[derive(serde::Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FfiManagerInstallInstanceSummary {
    instance_id: String,
    identity_kind: String,
    identity_value: String,
    display_path: String,
    canonical_path: Option<String>,
    alias_paths: Vec<String>,
    is_active: bool,
    version: Option<String>,
    provenance: String,
    confidence: f64,
    decision_margin: Option<f64>,
    automation_level: String,
    uninstall_strategy: String,
    update_strategy: String,
    remediation_strategy: String,
    explanation_primary: Option<String>,
    explanation_secondary: Option<String>,
    competing_provenance: Option<String>,
    competing_confidence: Option<f64>,
}

type FfiManagerUninstallPreview = ManagerUninstallPreview;

#[derive(Clone, Debug)]
struct ManagerUninstallPlan {
    target_manager: ManagerId,
    request: AdapterRequest,
    label_key: &'static str,
    label_args: Vec<(&'static str, String)>,
    preview: FfiManagerUninstallPreview,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
struct RustupUninstallResolution {
    strategy: StrategyKind,
    unknown_override_required: bool,
    used_unknown_override: bool,
}

type ManagerUninstallLegacyRequest = (
    ManagerId,
    AdapterRequest,
    &'static str,
    Vec<(&'static str, String)>,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ManagerAutomationPolicyContext {
    mode: ManagedAutomationPolicyMode,
}

static EXECUTABLE_DISCOVERY_CACHE: OnceLock<
    Mutex<std::collections::HashMap<ManagerId, Vec<String>>>,
> = OnceLock::new();
static MANAGER_AUTOMATION_POLICY_CONTEXT: OnceLock<ManagerAutomationPolicyContext> =
    OnceLock::new();
static COORDINATOR_REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static COORDINATOR_SERVER_STARTED: AtomicBool = AtomicBool::new(false);
static AUTO_CHECK_TICKER_STARTED: AtomicBool = AtomicBool::new(false);

const COORDINATOR_REQUEST_TIMEOUT_SECS: u64 = 30;
const COORDINATOR_POLL_SLEEP_MS: u64 = 25;
const AUTO_CHECK_TICK_SECS: u64 = 30;
#[cfg(any(test, target_os = "macos"))]
const LEGACY_FILE_COORDINATOR_IPC_ENV: &str = "HELM_LEGACY_FILE_COORDINATOR_IPC";
const DEFAULT_CLI_UPDATE_ENDPOINT: &str = "https://helmapp.dev/updates/cli/latest.json";
const DEFAULT_INSTALL_MARKER_RELATIVE_PATH: &str = ".config/helm/install.json";
const MANAGED_INSTALL_METHOD_POLICY_ENV: &str = "HELM_MANAGED_INSTALL_METHOD_POLICY";
const MANAGED_AUTOMATION_POLICY_ENV: &str = "HELM_MANAGED_AUTOMATION_POLICY";
const AUTO_CHECK_ALLOW_INSECURE_ENV: &str = "HELM_CLI_ALLOW_INSECURE_UPDATE_URLS";
const AUTO_CHECK_HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;
const AUTO_CHECK_HTTP_READ_TIMEOUT_SECS: u64 = 30;
const AUTO_CHECK_HTTP_WRITE_TIMEOUT_SECS: u64 = 30;
const AUTO_CHECK_ALLOWED_HOSTS: [&str; 5] = [
    "helmapp.dev",
    "github.com",
    "objects.githubusercontent.com",
    "github-releases.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

#[derive(Debug, serde::Deserialize)]
struct AutoCheckInstallMarker {
    channel: String,
    update_policy: String,
}

#[derive(Clone, Debug)]
enum CoordinatorBridge {
    Disabled,
    Local,
    External(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoordinatorBridgeMode {
    #[cfg(any(test, target_os = "macos"))]
    LocalXpcPreferred,
    LegacyFileIpc,
}

lazy_static! {
    static ref COORDINATOR_BRIDGE: Mutex<CoordinatorBridge> =
        Mutex::new(CoordinatorBridge::Disabled);
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoordinatorRequest {
    Ping,
    Submit {
        manager_id: String,
        request: CoordinatorSubmitRequest,
        wait: bool,
    },
    Cancel {
        task_id: u64,
    },
    StartWorkflow {
        workflow: CoordinatorWorkflowRequest,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoordinatorSubmitRequest {
    Detect,
    Install {
        package_name: String,
        version: Option<String>,
    },
    Uninstall {
        package_name: String,
    },
    Upgrade {
        package_name: Option<String>,
    },
    Pin {
        package_name: String,
        version: Option<String>,
    },
    Unpin {
        package_name: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoordinatorWorkflowRequest {
    RefreshAll,
    RefreshManager {
        manager_id: String,
    },
    DetectAll,
    UpdatesRun {
        include_pinned: bool,
        allow_os_updates: bool,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CoordinatorResponse {
    ok: bool,
    task_id: Option<u64>,
    job_id: Option<String>,
    payload: Option<CoordinatorPayload>,
    error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoordinatorPayload {
    Detection {
        installed: bool,
        version: Option<String>,
        executable_path: Option<String>,
    },
    Mutation {
        manager_id: String,
        package_name: String,
        action: String,
        before_version: Option<String>,
        after_version: Option<String>,
    },
    Refreshed,
    InstalledPackages {
        count: usize,
    },
    OutdatedPackages {
        count: usize,
    },
    SearchResults {
        count: usize,
    },
}

fn coordinator_socket_path_for_store(store: &SqliteStore) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    store.database_path().to_string_lossy().hash(&mut hasher);
    let suffix = format!("{:x}", hasher.finish());
    let root = std::env::var("TMPDIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    root.join(format!("helm-cli-coordinator-{suffix}"))
}

fn coordinator_ready_file(state_dir: &Path) -> PathBuf {
    state_dir.join("ready.json")
}

fn coordinator_requests_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("requests")
}

fn coordinator_responses_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("responses")
}

fn coordinator_request_file(state_dir: &Path, request_id: &str) -> PathBuf {
    coordinator_requests_dir(state_dir).join(format!("{request_id}.json"))
}

fn coordinator_response_file(state_dir: &Path, request_id: &str) -> PathBuf {
    coordinator_responses_dir(state_dir).join(format!("{request_id}.json"))
}

fn next_coordinator_request_id() -> String {
    let counter = COORDINATOR_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{counter}", std::process::id())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), String> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!(
            "failed to set private directory permissions on '{}': {error}",
            path.display()
        )
    })
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create coordinator directory '{}': {error}",
            path.display()
        )
    })?;
    #[cfg(unix)]
    {
        set_private_directory_permissions(path)?;
    }
    Ok(())
}

fn write_private_json_temp_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| {
                format!(
                    "failed to create private temp json file '{}': {error}",
                    path.display()
                )
            })?;
        file.write_all(bytes).map_err(|error| {
            format!(
                "failed to write private temp json file '{}': {error}",
                path.display()
            )
        })?;
        file.sync_all().map_err(|error| {
            format!(
                "failed to flush private temp json file '{}': {error}",
                path.display()
            )
        })?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes).map_err(|error| {
            format!(
                "failed to write temp json file '{}': {error}",
                path.display()
            )
        })
    }
}

fn write_json_file<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let rendered = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode json file '{}': {error}", path.display()))?;
    let temp_name = format!(
        "{}.tmp-{}-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("payload"),
        std::process::id(),
        COORDINATOR_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let temp_path = path.with_file_name(temp_name);
    write_private_json_temp_file(temp_path.as_path(), rendered.as_slice())?;
    std::fs::rename(&temp_path, path).map_err(|error| {
        format!(
            "failed to move temp json file '{}' into '{}': {error}",
            temp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn read_json_file<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read json file '{}': {error}", path.display()))?;
    serde_json::from_str::<T>(&content)
        .map_err(|error| format!("failed to decode json file '{}': {error}", path.display()))
}

fn coordinator_ready(state_dir: &Path) -> bool {
    let ready_file = coordinator_ready_file(state_dir);
    if !ready_file.exists() {
        return false;
    }
    send_coordinator_request_once(state_dir, &CoordinatorRequest::Ping)
        .map(|response| response.ok)
        .unwrap_or(false)
}

fn send_coordinator_request_once(
    state_dir: &Path,
    request: &CoordinatorRequest,
) -> Result<CoordinatorResponse, String> {
    let ready_file = coordinator_ready_file(state_dir);
    if !ready_file.exists() {
        return Err(format!(
            "failed to connect to coordinator at '{}': coordinator not ready",
            state_dir.display()
        ));
    }

    let request_id = next_coordinator_request_id();
    let request_path = coordinator_request_file(state_dir, &request_id);
    let response_path = coordinator_response_file(state_dir, &request_id);
    write_json_file(request_path.as_path(), request)?;

    let timeout = Duration::from_secs(COORDINATOR_REQUEST_TIMEOUT_SECS);
    let started = Instant::now();
    while started.elapsed() < timeout {
        if response_path.exists() {
            let response = read_json_file::<CoordinatorResponse>(response_path.as_path())?;
            let _ = std::fs::remove_file(response_path.as_path());
            return Ok(response);
        }
        thread::sleep(Duration::from_millis(COORDINATOR_POLL_SLEEP_MS));
    }

    let _ = std::fs::remove_file(request_path.as_path());
    Err(format!(
        "timed out waiting for coordinator response in '{}'",
        state_dir.display()
    ))
}

fn manager_executable_candidates(id: ManagerId) -> &'static [&'static str] {
    match id {
        ManagerId::HomebrewFormula | ManagerId::HomebrewCask => {
            &["brew", "/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        }
        ManagerId::Asdf => &["asdf"],
        ManagerId::Mise => &["mise"],
        ManagerId::Rustup => &[
            "rustup",
            "/opt/homebrew/opt/rustup/bin/rustup",
            "/usr/local/opt/rustup/bin/rustup",
        ],
        ManagerId::Npm => &["npm"],
        ManagerId::Pnpm => &["pnpm"],
        ManagerId::Yarn => &["yarn"],
        ManagerId::Pip => &["python3", "pip3", "pip"],
        ManagerId::Pipx => &["pipx"],
        ManagerId::Poetry => &["poetry"],
        ManagerId::RubyGems => &["gem"],
        ManagerId::Bundler => &["bundle"],
        ManagerId::Cargo => &["cargo"],
        ManagerId::CargoBinstall => &["cargo-binstall"],
        ManagerId::MacPorts => &["port", "/opt/local/bin/port"],
        ManagerId::NixDarwin => &["darwin-rebuild", "nix"],
        ManagerId::Mas => &["mas"],
        ManagerId::DockerDesktop => &["docker"],
        ManagerId::Podman => &["podman"],
        ManagerId::Colima => &["colima"],
        ManagerId::XcodeCommandLineTools => &["xcode-select"],
        ManagerId::SoftwareUpdate => &["/usr/sbin/softwareupdate"],
        _ => &[],
    }
}

fn normalize_path_string(path: &std::path::Path) -> Option<String> {
    let rendered = path.to_string_lossy().trim().to_string();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

fn manager_additional_bin_roots() -> Vec<std::path::PathBuf> {
    let mut roots = vec![
        std::path::PathBuf::from("/opt/homebrew/bin"),
        std::path::PathBuf::from("/usr/local/bin"),
        std::path::PathBuf::from("/opt/local/bin"),
    ];

    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        roots.push(home.join(".local/bin"));
        roots.push(home.join(".cargo/bin"));
        roots.push(home.join(".asdf/bin"));
        roots.push(home.join(".asdf/shims"));
    }

    roots
}

fn manager_versioned_install_roots(id: ManagerId) -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();

    if matches!(
        id,
        ManagerId::HomebrewFormula
            | ManagerId::HomebrewCask
            | ManagerId::Mise
            | ManagerId::Asdf
            | ManagerId::Rustup
            | ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
            | ManagerId::Mas
            | ManagerId::DockerDesktop
            | ManagerId::Podman
            | ManagerId::Colima
    ) {
        roots.push(std::path::PathBuf::from("/opt/homebrew/Cellar"));
        roots.push(std::path::PathBuf::from("/usr/local/Cellar"));
    }

    if matches!(
        id,
        ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
    ) && let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from)
    {
        roots.push(home.join(".asdf/installs"));
        roots.push(home.join(".local/share/mise/installs"));
    }

    roots
}

fn push_discovered_path(
    candidate_path: &std::path::Path,
    discovered: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if candidate_path.is_file()
        && let Some(rendered) = normalize_path_string(candidate_path)
        && seen.insert(rendered.clone())
    {
        discovered.push(rendered);
    }
}

fn discover_executable_paths(id: ManagerId, candidates: &[&str]) -> Vec<String> {
    let mut discovered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let additional_bin_roots = manager_additional_bin_roots();
    let versioned_roots = manager_versioned_install_roots(id);

    let path_dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .as_deref()
        .map(std::env::split_paths)
        .map(|iter| iter.collect::<Vec<_>>())
        .unwrap_or_default();

    for candidate in candidates {
        if candidate.contains('/') {
            let absolute = std::path::PathBuf::from(candidate);
            push_discovered_path(absolute.as_path(), &mut discovered, &mut seen);
            continue;
        }

        for path_dir in &path_dirs {
            let full = path_dir.join(candidate);
            push_discovered_path(full.as_path(), &mut discovered, &mut seen);
        }

        for path_dir in &additional_bin_roots {
            let full = path_dir.join(candidate);
            push_discovered_path(full.as_path(), &mut discovered, &mut seen);
        }

        for root in &versioned_roots {
            let Ok(tool_dirs) = std::fs::read_dir(root) else {
                continue;
            };
            for tool_dir in tool_dirs.flatten() {
                let tool_path = tool_dir.path();
                if !tool_path.is_dir() {
                    continue;
                }

                let Ok(version_dirs) = std::fs::read_dir(&tool_path) else {
                    continue;
                };
                for version_dir in version_dirs.flatten() {
                    let version_path = version_dir.path();
                    if !version_path.is_dir() {
                        continue;
                    }
                    let full = version_path.join("bin").join(candidate);
                    push_discovered_path(full.as_path(), &mut discovered, &mut seen);
                }
            }
        }
    }

    discovered
}

fn cached_discovered_executable_paths(id: ManagerId, candidates: &[&str]) -> Vec<String> {
    let cache =
        EXECUTABLE_DISCOVERY_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));

    if let Ok(guard) = cache.lock()
        && let Some(cached) = guard.get(&id)
    {
        return cached.clone();
    }

    let discovered = discover_executable_paths(id, candidates);

    if let Ok(mut guard) = cache.lock() {
        guard.insert(id, discovered.clone());
    }

    discovered
}

fn collect_manager_executable_paths(
    id: ManagerId,
    active_path: Option<&std::path::Path>,
) -> Vec<String> {
    let mut resolved = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(active_path) = active_path
        && let Some(rendered) = normalize_path_string(active_path)
    {
        seen.insert(rendered.clone());
        resolved.push(rendered);
    }

    for discovered in cached_discovered_executable_paths(id, manager_executable_candidates(id)) {
        if seen.insert(discovered.clone()) {
            resolved.push(discovered);
        }
    }

    resolved
}

fn default_manager_executable_path(id: ManagerId, executable_paths: &[String]) -> Option<String> {
    if let Some(first) = executable_paths.first() {
        return Some(first.clone());
    }
    if let Some(discovered) =
        cached_discovered_executable_paths(id, manager_executable_candidates(id))
            .into_iter()
            .next()
    {
        return Some(discovered);
    }
    None
}

fn manager_install_method_candidates(id: ManagerId) -> &'static [&'static str] {
    helm_core::registry::manager_install_method_candidates(id)
}

fn normalize_install_method(id: ManagerId, method: Option<String>) -> Option<String> {
    let method = normalize_nonempty(method)?;
    if manager_install_method_candidates(id).contains(&method.as_str()) {
        return Some(method);
    }
    None
}

fn resolve_selected_executable_path(
    preferred: Option<String>,
    default_path: Option<String>,
) -> Option<String> {
    if let Some(preferred) = preferred
        && std::path::Path::new(preferred.as_str()).is_file()
    {
        return Some(preferred);
    }
    default_path
}

fn selected_executable_differs_from_default(
    default_path: Option<&str>,
    selected_path: Option<&str>,
) -> bool {
    matches!(
        (default_path, selected_path),
        (Some(default), Some(selected)) if default != selected
    )
}

fn manager_executable_path_diagnostic(
    default_path: Option<&str>,
    selected_path: Option<&str>,
) -> &'static str {
    match (default_path, selected_path) {
        (Some(default), Some(selected)) if default == selected => "aligned",
        (Some(_), Some(_)) => "diverged",
        (None, Some(_)) => "selected_only",
        (Some(_), None) => "default_only",
        (None, None) => "unresolved",
    }
}

fn resolved_manager_selected_executable_path(
    manager: ManagerId,
    detection_map: &std::collections::HashMap<ManagerId, DetectionInfo>,
    pref_map: &std::collections::HashMap<ManagerId, ManagerPreference>,
) -> Option<String> {
    let detection = detection_map.get(&manager);
    let active_path = detection.and_then(|d| d.executable_path.as_deref());
    let executable_paths = collect_manager_executable_paths(manager, active_path);
    let default_path = default_manager_executable_path(manager, &executable_paths);
    let preferred_path = pref_map
        .get(&manager)
        .and_then(|pref| normalize_nonempty(pref.selected_executable_path.clone()));
    resolve_selected_executable_path(preferred_path, default_path)
}

fn sync_manager_executable_overrides(
    detection_map: &std::collections::HashMap<ManagerId, DetectionInfo>,
    pref_map: &std::collections::HashMap<ManagerId, ManagerPreference>,
) {
    let mut executable_overrides: std::collections::HashMap<ManagerId, std::path::PathBuf> =
        std::collections::HashMap::new();
    let mut timeout_profiles: std::collections::HashMap<ManagerId, ManagerTimeoutProfile> =
        std::collections::HashMap::new();
    for manager in ManagerId::ALL {
        let selected = resolved_manager_selected_executable_path(manager, detection_map, pref_map);
        if let Some(path) = selected {
            executable_overrides.insert(manager, std::path::PathBuf::from(path));
        }
        let hard_timeout = pref_map
            .get(&manager)
            .and_then(|preference| preference.timeout_hard_seconds)
            .filter(|value| *value > 0)
            .map(Duration::from_secs);
        let idle_timeout = pref_map
            .get(&manager)
            .and_then(|preference| preference.timeout_idle_seconds)
            .filter(|value| *value > 0)
            .map(Duration::from_secs);
        let profile = ManagerTimeoutProfile {
            hard_timeout,
            idle_timeout,
        };
        if profile.hard_timeout.is_some() || profile.idle_timeout.is_some() {
            timeout_profiles.insert(manager, profile);
        }
    }
    replace_manager_execution_preferences(executable_overrides, timeout_profiles);
}

fn build_manager_statuses(
    runtime: Option<&AdapterRuntime>,
    store: Option<&SqliteStore>,
    detection_map: &std::collections::HashMap<ManagerId, DetectionInfo>,
    pref_map: &std::collections::HashMap<ManagerId, ManagerPreference>,
) -> Vec<FfiManagerStatus> {
    let mut install_instances_by_manager: std::collections::HashMap<
        ManagerId,
        Vec<ManagerInstallInstance>,
    > = std::collections::HashMap::new();
    let mut multi_instance_ack_fingerprints: std::collections::HashMap<ManagerId, Option<String>> =
        std::collections::HashMap::new();
    if let Some(store) = store
        && let Ok(instances) = store.list_install_instances(None)
    {
        for instance in instances {
            let instance = apply_manager_automation_policy(&instance);
            install_instances_by_manager
                .entry(instance.manager)
                .or_default()
                .push(instance);
        }
        for instances in install_instances_by_manager.values_mut() {
            instances.sort_by(|left, right| {
                if left.is_active != right.is_active {
                    return right.is_active.cmp(&left.is_active);
                }
                left.instance_id.cmp(&right.instance_id)
            });
        }
        for manager in ManagerId::ALL {
            let fingerprint = store
                .manager_multi_instance_ack_fingerprint(manager)
                .ok()
                .and_then(normalize_nonempty);
            multi_instance_ack_fingerprints.insert(manager, fingerprint);
        }
    }

    let homebrew_installed_formulas: std::collections::HashSet<String> = store
        .and_then(|store| store.list_installed().ok())
        .map(|packages| {
            packages
                .into_iter()
                .filter(|package| package.package.manager == ManagerId::HomebrewFormula)
                .filter_map(|package| {
                    let name = package.package.name.trim().to_ascii_lowercase();
                    (!name.is_empty()).then_some(name)
                })
                .collect()
        })
        .unwrap_or_default();

    ManagerId::ALL
        .iter()
        .map(|&id| {
            let detection = detection_map.get(&id);
            let configured_enabled = pref_map
                .get(&id)
                .map(|pref| pref.enabled)
                .unwrap_or_else(|| default_enabled_for_manager(id));
            let selected_install_method = normalize_install_method(
                id,
                pref_map
                    .get(&id)
                    .and_then(|pref| pref.selected_install_method.clone()),
            );
            let install_method_options = manager_install_method_options(id);
            let timeout_hard_seconds = pref_map
                .get(&id)
                .and_then(|pref| pref.timeout_hard_seconds)
                .filter(|value| *value > 0);
            let timeout_idle_seconds = pref_map
                .get(&id)
                .and_then(|pref| pref.timeout_idle_seconds)
                .filter(|value| *value > 0);
            let is_implemented = is_implemented_manager(id);
            let is_optional = is_optional_manager(id);
            let is_detection_only = is_detection_only_manager(id);
            let detected = detection.map(|d| d.installed).unwrap_or(false);
            let executable_path = detection.and_then(|d| {
                normalize_nonempty(
                    d.executable_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string()),
                )
            });
            let executable_paths = if detected {
                collect_manager_executable_paths(
                    id,
                    detection.and_then(|d| d.executable_path.as_deref()),
                )
            } else {
                Vec::new()
            };
            let default_executable_path = default_manager_executable_path(id, &executable_paths);
            let selected_executable_path =
                resolved_manager_selected_executable_path(id, detection_map, pref_map);
            let selected_executable_differs_from_default = selected_executable_differs_from_default(
                default_executable_path.as_deref(),
                selected_executable_path.as_deref(),
            );
            let executable_path_diagnostic = manager_executable_path_diagnostic(
                default_executable_path.as_deref(),
                selected_executable_path.as_deref(),
            )
            .to_string();
            let eligibility = manager_enablement_eligibility(
                id,
                selected_executable_path
                    .as_deref()
                    .map(std::path::Path::new),
            );
            let version = detection.and_then(|d| normalize_nonempty(d.version.clone()));
            let supports_remote_search = runtime
                .map(|runtime| can_submit_remote_search(runtime, id))
                .unwrap_or_else(|| manager_participates_in_package_search(id));
            let supports_package_install = runtime
                .map(|runtime| supports_individual_package_install(runtime, id))
                .unwrap_or(false);
            let supports_package_uninstall = runtime
                .map(|runtime| supports_individual_package_uninstall(runtime, id))
                .unwrap_or(false);
            let supports_package_upgrade = runtime
                .map(|runtime| supports_individual_package_upgrade(runtime, id))
                .unwrap_or(false);
            let manager_install_instances = install_instances_by_manager.get(&id);
            let install_instance_count = manager_install_instances.map_or(0, Vec::len);
            let install_instances = manager_install_instances
                .map(|instances| {
                    instances
                        .iter()
                        .map(build_install_instance_summary)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let active_instance = manager_install_instances
                .and_then(|instances| instances.iter().find(|instance| instance.is_active))
                .or_else(|| manager_install_instances.and_then(|instances| instances.first()));
            let acknowledged_fingerprint = multi_instance_ack_fingerprints
                .get(&id)
                .and_then(|value| value.as_deref());
            let (multi_instance_state, multi_instance_fingerprint, multi_instance_acknowledged) =
                resolve_multi_instance_state(
                    manager_install_instances.into_iter().flat_map(|instances| {
                        instances
                            .iter()
                            .map(|instance| instance.instance_id.as_str())
                    }),
                    acknowledged_fingerprint,
                );
            let package_state_issues = manager_package_state_issues(
                id,
                manager_install_instances,
                &homebrew_installed_formulas,
            );
            let setup_required = package_state_issues.iter().any(|issue| {
                issue.issue_code == helm_core::doctor::ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED
            });
            let enabled = configured_enabled && eligibility.is_eligible && !setup_required;

            FfiManagerStatus {
                manager_id: id.as_str().to_string(),
                detected,
                version,
                executable_path,
                executable_paths,
                default_executable_path,
                selected_executable_path,
                selected_executable_differs_from_default,
                executable_path_diagnostic,
                selected_install_method,
                install_method_options,
                timeout_hard_seconds,
                timeout_idle_seconds,
                enabled,
                is_implemented,
                is_optional,
                is_detection_only,
                supports_remote_search,
                supports_package_install,
                supports_package_uninstall,
                supports_package_upgrade,
                package_state_issues,
                is_eligible: eligibility.is_eligible,
                ineligible_reason_code: eligibility.reason_code.map(str::to_string),
                ineligible_reason_message: eligibility.reason_message.map(str::to_string),
                ineligible_service_error_key: eligibility.service_error_key.map(str::to_string),
                install_instances,
                install_instance_count,
                multi_instance_state: multi_instance_state.as_str().to_string(),
                multi_instance_acknowledged,
                multi_instance_fingerprint,
                active_provenance: active_instance
                    .map(|instance| instance.provenance.as_str().to_string()),
                active_confidence: active_instance.map(|instance| instance.confidence),
                active_decision_margin: active_instance
                    .and_then(|instance| instance.decision_margin),
                active_automation_level: active_instance
                    .map(|instance| instance.automation_level.as_str().to_string()),
                active_uninstall_strategy: active_instance
                    .map(|instance| instance.uninstall_strategy.as_str().to_string()),
                active_update_strategy: active_instance
                    .map(|instance| instance.update_strategy.as_str().to_string()),
                active_remediation_strategy: active_instance
                    .map(|instance| instance.remediation_strategy.as_str().to_string()),
                active_explanation_primary: active_instance
                    .and_then(|instance| normalize_nonempty(instance.explanation_primary.clone())),
                active_explanation_secondary: active_instance.and_then(|instance| {
                    normalize_nonempty(instance.explanation_secondary.clone())
                }),
                competing_provenance: active_instance.and_then(|instance| {
                    instance
                        .competing_provenance
                        .map(|value| value.as_str().to_string())
                }),
                competing_confidence: active_instance
                    .and_then(|instance| instance.competing_confidence),
            }
        })
        .collect()
}

fn manager_package_state_issues(
    manager: ManagerId,
    manager_install_instances: Option<&Vec<ManagerInstallInstance>>,
    homebrew_installed_formulas: &std::collections::HashSet<String>,
) -> Vec<FfiManagerPackageStateIssue> {
    let findings = helm_core::doctor::scan_manager_package_state_issues(
        helm_core::doctor::ManagerPackageStateScanInput {
            manager,
            manager_install_instances: manager_install_instances.map(Vec::as_slice),
            homebrew_installed_formulas,
        },
    );

    findings
        .into_iter()
        .map(|finding| {
            let repair_plan = helm_core::repair::plan_for_finding(&finding);
            let repair_options = repair_plan
                .as_ref()
                .map(|plan| {
                    plan.options
                        .iter()
                        .map(|option| FfiManagerIssueRepairOption {
                            option_id: option.option_id.clone(),
                            action: option.action.as_str().to_string(),
                            title: option.title.clone(),
                            description: option.description.clone(),
                            recommended: option.recommended,
                            requires_confirmation: option.requires_confirmation,
                            automation_level: option.automation_level.as_str().to_string(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            FfiManagerPackageStateIssue {
                source_manager_id: finding.source_manager_id.unwrap_or_default(),
                package_name: finding.package_name.unwrap_or_default(),
                issue_code: finding.issue_code.clone(),
                finding_code: finding.finding_code,
                fingerprint: finding.fingerprint,
                severity: finding.severity.as_str().to_string(),
                summary: normalize_nonempty(Some(finding.summary)),
                evidence_primary: normalize_nonempty(finding.evidence_primary),
                evidence_secondary: normalize_nonempty(finding.evidence_secondary),
                knowledge_source: repair_plan
                    .as_ref()
                    .map(|plan| plan.knowledge_source.clone()),
                knowledge_version: repair_plan
                    .as_ref()
                    .map(|plan| plan.knowledge_version.clone()),
                repair_options,
            }
        })
        .collect()
}

fn manager_install_method_options(manager: ManagerId) -> Vec<FfiManagerInstallMethodOption> {
    helm_core::registry::manager_install_method_specs(manager)
        .iter()
        .map(|spec| FfiManagerInstallMethodOption {
            method_id: spec.id.to_string(),
            recommendation_rank: spec.recommendation_rank,
            recommendation_reason: spec
                .recommendation_reason
                .map(|reason| reason.as_str().to_string()),
            policy_tag: spec.policy_tag.as_str().to_string(),
            executable_path_hints: spec
                .executable_path_hints
                .iter()
                .map(|hint| hint.to_string())
                .collect(),
            package_hints: spec
                .package_hints
                .iter()
                .map(|hint| hint.to_string())
                .collect(),
        })
        .collect()
}

fn build_install_instance_summary(
    instance: &ManagerInstallInstance,
) -> FfiManagerInstallInstanceSummary {
    let mut alias_paths = instance
        .alias_paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    alias_paths.sort();
    alias_paths.dedup();
    FfiManagerInstallInstanceSummary {
        instance_id: instance.instance_id.clone(),
        identity_kind: instance.identity_kind.as_str().to_string(),
        identity_value: instance.identity_value.clone(),
        display_path: instance.display_path.to_string_lossy().to_string(),
        canonical_path: instance
            .canonical_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        alias_paths,
        is_active: instance.is_active,
        version: instance.version.clone(),
        provenance: instance.provenance.as_str().to_string(),
        confidence: instance.confidence,
        decision_margin: instance.decision_margin,
        automation_level: instance.automation_level.as_str().to_string(),
        uninstall_strategy: instance.uninstall_strategy.as_str().to_string(),
        update_strategy: instance.update_strategy.as_str().to_string(),
        remediation_strategy: instance.remediation_strategy.as_str().to_string(),
        explanation_primary: normalize_nonempty(instance.explanation_primary.clone()),
        explanation_secondary: normalize_nonempty(instance.explanation_secondary.clone()),
        competing_provenance: instance
            .competing_provenance
            .map(|provenance| provenance.as_str().to_string()),
        competing_confidence: instance.competing_confidence,
    }
}

fn set_task_label(task_id: helm_core::models::TaskId, key: &str, args: &[(&str, String)]) {
    let mut args_map = std::collections::BTreeMap::new();
    for (arg_key, arg_value) in args {
        args_map.insert((*arg_key).to_string(), arg_value.clone());
    }
    lock_or_recover(&TASK_LABELS, "task_labels").insert(
        task_id.0,
        TaskLabel {
            key: key.to_string(),
            args: args_map,
        },
    );
}

fn append_local_task_log(
    store: &SqliteStore,
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    status: TaskStatus,
    level: TaskLogLevel,
    message: impl Into<String>,
) {
    let _ = store.append_task_log(&helm_core::models::NewTaskLogRecord {
        task_id,
        manager,
        task_type,
        status: Some(status),
        level,
        message: message.into(),
        created_at: std::time::SystemTime::now(),
    });
}

fn create_local_task(
    store: &SqliteStore,
    manager: ManagerId,
    task_type: TaskType,
) -> Result<TaskId, &'static str> {
    let task_id = store
        .next_task_id()
        .map(TaskId)
        .map_err(|_| SERVICE_ERROR_STORAGE_FAILURE)?;
    let record = helm_core::models::TaskRecord {
        id: task_id,
        manager,
        task_type,
        status: TaskStatus::Queued,
        created_at: std::time::SystemTime::now(),
    };
    store
        .create_task(&record)
        .map_err(|_| SERVICE_ERROR_STORAGE_FAILURE)?;
    append_local_task_log(
        store,
        task_id,
        manager,
        task_type,
        TaskStatus::Queued,
        TaskLogLevel::Info,
        "task queued",
    );
    Ok(task_id)
}

fn update_local_task_status(
    store: &SqliteStore,
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    status: TaskStatus,
    level: TaskLogLevel,
    message: impl Into<String>,
) {
    let record = helm_core::models::TaskRecord {
        id: task_id,
        manager,
        task_type,
        status,
        created_at: std::time::SystemTime::now(),
    };
    let _ = store.update_task(&record);
    append_local_task_log(store, task_id, manager, task_type, status, level, message);
}

const TASK_PRUNE_MAX_AGE_SECS: i64 = 300;
const TASK_RECENT_FETCH_LIMIT: usize = 1000;
const TASK_TERMINAL_HISTORY_LIMIT: usize = 50;
const TASK_INFLIGHT_DEDUP_MAX_AGE_SECS: u64 = 1800;

fn is_inflight_status(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Queued | TaskStatus::Running)
}

fn should_replace_visible_inflight_task(
    current: &helm_core::models::TaskRecord,
    candidate: &helm_core::models::TaskRecord,
) -> bool {
    let current_running = current.status == TaskStatus::Running;
    let candidate_running = candidate.status == TaskStatus::Running;

    if current_running != candidate_running {
        return candidate_running;
    }

    candidate.id.0 > current.id.0
}

fn is_recent_inflight_task(task: &helm_core::models::TaskRecord) -> bool {
    std::time::SystemTime::now()
        .duration_since(task.created_at)
        .map(|elapsed| elapsed.as_secs() <= TASK_INFLIGHT_DEDUP_MAX_AGE_SECS)
        .unwrap_or(true)
}

fn task_signature_key(
    task: &helm_core::models::TaskRecord,
    labels: &std::collections::HashMap<u64, TaskLabel>,
) -> String {
    labels
        .get(&task.id.0)
        .map(|label| {
            let mut encoded = format!("{:?}:{:?}:{}", task.manager, task.task_type, label.key);
            for (arg_key, arg_value) in &label.args {
                encoded.push('|');
                encoded.push_str(arg_key);
                encoded.push('=');
                encoded.push_str(arg_value);
            }
            encoded
        })
        .unwrap_or_else(|| format!("{:?}:{:?}", task.manager, task.task_type))
}

fn build_visible_tasks(
    tasks: Vec<helm_core::models::TaskRecord>,
    labels: &std::collections::HashMap<u64, TaskLabel>,
) -> Vec<helm_core::models::TaskRecord> {
    let mut visible = Vec::with_capacity(tasks.len());
    let mut seen_inflight: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut seen_signatures: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut terminal_count = 0usize;

    for task in tasks {
        let signature = task_signature_key(&task, labels);
        if is_inflight_status(task.status) {
            if let Some(existing_index) = seen_inflight.get(&signature).copied() {
                if should_replace_visible_inflight_task(&visible[existing_index], &task) {
                    visible[existing_index] = task;
                }
            } else {
                seen_inflight.insert(signature.clone(), visible.len());
                visible.push(task);
            }
            seen_signatures.insert(signature);
            continue;
        }

        if task.status == TaskStatus::Failed && seen_signatures.contains(&signature) {
            continue;
        }

        if terminal_count < TASK_TERMINAL_HISTORY_LIMIT {
            visible.push(task);
            terminal_count = terminal_count.saturating_add(1);
        }
        seen_signatures.insert(signature);
    }

    visible
}

fn find_matching_inflight_task(
    store: &SqliteStore,
    manager: ManagerId,
    task_type: TaskType,
    label_key: Option<&str>,
    label_args: &[(&str, String)],
) -> Option<helm_core::models::TaskId> {
    let tasks = store.list_recent_tasks(TASK_RECENT_FETCH_LIMIT).ok()?;
    let labels = lock_or_recover(&TASK_LABELS, "task_labels");

    tasks.into_iter().find_map(|task| {
        if task.manager != manager
            || task.task_type != task_type
            || !is_inflight_status(task.status)
            || !is_recent_inflight_task(&task)
        {
            return None;
        }

        let Some(expected_label_key) = label_key else {
            return Some(task.id);
        };

        let label = labels.get(&task.id.0)?;
        if label.key != expected_label_key || label.args.len() != label_args.len() {
            return None;
        }

        let args_match = label_args.iter().all(|(arg_key, arg_value)| {
            label
                .args
                .get(*arg_key)
                .map(|v| v == arg_value)
                .unwrap_or(false)
        });

        if args_match { Some(task.id) } else { None }
    })
}

fn cancel_inflight_tasks_for_manager(
    store: &SqliteStore,
    runtime: &AdapterRuntime,
    rt_handle: &tokio::runtime::Handle,
    manager: ManagerId,
) {
    let task_ids: Vec<TaskId> = store
        .list_recent_tasks(TASK_RECENT_FETCH_LIMIT)
        .unwrap_or_default()
        .into_iter()
        .filter(|task| task.manager == manager && is_inflight_status(task.status))
        .map(|task| task.id)
        .collect();

    if task_ids.is_empty() {
        return;
    }

    if external_coordinator_state_dir().is_some() {
        for task_id in task_ids.iter().copied() {
            if let Err(error) = coordinator_cancel_external(task_id.0) {
                eprintln!(
                    "set_manager_enabled: failed to cancel task {} for {} via coordinator: {}",
                    task_id.0,
                    manager.as_str(),
                    error
                );
            }
        }
        let mut labels = lock_or_recover(&TASK_LABELS, "task_labels");
        for task_id in task_ids {
            labels.remove(&task_id.0);
        }
        return;
    }

    rt_handle.block_on(async {
        for task_id in task_ids.iter().copied() {
            if let Err(error) = runtime.cancel(task_id, CancellationMode::Immediate).await {
                eprintln!(
                    "set_manager_enabled: failed to cancel task {} for {}: {}",
                    task_id.0,
                    manager.as_str(),
                    error
                );
            }
        }
    });

    let mut labels = lock_or_recover(&TASK_LABELS, "task_labels");
    for task_id in task_ids {
        labels.remove(&task_id.0);
    }
}

fn purge_tasks_for_manager(
    store: &SqliteStore,
    runtime: &AdapterRuntime,
    rt_handle: &tokio::runtime::Handle,
    manager: ManagerId,
    context: &str,
) -> bool {
    let task_ids_for_manager: std::collections::HashSet<u64> = store
        .list_recent_tasks(TASK_RECENT_FETCH_LIMIT)
        .unwrap_or_default()
        .into_iter()
        .filter(|task| task.manager == manager)
        .map(|task| task.id.0)
        .collect();

    if task_ids_for_manager.is_empty() {
        return true;
    }

    cancel_inflight_tasks_for_manager(store, runtime, rt_handle, manager);
    if let Err(error) = store.delete_tasks_for_manager(manager) {
        eprintln!(
            "{}: failed to delete task history for '{}' ({})",
            context,
            manager.as_str(),
            error
        );
        return false;
    }

    let mut labels = lock_or_recover(&TASK_LABELS, "task_labels");
    labels.retain(|task_id, _| !task_ids_for_manager.contains(task_id));
    true
}

fn apply_manager_enablement_self_heal(
    store: &SqliteStore,
    runtime: &AdapterRuntime,
    rt_handle: &tokio::runtime::Handle,
    detection_map: &std::collections::HashMap<ManagerId, DetectionInfo>,
    pref_map: &mut std::collections::HashMap<ManagerId, ManagerPreference>,
) -> bool {
    let mut changed = false;

    for manager in ManagerId::ALL {
        let configured_enabled = pref_map
            .get(&manager)
            .map(|preference| preference.enabled)
            .unwrap_or_else(|| default_enabled_for_manager(manager));
        if !configured_enabled {
            continue;
        }

        let selected_executable =
            resolved_manager_selected_executable_path(manager, detection_map, pref_map);
        let eligibility = manager_enablement_eligibility(
            manager,
            selected_executable.as_deref().map(std::path::Path::new),
        );
        if eligibility.is_eligible {
            continue;
        }

        if let Err(error) = store.set_manager_enabled(manager, false) {
            eprintln!(
                "manager policy self-heal: failed to persist disable for '{}' (reason_code={}, error={})",
                manager.as_str(),
                eligibility.reason_code.unwrap_or("manager.ineligible"),
                error
            );
            continue;
        }

        if let Some(preference) = pref_map.get_mut(&manager) {
            preference.enabled = false;
        } else {
            pref_map.insert(
                manager,
                ManagerPreference {
                    manager,
                    enabled: false,
                    selected_executable_path: None,
                    selected_install_method: None,
                    timeout_hard_seconds: None,
                    timeout_idle_seconds: None,
                },
            );
        }

        eprintln!(
            "manager policy self-heal: auto-disabled '{}' (reason_code={}, executable_path='{}')",
            manager.as_str(),
            eligibility.reason_code.unwrap_or("manager.ineligible"),
            selected_executable.as_deref().unwrap_or("<none>")
        );
        let _ = purge_tasks_for_manager(
            store,
            runtime,
            rt_handle,
            manager,
            "manager policy self-heal",
        );
        changed = true;
    }

    changed
}

fn grouped_install_instances_by_manager(
    store: &SqliteStore,
) -> std::collections::HashMap<ManagerId, Vec<ManagerInstallInstance>> {
    let mut grouped = std::collections::HashMap::new();
    if let Ok(instances) = store.list_install_instances(None) {
        for instance in instances {
            grouped
                .entry(instance.manager)
                .or_insert_with(Vec::new)
                .push(instance);
        }
    }
    grouped
}

fn homebrew_installed_formula_set(store: &SqliteStore) -> std::collections::HashSet<String> {
    store
        .list_installed()
        .unwrap_or_default()
        .into_iter()
        .filter(|package| package.package.manager == ManagerId::HomebrewFormula)
        .filter_map(|package| {
            let normalized = package.package.name.trim().to_ascii_lowercase();
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect()
}

fn manager_has_setup_required_issue(
    manager: ManagerId,
    install_instances_by_manager: &std::collections::HashMap<ManagerId, Vec<ManagerInstallInstance>>,
    homebrew_installed_formulas: &std::collections::HashSet<String>,
) -> bool {
    manager_package_state_issues(
        manager,
        install_instances_by_manager.get(&manager),
        homebrew_installed_formulas,
    )
    .iter()
    .any(|issue| issue.issue_code == helm_core::doctor::ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED)
}

fn manager_enabled_map(store: &SqliteStore) -> std::collections::HashMap<ManagerId, bool> {
    let detections: std::collections::HashMap<ManagerId, DetectionInfo> = store
        .list_detections()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let preferences: std::collections::HashMap<ManagerId, ManagerPreference> = store
        .list_manager_preferences()
        .unwrap_or_default()
        .into_iter()
        .map(|preference| (preference.manager, preference))
        .collect();
    let install_instances_by_manager = grouped_install_instances_by_manager(store);
    let homebrew_installed_formulas = homebrew_installed_formula_set(store);

    let mut enabled_by_manager = std::collections::HashMap::new();
    for manager in ManagerId::ALL {
        let configured_enabled = preferences
            .get(&manager)
            .map(|preference| preference.enabled)
            .unwrap_or_else(|| default_enabled_for_manager(manager));
        let selected_executable =
            resolved_manager_selected_executable_path(manager, &detections, &preferences);
        let eligibility = manager_enablement_eligibility(
            manager,
            selected_executable.as_deref().map(std::path::Path::new),
        );
        let setup_required = manager_has_setup_required_issue(
            manager,
            &install_instances_by_manager,
            &homebrew_installed_formulas,
        );
        enabled_by_manager.insert(
            manager,
            configured_enabled && eligibility.is_eligible && !setup_required,
        );
    }

    enabled_by_manager
}

fn manager_is_enabled(
    enabled_by_manager: &std::collections::HashMap<ManagerId, bool>,
    manager: ManagerId,
) -> bool {
    enabled_by_manager.get(&manager).copied().unwrap_or(true)
}

fn active_install_instances_by_manager(
    store: &SqliteStore,
) -> std::collections::HashMap<ManagerId, ManagerInstallInstance> {
    let mut grouped: std::collections::HashMap<ManagerId, Vec<ManagerInstallInstance>> =
        std::collections::HashMap::new();
    if let Ok(instances) = store.list_install_instances(None) {
        for instance in instances {
            grouped.entry(instance.manager).or_default().push(instance);
        }
    }

    let mut active = std::collections::HashMap::new();
    for (manager, mut instances) in grouped {
        instances.sort_by(|left, right| {
            if left.is_active != right.is_active {
                return right.is_active.cmp(&left.is_active);
            }
            left.instance_id.cmp(&right.instance_id)
        });
        if let Some(instance) = instances.into_iter().next() {
            active.insert(manager, instance);
        }
    }
    active
}

fn enabled_dependents_for_manager(
    store: &SqliteStore,
    enabled_by_manager: &std::collections::HashMap<ManagerId, bool>,
    manager: ManagerId,
) -> Vec<ManagerId> {
    let active_instances = active_install_instances_by_manager(store);
    let mut dependents = Vec::new();
    for candidate in ManagerId::ALL {
        if candidate == manager || !manager_is_enabled(enabled_by_manager, candidate) {
            continue;
        }
        let Some(instance) = active_instances.get(&candidate) else {
            continue;
        };
        if provenance_dependency_manager(candidate, instance.provenance) == Some(manager) {
            dependents.push(candidate);
        }
    }

    dependents.sort_by_key(|id| id.as_str());
    dependents
}

fn has_recent_refresh_or_detection(
    store: &SqliteStore,
    enabled_by_manager: &std::collections::HashMap<ManagerId, bool>,
) -> bool {
    store
        .list_recent_tasks(TASK_RECENT_FETCH_LIMIT)
        .ok()
        .map(|tasks| {
            tasks.into_iter().any(|task| {
                manager_is_enabled(enabled_by_manager, task.manager)
                    && is_inflight_status(task.status)
                    && is_recent_inflight_task(&task)
                    && matches!(task.task_type, TaskType::Refresh | TaskType::Detection)
            })
        })
        .unwrap_or(false)
}

fn preseed_presence_detections(
    store: &SqliteStore,
    runtime: &AdapterRuntime,
    enabled_by_manager: &std::collections::HashMap<ManagerId, bool>,
) {
    for manager in ManagerId::ALL {
        if !manager_is_enabled(enabled_by_manager, manager) {
            continue;
        }
        if !is_implemented_manager(manager) || !runtime.has_manager(manager) {
            continue;
        }

        let discovered_paths = collect_manager_executable_paths(manager, None);
        if let Some(executable_path) = discovered_paths.first().map(std::path::PathBuf::from) {
            let info = DetectionInfo {
                installed: true,
                executable_path: Some(executable_path),
                version: None,
            };
            let _ = store.upsert_detection(manager, &info);
        }
    }
}

fn search_label_key_for_query(query: &str) -> &'static str {
    if query.trim().is_empty() {
        "service.task.label.search.manager"
    } else {
        "service.task.label.search.package"
    }
}

fn search_label_args(manager: ManagerId, query: &str) -> Vec<(&'static str, String)> {
    if query.trim().is_empty() {
        vec![("manager", manager_display_name(manager).to_string())]
    } else {
        vec![
            ("manager", manager_display_name(manager).to_string()),
            ("query", query.trim().to_string()),
        ]
    }
}

fn can_submit_remote_search(runtime: &AdapterRuntime, manager: ManagerId) -> bool {
    manager_participates_in_package_search(manager)
        && runtime.is_manager_enabled(manager)
        && runtime.supports_capability(manager, Capability::Search)
}

fn manager_participates_in_package_search(manager: ManagerId) -> bool {
    helm_core::registry::manager_participates_in_package_search(manager)
}

fn queue_remote_search_task(
    store: &SqliteStore,
    runtime: &AdapterRuntime,
    rt_handle: &tokio::runtime::Handle,
    manager: ManagerId,
    query: &str,
) -> Result<helm_core::models::TaskId, &'static str> {
    if !can_submit_remote_search(runtime, manager) {
        return Err(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }

    let label_key = search_label_key_for_query(query);
    let label_args = search_label_args(manager, query);

    if let Some(existing) = find_matching_inflight_task(
        store,
        manager,
        TaskType::Search,
        Some(label_key),
        &label_args,
    ) {
        return Ok(existing);
    }

    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: query.trim().to_string(),
            issued_at: std::time::SystemTime::now(),
        },
    });

    match rt_handle.block_on(runtime.submit(manager, request)) {
        Ok(task_id) => {
            set_task_label(task_id, label_key, &label_args);
            Ok(task_id)
        }
        Err(error) => {
            eprintln!(
                "Failed to queue remote search for manager {} with query '{}': {}",
                manager.as_str(),
                query.trim(),
                error
            );
            Err(SERVICE_ERROR_PROCESS_FAILURE)
        }
    }
}

fn remote_search_target_managers(runtime: &AdapterRuntime, store: &SqliteStore) -> Vec<ManagerId> {
    let detected_managers: std::collections::HashSet<ManagerId> = store
        .list_detections()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(manager, detection)| detection.installed.then_some(manager))
        .collect();

    ManagerId::ALL
        .into_iter()
        .filter(|manager| {
            can_submit_remote_search(runtime, *manager)
                && (!detected_managers.is_empty() || runtime.has_manager(*manager))
                && (detected_managers.is_empty() || detected_managers.contains(manager))
        })
        .collect()
}

fn supports_individual_package_install(runtime: &AdapterRuntime, manager: ManagerId) -> bool {
    if !runtime.is_manager_enabled(manager)
        || !runtime.supports_capability(manager, Capability::Install)
    {
        return false;
    }

    manager_allows_individual_package_install(manager)
}

fn manager_allows_individual_package_install(manager: ManagerId) -> bool {
    matches!(
        manager,
        ManagerId::HomebrewFormula
            | ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
    )
}

fn supports_individual_package_uninstall(runtime: &AdapterRuntime, manager: ManagerId) -> bool {
    if !runtime.is_manager_enabled(manager)
        || !runtime.supports_capability(manager, Capability::Uninstall)
    {
        return false;
    }

    manager_allows_individual_package_uninstall(manager)
}

fn manager_allows_individual_package_uninstall(manager: ManagerId) -> bool {
    matches!(
        manager,
        ManagerId::HomebrewFormula
            | ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
    )
}

fn supports_individual_package_upgrade(runtime: &AdapterRuntime, manager: ManagerId) -> bool {
    if !runtime.is_manager_enabled(manager)
        || !runtime.supports_capability(manager, Capability::Upgrade)
    {
        return false;
    }

    matches!(
        manager,
        ManagerId::HomebrewFormula
            | ManagerId::Mise
            | ManagerId::Npm
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
            | ManagerId::Rustup
            | ManagerId::RubyGems
    )
}

fn encode_homebrew_upgrade_target(package_name: &str, cleanup_old_kegs: bool) -> String {
    if cleanup_old_kegs {
        format!("{package_name}@@helm.cleanup")
    } else {
        package_name.to_string()
    }
}

fn effective_homebrew_keg_policy(store: &SqliteStore, package_name: &str) -> HomebrewKegPolicy {
    let package_ref = PackageRef {
        manager: ManagerId::HomebrewFormula,
        name: package_name.to_string(),
    };

    if let Ok(Some(policy)) = store.package_keg_policy(&package_ref) {
        return policy;
    }

    store
        .homebrew_keg_policy()
        .unwrap_or(HomebrewKegPolicy::Keep)
}

fn normalize_nonempty(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn homebrew_probe_candidates(executable_path: Option<&std::path::Path>) -> Vec<std::ffi::OsString> {
    let mut candidates = Vec::new();
    let mut push_unique = |candidate: std::ffi::OsString| {
        if !candidates.iter().any(|existing| existing == &candidate) {
            candidates.push(candidate);
        }
    };

    if let Some(path) = executable_path {
        push_unique(path.as_os_str().to_os_string());
    }
    push_unique(std::ffi::OsString::from("/opt/homebrew/bin/brew"));
    push_unique(std::ffi::OsString::from("/usr/local/bin/brew"));
    push_unique(std::ffi::OsString::from("brew"));

    candidates
}

fn run_homebrew_probe_output(program: &std::ffi::OsStr, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .env(
            "PATH",
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        )
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut combined = String::new();
    if !stdout.trim().is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.trim().is_empty() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&stderr);
    }

    normalize_nonempty(Some(combined))
}

fn parse_homebrew_config_version(output: &str) -> Option<String> {
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.to_ascii_lowercase().starts_with("homebrew_version:")
            && let Some((_, value)) = line.split_once(':')
        {
            let parsed = helm_core::adapters::homebrew::parse_homebrew_version(value.trim())
                .or_else(|| normalize_nonempty(Some(value.trim().to_string())));
            if parsed.is_some() {
                return parsed;
            }
        }
    }
    None
}

#[derive(Default)]
struct UpgradeAllTargets {
    homebrew: Vec<String>,
    mise: Vec<String>,
    npm: Vec<String>,
    pnpm: Vec<String>,
    yarn: Vec<String>,
    cargo: Vec<String>,
    cargo_binstall: Vec<String>,
    pip: Vec<String>,
    pipx: Vec<String>,
    poetry: Vec<String>,
    rubygems: Vec<String>,
    bundler: Vec<String>,
    rustup: Vec<String>,
    softwareupdate_outdated: bool,
}

#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FfiUpgradePlanStep {
    step_id: String,
    order_index: u64,
    manager_id: String,
    authority: String,
    action: String,
    package_name: String,
    reason_label_key: String,
    reason_label_args: std::collections::HashMap<String, String>,
    status: String,
}

fn manager_authority_key(id: ManagerId) -> &'static str {
    match helm_core::registry::manager(id).map(|descriptor| descriptor.authority) {
        Some(ManagerAuthority::Authoritative) => "authoritative",
        Some(ManagerAuthority::Standard) => "standard",
        Some(ManagerAuthority::Guarded) => "guarded",
        Some(ManagerAuthority::DetectionOnly) => "detection_only",
        None => "standard",
    }
}

fn upgrade_reason_label_for(
    manager: ManagerId,
    package_name: &str,
    cleanup_old_kegs: bool,
) -> (&'static str, Vec<(&'static str, String)>) {
    match manager {
        ManagerId::HomebrewFormula => {
            if cleanup_old_kegs {
                (
                    "service.task.label.upgrade.homebrew_cleanup",
                    vec![("package", package_name.to_string())],
                )
            } else {
                (
                    "service.task.label.upgrade.homebrew",
                    vec![("package", package_name.to_string())],
                )
            }
        }
        ManagerId::Mise => (
            "service.task.label.upgrade.mise",
            vec![("package", package_name.to_string())],
        ),
        ManagerId::Rustup => (
            "service.task.label.upgrade.rustup_toolchain",
            vec![("toolchain", package_name.to_string())],
        ),
        ManagerId::SoftwareUpdate => ("service.task.label.upgrade.softwareupdate_all", vec![]),
        _ => (
            "service.task.label.upgrade.package",
            vec![
                ("package", package_name.to_string()),
                ("manager", manager_display_name(manager).to_string()),
            ],
        ),
    }
}

fn upgrade_plan_step_id(manager: ManagerId, package_name: &str) -> String {
    format!("{}:{}", manager.as_str(), package_name)
}

fn upgrade_task_label_for(
    manager: ManagerId,
    package_name: &str,
    cleanup_old_kegs: bool,
) -> (&'static str, Vec<(&'static str, String)>) {
    let (label_key, mut label_args) =
        upgrade_reason_label_for(manager, package_name, cleanup_old_kegs);
    label_args.push(("plan_step_id", upgrade_plan_step_id(manager, package_name)));
    (label_key, label_args)
}

fn push_upgrade_plan_step(
    steps: &mut Vec<FfiUpgradePlanStep>,
    manager: ManagerId,
    package_name: String,
    cleanup_old_kegs: bool,
    next_order_index: &mut u64,
) {
    let (reason_label_key, reason_label_args_vec) =
        upgrade_reason_label_for(manager, &package_name, cleanup_old_kegs);
    let reason_label_args = reason_label_args_vec
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect();

    steps.push(FfiUpgradePlanStep {
        step_id: upgrade_plan_step_id(manager, &package_name),
        order_index: *next_order_index,
        manager_id: manager.as_str().to_string(),
        authority: manager_authority_key(manager).to_string(),
        action: "upgrade".to_string(),
        package_name,
        reason_label_key: reason_label_key.to_string(),
        reason_label_args,
        status: "queued".to_string(),
    });
    *next_order_index += 1;
}

fn collect_upgrade_all_targets(
    outdated: &[OutdatedPackage],
    pinned_keys: &std::collections::HashSet<String>,
    include_pinned: bool,
) -> UpgradeAllTargets {
    let mut targets = UpgradeAllTargets::default();
    let mut seen_homebrew = std::collections::HashSet::new();
    let mut seen_mise = std::collections::HashSet::new();
    let mut seen_npm = std::collections::HashSet::new();
    let mut seen_pnpm = std::collections::HashSet::new();
    let mut seen_yarn = std::collections::HashSet::new();
    let mut seen_cargo = std::collections::HashSet::new();
    let mut seen_cargo_binstall = std::collections::HashSet::new();
    let mut seen_pip = std::collections::HashSet::new();
    let mut seen_pipx = std::collections::HashSet::new();
    let mut seen_poetry = std::collections::HashSet::new();
    let mut seen_rubygems = std::collections::HashSet::new();
    let mut seen_bundler = std::collections::HashSet::new();
    let mut seen_rustup = std::collections::HashSet::new();

    for package in outdated {
        let package_key = format!(
            "{}:{}",
            package.package.manager.as_str(),
            package.package.name.as_str()
        );
        if !include_pinned && (package.pinned || pinned_keys.contains(&package_key)) {
            continue;
        }

        match package.package.manager {
            ManagerId::HomebrewFormula => {
                if seen_homebrew.insert(package.package.name.clone()) {
                    targets.homebrew.push(package.package.name.clone());
                }
            }
            ManagerId::Mise => {
                if seen_mise.insert(package.package.name.clone()) {
                    targets.mise.push(package.package.name.clone());
                }
            }
            ManagerId::Npm => {
                if seen_npm.insert(package.package.name.clone()) {
                    targets.npm.push(package.package.name.clone());
                }
            }
            ManagerId::Pnpm => {
                if seen_pnpm.insert(package.package.name.clone()) {
                    targets.pnpm.push(package.package.name.clone());
                }
            }
            ManagerId::Yarn => {
                if seen_yarn.insert(package.package.name.clone()) {
                    targets.yarn.push(package.package.name.clone());
                }
            }
            ManagerId::Cargo => {
                if seen_cargo.insert(package.package.name.clone()) {
                    targets.cargo.push(package.package.name.clone());
                }
            }
            ManagerId::CargoBinstall => {
                if seen_cargo_binstall.insert(package.package.name.clone()) {
                    targets.cargo_binstall.push(package.package.name.clone());
                }
            }
            ManagerId::Pip => {
                if seen_pip.insert(package.package.name.clone()) {
                    targets.pip.push(package.package.name.clone());
                }
            }
            ManagerId::Pipx => {
                if seen_pipx.insert(package.package.name.clone()) {
                    targets.pipx.push(package.package.name.clone());
                }
            }
            ManagerId::Poetry => {
                if seen_poetry.insert(package.package.name.clone()) {
                    targets.poetry.push(package.package.name.clone());
                }
            }
            ManagerId::RubyGems => {
                if seen_rubygems.insert(package.package.name.clone()) {
                    targets.rubygems.push(package.package.name.clone());
                }
            }
            ManagerId::Bundler => {
                if seen_bundler.insert(package.package.name.clone()) {
                    targets.bundler.push(package.package.name.clone());
                }
            }
            ManagerId::Rustup => {
                if seen_rustup.insert(package.package.name.clone()) {
                    targets.rustup.push(package.package.name.clone());
                }
            }
            ManagerId::SoftwareUpdate => targets.softwareupdate_outdated = true,
            _ => {}
        }
    }

    targets
}

fn probe_homebrew_version(executable_path: Option<&std::path::Path>) -> Option<String> {
    for candidate in homebrew_probe_candidates(executable_path) {
        if let Some(version_output) =
            run_homebrew_probe_output(candidate.as_os_str(), &["--version"])
            && let Some(version) = normalize_nonempty(
                helm_core::adapters::homebrew::parse_homebrew_version(&version_output),
            )
        {
            return Some(version);
        }

        if let Some(config_output) = run_homebrew_probe_output(candidate.as_os_str(), &["config"])
            && let Some(version) = parse_homebrew_config_version(&config_output).or_else(|| {
                normalize_nonempty(helm_core::adapters::homebrew::parse_homebrew_version(
                    &config_output,
                ))
            })
        {
            return Some(version);
        }
    }

    None
}

fn homebrew_dependency_available(store: &SqliteStore) -> bool {
    let mut detected_path: Option<std::path::PathBuf> = None;
    if let Ok(detections) = store.list_detections()
        && let Some((_, detection)) = detections
            .iter()
            .find(|(manager, _)| *manager == ManagerId::HomebrewFormula)
    {
        if detection.installed {
            return true;
        }
        detected_path = detection.executable_path.clone();
    }

    probe_homebrew_version(detected_path.as_deref()).is_some()
}

fn manager_selected_install_method(store: &SqliteStore, manager: ManagerId) -> Option<String> {
    let preferences = store.list_manager_preferences().ok()?;
    let preference = preferences
        .into_iter()
        .find(|pref| pref.manager == manager)?;
    normalize_install_method(manager, preference.selected_install_method)
}

fn env_flag_enabled(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

fn parse_managed_automation_policy_mode(raw: &str) -> Option<ManagedAutomationPolicyMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "automatic" | "auto" | "none" | "off" => Some(ManagedAutomationPolicyMode::Automatic),
        "needs_confirmation" | "confirm" | "confirmation" => {
            Some(ManagedAutomationPolicyMode::NeedsConfirmation)
        }
        "read_only" | "readonly" => Some(ManagedAutomationPolicyMode::ReadOnly),
        _ => None,
    }
}

fn manager_automation_policy_context() -> ManagerAutomationPolicyContext {
    *MANAGER_AUTOMATION_POLICY_CONTEXT.get_or_init(|| {
        let explicit_mode = std::env::var(MANAGED_AUTOMATION_POLICY_ENV)
            .ok()
            .and_then(|value| parse_managed_automation_policy_mode(value.as_str()));
        let mode = explicit_mode.unwrap_or_else(|| {
            if env_flag_enabled(MANAGED_INSTALL_METHOD_POLICY_ENV) {
                ManagedAutomationPolicyMode::NeedsConfirmation
            } else {
                ManagedAutomationPolicyMode::Automatic
            }
        });
        ManagerAutomationPolicyContext { mode }
    })
}

fn apply_manager_automation_policy(instance: &ManagerInstallInstance) -> ManagerInstallInstance {
    apply_managed_automation_policy(instance, manager_automation_policy_context().mode)
}

fn manager_install_instances_for(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<Vec<ManagerInstallInstance>, &'static str> {
    let mut instances = store
        .list_install_instances(Some(manager))
        .map_err(|_| SERVICE_ERROR_STORAGE_FAILURE)?;
    instances.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    Ok(instances)
}

fn active_manager_install_instance(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<Option<ManagerInstallInstance>, &'static str> {
    let instances = match manager_install_instances_for(store, manager) {
        Ok(instances) => instances,
        Err(error) => {
            eprintln!(
                "manager uninstall preview: failed to list install instances for '{}': {error}",
                manager.as_str()
            );
            return Err(SERVICE_ERROR_STORAGE_FAILURE);
        }
    };
    Ok(instances
        .into_iter()
        .next()
        .map(|instance| apply_manager_automation_policy(&instance)))
}

fn build_manager_uninstall_request_legacy(
    _store: &SqliteStore,
    manager: ManagerId,
) -> Result<ManagerUninstallLegacyRequest, &'static str> {
    let plan = match manager {
        ManagerId::Mise => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "mise".to_string(),
                },
            }),
            "service.task.label.uninstall.homebrew_formula",
            vec![("package", "mise".to_string())],
        ),
        ManagerId::Mas => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "mas".to_string(),
                },
            }),
            "service.task.label.uninstall.homebrew_formula",
            vec![("package", "mas".to_string())],
        ),
        ManagerId::Rustup => (
            ManagerId::Rustup,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                },
            }),
            "service.task.label.uninstall.rustup_self",
            Vec::new(),
        ),
        _ => return Err(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
    };
    Ok(plan)
}

#[cfg(test)]
fn build_manager_uninstall_plan(
    store: &SqliteStore,
    manager: ManagerId,
    allow_unknown_provenance: bool,
    preview_only: bool,
) -> Result<ManagerUninstallPlan, &'static str> {
    build_manager_uninstall_plan_with_options(
        store,
        manager,
        allow_unknown_provenance,
        preview_only,
        &helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
    )
}

fn build_manager_uninstall_plan_with_options(
    store: &SqliteStore,
    manager: ManagerId,
    allow_unknown_provenance: bool,
    preview_only: bool,
    uninstall_options: &helm_core::manager_lifecycle::ManagerUninstallOptions,
) -> Result<ManagerUninstallPlan, &'static str> {
    let active_instance = active_manager_install_instance(store, manager)?;

    match helm_core::manager_lifecycle::plan_manager_uninstall_route_with_options(
        manager,
        active_instance.as_ref(),
        allow_unknown_provenance,
        preview_only,
        uninstall_options,
    ) {
        Ok(route) => build_provenance_manager_uninstall_plan(
            store,
            manager,
            active_instance,
            preview_only,
            route,
        ),
        Err(helm_core::manager_lifecycle::ManagerUninstallRouteError::UnsupportedManager) => {
            // Remaining managers stay intentionally gated here until their uninstall strategy
            // can be proven safe and deterministic for provenance-first routing.
            let (target_manager, request, label_key, label_args) =
                build_manager_uninstall_request_legacy(store, manager)?;
            let strategy = active_instance
                .as_ref()
                .map(|instance| instance.uninstall_strategy)
                .unwrap_or(StrategyKind::InteractivePrompt);
            let preview = build_manager_uninstall_preview(
                store,
                ManagerUninstallPreviewContext {
                    requested_manager: manager,
                    target_manager,
                    request: &request,
                    strategy,
                    active_instance: active_instance.as_ref(),
                    unknown_override_required: false,
                    used_unknown_override: false,
                    legacy_fallback_used: true,
                },
                DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
            );

            if preview.read_only_blocked && !preview_only {
                return Err(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
            }
            if target_manager == ManagerId::HomebrewFormula
                && !homebrew_dependency_available(store)
                && !preview_only
            {
                return Err("service.error.homebrew_required");
            }

            Ok(ManagerUninstallPlan {
                target_manager,
                request,
                label_key,
                label_args,
                preview,
            })
        }
        Err(error) => Err(manager_uninstall_route_error_key(error)),
    }
}

fn build_provenance_manager_uninstall_plan(
    store: &SqliteStore,
    manager: ManagerId,
    active_instance: Option<ManagerInstallInstance>,
    preview_only: bool,
    route: helm_core::manager_lifecycle::ManagerUninstallRoutePlan,
) -> Result<ManagerUninstallPlan, &'static str> {
    let preview = build_manager_uninstall_preview(
        store,
        ManagerUninstallPreviewContext {
            requested_manager: manager,
            target_manager: route.target_manager,
            request: &route.request,
            strategy: route.strategy,
            active_instance: active_instance.as_ref(),
            unknown_override_required: route.unknown_override_required,
            used_unknown_override: route.used_unknown_override,
            legacy_fallback_used: false,
        },
        DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
    );

    if preview.read_only_blocked && !preview_only {
        return Err(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }
    if preview.unknown_override_required && !route.used_unknown_override && !preview_only {
        return Err(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }
    if route.target_manager == ManagerId::HomebrewFormula
        && !homebrew_dependency_available(store)
        && !preview_only
    {
        return Err("service.error.homebrew_required");
    }

    let (label_key, label_args) = manager_uninstall_label_for_route(
        manager,
        route.target_manager,
        &route.request,
        route.strategy,
    );

    Ok(ManagerUninstallPlan {
        target_manager: route.target_manager,
        request: route.request,
        label_key,
        label_args,
        preview,
    })
}

fn manager_uninstall_route_error_key(
    error: helm_core::manager_lifecycle::ManagerUninstallRouteError,
) -> &'static str {
    match error {
        helm_core::manager_lifecycle::ManagerUninstallRouteError::InvalidOptions => {
            SERVICE_ERROR_INVALID_INPUT
        }
        _ => SERVICE_ERROR_UNSUPPORTED_CAPABILITY,
    }
}

fn manager_uninstall_label_for_route(
    requested_manager: ManagerId,
    target_manager: ManagerId,
    request: &AdapterRequest,
    strategy: StrategyKind,
) -> (&'static str, Vec<(&'static str, String)>) {
    if target_manager == ManagerId::Rustup
        && matches!(strategy, StrategyKind::RustupSelf | StrategyKind::ReadOnly)
    {
        return ("service.task.label.uninstall.rustup_self", Vec::new());
    }

    if target_manager == ManagerId::HomebrewFormula {
        let package_name = match request {
            AdapterRequest::Uninstall(uninstall) => {
                if let Some(parsed) =
                    helm_core::manager_lifecycle::parse_homebrew_manager_uninstall_package_name(
                        uninstall.package.name.as_str(),
                    )
                {
                    parsed.formula_name
                } else {
                    uninstall.package.name.clone()
                }
            }
            _ => requested_manager.as_str().to_string(),
        };
        return (
            "service.task.label.uninstall.homebrew_formula",
            vec![("package", package_name)],
        );
    }

    if target_manager == ManagerId::MacPorts {
        let package_name = match request {
            AdapterRequest::Uninstall(uninstall) => uninstall.package.name.clone(),
            _ => requested_manager.as_str().to_string(),
        };
        return (
            "service.task.label.uninstall.package",
            vec![
                ("package", package_name),
                ("manager", ManagerId::MacPorts.as_str().to_string()),
            ],
        );
    }

    if target_manager == ManagerId::Mise {
        return (
            "service.task.label.uninstall.package",
            vec![
                ("package", "mise".to_string()),
                ("manager", ManagerId::Mise.as_str().to_string()),
            ],
        );
    }

    (
        "service.task.label.uninstall.homebrew_formula",
        vec![("package", requested_manager.as_str().to_string())],
    )
}

#[cfg(test)]
fn resolve_rustup_uninstall_strategy(
    active_instance: Option<&ManagerInstallInstance>,
    allow_unknown_provenance: bool,
    preview_only: bool,
) -> Result<RustupUninstallResolution, &'static str> {
    helm_core::manager_lifecycle::resolve_rustup_uninstall_strategy(
        active_instance,
        allow_unknown_provenance,
        preview_only,
    )
    .map(|resolution| RustupUninstallResolution {
        strategy: resolution.strategy,
        unknown_override_required: resolution.unknown_override_required,
        used_unknown_override: resolution.used_unknown_override,
    })
    .map_err(|_| SERVICE_ERROR_UNSUPPORTED_CAPABILITY)
}

#[cfg(test)]
fn resolve_rustup_update_strategy(
    active_instance: Option<&ManagerInstallInstance>,
) -> Result<StrategyKind, &'static str> {
    helm_core::manager_lifecycle::resolve_rustup_update_strategy(active_instance)
        .map_err(|_| SERVICE_ERROR_UNSUPPORTED_CAPABILITY)
}

#[cfg(test)]
fn resolve_homebrew_manager_update_strategy(
    active_instance: Option<&ManagerInstallInstance>,
) -> Result<StrategyKind, &'static str> {
    helm_core::manager_lifecycle::resolve_homebrew_manager_update_strategy(active_instance)
        .map_err(|_| SERVICE_ERROR_UNSUPPORTED_CAPABILITY)
}

#[cfg(any(test, target_os = "macos"))]
fn parse_legacy_file_coordinator_ipc_flag(value: Option<&str>) -> bool {
    matches!(
        value.map(|raw| raw.trim().to_ascii_lowercase()),
        Some(normalized)
            if matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
    )
}

#[cfg(any(test, target_os = "macos"))]
fn legacy_file_coordinator_ipc_opt_in() -> bool {
    parse_legacy_file_coordinator_ipc_flag(
        std::env::var(LEGACY_FILE_COORDINATOR_IPC_ENV)
            .ok()
            .as_deref(),
    )
}

fn coordinator_bridge_mode() -> CoordinatorBridgeMode {
    #[cfg(target_os = "macos")]
    {
        if legacy_file_coordinator_ipc_opt_in() {
            CoordinatorBridgeMode::LegacyFileIpc
        } else {
            CoordinatorBridgeMode::LocalXpcPreferred
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        CoordinatorBridgeMode::LegacyFileIpc
    }
}

fn should_use_external_file_coordinator_with_health(
    mode: CoordinatorBridgeMode,
    external_ready: bool,
) -> bool {
    mode == CoordinatorBridgeMode::LegacyFileIpc && external_ready
}

fn should_use_external_file_coordinator(mode: CoordinatorBridgeMode, state_dir: &Path) -> bool {
    should_use_external_file_coordinator_with_health(mode, coordinator_ready(state_dir))
}

fn initialize_coordinator_bridge(
    store: Arc<SqliteStore>,
    runtime: Arc<AdapterRuntime>,
    rt_handle: tokio::runtime::Handle,
) {
    let state_dir = coordinator_socket_path_for_store(store.as_ref());
    let bridge_mode = coordinator_bridge_mode();

    if should_use_external_file_coordinator(bridge_mode, state_dir.as_path()) {
        *lock_or_recover(&COORDINATOR_BRIDGE, "coordinator_bridge") =
            CoordinatorBridge::External(state_dir);
        return;
    }

    if bridge_mode == CoordinatorBridgeMode::LegacyFileIpc {
        if COORDINATOR_SERVER_STARTED
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            start_local_coordinator_server(
                state_dir.clone(),
                store.clone(),
                runtime.clone(),
                rt_handle.clone(),
            );
        }
    } else if AUTO_CHECK_TICKER_STARTED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
    {
        start_local_auto_check_ticker(store.clone());
    }

    *lock_or_recover(&COORDINATOR_BRIDGE, "coordinator_bridge") = CoordinatorBridge::Local;
}

fn external_coordinator_state_dir() -> Option<PathBuf> {
    let current = lock_or_recover(&COORDINATOR_BRIDGE, "coordinator_bridge").clone();
    match current {
        CoordinatorBridge::External(state_dir) => Some(state_dir),
        CoordinatorBridge::Local | CoordinatorBridge::Disabled => None,
    }
}

fn coordinator_submit_external(
    manager: ManagerId,
    request: CoordinatorSubmitRequest,
    wait: bool,
) -> Result<CoordinatorResponse, String> {
    let state_dir = external_coordinator_state_dir()
        .ok_or_else(|| "external coordinator transport is unavailable".to_string())?;
    let response = send_coordinator_request_once(
        state_dir.as_path(),
        &CoordinatorRequest::Submit {
            manager_id: manager.as_str().to_string(),
            request,
            wait,
        },
    )?;
    if response.ok {
        Ok(response)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "coordinator submit request failed".to_string()))
    }
}

fn coordinator_cancel_external(task_id: u64) -> Result<(), String> {
    let state_dir = external_coordinator_state_dir()
        .ok_or_else(|| "external coordinator transport is unavailable".to_string())?;
    let response = send_coordinator_request_once(
        state_dir.as_path(),
        &CoordinatorRequest::Cancel { task_id },
    )?;
    if response.ok {
        Ok(())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| format!("failed to cancel task '{}'", task_id)))
    }
}

fn coordinator_start_workflow_external(
    workflow: CoordinatorWorkflowRequest,
) -> Result<CoordinatorResponse, String> {
    let state_dir = external_coordinator_state_dir()
        .ok_or_else(|| "external coordinator transport is unavailable".to_string())?;
    let response = send_coordinator_request_once(
        state_dir.as_path(),
        &CoordinatorRequest::StartWorkflow { workflow },
    )?;
    if response.ok {
        Ok(response)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "coordinator workflow request failed".to_string()))
    }
}

fn auto_check_install_marker_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("HELM_INSTALL_MARKER_PATH")
        && !explicit.trim().is_empty()
    {
        return Some(PathBuf::from(explicit));
    }

    let home = std::env::var("HOME").ok()?;
    if home.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(home).join(DEFAULT_INSTALL_MARKER_RELATIVE_PATH))
}

fn auto_check_marker_allows_cli_endpoint() -> bool {
    let Some(marker_path) = auto_check_install_marker_path() else {
        return false;
    };
    let Ok(raw) = std::fs::read_to_string(marker_path) else {
        return false;
    };
    let Ok(marker) = serde_json::from_str::<AutoCheckInstallMarker>(&raw) else {
        return false;
    };
    marker.channel.trim() == "direct-script" && marker.update_policy.trim() == "self"
}

fn auto_check_allow_insecure_urls() -> bool {
    matches!(
        std::env::var(AUTO_CHECK_ALLOW_INSECURE_ENV)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "1" | "true" | "yes" | "on")
    )
}

fn auto_check_parse_url_scheme_host(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    if trimmed.starts_with("file://") {
        return Some(("file".to_string(), String::new()));
    }
    let (scheme, remainder) = trimmed.split_once("://")?;
    if scheme.is_empty() || remainder.is_empty() {
        return None;
    }
    let authority = remainder.split('/').next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = authority.split(':').next()?.trim();
    if host.is_empty() {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), host.to_ascii_lowercase()))
}

fn auto_check_endpoint_allowed(endpoint: &str) -> bool {
    let Some((scheme, host)) = auto_check_parse_url_scheme_host(endpoint) else {
        return false;
    };
    if scheme == "file" {
        return auto_check_allow_insecure_urls();
    }
    if scheme != "https" {
        return false;
    }
    AUTO_CHECK_ALLOWED_HOSTS
        .iter()
        .any(|candidate| host.eq_ignore_ascii_case(candidate))
}

fn auto_check_http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(AUTO_CHECK_HTTP_CONNECT_TIMEOUT_SECS))
        .timeout_read(Duration::from_secs(AUTO_CHECK_HTTP_READ_TIMEOUT_SECS))
        .timeout_write(Duration::from_secs(AUTO_CHECK_HTTP_WRITE_TIMEOUT_SECS))
        .build()
}

fn run_due_auto_check_tick(store: &SqliteStore) {
    let enabled = match store.auto_check_for_updates() {
        Ok(enabled) => enabled,
        Err(error) => {
            eprintln!("coordinator auto-check tick failed to read enabled setting: {error}");
            return;
        }
    };
    if !enabled {
        return;
    }

    let frequency_minutes = match store.auto_check_frequency_minutes() {
        Ok(value) => value.max(1),
        Err(error) => {
            eprintln!("coordinator auto-check tick failed to read frequency setting: {error}");
            return;
        }
    };
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let last_checked = match store.auto_check_last_checked_unix() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("coordinator auto-check tick failed to read last-run timestamp: {error}");
            return;
        }
    };
    if let Some(last_checked) = last_checked {
        let elapsed = now_unix.saturating_sub(last_checked);
        let required = (frequency_minutes as i64).saturating_mul(60);
        if elapsed < required {
            return;
        }
    }

    if !auto_check_marker_allows_cli_endpoint() {
        eprintln!(
            "coordinator auto-check skipped: install provenance does not allow direct CLI checks"
        );
        return;
    }

    let endpoint = std::env::var("HELM_CLI_UPDATE_ENDPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLI_UPDATE_ENDPOINT.to_string());
    if !auto_check_endpoint_allowed(endpoint.as_str()) {
        eprintln!(
            "coordinator auto-check rejected endpoint URL '{}': not allowlisted",
            endpoint
        );
        return;
    }

    match auto_check_http_agent().get(endpoint.as_str()).call() {
        Ok(response) => {
            let _ = response.into_string();
        }
        Err(error) => {
            eprintln!("coordinator auto-check request failed: {error}");
        }
    }

    if let Err(error) = store.set_auto_check_last_checked_unix(now_unix) {
        eprintln!("coordinator auto-check failed to persist last-run timestamp: {error}");
    }
}

fn start_local_auto_check_ticker(store: Arc<SqliteStore>) {
    thread::spawn(move || {
        let mut next_auto_check_tick = Instant::now();
        loop {
            if Instant::now() >= next_auto_check_tick {
                run_due_auto_check_tick(store.as_ref());
                next_auto_check_tick = Instant::now() + Duration::from_secs(AUTO_CHECK_TICK_SECS);
            }
            thread::sleep(Duration::from_millis(COORDINATOR_POLL_SLEEP_MS));
        }
    });
}

fn start_local_coordinator_server(
    state_dir: PathBuf,
    store: Arc<SqliteStore>,
    runtime: Arc<AdapterRuntime>,
    rt_handle: tokio::runtime::Handle,
) {
    thread::spawn(move || {
        if reset_coordinator_state_dir(state_dir.as_path()).is_err() {
            return;
        }

        if write_json_file(
            coordinator_ready_file(state_dir.as_path()).as_path(),
            &serde_json::json!({
                "pid": std::process::id(),
                "started_at": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|duration| duration.as_secs() as i64)
                    .unwrap_or(0)
            }),
        )
        .is_err()
        {
            return;
        }

        let requests_dir = coordinator_requests_dir(state_dir.as_path());
        let mut next_auto_check_tick = Instant::now();
        loop {
            if Instant::now() >= next_auto_check_tick {
                run_due_auto_check_tick(store.as_ref());
                next_auto_check_tick = Instant::now() + Duration::from_secs(AUTO_CHECK_TICK_SECS);
            }

            let mut entries: Vec<_> = match std::fs::read_dir(requests_dir.as_path()) {
                Ok(entries) => entries.flatten().collect(),
                Err(_) => {
                    thread::sleep(Duration::from_millis(COORDINATOR_POLL_SLEEP_MS));
                    continue;
                }
            };
            entries.sort_by_key(|entry| entry.file_name());

            if entries.is_empty() {
                thread::sleep(Duration::from_millis(COORDINATOR_POLL_SLEEP_MS));
                continue;
            }

            for entry in entries {
                let request_path = entry.path();
                if !request_path.is_file() {
                    continue;
                }

                let request_id = request_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_string())
                    .unwrap_or_else(next_coordinator_request_id);
                let response_path = coordinator_response_file(state_dir.as_path(), &request_id);

                let response = match read_json_file::<CoordinatorRequest>(request_path.as_path()) {
                    Ok(request) => handle_local_coordinator_request(
                        store.as_ref(),
                        runtime.clone(),
                        &rt_handle,
                        request,
                    ),
                    Err(error) => CoordinatorResponse {
                        ok: false,
                        task_id: None,
                        job_id: None,
                        payload: None,
                        error: Some(error),
                    },
                };

                let _ = write_json_file(response_path.as_path(), &response);
                let _ = std::fs::remove_file(request_path.as_path());
            }
        }
    });
}

fn reset_coordinator_state_dir(state_dir: &Path) -> Result<(), String> {
    if state_dir.exists() {
        std::fs::remove_dir_all(state_dir).map_err(|error| {
            format!(
                "failed to reset coordinator state directory '{}': {error}",
                state_dir.display()
            )
        })?;
    }
    ensure_private_directory(state_dir)?;
    ensure_private_directory(coordinator_requests_dir(state_dir).as_path())?;
    ensure_private_directory(coordinator_responses_dir(state_dir).as_path())?;
    Ok(())
}

fn handle_local_coordinator_request(
    store: &SqliteStore,
    runtime: Arc<AdapterRuntime>,
    rt_handle: &tokio::runtime::Handle,
    request: CoordinatorRequest,
) -> CoordinatorResponse {
    match request {
        CoordinatorRequest::Ping => CoordinatorResponse {
            ok: true,
            task_id: None,
            job_id: None,
            payload: None,
            error: None,
        },
        CoordinatorRequest::Cancel { task_id } => {
            match rt_handle.block_on(
                runtime
                    .as_ref()
                    .cancel(TaskId(task_id), CancellationMode::Immediate),
            ) {
                Ok(()) => CoordinatorResponse {
                    ok: true,
                    task_id: Some(task_id),
                    job_id: None,
                    payload: None,
                    error: None,
                },
                Err(error) => CoordinatorResponse {
                    ok: false,
                    task_id: Some(task_id),
                    job_id: None,
                    payload: None,
                    error: Some(format_core_error(error)),
                },
            }
        }
        CoordinatorRequest::Submit {
            manager_id,
            request,
            wait,
        } => {
            let manager = match manager_id.parse::<ManagerId>() {
                Ok(manager) => manager,
                Err(_) => {
                    return CoordinatorResponse {
                        ok: false,
                        task_id: None,
                        job_id: None,
                        payload: None,
                        error: Some(format!("unknown manager id '{}'", manager_id)),
                    };
                }
            };

            let detection_map: std::collections::HashMap<_, _> = store
                .list_detections()
                .unwrap_or_default()
                .into_iter()
                .collect();
            let pref_map: std::collections::HashMap<_, _> = store
                .list_manager_preferences()
                .unwrap_or_default()
                .into_iter()
                .map(|pref| (pref.manager, pref))
                .collect();
            sync_manager_executable_overrides(&detection_map, &pref_map);

            let adapter_request = coordinator_submit_to_adapter(manager, request);
            let task_id =
                match rt_handle.block_on(runtime.as_ref().submit(manager, adapter_request)) {
                    Ok(task_id) => task_id,
                    Err(error) => {
                        return CoordinatorResponse {
                            ok: false,
                            task_id: None,
                            job_id: None,
                            payload: None,
                            error: Some(format_core_error(error)),
                        };
                    }
                };

            if !wait {
                return CoordinatorResponse {
                    ok: true,
                    task_id: Some(task_id.0),
                    job_id: None,
                    payload: None,
                    error: None,
                };
            }

            match rt_handle.block_on(runtime.as_ref().wait_for_terminal(task_id, None)) {
                Ok(snapshot) => match snapshot.terminal_state {
                    Some(AdapterTaskTerminalState::Succeeded(response)) => CoordinatorResponse {
                        ok: true,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: Some(adapter_response_to_coordinator_payload(response)),
                        error: None,
                    },
                    Some(AdapterTaskTerminalState::Failed(error)) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        error: Some(format_core_error(error)),
                    },
                    Some(AdapterTaskTerminalState::Cancelled(Some(error))) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        error: Some(format_core_error(error)),
                    },
                    Some(AdapterTaskTerminalState::Cancelled(None)) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        error: Some(format!("task {} was cancelled", task_id.0)),
                    },
                    None => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        error: Some(format!(
                            "task {} reached terminal state without outcome payload",
                            task_id.0
                        )),
                    },
                },
                Err(error) => CoordinatorResponse {
                    ok: false,
                    task_id: Some(task_id.0),
                    job_id: None,
                    payload: None,
                    error: Some(format_core_error(error)),
                },
            }
        }
        CoordinatorRequest::StartWorkflow { workflow } => {
            let job_id = next_coordinator_request_id();
            let workflow_runtime = runtime.clone();
            let store = Arc::new(SqliteStore::new(store.database_path().to_path_buf()));
            if let Err(error) = store.migrate_to_latest() {
                return CoordinatorResponse {
                    ok: false,
                    task_id: None,
                    job_id: Some(job_id),
                    payload: None,
                    error: Some(format!(
                        "failed to initialize coordinator workflow store: {error}"
                    )),
                };
            }
            let rt_handle = rt_handle.clone();
            thread::spawn(move || {
                let _ = run_coordinator_workflow(
                    workflow_runtime.as_ref(),
                    store.as_ref(),
                    &rt_handle,
                    workflow,
                );
            });
            CoordinatorResponse {
                ok: true,
                task_id: None,
                job_id: Some(job_id),
                payload: None,
                error: None,
            }
        }
    }
}

fn run_coordinator_workflow(
    runtime: &AdapterRuntime,
    store: &SqliteStore,
    rt_handle: &tokio::runtime::Handle,
    workflow: CoordinatorWorkflowRequest,
) -> Result<(), String> {
    match workflow {
        CoordinatorWorkflowRequest::RefreshAll => {
            let results = rt_handle.block_on(runtime.refresh_all_ordered());
            let failures = results
                .into_iter()
                .filter(|(_, result)| result.is_err())
                .count();
            if failures > 0 {
                return Err(format!("{failures} manager refresh operations failed"));
            }
            Ok(())
        }
        CoordinatorWorkflowRequest::RefreshManager { manager_id } => {
            let manager = manager_id
                .parse::<ManagerId>()
                .map_err(|_| format!("unknown manager id '{}'", manager_id))?;
            refresh_single_manager(runtime, rt_handle, manager)
        }
        CoordinatorWorkflowRequest::DetectAll => {
            let results = rt_handle.block_on(runtime.detect_all_ordered());
            let failures = results
                .into_iter()
                .filter(|(_, result)| result.is_err())
                .count();
            if failures > 0 {
                return Err(format!("{failures} manager detection operations failed"));
            }
            Ok(())
        }
        CoordinatorWorkflowRequest::UpdatesRun {
            include_pinned,
            allow_os_updates,
        } => run_updates_workflow(runtime, store, rt_handle, include_pinned, allow_os_updates),
    }
}

fn refresh_single_manager(
    runtime: &AdapterRuntime,
    rt_handle: &tokio::runtime::Handle,
    manager: ManagerId,
) -> Result<(), String> {
    if !runtime.has_manager(manager) {
        return Err(format!(
            "manager '{}' is not registered in runtime",
            manager.as_str()
        ));
    }
    if !runtime.is_manager_enabled(manager) {
        return Ok(());
    }

    let mut detected_installed = None;
    let mut ran_any_action = false;

    if runtime.supports_capability(manager, Capability::Detect) {
        let response = submit_request_wait(
            runtime,
            rt_handle,
            manager,
            AdapterRequest::Detect(helm_core::adapters::DetectRequest),
        )?;
        match response {
            helm_core::adapters::AdapterResponse::Detection(info) => {
                detected_installed = Some(info.installed);
                if !info.installed {
                    return Ok(());
                }
            }
            _ => {
                return Err(format!(
                    "manager '{}' detect action returned unexpected payload",
                    manager.as_str()
                ));
            }
        }
        ran_any_action = true;
    }

    if detected_installed != Some(false)
        && runtime.supports_capability(manager, Capability::ListInstalled)
    {
        let _ = submit_request_wait(
            runtime,
            rt_handle,
            manager,
            AdapterRequest::ListInstalled(helm_core::adapters::ListInstalledRequest),
        )?;
        ran_any_action = true;
    }

    if detected_installed != Some(false)
        && runtime.supports_capability(manager, Capability::ListOutdated)
    {
        let _ = submit_request_wait(
            runtime,
            rt_handle,
            manager,
            AdapterRequest::ListOutdated(helm_core::adapters::ListOutdatedRequest),
        )?;
        ran_any_action = true;
    }

    if !ran_any_action {
        return Err(format!(
            "manager '{}' has no detection or refresh capabilities",
            manager.as_str()
        ));
    }
    Ok(())
}

fn run_updates_workflow(
    runtime: &AdapterRuntime,
    store: &SqliteStore,
    rt_handle: &tokio::runtime::Handle,
    include_pinned: bool,
    allow_os_updates: bool,
) -> Result<(), String> {
    let outdated = store
        .list_outdated()
        .map_err(|error| format!("failed to list outdated packages: {error}"))?;
    let pinned_keys: std::collections::HashSet<String> = store
        .list_pins()
        .map(|pins| {
            pins.into_iter()
                .map(|pin| format!("{}:{}", pin.package.manager.as_str(), pin.package.name))
                .collect()
        })
        .unwrap_or_default();
    let targets = collect_upgrade_all_targets(&outdated, &pinned_keys, include_pinned);

    if runtime.is_manager_enabled(ManagerId::HomebrewFormula) {
        for package_name in targets.homebrew {
            let policy = effective_homebrew_keg_policy(store, &package_name);
            let cleanup_old_kegs = policy == HomebrewKegPolicy::Cleanup;
            let target_name = encode_homebrew_upgrade_target(&package_name, cleanup_old_kegs);
            let request = AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: target_name,
                }),
            });
            let _ = submit_request_wait(runtime, rt_handle, ManagerId::HomebrewFormula, request)?;
        }
    }

    for (manager, packages) in [
        (ManagerId::Mise, targets.mise),
        (ManagerId::Npm, targets.npm),
        (ManagerId::Pnpm, targets.pnpm),
        (ManagerId::Yarn, targets.yarn),
        (ManagerId::Cargo, targets.cargo),
        (ManagerId::CargoBinstall, targets.cargo_binstall),
        (ManagerId::Pip, targets.pip),
        (ManagerId::Pipx, targets.pipx),
        (ManagerId::Poetry, targets.poetry),
        (ManagerId::RubyGems, targets.rubygems),
        (ManagerId::Bundler, targets.bundler),
        (ManagerId::Rustup, targets.rustup),
    ] {
        if !runtime.is_manager_enabled(manager) {
            continue;
        }
        for package_name in packages {
            let request = AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager,
                    name: package_name,
                }),
            });
            let _ = submit_request_wait(runtime, rt_handle, manager, request)?;
        }
    }

    if allow_os_updates
        && targets.softwareupdate_outdated
        && runtime.is_manager_enabled(ManagerId::SoftwareUpdate)
        && !runtime.is_safe_mode()
    {
        let request = AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(PackageRef {
                manager: ManagerId::SoftwareUpdate,
                name: "__confirm_os_updates__".to_string(),
            }),
        });
        let _ = submit_request_wait(runtime, rt_handle, ManagerId::SoftwareUpdate, request)?;
    }

    Ok(())
}

fn submit_request_wait(
    runtime: &AdapterRuntime,
    rt_handle: &tokio::runtime::Handle,
    manager: ManagerId,
    request: AdapterRequest,
) -> Result<helm_core::adapters::AdapterResponse, String> {
    let task_id = rt_handle
        .block_on(runtime.submit(manager, request))
        .map_err(format_core_error)?;
    let snapshot = rt_handle
        .block_on(runtime.wait_for_terminal(task_id, None))
        .map_err(format_core_error)?;
    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(response)) => Ok(response),
        Some(AdapterTaskTerminalState::Failed(error)) => Err(format_core_error(error)),
        Some(AdapterTaskTerminalState::Cancelled(Some(error))) => Err(format_core_error(error)),
        Some(AdapterTaskTerminalState::Cancelled(None)) => {
            Err(format!("task {} was cancelled", task_id.0))
        }
        None => Err(format!(
            "task {} reached terminal state without outcome payload",
            task_id.0
        )),
    }
}

fn format_core_error(error: helm_core::models::CoreError) -> String {
    let manager = error
        .manager
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!(
        "{} (manager={}, task={:?}, action={:?}, kind={:?})",
        error.message, manager, error.task, error.action, error.kind
    )
}

fn coordinator_submit_to_adapter(
    manager: ManagerId,
    request: CoordinatorSubmitRequest,
) -> AdapterRequest {
    match request {
        CoordinatorSubmitRequest::Detect => {
            AdapterRequest::Detect(helm_core::adapters::DetectRequest)
        }
        CoordinatorSubmitRequest::Install {
            package_name,
            version,
        } => AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager,
                name: package_name,
            },
            version,
        }),
        CoordinatorSubmitRequest::Uninstall { package_name } => {
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager,
                    name: package_name,
                },
            })
        }
        CoordinatorSubmitRequest::Upgrade { package_name } => {
            AdapterRequest::Upgrade(UpgradeRequest {
                package: package_name.map(|name| PackageRef { manager, name }),
            })
        }
        CoordinatorSubmitRequest::Pin {
            package_name,
            version,
        } => AdapterRequest::Pin(PinRequest {
            package: PackageRef {
                manager,
                name: package_name,
            },
            version,
        }),
        CoordinatorSubmitRequest::Unpin { package_name } => AdapterRequest::Unpin(UnpinRequest {
            package: PackageRef {
                manager,
                name: package_name,
            },
        }),
    }
}

fn adapter_request_to_coordinator_submit(
    request: AdapterRequest,
) -> Result<CoordinatorSubmitRequest, String> {
    match request {
        AdapterRequest::Detect(_) => Ok(CoordinatorSubmitRequest::Detect),
        AdapterRequest::Install(install) => Ok(CoordinatorSubmitRequest::Install {
            package_name: install.package.name,
            version: install.version,
        }),
        AdapterRequest::Uninstall(uninstall) => Ok(CoordinatorSubmitRequest::Uninstall {
            package_name: uninstall.package.name,
        }),
        AdapterRequest::Upgrade(upgrade) => Ok(CoordinatorSubmitRequest::Upgrade {
            package_name: upgrade.package.map(|package| package.name),
        }),
        AdapterRequest::Pin(pin) => Ok(CoordinatorSubmitRequest::Pin {
            package_name: pin.package.name,
            version: pin.version,
        }),
        AdapterRequest::Unpin(unpin) => Ok(CoordinatorSubmitRequest::Unpin {
            package_name: unpin.package.name,
        }),
        unsupported => Err(format!(
            "coordinator submit request does not support adapter action '{:?}'",
            unsupported.action()
        )),
    }
}

fn adapter_response_to_coordinator_payload(
    response: helm_core::adapters::AdapterResponse,
) -> CoordinatorPayload {
    match response {
        helm_core::adapters::AdapterResponse::Detection(info) => CoordinatorPayload::Detection {
            installed: info.installed,
            version: info.version,
            executable_path: info
                .executable_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
        },
        helm_core::adapters::AdapterResponse::Refreshed => CoordinatorPayload::Refreshed,
        helm_core::adapters::AdapterResponse::InstalledPackages(packages) => {
            CoordinatorPayload::InstalledPackages {
                count: packages.len(),
            }
        }
        helm_core::adapters::AdapterResponse::OutdatedPackages(packages) => {
            CoordinatorPayload::OutdatedPackages {
                count: packages.len(),
            }
        }
        helm_core::adapters::AdapterResponse::SearchResults(results) => {
            CoordinatorPayload::SearchResults {
                count: results.len(),
            }
        }
        helm_core::adapters::AdapterResponse::Mutation(mutation) => CoordinatorPayload::Mutation {
            manager_id: mutation.package.manager.as_str().to_string(),
            package_name: mutation.package.name,
            action: format!("{:?}", mutation.action).to_lowercase(),
            before_version: mutation.before_version,
            after_version: mutation.after_version,
        },
    }
}

/// Initialize the Helm core engine with the given SQLite database path.
///
/// # Safety
///
/// `db_path` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_init(db_path: *const c_char) -> bool {
    if db_path.is_null() {
        return false;
    }

    // If already initialized, return true
    if lock_or_recover(&STATE, "state").is_some() {
        return true;
    }

    let c_str = unsafe { CStr::from_ptr(db_path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    // Initialize logging
    let _ = tracing_subscriber::fmt::try_init();

    // Create Tokio Runtime
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Failed to create Tokio runtime: {}", e);
            return false;
        }
    };

    // Initialize Store
    let store = Arc::new(SqliteStore::new(path_str));
    if let Err(e) = store.migrate_to_latest() {
        eprintln!("Failed to migrate DB: {}", e);
        return false;
    }

    // Initialize Adapters
    let executor = Arc::new(TokioProcessExecutor);

    let homebrew_adapter = Arc::new(HomebrewAdapter::new(ProcessHomebrewSource::new(
        executor.clone(),
    )));
    let homebrew_cask_adapter = Arc::new(HomebrewCaskAdapter::new(ProcessHomebrewCaskSource::new(
        executor.clone(),
    )));
    let mise_adapter = Arc::new(MiseAdapter::new(ProcessMiseSource::new(executor.clone())));
    let asdf_adapter = Arc::new(AsdfAdapter::new(ProcessAsdfSource::new(executor.clone())));
    let npm_adapter = Arc::new(NpmAdapter::new(ProcessNpmSource::new(executor.clone())));
    let pnpm_adapter = Arc::new(PnpmAdapter::new(ProcessPnpmSource::new(executor.clone())));
    let yarn_adapter = Arc::new(YarnAdapter::new(ProcessYarnSource::new(executor.clone())));
    let cargo_adapter = Arc::new(CargoAdapter::new(ProcessCargoSource::new(executor.clone())));
    let cargo_binstall_adapter = Arc::new(CargoBinstallAdapter::new(
        ProcessCargoBinstallSource::new(executor.clone()),
    ));
    let pip_adapter = Arc::new(PipAdapter::new(ProcessPipSource::new(executor.clone())));
    let pipx_adapter = Arc::new(PipxAdapter::new(ProcessPipxSource::new(executor.clone())));
    let poetry_adapter = Arc::new(PoetryAdapter::new(ProcessPoetrySource::new(
        executor.clone(),
    )));
    let rubygems_adapter = Arc::new(RubyGemsAdapter::new(ProcessRubyGemsSource::new(
        executor.clone(),
    )));
    let bundler_adapter = Arc::new(BundlerAdapter::new(ProcessBundlerSource::new(
        executor.clone(),
    )));
    let rustup_adapter = Arc::new(RustupAdapter::new(ProcessRustupSource::new(
        executor.clone(),
    )));
    let softwareupdate_adapter = Arc::new(SoftwareUpdateAdapter::new(
        ProcessSoftwareUpdateSource::new(executor.clone()),
    ));
    let macports_adapter = Arc::new(MacPortsAdapter::new(ProcessMacPortsSource::new(
        executor.clone(),
    )));
    let nix_darwin_adapter = Arc::new(NixDarwinAdapter::new(ProcessNixDarwinSource::new(
        executor.clone(),
    )));
    let mas_adapter = Arc::new(MasAdapter::new(ProcessMasSource::new(executor.clone())));
    let docker_desktop_adapter = Arc::new(DockerDesktopAdapter::new(
        ProcessDockerDesktopSource::new(executor.clone()),
    ));
    let podman_adapter = Arc::new(PodmanAdapter::new(ProcessPodmanSource::new(
        executor.clone(),
    )));
    let colima_adapter = Arc::new(ColimaAdapter::new(ProcessColimaSource::new(
        executor.clone(),
    )));
    let sparkle_adapter = Arc::new(SparkleAdapter::new(ProcessSparkleSource::new(
        executor.clone(),
    )));
    let setapp_adapter = Arc::new(SetappAdapter::new(ProcessSetappSource::new(
        executor.clone(),
    )));
    let parallels_desktop_adapter = Arc::new(ParallelsDesktopAdapter::new(
        ProcessParallelsDesktopSource::new(executor.clone()),
    ));
    let xcode_command_line_tools_adapter = Arc::new(XcodeCommandLineToolsAdapter::new(
        ProcessXcodeCommandLineToolsSource::new(executor.clone()),
    ));
    let rosetta2_adapter = Arc::new(Rosetta2Adapter::new(ProcessRosetta2Source::new(
        executor.clone(),
    )));
    let firmware_updates_adapter = Arc::new(FirmwareUpdatesAdapter::new(
        ProcessFirmwareUpdatesSource::new(executor.clone()),
    ));

    let adapters: Vec<Arc<dyn helm_core::adapters::ManagerAdapter>> = vec![
        homebrew_adapter,
        homebrew_cask_adapter,
        mise_adapter,
        asdf_adapter,
        npm_adapter,
        pnpm_adapter,
        yarn_adapter,
        cargo_adapter,
        cargo_binstall_adapter,
        pip_adapter,
        pipx_adapter,
        poetry_adapter,
        rubygems_adapter,
        bundler_adapter,
        rustup_adapter,
        softwareupdate_adapter,
        macports_adapter,
        nix_darwin_adapter,
        mas_adapter,
        docker_desktop_adapter,
        podman_adapter,
        colima_adapter,
        sparkle_adapter,
        setapp_adapter,
        parallels_desktop_adapter,
        xcode_command_line_tools_adapter,
        rosetta2_adapter,
        firmware_updates_adapter,
    ];

    // Initialize Orchestration
    let runtime = match AdapterRuntime::with_all_stores(
        adapters,
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    ) {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            eprintln!("Failed to create adapter runtime: {}", e);
            return false;
        }
    };

    let rt_handle = rt.handle().clone();

    let detection_map: std::collections::HashMap<_, _> = store
        .list_detections()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let pref_map: std::collections::HashMap<_, _> = store
        .list_manager_preferences()
        .unwrap_or_default()
        .into_iter()
        .map(|pref| (pref.manager, pref))
        .collect();
    sync_manager_executable_overrides(&detection_map, &pref_map);

    let coordinator_rt_handle = rt_handle.clone();

    let state = HelmState {
        store: store.clone(),
        runtime: runtime.clone(),
        rt_handle,
        _tokio_rt: rt,
    };

    *lock_or_recover(&STATE, "state") = Some(state);
    initialize_coordinator_bridge(store, runtime, coordinator_rt_handle);

    true
}

#[unsafe(no_mangle)]
pub extern "C" fn helm_list_installed_packages() -> *mut c_char {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let enabled_by_manager = manager_enabled_map(state.store.as_ref());

    let packages = match state.store.list_installed() {
        Ok(pkgs) => pkgs,
        Err(e) => {
            eprintln!("Failed to list installed packages: {}", e);
            return std::ptr::null_mut();
        }
    }
    .into_iter()
    .filter(|package| manager_is_enabled(&enabled_by_manager, package.package.manager))
    .collect::<Vec<_>>();

    let json = match serde_json::to_string(&packages) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn helm_list_outdated_packages() -> *mut c_char {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let enabled_by_manager = manager_enabled_map(state.store.as_ref());

    let packages = match state.store.list_outdated() {
        Ok(pkgs) => pkgs,
        Err(e) => {
            eprintln!("Failed to list outdated packages: {}", e);
            return std::ptr::null_mut();
        }
    }
    .into_iter()
    .filter(|package| manager_is_enabled(&enabled_by_manager, package.package.manager))
    .collect::<Vec<_>>();

    let json = match serde_json::to_string(&packages) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn helm_list_tasks() -> *mut c_char {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    // Auto-prune completed/cancelled tasks older than 5 minutes.
    let _ = state.store.prune_completed_tasks(TASK_PRUNE_MAX_AGE_SECS);

    let enabled_by_manager = manager_enabled_map(state.store.as_ref());

    // Fetch a wider snapshot so long-running queued/running tasks do not disappear
    // behind a tight recent-task limit.
    let raw_tasks = match state.store.list_recent_tasks(TASK_RECENT_FETCH_LIMIT) {
        Ok(tasks) => tasks,
        Err(e) => {
            eprintln!("Failed to list tasks: {}", e);
            return std::ptr::null_mut();
        }
    }
    .into_iter()
    .filter(|task| manager_is_enabled(&enabled_by_manager, task.manager))
    .collect::<Vec<_>>();
    #[derive(serde::Serialize)]
    struct FfiTaskRecord {
        id: helm_core::models::TaskId,
        manager: ManagerId,
        task_type: helm_core::models::TaskType,
        status: helm_core::models::TaskStatus,
        label_key: Option<String>,
        label_args: Option<std::collections::BTreeMap<String, String>>,
    }

    let mut labels = lock_or_recover(&TASK_LABELS, "task_labels");
    let fetched_ids: std::collections::HashSet<u64> =
        raw_tasks.iter().map(|task| task.id.0).collect();
    let visible_tasks = build_visible_tasks(raw_tasks, &labels);
    labels.retain(|task_id, _| fetched_ids.contains(task_id));

    let ffi_tasks: Vec<FfiTaskRecord> = visible_tasks
        .iter()
        .map(|task| FfiTaskRecord {
            id: task.id,
            manager: task.manager,
            task_type: task.task_type,
            status: task.status,
            label_key: labels.get(&task.id.0).map(|label| label.key.clone()),
            label_args: labels.get(&task.id.0).and_then(|label| {
                if label.args.is_empty() {
                    None
                } else {
                    Some(label.args.clone())
                }
            }),
        })
        .collect();
    drop(labels);

    let json = match serde_json::to_string(&ffi_tasks) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FfiTaskOutputRecord {
    task_id: TaskId,
    command: Option<String>,
    cwd: Option<String>,
    program_path: Option<String>,
    path_snippet: Option<String>,
    started_at_unix_ms: Option<i64>,
    finished_at_unix_ms: Option<i64>,
    duration_ms: Option<u64>,
    exit_code: Option<i32>,
    termination_reason: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FfiTaskLogRecord {
    id: u64,
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    status: Option<&'static str>,
    level: &'static str,
    message: String,
    created_at_unix: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FfiTaskTimeoutPromptRecord {
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    action: &'static str,
    requested_at_unix_ms: i64,
    grace_seconds: u64,
    suggested_extension_seconds: u64,
}

const DIAGNOSTICS_REDACTION_PLACEHOLDER: &str = "[REDACTED]";
const DIAGNOSTICS_ALLOWED_ENV_KEYS: &[&str] = &[
    "PATH", "PWD", "SHELL", "TERM", "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR", "TMP", "TEMP",
];

fn diagnostics_env_key_allowed(key: &str) -> bool {
    DIAGNOSTICS_ALLOWED_ENV_KEYS
        .iter()
        .any(|allowed| key.eq_ignore_ascii_case(allowed))
}

fn looks_like_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn normalize_sensitive_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim_matches(|character: char| {
        !(character.is_ascii_alphanumeric() || character == '_' || character == '-')
    });
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_ascii_lowercase().replace('-', "_"))
}

fn is_sensitive_key_name(key: &str) -> bool {
    key.contains("token")
        || key.contains("secret")
        || key.contains("password")
        || key.contains("passwd")
        || key.contains("api_key")
        || key.contains("apikey")
        || key.contains("auth")
        || key.contains("cookie")
        || key.contains("session")
        || key.contains("license_key")
        || key.contains("private_key")
}

fn redact_env_assignment_token(token: &str) -> Option<String> {
    let (key, value) = token.split_once('=')?;
    if value.is_empty() || !looks_like_env_key(key) {
        return None;
    }
    if diagnostics_env_key_allowed(key) {
        return Some(token.to_string());
    }
    Some(format!("{key}={DIAGNOSTICS_REDACTION_PLACEHOLDER}"))
}

fn redact_sensitive_pair_token(token: &str) -> Option<String> {
    let (key, delimiter, value) = if let Some((key, value)) = token.split_once('=') {
        (key, '=', value)
    } else if let Some((key, value)) = token.split_once(':') {
        (key, ':', value)
    } else {
        return None;
    };
    if value.is_empty() {
        return None;
    }
    let normalized = normalize_sensitive_key(key)?;
    if !is_sensitive_key_name(normalized.as_str()) {
        return None;
    }
    Some(format!(
        "{key}{delimiter}{DIAGNOSTICS_REDACTION_PLACEHOLDER}"
    ))
}

fn redact_auth_header_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let lowercase = trimmed.to_ascii_lowercase();
    if !(lowercase.starts_with("authorization:") || lowercase.starts_with("proxy-authorization:")) {
        return None;
    }
    let indent_len = line.len().saturating_sub(trimmed.len());
    let indent = &line[..indent_len];
    let header_name = trimmed
        .split_once(':')
        .map(|(name, _)| name)
        .unwrap_or("Authorization");
    Some(format!(
        "{indent}{header_name}: {DIAGNOSTICS_REDACTION_PLACEHOLDER}"
    ))
}

fn redact_diagnostics_text(value: &str) -> String {
    let line_redacted = value
        .lines()
        .map(|line| redact_auth_header_line(line).unwrap_or_else(|| line.to_string()))
        .collect::<Vec<_>>()
        .join("\n");
    let mut rendered = String::with_capacity(line_redacted.len());
    let mut token = String::new();
    for character in line_redacted.chars() {
        if character.is_whitespace() {
            if !token.is_empty() {
                let redacted = redact_env_assignment_token(token.as_str())
                    .or_else(|| redact_sensitive_pair_token(token.as_str()))
                    .unwrap_or_else(|| token.clone());
                rendered.push_str(redacted.as_str());
                token.clear();
            }
            rendered.push(character);
            continue;
        }
        token.push(character);
    }
    if !token.is_empty() {
        let redacted = redact_env_assignment_token(token.as_str())
            .or_else(|| redact_sensitive_pair_token(token.as_str()))
            .unwrap_or(token);
        rendered.push_str(redacted.as_str());
    }
    if value.ends_with('\n') && !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    rendered
}

fn redact_diagnostics_optional(value: Option<String>) -> Option<String> {
    value.map(|text| redact_diagnostics_text(text.as_str()))
}

fn build_ffi_task_output_record(task_id: TaskId) -> FfiTaskOutputRecord {
    let output = helm_core::execution::task_output(task_id);
    FfiTaskOutputRecord {
        task_id,
        command: redact_diagnostics_optional(
            output.as_ref().and_then(|entry| entry.command.clone()),
        ),
        cwd: redact_diagnostics_optional(output.as_ref().and_then(|entry| entry.cwd.clone())),
        program_path: redact_diagnostics_optional(
            output.as_ref().and_then(|entry| entry.program_path.clone()),
        ),
        path_snippet: redact_diagnostics_optional(
            output.as_ref().and_then(|entry| entry.path_snippet.clone()),
        ),
        started_at_unix_ms: output.as_ref().and_then(|entry| entry.started_at_unix_ms),
        finished_at_unix_ms: output.as_ref().and_then(|entry| entry.finished_at_unix_ms),
        duration_ms: output.as_ref().and_then(|entry| entry.duration_ms),
        exit_code: output.as_ref().and_then(|entry| entry.exit_code),
        termination_reason: output
            .as_ref()
            .and_then(|entry| entry.termination_reason.clone()),
        error_code: output.as_ref().and_then(|entry| entry.error_code.clone()),
        error_message: redact_diagnostics_optional(
            output
                .as_ref()
                .and_then(|entry| entry.error_message.clone()),
        ),
        stdout: redact_diagnostics_optional(output.as_ref().and_then(|entry| entry.stdout.clone())),
        stderr: redact_diagnostics_optional(output.as_ref().and_then(|entry| entry.stderr.clone())),
    }
}

fn map_task_log_record(entry: TaskLogRecord) -> FfiTaskLogRecord {
    FfiTaskLogRecord {
        id: entry.id,
        task_id: entry.task_id,
        manager: entry.manager,
        task_type: entry.task_type,
        status: entry.status.map(task_status_str),
        level: task_log_level_str(entry.level),
        message: redact_diagnostics_text(entry.message.as_str()),
        created_at_unix: entry
            .created_at
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0),
    }
}

fn map_timeout_prompt_record(
    entry: helm_core::execution::timeout_prompt_store::TaskTimeoutPromptRecord,
) -> FfiTaskTimeoutPromptRecord {
    FfiTaskTimeoutPromptRecord {
        task_id: entry.task_id,
        manager: entry.manager,
        task_type: entry.task_type,
        action: manager_action_str(entry.action),
        requested_at_unix_ms: entry.requested_at_unix_ms,
        grace_seconds: entry.grace_seconds,
        suggested_extension_seconds: entry.suggested_extension_seconds,
    }
}

/// Return captured stdout/stderr for a task ID as JSON.
///
/// Returns `null` only on serialization/allocation failure.
#[unsafe(no_mangle)]
pub extern "C" fn helm_get_task_output(task_id: i64) -> *mut c_char {
    if task_id < 0 {
        return std::ptr::null_mut();
    }

    let task_id = TaskId(task_id as u64);
    let record = build_ffi_task_output_record(task_id);

    let json = match serde_json::to_string(&record) {
        Ok(value) => value,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Return persisted lifecycle task logs for a task ID as JSON.
///
/// Returns `null` only on invalid input or serialization/allocation failure.
#[unsafe(no_mangle)]
pub extern "C" fn helm_list_task_logs(task_id: i64, limit: i64) -> *mut c_char {
    if task_id < 0 || limit < 0 {
        return std::ptr::null_mut();
    }

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let entries = match state
        .store
        .list_task_logs(TaskId(task_id as u64), limit as usize)
    {
        Ok(entries) => entries,
        Err(error) => {
            eprintln!("Failed to list task logs for task {}: {}", task_id, error);
            return std::ptr::null_mut();
        }
    };

    let payload: Vec<FfiTaskLogRecord> = entries.into_iter().map(map_task_log_record).collect();

    let json = match serde_json::to_string(&payload) {
        Ok(value) => value,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// List pending hard-timeout prompts for running tasks as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn helm_list_task_timeout_prompts() -> *mut c_char {
    let entries: Vec<FfiTaskTimeoutPromptRecord> =
        helm_core::execution::timeout_prompt_store::list_prompts()
            .into_iter()
            .map(map_timeout_prompt_record)
            .collect();

    let json = match serde_json::to_string(&entries) {
        Ok(value) => value,
        Err(_) => return std::ptr::null_mut(),
    };
    match CString::new(json) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Respond to a pending task hard-timeout prompt by task ID.
///
/// When `wait_for_completion` is true, the task deadline is extended.
/// When false, the task is terminated immediately.
#[unsafe(no_mangle)]
pub extern "C" fn helm_respond_task_timeout_prompt(
    task_id: i64,
    wait_for_completion: bool,
) -> bool {
    if task_id < 0 {
        return false;
    }
    let decision = if wait_for_completion {
        helm_core::execution::timeout_prompt_store::TimeoutPromptDecision::Wait
    } else {
        helm_core::execution::timeout_prompt_store::TimeoutPromptDecision::Stop
    };
    helm_core::execution::timeout_prompt_store::respond(TaskId(task_id as u64), decision)
}

fn task_status_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Failed => "failed",
    }
}

fn manager_action_str(action: ManagerAction) -> &'static str {
    match action {
        ManagerAction::Detect => "detect",
        ManagerAction::Refresh => "refresh",
        ManagerAction::Search => "search",
        ManagerAction::ListInstalled => "list_installed",
        ManagerAction::ListOutdated => "list_outdated",
        ManagerAction::Install => "install",
        ManagerAction::Uninstall => "uninstall",
        ManagerAction::Upgrade => "upgrade",
        ManagerAction::Pin => "pin",
        ManagerAction::Unpin => "unpin",
    }
}

fn task_log_level_str(level: TaskLogLevel) -> &'static str {
    match level {
        TaskLogLevel::Info => "info",
        TaskLogLevel::Warn => "warn",
        TaskLogLevel::Error => "error",
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn helm_trigger_refresh() -> bool {
    clear_last_error_key();
    if external_coordinator_state_dir().is_some() {
        return coordinator_start_workflow_external(CoordinatorWorkflowRequest::RefreshAll).is_ok();
    }
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    let runtime = state.runtime.clone();
    let store = state.store.clone();
    let enabled_by_manager = manager_enabled_map(store.as_ref());

    let has_refresh_or_detection =
        has_recent_refresh_or_detection(store.as_ref(), &enabled_by_manager);
    if has_refresh_or_detection {
        return true;
    }

    state._tokio_rt.spawn(async move {
        let results = runtime.refresh_all_ordered().await;
        for (manager, result) in results {
            if let Err(e) = result {
                log_manager_operation_failure("refresh", manager, &e);
            }
        }
    });

    true
}

#[unsafe(no_mangle)]
pub extern "C" fn helm_trigger_detection() -> bool {
    clear_last_error_key();
    if external_coordinator_state_dir().is_some() {
        return coordinator_start_workflow_external(CoordinatorWorkflowRequest::DetectAll).is_ok();
    }
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    let runtime = state.runtime.clone();
    let store = state.store.clone();
    let enabled_by_manager = manager_enabled_map(store.as_ref());

    let has_refresh_or_detection =
        has_recent_refresh_or_detection(store.as_ref(), &enabled_by_manager);
    if has_refresh_or_detection {
        return true;
    }

    preseed_presence_detections(store.as_ref(), runtime.as_ref(), &enabled_by_manager);

    state._tokio_rt.spawn(async move {
        let results = runtime.detect_all_ordered().await;
        for (manager, result) in results {
            if let Err(e) = result {
                log_manager_operation_failure("detection", manager, &e);
            }
        }
    });

    true
}

/// Trigger detection/refresh for a single manager.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_trigger_detection_for_manager(manager_id: *const c_char) -> bool {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };
    let manager = match id_str.parse::<ManagerId>() {
        Ok(manager) => manager,
        Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };

    if external_coordinator_state_dir().is_some() {
        return coordinator_start_workflow_external(CoordinatorWorkflowRequest::RefreshManager {
            manager_id: manager.as_str().to_string(),
        })
        .is_ok();
    }

    let (runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool(SERVICE_ERROR_INTERNAL),
        };
        (state.runtime.clone(), state.rt_handle.clone())
    };

    if !runtime.has_manager(manager) || !runtime.is_manager_enabled(manager) {
        return return_error_bool(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }

    let supports_detection_like_refresh = runtime.supports_capability(manager, Capability::Detect)
        || runtime.supports_capability(manager, Capability::ListInstalled)
        || runtime.supports_capability(manager, Capability::ListOutdated);
    if !supports_detection_like_refresh {
        return return_error_bool(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }

    thread::spawn(move || {
        if let Err(error) = refresh_single_manager(runtime.as_ref(), &rt_handle, manager) {
            log_manager_operation_failure("detection", manager, &error);
        }
    });

    true
}

/// Query the local search cache synchronously and return JSON results.
///
/// # Safety
///
/// `query` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_search_local(query: *const c_char) -> *mut c_char {
    if query.is_null() {
        return std::ptr::null_mut();
    }

    let c_str = unsafe { CStr::from_ptr(query) };
    let query_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let enabled_by_manager = manager_enabled_map(state.store.as_ref());

    let results = match state.store.query_local(query_str, 500) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to query local search cache: {}", e);
            return std::ptr::null_mut();
        }
    }
    .into_iter()
    .filter(|result| {
        manager_participates_in_package_search(result.result.package.manager)
            && manager_participates_in_package_search(result.source_manager)
            && manager_is_enabled(&enabled_by_manager, result.result.package.manager)
            && manager_is_enabled(&enabled_by_manager, result.source_manager)
    })
    .collect::<Vec<_>>();

    #[derive(serde::Serialize)]
    struct FfiSearchResult {
        manager: String,
        name: String,
        version: Option<String>,
        summary: Option<String>,
        source_manager: String,
    }

    let ffi_results: Vec<FfiSearchResult> = results
        .into_iter()
        .map(|r| FfiSearchResult {
            manager: r.result.package.manager.as_str().to_string(),
            name: r.result.package.name,
            version: r.result.version,
            summary: r.result.summary,
            source_manager: r.source_manager.as_str().to_string(),
        })
        .collect();

    let json = match serde_json::to_string(&ffi_results) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Submit a remote search request for the given query. Returns the task ID, or -1 on error.
///
/// # Safety
///
/// `query` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_trigger_remote_search(query: *const c_char) -> i64 {
    clear_last_error_key();
    if query.is_null() {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let c_str = unsafe { CStr::from_ptr(query) };
    let query_str = match c_str.to_str() {
        Ok(s) => s.trim(),
        Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    let mut first_task_id: Option<i64> = None;
    let mut last_error_key: Option<&'static str> = None;

    for manager in remote_search_target_managers(runtime.as_ref(), store.as_ref()) {
        match queue_remote_search_task(
            store.as_ref(),
            runtime.as_ref(),
            &rt_handle,
            manager,
            query_str,
        ) {
            Ok(task_id) => {
                if first_task_id.is_none() {
                    first_task_id = Some(task_id.0 as i64);
                }
            }
            Err(error_key) => {
                last_error_key = Some(error_key);
            }
        }
    }

    match first_task_id {
        Some(task_id) => task_id,
        None => return_error_i64(last_error_key.unwrap_or(SERVICE_ERROR_UNSUPPORTED_CAPABILITY)),
    }
}

/// Submit a remote search request for a specific manager. Returns the task ID, or -1 on error.
///
/// # Safety
///
/// `manager_id` and `query` must be valid, non-null pointers to NUL-terminated UTF-8 C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_trigger_remote_search_for_manager(
    manager_id: *const c_char,
    query: *const c_char,
) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() || query.is_null() {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let query_cstr = unsafe { CStr::from_ptr(query) };
    let query_str = match query_cstr.to_str() {
        Ok(query_text) => query_text.trim(),
        Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    match queue_remote_search_task(
        store.as_ref(),
        runtime.as_ref(),
        &rt_handle,
        manager,
        query_str,
    ) {
        Ok(task_id) => task_id.0 as i64,
        Err(error_key) => return_error_i64(error_key),
    }
}

/// Cancel a running task by ID. Returns true on success.
#[unsafe(no_mangle)]
pub extern "C" fn helm_cancel_task(task_id: i64) -> bool {
    if task_id < 0 {
        return false;
    }

    if external_coordinator_state_dir().is_some() {
        return coordinator_cancel_external(task_id as u64).is_ok();
    }

    let (runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return false,
        };
        (state.runtime.clone(), state.rt_handle.clone())
    };

    let mode = CancellationMode::Graceful {
        grace_period: Duration::from_millis(500),
    };

    match rt_handle.block_on(runtime.cancel(helm_core::models::TaskId(task_id as u64), mode)) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("Failed to cancel task {}: {}", task_id, e);
            false
        }
    }
}

/// Dismiss a terminal task by ID. Returns true on success.
#[unsafe(no_mangle)]
pub extern "C" fn helm_dismiss_task(task_id: i64) -> bool {
    clear_last_error_key();
    if task_id < 0 {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    }

    let store = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool(SERVICE_ERROR_INTERNAL),
        };
        state.store.clone()
    };

    if store
        .delete_task(TaskId(task_id as u64))
        .map_err(|_| set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE))
        .is_err()
    {
        return false;
    }

    lock_or_recover(&TASK_LABELS, "task_labels").remove(&(task_id as u64));
    true
}

/// List manager status: detection info + preferences + implementation status as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn helm_list_manager_status() -> *mut c_char {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let detections = state.store.list_detections().unwrap_or_default();
    let preferences = state.store.list_manager_preferences().unwrap_or_default();

    let mut detection_map: std::collections::HashMap<_, _> = detections.into_iter().collect();
    let mut pref_map: std::collections::HashMap<_, _> = preferences
        .into_iter()
        .map(|pref| (pref.manager, pref))
        .collect();
    apply_manager_enablement_self_heal(
        state.store.as_ref(),
        state.runtime.as_ref(),
        &state.rt_handle,
        &detection_map,
        &mut pref_map,
    );
    sync_manager_executable_overrides(&detection_map, &pref_map);

    let mut statuses = build_manager_statuses(
        Some(state.runtime.as_ref()),
        Some(state.store.as_ref()),
        &detection_map,
        &pref_map,
    );

    // Homebrew detection/version probing is occasionally flaky during first detection.
    // If status is missing or incomplete, probe directly from brew.
    if let Some(status) = statuses
        .iter_mut()
        .find(|status| status.manager_id == ManagerId::HomebrewFormula.as_str())
        && (status.version.is_none() || !status.detected)
        && let Some(probed) = probe_homebrew_version(
            detection_map
                .get(&ManagerId::HomebrewFormula)
                .and_then(|d| d.executable_path.as_deref()),
        )
    {
        status.version = Some(probed.clone());
        status.detected = true;

        let refreshed = if let Some(existing) = detection_map.get(&ManagerId::HomebrewFormula) {
            DetectionInfo {
                installed: true,
                executable_path: existing.executable_path.clone(),
                version: Some(probed),
            }
        } else {
            DetectionInfo {
                installed: true,
                executable_path: None,
                version: Some(probed),
            }
        };
        let _ = state
            .store
            .upsert_detection(ManagerId::HomebrewFormula, &refreshed);
        detection_map.insert(ManagerId::HomebrewFormula, refreshed);
    }

    sync_manager_executable_overrides(&detection_map, &pref_map);

    let json = match serde_json::to_string(&statuses) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Run a local doctor scan and return a health report JSON payload.
///
/// Current implementation scope:
/// - package-state diagnostics for metadata-only Homebrew manager installs.
///
/// TODO(doctor-repair): wire additional detectors and remote fingerprint lookups.
#[unsafe(no_mangle)]
pub extern "C" fn helm_doctor_scan() -> *mut c_char {
    clear_last_error_key();
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => {
            set_last_error_key(SERVICE_ERROR_INTERNAL);
            return std::ptr::null_mut();
        }
    };

    let installed_packages = match state.store.list_installed() {
        Ok(packages) => packages,
        Err(_) => {
            set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE);
            return std::ptr::null_mut();
        }
    };
    let instances = match state.store.list_install_instances(None) {
        Ok(instances) => instances,
        Err(_) => {
            set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE);
            return std::ptr::null_mut();
        }
    };
    let mut instances_by_manager: std::collections::HashMap<
        ManagerId,
        Vec<ManagerInstallInstance>,
    > = std::collections::HashMap::new();
    for instance in instances {
        instances_by_manager
            .entry(instance.manager)
            .or_default()
            .push(instance);
    }

    let report = helm_core::doctor::scan_package_state_report(
        ManagerId::ALL,
        &instances_by_manager,
        installed_packages.as_slice(),
    );
    let json = match serde_json::to_string(&report) {
        Ok(json) => json,
        Err(_) => {
            set_last_error_key(SERVICE_ERROR_INTERNAL);
            return std::ptr::null_mut();
        }
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Return whether shared onboarding has been completed.
#[unsafe(no_mangle)]
pub extern "C" fn helm_get_cli_onboarding_completed() -> bool {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };
    state.store.cli_onboarding_completed().unwrap_or(false)
}

/// Set shared onboarding completion state. Returns true on success.
#[unsafe(no_mangle)]
pub extern "C" fn helm_set_cli_onboarding_completed(completed: bool) -> bool {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };
    state.store.set_cli_onboarding_completed(completed).is_ok()
}

/// Return accepted shared license terms version.
///
/// Returns null when unset or unavailable.
#[unsafe(no_mangle)]
pub extern "C" fn helm_get_cli_accepted_license_terms_version() -> *mut c_char {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let version = match state.store.cli_accepted_license_terms_version() {
        Ok(version) => version,
        Err(_) => return std::ptr::null_mut(),
    };
    let Some(version) = version else {
        return std::ptr::null_mut();
    };

    match CString::new(version) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Set accepted shared license terms version.
///
/// Pass null to clear. Returns true on success.
///
/// # Safety
///
/// `version` may be null; when non-null, it must point to a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_set_cli_accepted_license_terms_version(
    version: *const c_char,
) -> bool {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };

    let value = if version.is_null() {
        None
    } else {
        let version_cstr = unsafe { CStr::from_ptr(version) };
        let version_str = match version_cstr.to_str() {
            Ok(s) => s,
            Err(_) => return false,
        };
        Some(version_str)
    };

    state
        .store
        .set_cli_accepted_license_terms_version(value)
        .is_ok()
}

/// Return whether safe mode is enabled.
#[unsafe(no_mangle)]
pub extern "C" fn helm_get_safe_mode() -> bool {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };
    state.store.safe_mode().unwrap_or(false)
}

/// Set safe mode state. Returns true on success.
#[unsafe(no_mangle)]
pub extern "C" fn helm_set_safe_mode(enabled: bool) -> bool {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };
    state.store.set_safe_mode(enabled).is_ok()
}

/// Return whether Homebrew upgrades should auto-clean old kegs by default.
#[unsafe(no_mangle)]
pub extern "C" fn helm_get_homebrew_keg_auto_cleanup() -> bool {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };

    matches!(
        state.store.homebrew_keg_policy(),
        Ok(HomebrewKegPolicy::Cleanup)
    )
}

/// Set the global Homebrew keg policy.
#[unsafe(no_mangle)]
pub extern "C" fn helm_set_homebrew_keg_auto_cleanup(enabled: bool) -> bool {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };

    let policy = if enabled {
        HomebrewKegPolicy::Cleanup
    } else {
        HomebrewKegPolicy::Keep
    };
    state.store.set_homebrew_keg_policy(policy).is_ok()
}

/// List per-package Homebrew keg policy overrides as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn helm_list_package_keg_policies() -> *mut c_char {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    #[derive(serde::Serialize)]
    struct FfiPackageKegPolicy {
        manager_id: String,
        package_name: String,
        policy: String,
    }

    let policies = match state.store.list_package_keg_policies() {
        Ok(entries) => entries
            .into_iter()
            .map(|entry| FfiPackageKegPolicy {
                manager_id: entry.package.manager.as_str().to_string(),
                package_name: entry.package.name,
                policy: entry.policy.as_str().to_string(),
            })
            .collect::<Vec<_>>(),
        Err(error) => {
            eprintln!("Failed to list package keg policies: {error}");
            return std::ptr::null_mut();
        }
    };

    let json = match serde_json::to_string(&policies) {
        Ok(json) => json,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Set per-package Homebrew keg policy override.
///
/// `policy_mode` values:
/// - `-1`: clear override (use global)
/// - `0`: keep old kegs
/// - `1`: cleanup old kegs
///
/// # Safety
///
/// `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
/// strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_set_package_keg_policy(
    manager_id: *const c_char,
    package_name: *const c_char,
    policy_mode: i32,
) -> bool {
    if manager_id.is_null() || package_name.is_null() {
        return false;
    }

    let manager = {
        let c_str = unsafe { CStr::from_ptr(manager_id) };
        match c_str
            .to_str()
            .ok()
            .and_then(|value| value.parse::<ManagerId>().ok())
        {
            Some(manager) => manager,
            None => return false,
        }
    };

    if manager != ManagerId::HomebrewFormula {
        return false;
    }

    let package_name = {
        let c_str = unsafe { CStr::from_ptr(package_name) };
        match c_str.to_str() {
            Ok(value) if !value.trim().is_empty() => value.trim().to_string(),
            _ => return false,
        }
    };

    let policy = match policy_mode {
        -1 => None,
        0 => Some(HomebrewKegPolicy::Keep),
        1 => Some(HomebrewKegPolicy::Cleanup),
        _ => return false,
    };

    let package = PackageRef {
        manager,
        name: package_name,
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };

    state.store.set_package_keg_policy(&package, policy).is_ok()
}

/// Build an ordered upgrade execution plan from cached outdated snapshot as JSON.
///
/// - `include_pinned`: if false, pinned packages are excluded.
/// - `allow_os_updates`: explicit confirmation gate for `softwareupdate` steps.
#[unsafe(no_mangle)]
pub extern "C" fn helm_preview_upgrade_plan(
    include_pinned: bool,
    allow_os_updates: bool,
) -> *mut c_char {
    clear_last_error_key();
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let outdated = match state.store.list_outdated() {
        Ok(packages) => packages,
        Err(error) => {
            eprintln!("preview_upgrade_plan: failed to list outdated packages: {error}");
            return std::ptr::null_mut();
        }
    };

    let pinned_keys: std::collections::HashSet<String> = state
        .store
        .list_pins()
        .map(|pins| {
            pins.into_iter()
                .map(|pin| format!("{}:{}", pin.package.manager.as_str(), pin.package.name))
                .collect()
        })
        .unwrap_or_default();

    let targets = collect_upgrade_all_targets(&outdated, &pinned_keys, include_pinned);
    let mut steps: Vec<FfiUpgradePlanStep> = Vec::new();
    let mut order_index = 0_u64;

    if state.runtime.is_manager_enabled(ManagerId::HomebrewFormula) {
        for package_name in targets.homebrew {
            let cleanup_old_kegs = effective_homebrew_keg_policy(&state.store, &package_name)
                == HomebrewKegPolicy::Cleanup;
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::HomebrewFormula,
                package_name,
                cleanup_old_kegs,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Mise) {
        for package_name in targets.mise {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Mise,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Npm) {
        for package_name in targets.npm {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Npm,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Pnpm) {
        for package_name in targets.pnpm {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Pnpm,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Yarn) {
        for package_name in targets.yarn {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Yarn,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Cargo) {
        for package_name in targets.cargo {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Cargo,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::CargoBinstall) {
        for package_name in targets.cargo_binstall {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::CargoBinstall,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Pip) {
        for package_name in targets.pip {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Pip,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Pipx) {
        for package_name in targets.pipx {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Pipx,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Poetry) {
        for package_name in targets.poetry {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Poetry,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::RubyGems) {
        for package_name in targets.rubygems {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::RubyGems,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Bundler) {
        for package_name in targets.bundler {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Bundler,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if state.runtime.is_manager_enabled(ManagerId::Rustup) {
        for package_name in targets.rustup {
            push_upgrade_plan_step(
                &mut steps,
                ManagerId::Rustup,
                package_name,
                false,
                &mut order_index,
            );
        }
    }

    if allow_os_updates
        && targets.softwareupdate_outdated
        && state.runtime.is_manager_enabled(ManagerId::SoftwareUpdate)
        && !state.runtime.is_safe_mode()
    {
        push_upgrade_plan_step(
            &mut steps,
            ManagerId::SoftwareUpdate,
            "__confirm_os_updates__".to_string(),
            false,
            &mut order_index,
        );
    }

    let json = match serde_json::to_string(&steps) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("preview_upgrade_plan: failed to encode JSON: {error}");
            return std::ptr::null_mut();
        }
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Queue upgrade tasks for supported managers using cached outdated snapshot.
///
/// - `include_pinned`: if false, pinned packages are excluded.
/// - `allow_os_updates`: explicit confirmation gate for `softwareupdate` upgrades.
#[unsafe(no_mangle)]
pub extern "C" fn helm_upgrade_all(include_pinned: bool, allow_os_updates: bool) -> bool {
    clear_last_error_key();
    if external_coordinator_state_dir().is_some() {
        return coordinator_start_workflow_external(CoordinatorWorkflowRequest::UpdatesRun {
            include_pinned,
            allow_os_updates,
        })
        .is_ok();
    }
    let (store, runtime, tokio_rt) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state._tokio_rt.handle().clone(),
        )
    };

    tokio_rt.spawn(async move {
        let outdated = match store.list_outdated() {
            Ok(packages) => packages,
            Err(error) => {
                eprintln!("upgrade_all: failed to list outdated packages: {error}");
                return;
            }
        };

        let pinned_keys: std::collections::HashSet<String> = store
            .list_pins()
            .map(|pins| {
                pins.into_iter()
                    .map(|pin| format!("{}:{}", pin.package.manager.as_str(), pin.package.name))
                    .collect()
            })
            .unwrap_or_default();

        let targets = collect_upgrade_all_targets(&outdated, &pinned_keys, include_pinned);

        if runtime.is_manager_enabled(ManagerId::HomebrewFormula) {
            for package_name in targets.homebrew {
                let policy = effective_homebrew_keg_policy(&store, &package_name);
                let cleanup_old_kegs = policy == HomebrewKegPolicy::Cleanup;
                let target_name = encode_homebrew_upgrade_target(&package_name, cleanup_old_kegs);
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: target_name,
                    }),
                });
                match runtime.submit(ManagerId::HomebrewFormula, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) = upgrade_task_label_for(
                            ManagerId::HomebrewFormula,
                            &package_name,
                            cleanup_old_kegs,
                        );
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue homebrew upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Mise) {
            for package_name in targets.mise {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Mise,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Mise, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Mise, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue mise upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Npm) {
            for package_name in targets.npm {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Npm,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Npm, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Npm, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue npm upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Pnpm) {
            for package_name in targets.pnpm {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Pnpm,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Pnpm, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Pnpm, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue pnpm upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Yarn) {
            for package_name in targets.yarn {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Yarn,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Yarn, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Yarn, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue yarn upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Cargo) {
            for package_name in targets.cargo {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Cargo,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Cargo, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Cargo, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue cargo upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::CargoBinstall) {
            for package_name in targets.cargo_binstall {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::CargoBinstall,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::CargoBinstall, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::CargoBinstall, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!(
                            "upgrade_all: failed to queue cargo-binstall upgrade task: {error}"
                        );
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Pip) {
            for package_name in targets.pip {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Pip,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Pip, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Pip, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue pip upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Pipx) {
            for package_name in targets.pipx {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Pipx,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Pipx, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Pipx, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue pipx upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Poetry) {
            for package_name in targets.poetry {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Poetry,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Poetry, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Poetry, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue poetry upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::RubyGems) {
            for package_name in targets.rubygems {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::RubyGems,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::RubyGems, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::RubyGems, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue rubygems upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Bundler) {
            for package_name in targets.bundler {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Bundler,
                        name: package_name.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Bundler, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Bundler, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue bundler upgrade task: {error}");
                    }
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Rustup) {
            for toolchain in targets.rustup {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Rustup,
                        name: toolchain.clone(),
                    }),
                });
                match runtime.submit(ManagerId::Rustup, request).await {
                    Ok(task_id) => {
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::Rustup, &toolchain, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!(
                            "upgrade_all: failed to queue rustup toolchain upgrade task: {error}"
                        );
                    }
                }
            }
        }

        if allow_os_updates
            && targets.softwareupdate_outdated
            && runtime.is_manager_enabled(ManagerId::SoftwareUpdate)
        {
            if runtime.is_safe_mode() {
                eprintln!("upgrade_all: safe mode enabled; skipping softwareupdate upgrade");
            } else {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::SoftwareUpdate,
                        name: "__confirm_os_updates__".to_string(),
                    }),
                });
                match runtime.submit(ManagerId::SoftwareUpdate, request).await {
                    Ok(task_id) => {
                        let package_name = "__confirm_os_updates__".to_string();
                        let (label_key, label_args) =
                            upgrade_task_label_for(ManagerId::SoftwareUpdate, &package_name, false);
                        set_task_label(task_id, label_key, &label_args);
                    }
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue softwareupdate task: {error}");
                    }
                }
            }
        }
    });

    true
}

/// Queue an upgrade task for a single package. Returns the task ID, or -1 on error.
///
/// Currently supported manager IDs:
/// - "homebrew_formula"
/// - "mise"
/// - "npm"
/// - "pnpm"
/// - "yarn"
/// - "cargo"
/// - "cargo_binstall"
/// - "pip"
/// - "pipx"
/// - "poetry"
/// - "rubygems"
/// - "bundler"
/// - "rustup"
/// - "softwareupdate" (requires package_name "__confirm_os_updates__")
///
/// # Safety
///
/// `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
/// strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_upgrade_package(
    manager_id: *const c_char,
    package_name: *const c_char,
) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() || package_name.is_null() {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let package_cstr = unsafe { CStr::from_ptr(package_name) };
    let package_name = match package_cstr.to_str() {
        Ok(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let (target_manager, request, label_key, label_args): (
        ManagerId,
        AdapterRequest,
        Option<&str>,
        Vec<(&str, String)>,
    ) = match manager {
        ManagerId::HomebrewFormula => {
            let policy = {
                let guard = lock_or_recover(&STATE, "state");
                let state = match guard.as_ref() {
                    Some(s) => s,
                    None => return return_error_i64(SERVICE_ERROR_INTERNAL),
                };
                effective_homebrew_keg_policy(&state.store, &package_name)
            };
            let cleanup_old_kegs = policy == HomebrewKegPolicy::Cleanup;
            let target_name = encode_homebrew_upgrade_target(&package_name, cleanup_old_kegs);
            (
                ManagerId::HomebrewFormula,
                AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: target_name,
                    }),
                }),
                Some(if cleanup_old_kegs {
                    "service.task.label.upgrade.homebrew_cleanup"
                } else {
                    "service.task.label.upgrade.homebrew"
                }),
                vec![("package", package_name.clone())],
            )
        }
        ManagerId::Mise => (
            ManagerId::Mise,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Mise,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.mise"),
            vec![("package", package_name.clone())],
        ),
        ManagerId::Npm => (
            ManagerId::Npm,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Npm,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                ("manager", manager_display_name(ManagerId::Npm).to_string()),
            ],
        ),
        ManagerId::Pnpm => (
            ManagerId::Pnpm,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pnpm,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                ("manager", manager_display_name(ManagerId::Pnpm).to_string()),
            ],
        ),
        ManagerId::Yarn => (
            ManagerId::Yarn,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Yarn,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                ("manager", manager_display_name(ManagerId::Yarn).to_string()),
            ],
        ),
        ManagerId::Cargo => (
            ManagerId::Cargo,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Cargo,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                (
                    "manager",
                    manager_display_name(ManagerId::Cargo).to_string(),
                ),
            ],
        ),
        ManagerId::CargoBinstall => (
            ManagerId::CargoBinstall,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::CargoBinstall,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                (
                    "manager",
                    manager_display_name(ManagerId::CargoBinstall).to_string(),
                ),
            ],
        ),
        ManagerId::Pip => (
            ManagerId::Pip,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pip,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                ("manager", manager_display_name(ManagerId::Pip).to_string()),
            ],
        ),
        ManagerId::Pipx => (
            ManagerId::Pipx,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pipx,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                ("manager", manager_display_name(ManagerId::Pipx).to_string()),
            ],
        ),
        ManagerId::Poetry => (
            ManagerId::Poetry,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Poetry,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                (
                    "manager",
                    manager_display_name(ManagerId::Poetry).to_string(),
                ),
            ],
        ),
        ManagerId::RubyGems => (
            ManagerId::RubyGems,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::RubyGems,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                (
                    "manager",
                    manager_display_name(ManagerId::RubyGems).to_string(),
                ),
            ],
        ),
        ManagerId::Bundler => (
            ManagerId::Bundler,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Bundler,
                    name: package_name.clone(),
                }),
            }),
            Some("service.task.label.upgrade.package"),
            vec![
                ("package", package_name.clone()),
                (
                    "manager",
                    manager_display_name(ManagerId::Bundler).to_string(),
                ),
            ],
        ),
        ManagerId::Rustup => {
            let label_key = if package_name == "__self__" {
                "service.task.label.update.rustup_self"
            } else {
                "service.task.label.upgrade.rustup_toolchain"
            };
            (
                ManagerId::Rustup,
                AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Rustup,
                        name: package_name.clone(),
                    }),
                }),
                Some(label_key),
                if package_name == "__self__" {
                    Vec::new()
                } else {
                    vec![("toolchain", package_name.clone())]
                },
            )
        }
        ManagerId::SoftwareUpdate => {
            if package_name != "__confirm_os_updates__" {
                return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
            }
            (
                ManagerId::SoftwareUpdate,
                AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::SoftwareUpdate,
                        name: "__confirm_os_updates__".to_string(),
                    }),
                }),
                Some("service.task.label.upgrade.softwareupdate_all"),
                Vec::new(),
            )
        }
        _ => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
    };
    let mut label_args = label_args;
    if label_key.is_some() {
        label_args.push((
            "plan_step_id",
            upgrade_plan_step_id(target_manager, &package_name),
        ));
    }

    if external_coordinator_state_dir().is_some() {
        let submit_request = match adapter_request_to_coordinator_submit(request.clone()) {
            Ok(request) => request,
            Err(_) => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
        };
        return match coordinator_submit_external(target_manager, submit_request, false) {
            Ok(response) => response
                .task_id
                .map(|task_id| task_id as i64)
                .unwrap_or_else(|| return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)),
            Err(_) => return_error_i64(SERVICE_ERROR_PROCESS_FAILURE),
        };
    }

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    if !runtime.is_manager_enabled(target_manager)
        || !runtime.supports_capability(target_manager, Capability::Upgrade)
    {
        return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }

    if let Some(existing) = find_matching_inflight_task(
        store.as_ref(),
        target_manager,
        TaskType::Upgrade,
        label_key,
        &label_args,
    ) {
        return existing.0 as i64;
    }

    match rt_handle.block_on(runtime.submit(target_manager, request)) {
        Ok(task_id) => {
            if let Some(label_key) = label_key {
                set_task_label(task_id, label_key, &label_args);
            }
            task_id.0 as i64
        }
        Err(error) => {
            eprintln!("upgrade_package: failed to queue task: {error}");
            return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)
        }
    }
}

/// Queue an install task for a single package. Returns the task ID, or -1 on error.
///
/// # Safety
///
/// `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
/// strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_install_package(
    manager_id: *const c_char,
    package_name: *const c_char,
) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() || package_name.is_null() {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let package_cstr = unsafe { CStr::from_ptr(package_name) };
    let package_name = match package_cstr.to_str() {
        Ok(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let label_key = if manager == ManagerId::HomebrewFormula {
        "service.task.label.install.homebrew_formula"
    } else {
        "service.task.label.install.package"
    };
    let label_args = if manager == ManagerId::HomebrewFormula {
        vec![("package", package_name.clone())]
    } else {
        vec![
            ("package", package_name.clone()),
            ("manager", manager_display_name(manager).to_string()),
        ]
    };

    let request = AdapterRequest::Install(InstallRequest {
        package: PackageRef {
            manager,
            name: package_name,
        },
        version: None,
    });

    if external_coordinator_state_dir().is_some() {
        let submit_request = match adapter_request_to_coordinator_submit(request.clone()) {
            Ok(request) => request,
            Err(_) => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
        };
        return match coordinator_submit_external(manager, submit_request, false) {
            Ok(response) => response
                .task_id
                .map(|task_id| task_id as i64)
                .unwrap_or_else(|| return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)),
            Err(_) => return_error_i64(SERVICE_ERROR_PROCESS_FAILURE),
        };
    }

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    if !supports_individual_package_install(runtime.as_ref(), manager) {
        return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }

    if let Some(existing) = find_matching_inflight_task(
        store.as_ref(),
        manager,
        TaskType::Install,
        Some(label_key),
        &label_args,
    ) {
        return existing.0 as i64;
    }

    match rt_handle.block_on(runtime.submit(manager, request)) {
        Ok(task_id) => {
            set_task_label(task_id, label_key, &label_args);
            task_id.0 as i64
        }
        Err(error) => {
            eprintln!("install_package: failed to queue task: {error}");
            return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)
        }
    }
}

/// Queue an uninstall task for a single package. Returns the task ID, or -1 on error.
///
/// # Safety
///
/// `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
/// strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_uninstall_package(
    manager_id: *const c_char,
    package_name: *const c_char,
) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() || package_name.is_null() {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let package_cstr = unsafe { CStr::from_ptr(package_name) };
    let package_name = match package_cstr.to_str() {
        Ok(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let label_key = if manager == ManagerId::HomebrewFormula {
        "service.task.label.uninstall.homebrew_formula"
    } else {
        "service.task.label.uninstall.package"
    };
    let label_args = if manager == ManagerId::HomebrewFormula {
        vec![("package", package_name.clone())]
    } else {
        vec![
            ("package", package_name.clone()),
            ("manager", manager_display_name(manager).to_string()),
        ]
    };

    let request = AdapterRequest::Uninstall(UninstallRequest {
        package: PackageRef {
            manager,
            name: package_name,
        },
    });

    if external_coordinator_state_dir().is_some() {
        let submit_request = match adapter_request_to_coordinator_submit(request.clone()) {
            Ok(request) => request,
            Err(_) => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
        };
        return match coordinator_submit_external(manager, submit_request, false) {
            Ok(response) => response
                .task_id
                .map(|task_id| task_id as i64)
                .unwrap_or_else(|| return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)),
            Err(_) => return_error_i64(SERVICE_ERROR_PROCESS_FAILURE),
        };
    }

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    if !supports_individual_package_uninstall(runtime.as_ref(), manager) {
        return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }

    if let Some(existing) = find_matching_inflight_task(
        store.as_ref(),
        manager,
        TaskType::Uninstall,
        Some(label_key),
        &label_args,
    ) {
        return existing.0 as i64;
    }

    match rt_handle.block_on(runtime.submit(manager, request)) {
        Ok(task_id) => {
            set_task_label(task_id, label_key, &label_args);
            task_id.0 as i64
        }
        Err(error) => {
            eprintln!("uninstall_package: failed to queue task: {error}");
            return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)
        }
    }
}

/// Preview package uninstall blast radius as JSON.
///
/// # Safety
///
/// `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
/// strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_preview_package_uninstall(
    manager_id: *const c_char,
    package_name: *const c_char,
) -> *mut c_char {
    clear_last_error_key();
    if manager_id.is_null() || package_name.is_null() {
        set_last_error_key(SERVICE_ERROR_INVALID_INPUT);
        return std::ptr::null_mut();
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => {
            set_last_error_key(SERVICE_ERROR_INVALID_INPUT);
            return std::ptr::null_mut();
        }
    };

    let package_cstr = unsafe { CStr::from_ptr(package_name) };
    let package_name = match package_cstr.to_str() {
        Ok(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => {
            set_last_error_key(SERVICE_ERROR_INVALID_INPUT);
            return std::ptr::null_mut();
        }
    };

    let (store, runtime) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => {
                set_last_error_key(SERVICE_ERROR_INTERNAL);
                return std::ptr::null_mut();
            }
        };
        (state.store.clone(), state.runtime.clone())
    };

    if !supports_individual_package_uninstall(runtime.as_ref(), manager) {
        set_last_error_key(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
        return std::ptr::null_mut();
    }

    let active_instance = match active_manager_install_instance(store.as_ref(), manager) {
        Ok(instance) => instance,
        Err(error_key) => {
            set_last_error_key(error_key);
            return std::ptr::null_mut();
        }
    };
    let package = PackageRef {
        manager,
        name: package_name,
    };
    let preview = build_package_uninstall_preview(
        PackageUninstallPreviewContext {
            package: &package,
            active_instance: active_instance.as_ref(),
        },
        DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
    );

    let json = match serde_json::to_string(&preview) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("preview_package_uninstall: failed to encode JSON: {error}");
            set_last_error_key(SERVICE_ERROR_INTERNAL);
            return std::ptr::null_mut();
        }
    };

    match CString::new(json) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => {
            set_last_error_key(SERVICE_ERROR_INTERNAL);
            std::ptr::null_mut()
        }
    }
}

/// List pin records as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn helm_list_pins() -> *mut c_char {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    #[derive(serde::Serialize)]
    struct FfiPinRecord {
        manager_id: String,
        package_name: String,
        pin_kind: String,
        pinned_version: Option<String>,
        created_at_unix: i64,
    }

    let pins = match state.store.list_pins() {
        Ok(records) => records
            .into_iter()
            .map(|record| FfiPinRecord {
                manager_id: record.package.manager.as_str().to_string(),
                package_name: record.package.name,
                pin_kind: match record.kind {
                    PinKind::Native => "native".to_string(),
                    PinKind::Virtual => "virtual".to_string(),
                },
                pinned_version: record.pinned_version,
                created_at_unix: record
                    .created_at
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
            })
            .collect::<Vec<_>>(),
        Err(e) => {
            eprintln!("Failed to list pins: {}", e);
            return std::ptr::null_mut();
        }
    };

    let json = match serde_json::to_string(&pins) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Persist a virtual pin for a package. Returns true on success.
///
/// # Safety
///
/// `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
/// strings. `pinned_version` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_pin_package(
    manager_id: *const c_char,
    package_name: *const c_char,
    pinned_version: *const c_char,
) -> bool {
    clear_last_error_key();
    if manager_id.is_null() || package_name.is_null() {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    }

    let manager = {
        let c_str = unsafe { CStr::from_ptr(manager_id) };
        match c_str
            .to_str()
            .ok()
            .and_then(|s| s.parse::<ManagerId>().ok())
        {
            Some(id) => id,
            None => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
        }
    };

    let package_name = {
        let c_str = unsafe { CStr::from_ptr(package_name) };
        match c_str.to_str() {
            Ok(value) if !value.trim().is_empty() => value.to_string(),
            _ => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
        }
    };

    let pinned_version = if pinned_version.is_null() {
        None
    } else {
        let c_str = unsafe { CStr::from_ptr(pinned_version) };
        match c_str.to_str() {
            Ok(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            }
            Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
        }
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    let package = PackageRef {
        manager,
        name: package_name,
    };
    let pin_kind = if manager == ManagerId::HomebrewFormula {
        let request = AdapterRequest::Pin(PinRequest {
            package: package.clone(),
            version: pinned_version.clone(),
        });
        if external_coordinator_state_dir().is_some() {
            let submit_request = match adapter_request_to_coordinator_submit(request.clone()) {
                Ok(request) => request,
                Err(_) => return return_error_bool(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
            };
            if coordinator_submit_external(manager, submit_request, true).is_err() {
                return return_error_bool(SERVICE_ERROR_PROCESS_FAILURE);
            }
            PinKind::Native
        } else {
            let task_id = match rt_handle.block_on(runtime.submit(manager, request)) {
                Ok(task_id) => task_id,
                Err(_) => return return_error_bool(SERVICE_ERROR_PROCESS_FAILURE),
            };

            set_task_label(
                task_id,
                "service.task.label.pin.homebrew",
                &[("package", package.name.clone())],
            );

            let snapshot = match rt_handle.block_on(runtime.wait_for_terminal(task_id, None)) {
                Ok(snapshot) => snapshot,
                Err(_) => return return_error_bool(SERVICE_ERROR_PROCESS_FAILURE),
            };

            match snapshot.terminal_state {
                Some(AdapterTaskTerminalState::Succeeded(_)) => {}
                _ => return return_error_bool(SERVICE_ERROR_PROCESS_FAILURE),
            }
            PinKind::Native
        }
    } else {
        PinKind::Virtual
    };

    store
        .upsert_pin(&PinRecord {
            package,
            kind: pin_kind,
            pinned_version,
            created_at: std::time::SystemTime::now(),
        })
        .map_err(|_| set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE))
        .is_ok()
}

/// Remove a pin for a package. Returns true on success.
///
/// # Safety
///
/// `manager_id` and `package_name` must be valid, non-null pointers to NUL-terminated UTF-8 C
/// strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_unpin_package(
    manager_id: *const c_char,
    package_name: *const c_char,
) -> bool {
    clear_last_error_key();
    if manager_id.is_null() || package_name.is_null() {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    }

    let manager = {
        let c_str = unsafe { CStr::from_ptr(manager_id) };
        match c_str
            .to_str()
            .ok()
            .and_then(|s| s.parse::<ManagerId>().ok())
        {
            Some(id) => id,
            None => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
        }
    };

    let package_name = {
        let c_str = unsafe { CStr::from_ptr(package_name) };
        match c_str.to_str() {
            Ok(value) if !value.trim().is_empty() => value.to_string(),
            _ => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
        }
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    if manager == ManagerId::HomebrewFormula {
        let request = AdapterRequest::Unpin(UnpinRequest {
            package: PackageRef {
                manager,
                name: package_name.clone(),
            },
        });
        if external_coordinator_state_dir().is_some() {
            let submit_request = match adapter_request_to_coordinator_submit(request.clone()) {
                Ok(request) => request,
                Err(_) => return return_error_bool(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
            };
            if coordinator_submit_external(manager, submit_request, true).is_err() {
                return return_error_bool(SERVICE_ERROR_PROCESS_FAILURE);
            }
        } else {
            let task_id = match rt_handle.block_on(runtime.submit(manager, request)) {
                Ok(task_id) => task_id,
                Err(_) => return return_error_bool(SERVICE_ERROR_PROCESS_FAILURE),
            };

            set_task_label(
                task_id,
                "service.task.label.unpin.homebrew",
                &[("package", package_name.clone())],
            );

            let snapshot = match rt_handle.block_on(runtime.wait_for_terminal(task_id, None)) {
                Ok(snapshot) => snapshot,
                Err(_) => return return_error_bool(SERVICE_ERROR_PROCESS_FAILURE),
            };

            match snapshot.terminal_state {
                Some(AdapterTaskTerminalState::Succeeded(_)) => {}
                _ => return return_error_bool(SERVICE_ERROR_PROCESS_FAILURE),
            }
        }
    }

    let package_key = format!("{}:{}", manager.as_str(), package_name);
    store
        .remove_pin(&package_key)
        .map_err(|_| set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE))
        .is_ok()
}

/// Set a manager as enabled or disabled.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_set_manager_enabled(
    manager_id: *const c_char,
    enabled: bool,
) -> bool {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(m) => m,
        Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    if enabled {
        let detection_map: std::collections::HashMap<ManagerId, DetectionInfo> = store
            .list_detections()
            .unwrap_or_default()
            .into_iter()
            .collect();
        let pref_map: std::collections::HashMap<ManagerId, ManagerPreference> = store
            .list_manager_preferences()
            .unwrap_or_default()
            .into_iter()
            .map(|preference| (preference.manager, preference))
            .collect();

        let selected_executable =
            resolved_manager_selected_executable_path(manager, &detection_map, &pref_map);
        let eligibility = manager_enablement_eligibility(
            manager,
            selected_executable.as_deref().map(std::path::Path::new),
        );
        if !eligibility.is_eligible {
            return return_error_bool(
                eligibility
                    .service_error_key
                    .unwrap_or(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
            );
        }

        let install_instances_by_manager = grouped_install_instances_by_manager(store.as_ref());
        let homebrew_installed_formulas = homebrew_installed_formula_set(store.as_ref());
        if manager_has_setup_required_issue(
            manager,
            &install_instances_by_manager,
            &homebrew_installed_formulas,
        ) {
            return return_error_bool(SERVICE_ERROR_MANAGER_SETUP_REQUIRED);
        }
    } else {
        let enabled_by_manager = manager_enabled_map(store.as_ref());
        let dependents =
            enabled_dependents_for_manager(store.as_ref(), &enabled_by_manager, manager);
        if !dependents.is_empty() {
            let dependent_ids = dependents
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            eprintln!(
                "helm-ffi: blocked disabling '{}' because enabled dependents rely on it: {}",
                manager.as_str(),
                dependent_ids
            );
            return return_error_bool(SERVICE_ERROR_MANAGER_DEPENDENCY_BLOCKED);
        }
    }

    if store
        .set_manager_enabled(manager, enabled)
        .map_err(|_| set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE))
        .is_err()
    {
        return false;
    }

    if !enabled
        && !purge_tasks_for_manager(
            store.as_ref(),
            runtime.as_ref(),
            &rt_handle,
            manager,
            "set_manager_enabled",
        )
    {
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }

    true
}

/// Set (or clear) the selected executable path for a manager.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
/// `selected_path` may be null (to clear override).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_set_manager_selected_executable_path(
    manager_id: *const c_char,
    selected_path: *const c_char,
) -> bool {
    clear_last_error_key();
    let manager = match unsafe { parse_manager_id_arg(manager_id) } {
        Ok(manager) => manager,
        Err(error_key) => return return_error_bool(error_key),
    };

    let selected_path = if selected_path.is_null() {
        None
    } else {
        let selected_cstr = unsafe { CStr::from_ptr(selected_path) };
        let selected = match selected_cstr.to_str() {
            Ok(s) => s.trim(),
            Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
        };
        if selected.is_empty() {
            None
        } else {
            if !std::path::Path::new(selected).is_absolute() {
                return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
            }
            if !std::path::Path::new(selected).is_file() {
                return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
            }
            Some(selected.to_string())
        }
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    if let Err(_error) = state
        .store
        .set_manager_selected_executable_path(manager, selected_path.as_deref())
    {
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }

    let detection_map: std::collections::HashMap<_, _> = state
        .store
        .list_detections()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let pref_map: std::collections::HashMap<_, _> = state
        .store
        .list_manager_preferences()
        .unwrap_or_default()
        .into_iter()
        .map(|pref| (pref.manager, pref))
        .collect();
    sync_manager_executable_overrides(&detection_map, &pref_map);

    true
}

/// Set the managed install instance for a manager by stable `instance_id`.
///
/// This updates selected executable-path preference, marks the selected instance active,
/// clears multi-instance acknowledgement, and refreshes in-memory executable overrides.
///
/// # Safety
///
/// `manager_id` and `instance_id` must be valid, non-null pointers to NUL-terminated UTF-8
/// C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_set_manager_active_install_instance(
    manager_id: *const c_char,
    instance_id: *const c_char,
) -> bool {
    clear_last_error_key();
    let manager = match unsafe { parse_manager_id_arg(manager_id) } {
        Ok(manager) => manager,
        Err(error_key) => return return_error_bool(error_key),
    };
    let instance_id = match unsafe { parse_required_cstr_arg(instance_id) } {
        Ok(value) => value,
        Err(error_key) => return return_error_bool(error_key),
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(state) => state,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    let mut instances = match manager_install_instances_for(state.store.as_ref(), manager) {
        Ok(instances) => instances,
        Err(error_key) => return return_error_bool(error_key),
    };
    if instances.is_empty() {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    }

    let selected_path = match instances
        .iter()
        .find(|instance| instance.instance_id == instance_id)
    {
        Some(instance) => instance.display_path.to_string_lossy().to_string(),
        None => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };

    for instance in &mut instances {
        instance.is_active = instance.instance_id == instance_id;
    }

    if state
        .store
        .replace_install_instances(manager, &instances)
        .is_err()
    {
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }
    if state
        .store
        .set_manager_selected_executable_path(manager, Some(selected_path.as_str()))
        .is_err()
    {
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }
    if state
        .store
        .set_manager_multi_instance_ack_fingerprint(manager, None)
        .is_err()
    {
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }

    let detection_map: std::collections::HashMap<_, _> = state
        .store
        .list_detections()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let pref_map: std::collections::HashMap<_, _> = state
        .store
        .list_manager_preferences()
        .unwrap_or_default()
        .into_iter()
        .map(|pref| (pref.manager, pref))
        .collect();
    sync_manager_executable_overrides(&detection_map, &pref_map);

    true
}

/// Acknowledge current multi-instance install set for a manager.
///
/// Stores the active install-set fingerprint so manager health can be considered acknowledged.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_ack_manager_multi_instance_state(manager_id: *const c_char) -> bool {
    clear_last_error_key();
    let manager = match unsafe { parse_manager_id_arg(manager_id) } {
        Ok(manager) => manager,
        Err(error_key) => return return_error_bool(error_key),
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(state) => state,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    let instances = match manager_install_instances_for(state.store.as_ref(), manager) {
        Ok(instances) => instances,
        Err(error_key) => return return_error_bool(error_key),
    };
    let Some(fingerprint) = install_instance_fingerprint(&instances) else {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    };

    if state
        .store
        .set_manager_multi_instance_ack_fingerprint(manager, Some(fingerprint.as_str()))
        .is_err()
    {
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }

    true
}

/// Clear multi-instance acknowledgement state for a manager.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_clear_manager_multi_instance_ack(manager_id: *const c_char) -> bool {
    clear_last_error_key();
    let manager = match unsafe { parse_manager_id_arg(manager_id) } {
        Ok(manager) => manager,
        Err(error_key) => return return_error_bool(error_key),
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(state) => state,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    if state
        .store
        .set_manager_multi_instance_ack_fingerprint(manager, None)
        .is_err()
    {
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }

    true
}

/// Set (or clear) the selected install method for a manager.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
/// `install_method` may be null (to clear override).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_set_manager_install_method(
    manager_id: *const c_char,
    install_method: *const c_char,
) -> bool {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(m) => m,
        Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };

    let install_method = if install_method.is_null() {
        None
    } else {
        let method_cstr = unsafe { CStr::from_ptr(install_method) };
        let method = match method_cstr.to_str() {
            Ok(s) => s.trim(),
            Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
        };
        if method.is_empty() {
            None
        } else if manager_install_method_candidates(manager).contains(&method) {
            Some(method.to_string())
        } else {
            return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
        }
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    state
        .store
        .set_manager_selected_install_method(manager, install_method.as_deref())
        .map_err(|_| set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE))
        .is_ok()
}

/// Set manager timeout profile overrides in seconds.
///
/// Positive values set an override; zero/negative values clear the override.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_set_manager_timeout_profile(
    manager_id: *const c_char,
    hard_timeout_seconds: i64,
    idle_timeout_seconds: i64,
) -> bool {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_bool(SERVICE_ERROR_INVALID_INPUT);
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(m) => m,
        Err(_) => return return_error_bool(SERVICE_ERROR_INVALID_INPUT),
    };

    let hard_timeout = if hard_timeout_seconds > 0 {
        Some(hard_timeout_seconds as u64)
    } else {
        None
    };
    let idle_timeout = if idle_timeout_seconds > 0 {
        Some(idle_timeout_seconds as u64)
    } else {
        None
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    if state
        .store
        .set_manager_timeout_hard_seconds(manager, hard_timeout)
        .map_err(|_| set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE))
        .is_err()
    {
        return false;
    }
    if state
        .store
        .set_manager_timeout_idle_seconds(manager, idle_timeout)
        .map_err(|_| set_last_error_key(SERVICE_ERROR_STORAGE_FAILURE))
        .is_err()
    {
        return false;
    }

    let detection_map: std::collections::HashMap<_, _> = state
        .store
        .list_detections()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let pref_map: std::collections::HashMap<_, _> = state
        .store
        .list_manager_preferences()
        .unwrap_or_default()
        .into_iter()
        .map(|pref| (pref.manager, pref))
        .collect();
    sync_manager_executable_overrides(&detection_map, &pref_map);

    true
}

fn manager_supports_post_install_setup(manager: ManagerId) -> bool {
    matches!(manager, ManagerId::Rustup | ManagerId::Mise | ManagerId::Asdf)
}

fn spawn_post_install_setup_task(
    store: Arc<SqliteStore>,
    runtime: Arc<AdapterRuntime>,
    rt_handle: tokio::runtime::Handle,
    manager: ManagerId,
    wait_for_install_task: Option<TaskId>,
) -> Result<TaskId, &'static str> {
    let task_type = TaskType::Install;
    let task_id = create_local_task(store.as_ref(), manager, task_type)?;
    set_task_label(
        task_id,
        "service.task.label.setup.manager",
        &[("manager", manager_display_name(manager).to_string())],
    );

    thread::spawn(move || {
        update_local_task_status(
            store.as_ref(),
            task_id,
            manager,
            task_type,
            TaskStatus::Running,
            TaskLogLevel::Info,
            "task started",
        );

        if let Some(install_task_id) = wait_for_install_task {
            append_local_task_log(
                store.as_ref(),
                task_id,
                manager,
                task_type,
                TaskStatus::Running,
                TaskLogLevel::Info,
                format!("waiting for install task {} to complete", install_task_id.0),
            );
            let install_terminal = rt_handle.block_on(runtime.wait_for_terminal(install_task_id, None));
            match install_terminal {
                Ok(snapshot) => match snapshot.terminal_state {
                    Some(AdapterTaskTerminalState::Succeeded(_)) => {}
                    Some(AdapterTaskTerminalState::Cancelled(_)) => {
                        update_local_task_status(
                            store.as_ref(),
                            task_id,
                            manager,
                            task_type,
                            TaskStatus::Cancelled,
                            TaskLogLevel::Warn,
                            "post-install setup cancelled because install task was cancelled",
                        );
                        return;
                    }
                    Some(AdapterTaskTerminalState::Failed(error)) => {
                        update_local_task_status(
                            store.as_ref(),
                            task_id,
                            manager,
                            task_type,
                            TaskStatus::Failed,
                            TaskLogLevel::Error,
                            format!(
                                "post-install setup skipped because install task failed: {}",
                                error.message
                            ),
                        );
                        return;
                    }
                    None => {
                        update_local_task_status(
                            store.as_ref(),
                            task_id,
                            manager,
                            task_type,
                            TaskStatus::Failed,
                            TaskLogLevel::Error,
                            "post-install setup skipped because install task ended without terminal status",
                        );
                        return;
                    }
                },
                Err(error) => {
                    update_local_task_status(
                        store.as_ref(),
                        task_id,
                        manager,
                        task_type,
                        TaskStatus::Failed,
                        TaskLogLevel::Error,
                        format!(
                            "post-install setup skipped because install task wait failed: {}",
                            error.message
                        ),
                    );
                    return;
                }
            }
        }

        let instances = match store.list_install_instances(Some(manager)) {
            Ok(instances) => instances,
            Err(error) => {
                update_local_task_status(
                    store.as_ref(),
                    task_id,
                    manager,
                    task_type,
                    TaskStatus::Failed,
                    TaskLogLevel::Error,
                    format!("failed to load manager install instances: {error}"),
                );
                return;
            }
        };

        let apply_result = helm_core::post_install_setup::apply_recommended_post_install_setup(
            manager,
            Some(instances.as_slice()),
        );
        match apply_result {
            Ok(result) => {
                append_local_task_log(
                    store.as_ref(),
                    task_id,
                    manager,
                    task_type,
                    TaskStatus::Running,
                    TaskLogLevel::Info,
                    format!("shell rc file: '{}'", result.rc_file.display()),
                );
                let report = helm_core::post_install_setup::evaluate_manager_post_install_setup(
                    manager,
                    Some(instances.as_slice()),
                );
                if report.is_some_and(|value| value.has_unmet_required()) {
                    update_local_task_status(
                        store.as_ref(),
                        task_id,
                        manager,
                        task_type,
                        TaskStatus::Failed,
                        TaskLogLevel::Error,
                        "post-install setup automation completed but requirements are still unmet",
                    );
                } else {
                    update_local_task_status(
                        store.as_ref(),
                        task_id,
                        manager,
                        task_type,
                        TaskStatus::Completed,
                        TaskLogLevel::Info,
                        result.summary,
                    );
                }
            }
            Err(error) => {
                update_local_task_status(
                    store.as_ref(),
                    task_id,
                    manager,
                    task_type,
                    TaskStatus::Failed,
                    TaskLogLevel::Error,
                    error,
                );
            }
        }
    });

    Ok(task_id)
}

/// Apply a manager package-state repair option and queue the corresponding task.
///
/// The current scaffold supports metadata-only Homebrew manager installs by routing one of:
/// - `reinstall_manager_via_homebrew`
/// - `remove_stale_package_entry`
///
/// # Safety
///
/// All pointers must be valid, non-null pointers to NUL-terminated UTF-8 C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_apply_manager_package_state_issue_repair(
    manager_id: *const c_char,
    source_manager_id: *const c_char,
    package_name: *const c_char,
    issue_code: *const c_char,
    option_id: *const c_char,
) -> i64 {
    clear_last_error_key();
    if manager_id.is_null()
        || source_manager_id.is_null()
        || package_name.is_null()
        || issue_code.is_null()
        || option_id.is_null()
    {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let manager = {
        let value = unsafe { CStr::from_ptr(manager_id) };
        match value
            .to_str()
            .ok()
            .and_then(|raw| raw.parse::<ManagerId>().ok())
        {
            Some(manager) => manager,
            None => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
        }
    };
    let source_manager = {
        let value = unsafe { CStr::from_ptr(source_manager_id) };
        match value
            .to_str()
            .ok()
            .and_then(|raw| raw.parse::<ManagerId>().ok())
        {
            Some(manager) => manager,
            None => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
        }
    };
    let package_name = {
        let value = unsafe { CStr::from_ptr(package_name) };
        match value.to_str() {
            Ok(raw) => raw.trim().to_string(),
            _ => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
        }
    };
    let issue_code = {
        let value = unsafe { CStr::from_ptr(issue_code) };
        match value.to_str() {
            Ok(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
            _ => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
        }
    };
    let option_id = {
        let value = unsafe { CStr::from_ptr(option_id) };
        match value.to_str() {
            Ok(raw) if !raw.trim().is_empty() => raw.trim().to_string(),
            _ => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
        }
    };

    let plan = match helm_core::repair::plan_for_issue(
        manager,
        source_manager,
        package_name.as_str(),
        issue_code.as_str(),
    ) {
        Some(plan) => plan,
        None => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };
    let option = match helm_core::repair::resolve_option(&plan, option_id.as_str()) {
        Some(option) => option,
        None => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    match option.action {
        helm_core::repair::RepairAction::ReinstallManagerViaHomebrew => {
            let manager_c = match CString::new(manager.as_str()) {
                Ok(value) => value,
                Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
            };
            let options_c = match CString::new("{\"installMethodOverride\":\"homebrew\"}") {
                Ok(value) => value,
                Err(_) => return return_error_i64(SERVICE_ERROR_INTERNAL),
            };
            unsafe { helm_install_manager_with_options(manager_c.as_ptr(), options_c.as_ptr()) }
        }
        helm_core::repair::RepairAction::RemoveStalePackageEntry => {
            if package_name.trim().is_empty() {
                return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
            }
            let source_c = match CString::new(source_manager.as_str()) {
                Ok(value) => value,
                Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
            };
            let package_c = match CString::new(package_name) {
                Ok(value) => value,
                Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
            };
            unsafe { helm_uninstall_package(source_c.as_ptr(), package_c.as_ptr()) }
        }
        helm_core::repair::RepairAction::ApplyPostInstallSetupDefaults => {
            let (store, runtime, rt_handle) = {
                let guard = lock_or_recover(&STATE, "state");
                let state = match guard.as_ref() {
                    Some(s) => s,
                    None => return return_error_i64(SERVICE_ERROR_INTERNAL),
                };
                (
                    state.store.clone(),
                    state.runtime.clone(),
                    state.rt_handle.clone(),
                )
            };
            if !manager_supports_post_install_setup(manager) {
                return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
            }
            match spawn_post_install_setup_task(store, runtime, rt_handle, manager, None) {
                Ok(task_id) => task_id.0 as i64,
                Err(error_key) => return return_error_i64(error_key),
            }
        }
    }
}

/// Install a manager tool. Returns the task ID, or -1 on error.
///
/// Supported manager IDs:
/// - "mise" -> script installer (default), Homebrew, MacPorts, or cargo install
/// - "asdf" -> script installer (default) or Homebrew
/// - "mas" -> Homebrew
/// - "rustup" -> rustup-init (default) or Homebrew, based on selected install method
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_install_manager(manager_id: *const c_char) -> i64 {
    unsafe { helm_install_manager_with_options(manager_id, std::ptr::null()) }
}

/// Install a manager tool with optional JSON options. Returns the task ID, or -1 on error.
///
/// Supported manager IDs:
/// - "mise" -> script installer (default), Homebrew, MacPorts, or cargo install
/// - "asdf" -> script installer (default) or Homebrew
/// - "mas" -> Homebrew
/// - "rustup" -> rustup-init (default) or Homebrew, based on selected install method
///
/// Supported options (method-specific):
/// - `installMethodOverride`: one-off method id (e.g. `homebrew`) without mutating saved manager preference
/// - `rustupInstallSource`: `officialDownload` (default) or `existingBinaryPath`
/// - `rustupBinaryPath`: absolute path used when `rustupInstallSource=existingBinaryPath`
/// - `miseInstallSource`: `officialDownload` (default) or `existingBinaryPath`
/// - `miseBinaryPath`: absolute path used when `miseInstallSource=existingBinaryPath`
/// - `completePostInstallSetupAutomatically`: automatically apply recommended setup defaults
///   after install succeeds for managers that support post-install setup (`rustup`, `mise`,
///   `asdf`)
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
/// `options_json` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_install_manager_with_options(
    manager_id: *const c_char,
    options_json: *const c_char,
) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(manager) => manager,
        Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let install_options = match parse_install_options_payload(options_json) {
        Ok(options) => options,
        Err(error_key) => return return_error_i64(error_key),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    let selected_method = manager_selected_install_method(store.as_ref(), manager);
    let install_plan = match helm_core::manager_lifecycle::plan_manager_install(
        manager,
        selected_method.as_deref(),
        &install_options,
    ) {
        Ok(plan) => plan,
        Err(helm_core::manager_lifecycle::ManagerInstallPlanError::UnsupportedMethod)
            if manager == ManagerId::Rustup && selected_method.is_some() =>
        {
            eprintln!(
                "helm-ffi: unrecognized rustup install method '{}' - falling back to rustupInstaller",
                selected_method.as_deref().unwrap_or("unknown")
            );
            match helm_core::manager_lifecycle::plan_manager_install(
                manager,
                Some("rustupInstaller"),
                &install_options,
            ) {
                Ok(plan) => plan,
                Err(error) => return return_error_i64(manager_install_plan_error_key(error)),
            }
        }
        Err(error) => return return_error_i64(manager_install_plan_error_key(error)),
    };
    let target_manager = install_plan.target_manager;
    let request = install_plan.request;
    let label_key = install_plan.label_key;
    let label_args = install_plan.label_args;

    if target_manager == ManagerId::HomebrewFormula && !homebrew_dependency_available(&store) {
        return return_error_i64("service.error.homebrew_required");
    }

    if external_coordinator_state_dir().is_some() {
        let submit_request = match adapter_request_to_coordinator_submit(request.clone()) {
            Ok(request) => request,
            Err(_) => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
        };
        return match coordinator_submit_external(target_manager, submit_request, false) {
            Ok(response) => response
                .task_id
                .map(|task_id| task_id as i64)
                .unwrap_or_else(|| return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)),
            Err(_) => return_error_i64(SERVICE_ERROR_PROCESS_FAILURE),
        };
    }

    if let Some(existing) = find_matching_inflight_task(
        store.as_ref(),
        target_manager,
        TaskType::Install,
        Some(label_key),
        label_args.as_slice(),
    ) {
        return existing.0 as i64;
    }

    match rt_handle.block_on(runtime.submit(target_manager, request)) {
        Ok(task_id) => {
            set_task_label(task_id, label_key, label_args.as_slice());
            if install_options.complete_post_install_setup_automatically
                && manager_supports_post_install_setup(manager)
            {
                if let Err(error_key) = spawn_post_install_setup_task(
                    store.clone(),
                    runtime.clone(),
                    rt_handle.clone(),
                    manager,
                    Some(task_id),
                ) {
                    eprintln!(
                        "helm-ffi: failed to queue post-install setup task for '{}': {}",
                        manager.as_str(),
                        error_key
                    );
                }
            }
            task_id.0 as i64
        }
        Err(e) => {
            eprintln!("Failed to install manager {}: {}", id_str, e);
            return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)
        }
    }
}

/// Update a manager tool. Returns the task ID, or -1 on error.
///
/// Supported manager IDs:
/// - "homebrew_formula" -> `brew update`
/// - "rustup" -> provenance-driven (`brew upgrade rustup` or `rustup self update`)
/// - Homebrew one-to-one managers -> provenance-driven (`asdf`, `mise`, `mas`, `pnpm`,
///   `yarn`, `pipx`, `poetry`, `cargo-binstall`, `podman`, `colima`)
/// - Homebrew parent-formula managers -> provenance-driven (`npm`, `pip`, `rubygems`,
///   `bundler`, `cargo`) when active install-instance formula ownership can be resolved.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_update_manager(manager_id: *const c_char) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(manager) => manager,
        Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    let homebrew_upgrade_target = |package_name: &str| {
        let policy = effective_homebrew_keg_policy(store.as_ref(), package_name);
        let cleanup_old_kegs = policy == HomebrewKegPolicy::Cleanup;
        let target_name = encode_homebrew_upgrade_target(package_name, cleanup_old_kegs);
        let label_key = if cleanup_old_kegs {
            "service.task.label.update.homebrew_formula_cleanup"
        } else {
            "service.task.label.update.homebrew_formula"
        };
        (target_name, label_key)
    };

    let active_instance = match active_manager_install_instance(store.as_ref(), manager) {
        Ok(instance) => instance,
        Err(error_key) => return return_error_i64(error_key),
    };
    let update_plan = match helm_core::manager_lifecycle::plan_manager_update(
        manager,
        active_instance.as_ref(),
    ) {
        Ok(plan) => plan,
        Err(error) => return return_error_i64(manager_update_plan_error_key(error)),
    };

    let (target_manager, request, label_key, label_args): (
        ManagerId,
        AdapterRequest,
        &str,
        Vec<(&str, String)>,
    ) = match &update_plan.target {
        helm_core::manager_lifecycle::ManagerUpdateTarget::ManagerSelf => {
            let request =
                match helm_core::manager_lifecycle::build_update_request(&update_plan, None) {
                    Some(request) => request,
                    None => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
                };
            let label_key = if update_plan.target_manager == ManagerId::HomebrewFormula {
                "service.task.label.update.homebrew_self"
            } else {
                "service.task.label.update.rustup_self"
            };
            (update_plan.target_manager, request, label_key, Vec::new())
        }
        helm_core::manager_lifecycle::ManagerUpdateTarget::HomebrewFormula { formula_name } => {
            let (target_name, label_key) = homebrew_upgrade_target(formula_name.as_str());
            let request = match helm_core::manager_lifecycle::build_update_request(
                &update_plan,
                Some(target_name),
            ) {
                Some(request) => request,
                None => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
            };
            (
                update_plan.target_manager,
                request,
                label_key,
                vec![("package", formula_name.clone())],
            )
        }
    };

    if external_coordinator_state_dir().is_some() {
        let submit_request = match adapter_request_to_coordinator_submit(request.clone()) {
            Ok(request) => request,
            Err(_) => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
        };
        return match coordinator_submit_external(target_manager, submit_request, false) {
            Ok(response) => response
                .task_id
                .map(|task_id| task_id as i64)
                .unwrap_or_else(|| return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)),
            Err(_) => return_error_i64(SERVICE_ERROR_PROCESS_FAILURE),
        };
    }

    if let Some(existing) = find_matching_inflight_task(
        store.as_ref(),
        target_manager,
        TaskType::Upgrade,
        Some(label_key),
        &label_args,
    ) {
        return existing.0 as i64;
    }

    match rt_handle.block_on(runtime.submit(target_manager, request)) {
        Ok(task_id) => {
            set_task_label(task_id, label_key, &label_args);
            task_id.0 as i64
        }
        Err(e) => {
            eprintln!("Failed to update manager {}: {}", id_str, e);
            return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)
        }
    }
}

/// Uninstall a manager tool. Returns the task ID, or -1 on error.
///
/// Supported manager IDs include rustup and Homebrew-routed manager adapters where
/// provenance strategy is supported.
///
/// This is a strict compatibility wrapper over `helm_uninstall_manager_with_options` with
/// `allow_unknown_provenance=false`.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_uninstall_manager(manager_id: *const c_char) -> i64 {
    unsafe { helm_uninstall_manager_with_options(manager_id, false) }
}

/// Preview manager uninstall blast radius and strategy as JSON.
///
/// `allow_unknown_provenance` controls whether unknown-provenance routing uses override mode.
/// For preview-only UI flows, callers typically pass `false` and rely on `unknown_override_required`
/// in the JSON response to gate destructive execution.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_preview_manager_uninstall(
    manager_id: *const c_char,
    allow_unknown_provenance: bool,
) -> *mut c_char {
    let uninstall_options = helm_core::manager_lifecycle::ManagerUninstallOptions::default();
    unsafe {
        helm_preview_manager_uninstall_internal(
            manager_id,
            allow_unknown_provenance,
            &uninstall_options,
        )
    }
}

/// Preview manager uninstall blast radius and strategy as JSON with structured options.
///
/// `options_json` supports:
/// - `allowUnknownProvenance` (bool)
/// - `homebrewCleanupMode` (`managerOnly` | `fullCleanup`)
/// - `miseCleanupMode` (`managerOnly` | `fullCleanup`)
/// - `miseConfigRemoval` (`keepConfig` | `removeConfig`)
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
/// `options_json` must be null or a valid pointer to a NUL-terminated UTF-8 JSON string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_preview_manager_uninstall_with_options(
    manager_id: *const c_char,
    options_json: *const c_char,
) -> *mut c_char {
    let (allow_unknown_provenance, uninstall_options) =
        match parse_uninstall_options_payload(options_json, false) {
            Ok(parsed) => parsed,
            Err(error_key) => {
                set_last_error_key(error_key);
                return std::ptr::null_mut();
            }
        };
    unsafe {
        helm_preview_manager_uninstall_internal(
            manager_id,
            allow_unknown_provenance,
            &uninstall_options,
        )
    }
}

unsafe fn helm_preview_manager_uninstall_internal(
    manager_id: *const c_char,
    allow_unknown_provenance: bool,
    uninstall_options: &helm_core::manager_lifecycle::ManagerUninstallOptions,
) -> *mut c_char {
    clear_last_error_key();
    if manager_id.is_null() {
        set_last_error_key(SERVICE_ERROR_INVALID_INPUT);
        return std::ptr::null_mut();
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error_key(SERVICE_ERROR_INVALID_INPUT);
            return std::ptr::null_mut();
        }
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(manager) => manager,
        Err(_) => {
            set_last_error_key(SERVICE_ERROR_INVALID_INPUT);
            return std::ptr::null_mut();
        }
    };

    let store = {
        let guard = lock_or_recover(&STATE, "state");
        match guard.as_ref() {
            Some(state) => state.store.clone(),
            None => {
                set_last_error_key(SERVICE_ERROR_INTERNAL);
                return std::ptr::null_mut();
            }
        }
    };

    let plan = match build_manager_uninstall_plan_with_options(
        store.as_ref(),
        manager,
        allow_unknown_provenance,
        true,
        uninstall_options,
    ) {
        Ok(plan) => plan,
        Err(error_key) => {
            set_last_error_key(error_key);
            return std::ptr::null_mut();
        }
    };

    let json = match serde_json::to_string(&plan.preview) {
        Ok(json) => json,
        Err(error) => {
            eprintln!("preview_manager_uninstall: failed to encode JSON: {error}");
            return std::ptr::null_mut();
        }
    };

    match CString::new(json) {
        Ok(c_string) => c_string.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Uninstall a manager tool. Returns the task ID, or -1 on error.
///
/// Supported manager IDs include rustup and Homebrew-routed manager adapters where
/// provenance strategy is supported.
///
/// `allow_unknown_provenance` enables explicit override for ambiguous manager provenance where
/// uninstall routing supports override-based fallback.
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_uninstall_manager_with_options(
    manager_id: *const c_char,
    allow_unknown_provenance: bool,
) -> i64 {
    let uninstall_options = helm_core::manager_lifecycle::ManagerUninstallOptions::default();
    unsafe {
        helm_uninstall_manager_with_options_internal(
            manager_id,
            allow_unknown_provenance,
            &uninstall_options,
        )
    }
}

/// Uninstall a manager tool with structured options. Returns the task ID, or -1 on error.
///
/// `options_json` supports:
/// - `allowUnknownProvenance` (bool)
/// - `homebrewCleanupMode` (`managerOnly` | `fullCleanup`)
/// - `miseCleanupMode` (`managerOnly` | `fullCleanup`)
/// - `miseConfigRemoval` (`keepConfig` | `removeConfig`)
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
/// `options_json` must be null or a valid pointer to a NUL-terminated UTF-8 JSON string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_uninstall_manager_with_uninstall_options(
    manager_id: *const c_char,
    options_json: *const c_char,
) -> i64 {
    let (allow_unknown_provenance, uninstall_options) =
        match parse_uninstall_options_payload(options_json, false) {
            Ok(parsed) => parsed,
            Err(error_key) => return return_error_i64(error_key),
        };
    unsafe {
        helm_uninstall_manager_with_options_internal(
            manager_id,
            allow_unknown_provenance,
            &uninstall_options,
        )
    }
}

unsafe fn helm_uninstall_manager_with_options_internal(
    manager_id: *const c_char,
    allow_unknown_provenance: bool,
    uninstall_options: &helm_core::manager_lifecycle::ManagerUninstallOptions,
) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_i64(SERVICE_ERROR_INVALID_INPUT);
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(manager) => manager,
        Err(_) => return return_error_i64(SERVICE_ERROR_INVALID_INPUT),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64(SERVICE_ERROR_INTERNAL),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    let plan = match build_manager_uninstall_plan_with_options(
        store.as_ref(),
        manager,
        allow_unknown_provenance,
        false,
        uninstall_options,
    ) {
        Ok(plan) => plan,
        Err(error_key) => return return_error_i64(error_key),
    };

    if external_coordinator_state_dir().is_some() {
        let submit_request = match adapter_request_to_coordinator_submit(plan.request.clone()) {
            Ok(request) => request,
            Err(_) => return return_error_i64(SERVICE_ERROR_UNSUPPORTED_CAPABILITY),
        };
        return match coordinator_submit_external(plan.target_manager, submit_request, false) {
            Ok(response) => response
                .task_id
                .map(|task_id| task_id as i64)
                .unwrap_or_else(|| return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)),
            Err(_) => return_error_i64(SERVICE_ERROR_PROCESS_FAILURE),
        };
    }

    if let Some(existing) = find_matching_inflight_task(
        store.as_ref(),
        plan.target_manager,
        TaskType::Uninstall,
        Some(plan.label_key),
        &plan.label_args,
    ) {
        return existing.0 as i64;
    }

    match rt_handle.block_on(runtime.submit(plan.target_manager, plan.request)) {
        Ok(task_id) => {
            set_task_label(task_id, plan.label_key, &plan.label_args);
            task_id.0 as i64
        }
        Err(e) => {
            eprintln!("Failed to uninstall manager {}: {}", id_str, e);
            return_error_i64(SERVICE_ERROR_PROCESS_FAILURE)
        }
    }
}

/// Reset the database by rolling back all migrations and re-applying them.
/// Returns true on success.
#[unsafe(no_mangle)]
pub extern "C" fn helm_reset_database() -> bool {
    clear_last_error_key();
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return return_error_bool(SERVICE_ERROR_INTERNAL),
    };

    // Roll back to version 0 (drops all data tables)
    if let Err(e) = state.store.apply_migration(0) {
        eprintln!("Failed to roll back migrations: {}", e);
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }

    // Re-apply all migrations (recreates empty tables)
    if let Err(e) = state.store.migrate_to_latest() {
        eprintln!("Failed to re-apply migrations: {}", e);
        return return_error_bool(SERVICE_ERROR_STORAGE_FAILURE);
    }

    // Final cleanup: delete any task records that in-flight persistence
    // watchers may have re-inserted during the brief reset window.
    let _ = state.store.delete_all_tasks();
    clear_manager_selected_executables();

    true
}

/// Return and clear the most recent service error localization key.
#[unsafe(no_mangle)]
pub extern "C" fn helm_take_last_error_key() -> *mut c_char {
    let key = lock_or_recover(&LAST_ERROR_KEY, "last_error_key").take();
    let Some(key) = key else {
        return std::ptr::null_mut();
    };

    match CString::new(key) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a string previously returned by a `helm_*` function.
///
/// # Safety
///
/// `s` must be a pointer previously returned by a `helm_*` function, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_free_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(s);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FfiUpgradePlanStep, SERVICE_ERROR_UNSUPPORTED_CAPABILITY, build_manager_statuses,
        build_manager_uninstall_plan, build_manager_uninstall_preview, build_visible_tasks,
        collect_upgrade_all_targets, homebrew_probe_candidates,
        manager_allows_individual_package_install, manager_authority_key,
        manager_participates_in_package_search, manager_uninstall_label_for_route,
        parse_homebrew_config_version, push_upgrade_plan_step,
        resolve_homebrew_manager_update_strategy, resolve_rustup_uninstall_strategy,
        search_label_args, search_label_key_for_query, upgrade_plan_step_id,
        upgrade_reason_label_for, upgrade_task_label_for,
    };
    use helm_core::adapters::{AdapterRequest, ManagerAdapter, UninstallRequest};
    use helm_core::manager_policy::{
        PIP_SYSTEM_UNMANAGED_REASON_CODE, RUBYGEMS_SYSTEM_UNMANAGED_REASON_CODE,
    };
    use helm_core::models::{
        AutomationLevel, DetectionInfo, InstallProvenance, InstalledPackage, ManagerId,
        ManagerInstallInstance, OutdatedPackage, PackageRef, StrategyKind, TaskId, TaskLogRecord,
        TaskRecord, TaskStatus, TaskType,
    };
    use helm_core::orchestration::adapter_runtime::AdapterRuntime;
    use helm_core::persistence::{DetectionStore, ManagerPreference, PackageStore, TaskStore};
    use helm_core::sqlite::SqliteStore;
    use helm_core::uninstall_preview::{
        DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD, ManagerUninstallPreviewContext,
    };
    use std::collections::HashMap;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::Path;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[cfg(unix)]
    fn unix_mode(path: &Path) -> u32 {
        fs::metadata(path)
            .expect("expected path metadata")
            .permissions()
            .mode()
            & 0o777
    }

    #[cfg(unix)]
    fn unix_uid(path: &Path) -> u32 {
        fs::metadata(path).expect("expected path metadata").uid()
    }

    fn sample_rustup_install_instance(
        uninstall_strategy: StrategyKind,
        provenance: InstallProvenance,
        display_path: &str,
    ) -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "rustup-instance".to_string(),
            identity_kind: helm_core::models::InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "rustup-instance".to_string(),
            display_path: std::path::PathBuf::from(display_path),
            canonical_path: Some(std::path::PathBuf::from(display_path)),
            alias_paths: Vec::new(),
            is_active: true,
            version: Some("1.0.0".to_string()),
            provenance,
            confidence: 0.55,
            decision_margin: Some(0.08),
            automation_level: AutomationLevel::NeedsConfirmation,
            uninstall_strategy,
            update_strategy: StrategyKind::RustupSelf,
            remediation_strategy: StrategyKind::ManualRemediation,
            explanation_primary: Some("test primary explanation".to_string()),
            explanation_secondary: Some("test secondary explanation".to_string()),
            competing_provenance: None,
            competing_confidence: None,
        }
    }

    fn sample_manager_install_instance(
        manager: ManagerId,
        uninstall_strategy: StrategyKind,
        update_strategy: StrategyKind,
        provenance: InstallProvenance,
        display_path: &str,
        canonical_path: &str,
    ) -> ManagerInstallInstance {
        let mut instance =
            sample_rustup_install_instance(uninstall_strategy, provenance, display_path);
        instance.manager = manager;
        instance.instance_id = format!("{}-instance", manager.as_str());
        instance.identity_value = instance.instance_id.clone();
        instance.display_path = std::path::PathBuf::from(display_path);
        instance.canonical_path = Some(std::path::PathBuf::from(canonical_path));
        instance.alias_paths = vec![std::path::PathBuf::from(display_path)];
        instance.update_strategy = update_strategy;
        instance
    }

    fn sample_installed_package(
        manager: ManagerId,
        name: &str,
        installed_version: Option<&str>,
    ) -> InstalledPackage {
        InstalledPackage {
            package: PackageRef {
                manager,
                name: name.to_string(),
            },
            installed_version: installed_version.map(str::to_string),
            pinned: false,
        }
    }

    fn temp_sqlite_store(name: &str) -> SqliteStore {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("helm-ffi-{name}-{nanos}.db"));
        SqliteStore::new(path)
    }

    #[test]
    fn package_search_excludes_rustup_manager() {
        assert!(!manager_participates_in_package_search(ManagerId::Rustup));
        assert!(manager_participates_in_package_search(
            ManagerId::HomebrewFormula
        ));
    }

    #[test]
    fn homebrew_uninstall_label_decodes_internal_cleanup_marker() {
        let encoded = helm_core::manager_lifecycle::encode_homebrew_manager_uninstall_package_name(
            "rustup",
            ManagerId::Rustup,
            helm_core::manager_lifecycle::HomebrewUninstallCleanupMode::FullCleanup,
        );
        let request = AdapterRequest::Uninstall(UninstallRequest {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: encoded,
            },
        });

        let (label_key, label_args) = manager_uninstall_label_for_route(
            ManagerId::Rustup,
            ManagerId::HomebrewFormula,
            &request,
            StrategyKind::HomebrewFormula,
        );

        assert_eq!(label_key, "service.task.label.uninstall.homebrew_formula");
        assert_eq!(label_args, vec![("package", "rustup".to_string())]);
    }

    #[test]
    fn manager_self_heal_does_not_cancel_tasks_for_not_installed_detection() {
        let store = temp_sqlite_store("self-heal-no-task-purge");
        store
            .migrate_to_latest()
            .expect("sqlite migrations should apply");
        store
            .upsert_detection(
                ManagerId::Rustup,
                &DetectionInfo {
                    installed: false,
                    executable_path: None,
                    version: None,
                },
            )
            .expect("rustup detection upsert should succeed");
        store
            .create_task(&TaskRecord {
                id: TaskId(83),
                manager: ManagerId::Rustup,
                task_type: TaskType::Install,
                status: TaskStatus::Queued,
                created_at: SystemTime::now(),
            })
            .expect("task insert should succeed");

        let runtime = AdapterRuntime::new(Vec::<Arc<dyn ManagerAdapter>>::new())
            .expect("empty adapter runtime should initialize");
        let tokio_runtime =
            tokio::runtime::Runtime::new().expect("tokio runtime should initialize");

        let detection_map = store
            .list_detections()
            .expect("detection listing should succeed")
            .into_iter()
            .collect::<HashMap<ManagerId, DetectionInfo>>();
        let mut pref_map = store
            .list_manager_preferences()
            .expect("manager preferences listing should succeed")
            .into_iter()
            .map(|preference| (preference.manager, preference))
            .collect::<HashMap<ManagerId, ManagerPreference>>();

        super::apply_manager_enablement_self_heal(
            &store,
            &runtime,
            tokio_runtime.handle(),
            &detection_map,
            &mut pref_map,
        );

        let tasks = store
            .list_recent_tasks(10)
            .expect("task listing should succeed");
        assert!(
            tasks.iter().any(|task| task.id == TaskId(83)),
            "self-heal should not purge queued manager task history when detection reports not installed"
        );
    }

    #[test]
    fn rustup_unknown_strategy_requires_override_for_execute_but_not_preview() {
        let instance = sample_rustup_install_instance(
            StrategyKind::InteractivePrompt,
            InstallProvenance::Unknown,
            "/Users/test/.cargo/bin/rustup",
        );

        let execute_result = resolve_rustup_uninstall_strategy(Some(&instance), false, false);
        assert!(matches!(
            execute_result,
            Err(SERVICE_ERROR_UNSUPPORTED_CAPABILITY)
        ));

        let preview_result = resolve_rustup_uninstall_strategy(Some(&instance), false, true)
            .expect("preview path should resolve fallback strategy");
        assert_eq!(preview_result.strategy, StrategyKind::RustupSelf);
        assert!(preview_result.unknown_override_required);
        assert!(!preview_result.used_unknown_override);
    }

    #[test]
    fn rustup_unknown_strategy_marks_override_when_allowed() {
        let instance = sample_rustup_install_instance(
            StrategyKind::InteractivePrompt,
            InstallProvenance::Unknown,
            "/Users/test/.cargo/bin/rustup",
        );

        let result = resolve_rustup_uninstall_strategy(Some(&instance), true, false)
            .expect("explicit override should resolve strategy");
        assert_eq!(result.strategy, StrategyKind::RustupSelf);
        assert!(result.unknown_override_required);
        assert!(result.used_unknown_override);
    }

    #[test]
    fn rustup_update_strategy_defaults_to_rustup_self_without_instance() {
        let strategy = super::resolve_rustup_update_strategy(None)
            .expect("missing instance should default to rustup self-update");
        assert_eq!(strategy, StrategyKind::RustupSelf);
    }

    #[test]
    fn rustup_update_strategy_uses_homebrew_for_homebrew_provenance() {
        let mut instance = sample_rustup_install_instance(
            StrategyKind::HomebrewFormula,
            InstallProvenance::Homebrew,
            "/opt/homebrew/bin/rustup",
        );
        instance.update_strategy = StrategyKind::HomebrewFormula;

        let strategy = super::resolve_rustup_update_strategy(Some(&instance))
            .expect("homebrew strategy should be accepted");
        assert_eq!(strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn rustup_update_strategy_blocks_ambiguous_provenance() {
        let mut instance = sample_rustup_install_instance(
            StrategyKind::InteractivePrompt,
            InstallProvenance::Unknown,
            "/Users/test/.cargo/bin/rustup",
        );
        instance.update_strategy = StrategyKind::InteractivePrompt;

        let error = super::resolve_rustup_update_strategy(Some(&instance))
            .expect_err("ambiguous update strategy should be blocked");
        assert_eq!(error, SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }

    #[test]
    fn homebrew_manager_update_strategy_defaults_to_homebrew_without_instance() {
        let strategy = resolve_homebrew_manager_update_strategy(None)
            .expect("missing instance should default to homebrew strategy");
        assert_eq!(strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn homebrew_manager_update_strategy_blocks_ambiguous_provenance() {
        let instance = sample_manager_install_instance(
            ManagerId::Pnpm,
            StrategyKind::InteractivePrompt,
            StrategyKind::InteractivePrompt,
            InstallProvenance::Unknown,
            "/usr/local/bin/pnpm",
            "/usr/local/bin/pnpm",
        );

        let error = resolve_homebrew_manager_update_strategy(Some(&instance))
            .expect_err("ambiguous update strategy should be blocked");
        assert_eq!(error, SERVICE_ERROR_UNSUPPORTED_CAPABILITY);
    }

    #[test]
    fn homebrew_one_to_one_manager_uninstall_plan_routes_by_provenance_strategy() {
        let store = temp_sqlite_store("ffi-uninstall-pnpm-homebrew");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let instance = sample_manager_install_instance(
            ManagerId::Pnpm,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
            InstallProvenance::Homebrew,
            "/opt/homebrew/bin/pnpm",
            "/opt/homebrew/Cellar/pnpm/9.0.0/bin/pnpm",
        );
        store
            .replace_install_instances(ManagerId::Pnpm, &[instance])
            .expect("install instances should persist");

        let plan = build_manager_uninstall_plan(&store, ManagerId::Pnpm, false, true)
            .expect("preview uninstall plan should resolve");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        assert_eq!(plan.preview.target_manager_id, "homebrew_formula");
        assert_eq!(plan.preview.package_name, "pnpm");
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.unknown_override_required);
        assert!(!plan.preview.used_unknown_override);
        match &plan.request {
            AdapterRequest::Uninstall(request) => {
                assert_eq!(request.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(request.package.name, "pnpm");
            }
            _ => panic!("expected uninstall request"),
        }

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn homebrew_parent_formula_manager_uninstall_plan_derives_formula_from_instance_path() {
        let store = temp_sqlite_store("ffi-uninstall-npm-parent-formula");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let instance = sample_manager_install_instance(
            ManagerId::Npm,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
            InstallProvenance::Homebrew,
            "/opt/homebrew/bin/npm",
            "/opt/homebrew/Cellar/node/22.1.0/bin/npm",
        );
        store
            .replace_install_instances(ManagerId::Npm, &[instance])
            .expect("install instances should persist");

        let plan = build_manager_uninstall_plan(&store, ManagerId::Npm, false, true)
            .expect("preview uninstall plan should resolve");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        assert_eq!(plan.preview.package_name, "node");
        match &plan.request {
            AdapterRequest::Uninstall(request) => {
                assert_eq!(request.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(request.package.name, "node");
            }
            _ => panic!("expected uninstall request"),
        }

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn homebrew_one_to_one_manager_uninstall_plan_routes_supported_manager_set() {
        let store = temp_sqlite_store("ffi-uninstall-one-to-one-set");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let cases = [
            (
                ManagerId::Asdf,
                "asdf",
                "/opt/homebrew/bin/asdf",
                "/opt/homebrew/Cellar/asdf/0.15.0/bin/asdf",
            ),
            (
                ManagerId::Mise,
                "mise",
                "/opt/homebrew/bin/mise",
                "/opt/homebrew/Cellar/mise/2024.11.6/bin/mise",
            ),
            (
                ManagerId::Mas,
                "mas",
                "/opt/homebrew/bin/mas",
                "/opt/homebrew/Cellar/mas/1.8.7/bin/mas",
            ),
            (
                ManagerId::Yarn,
                "yarn",
                "/opt/homebrew/bin/yarn",
                "/opt/homebrew/Cellar/yarn/1.22.0/bin/yarn",
            ),
            (
                ManagerId::Pipx,
                "pipx",
                "/opt/homebrew/bin/pipx",
                "/opt/homebrew/Cellar/pipx/1.7.0/bin/pipx",
            ),
            (
                ManagerId::Poetry,
                "poetry",
                "/opt/homebrew/bin/poetry",
                "/opt/homebrew/Cellar/poetry/1.8.4/bin/poetry",
            ),
            (
                ManagerId::CargoBinstall,
                "cargo-binstall",
                "/opt/homebrew/bin/cargo-binstall",
                "/opt/homebrew/Cellar/cargo-binstall/1.13.0/bin/cargo-binstall",
            ),
            (
                ManagerId::Podman,
                "podman",
                "/opt/homebrew/bin/podman",
                "/opt/homebrew/Cellar/podman/5.0.0/bin/podman",
            ),
            (
                ManagerId::Colima,
                "colima",
                "/opt/homebrew/bin/colima",
                "/opt/homebrew/Cellar/colima/0.7.0/bin/colima",
            ),
        ];

        for (manager, expected_formula, display_path, canonical_path) in cases {
            let instance = sample_manager_install_instance(
                manager,
                StrategyKind::HomebrewFormula,
                StrategyKind::HomebrewFormula,
                InstallProvenance::Homebrew,
                display_path,
                canonical_path,
            );
            store
                .replace_install_instances(manager, &[instance])
                .expect("install instances should persist");

            let plan = build_manager_uninstall_plan(&store, manager, false, true)
                .expect("preview uninstall plan should resolve");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            assert_eq!(plan.preview.package_name, expected_formula);
            match &plan.request {
                AdapterRequest::Uninstall(request) => {
                    assert_eq!(request.package.manager, ManagerId::HomebrewFormula);
                    assert_eq!(request.package.name, expected_formula);
                }
                _ => panic!("expected uninstall request"),
            }
        }

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn homebrew_parent_formula_manager_uninstall_plan_routes_supported_manager_set() {
        let store = temp_sqlite_store("ffi-uninstall-parent-formula-set");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let cases = [
            (
                ManagerId::Npm,
                "node",
                "/opt/homebrew/bin/npm",
                "/opt/homebrew/Cellar/node/22.1.0/bin/npm",
            ),
            (
                ManagerId::Pip,
                "python@3.12",
                "/opt/homebrew/bin/pip3",
                "/opt/homebrew/Cellar/python@3.12/3.12.2/bin/pip3",
            ),
            (
                ManagerId::RubyGems,
                "ruby",
                "/opt/homebrew/bin/gem",
                "/opt/homebrew/Cellar/ruby/3.4.0/bin/gem",
            ),
            (
                ManagerId::Bundler,
                "ruby",
                "/opt/homebrew/bin/bundle",
                "/opt/homebrew/Cellar/ruby/3.4.0/bin/bundle",
            ),
            (
                ManagerId::Cargo,
                "rust",
                "/opt/homebrew/bin/cargo",
                "/opt/homebrew/Cellar/rust/1.84.0/bin/cargo",
            ),
        ];

        for (manager, expected_formula, display_path, canonical_path) in cases {
            let instance = sample_manager_install_instance(
                manager,
                StrategyKind::HomebrewFormula,
                StrategyKind::HomebrewFormula,
                InstallProvenance::Homebrew,
                display_path,
                canonical_path,
            );
            store
                .replace_install_instances(manager, &[instance])
                .expect("install instances should persist");

            let plan = build_manager_uninstall_plan(&store, manager, false, true)
                .expect("preview uninstall plan should resolve");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            assert_eq!(plan.preview.package_name, expected_formula);
            match &plan.request {
                AdapterRequest::Uninstall(request) => {
                    assert_eq!(request.package.manager, ManagerId::HomebrewFormula);
                    assert_eq!(request.package.name, expected_formula);
                }
                _ => panic!("expected uninstall request"),
            }
        }

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn homebrew_parent_formula_manager_uninstall_plan_blocks_when_formula_unresolved() {
        let store = temp_sqlite_store("ffi-uninstall-parent-formula-unresolved");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let instance = sample_manager_install_instance(
            ManagerId::Npm,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
            InstallProvenance::Homebrew,
            "/usr/local/bin/npm",
            "/usr/local/bin/npm",
        );
        store
            .replace_install_instances(ManagerId::Npm, &[instance])
            .expect("install instances should persist");

        let error = build_manager_uninstall_plan(&store, ManagerId::Npm, false, true)
            .expect_err("unresolved parent formula should block");
        assert_eq!(error, SERVICE_ERROR_UNSUPPORTED_CAPABILITY);

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn homebrew_manager_uninstall_plan_blocks_ambiguous_without_override_but_allows_preview() {
        let store = temp_sqlite_store("ffi-uninstall-ambiguous-manager");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let instance = sample_manager_install_instance(
            ManagerId::Pnpm,
            StrategyKind::InteractivePrompt,
            StrategyKind::InteractivePrompt,
            InstallProvenance::Unknown,
            "/usr/local/bin/pnpm",
            "/usr/local/bin/pnpm",
        );
        store
            .replace_install_instances(ManagerId::Pnpm, &[instance])
            .expect("install instances should persist");

        let error = build_manager_uninstall_plan(&store, ManagerId::Pnpm, false, false)
            .expect_err("ambiguous uninstall should be blocked without override");
        assert_eq!(error, SERVICE_ERROR_UNSUPPORTED_CAPABILITY);

        let preview_plan = build_manager_uninstall_plan(&store, ManagerId::Pnpm, false, true)
            .expect("preview should resolve without override");
        assert!(preview_plan.preview.unknown_override_required);
        assert!(!preview_plan.preview.used_unknown_override);

        let override_preview_plan =
            build_manager_uninstall_plan(&store, ManagerId::Pnpm, true, true)
                .expect("override preview should resolve");
        assert!(override_preview_plan.preview.unknown_override_required);
        assert!(override_preview_plan.preview.used_unknown_override);

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn homebrew_manager_uninstall_plan_read_only_blocks_execution_and_allows_preview() {
        let store = temp_sqlite_store("ffi-uninstall-read-only-manager");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let instance = sample_manager_install_instance(
            ManagerId::Pipx,
            StrategyKind::ReadOnly,
            StrategyKind::ReadOnly,
            InstallProvenance::System,
            "/usr/bin/pipx",
            "/usr/bin/pipx",
        );
        store
            .replace_install_instances(ManagerId::Pipx, &[instance])
            .expect("install instances should persist");

        let error = build_manager_uninstall_plan(&store, ManagerId::Pipx, false, false)
            .expect_err("read-only uninstall execution should be blocked");
        assert_eq!(error, SERVICE_ERROR_UNSUPPORTED_CAPABILITY);

        let preview = build_manager_uninstall_plan(&store, ManagerId::Pipx, false, true)
            .expect("read-only preview should be available");
        assert!(preview.preview.read_only_blocked);
        assert_eq!(preview.preview.target_manager_id, "pipx");
        assert_eq!(preview.preview.strategy, "read_only");

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn manager_uninstall_preview_marks_read_only_blocked() {
        let store = temp_sqlite_store("preview-read-only");
        let instance = sample_rustup_install_instance(
            StrategyKind::ReadOnly,
            InstallProvenance::System,
            "/usr/bin/rustup",
        );
        let request = AdapterRequest::Uninstall(UninstallRequest {
            package: PackageRef {
                manager: ManagerId::Rustup,
                name: "__self__".to_string(),
            },
        });
        let preview = build_manager_uninstall_preview(
            &store,
            ManagerUninstallPreviewContext {
                requested_manager: ManagerId::Rustup,
                target_manager: ManagerId::Rustup,
                request: &request,
                strategy: StrategyKind::ReadOnly,
                active_instance: Some(&instance),
                unknown_override_required: false,
                used_unknown_override: false,
                legacy_fallback_used: false,
            },
            DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
        );

        assert!(preview.read_only_blocked);
        assert!(preview.requires_yes);
    }

    #[test]
    fn parses_homebrew_version_from_config_output() {
        let parsed = parse_homebrew_config_version(
            "HOMEBREW_VERSION: 5.0.14-52-g807be07\nORIGIN: https://github.com/Homebrew/brew\n",
        );
        assert_eq!(parsed.as_deref(), Some("5.0.14-52-g807be07"));
    }

    #[test]
    fn homebrew_probe_candidates_include_known_locations_without_duplicates() {
        let candidates = homebrew_probe_candidates(Some(Path::new("/usr/local/bin/brew")));
        let as_strings: Vec<String> = candidates
            .iter()
            .map(|candidate| candidate.to_string_lossy().to_string())
            .collect();

        assert!(as_strings.contains(&"/usr/local/bin/brew".to_string()));
        assert!(as_strings.contains(&"/opt/homebrew/bin/brew".to_string()));
        assert!(as_strings.contains(&"brew".to_string()));
        assert_eq!(
            as_strings
                .iter()
                .filter(|candidate| *candidate == "/usr/local/bin/brew")
                .count(),
            1
        );
    }

    #[test]
    fn collect_upgrade_all_targets_routes_supported_managers() {
        let outdated = vec![
            outdated_pkg(ManagerId::HomebrewFormula, "git", false),
            outdated_pkg(ManagerId::Mise, "node", false),
            outdated_pkg(ManagerId::Rustup, "stable-x86_64-apple-darwin", false),
            outdated_pkg(ManagerId::SoftwareUpdate, "macos", false),
        ];
        let pinned = std::collections::HashSet::new();

        let targets = collect_upgrade_all_targets(&outdated, &pinned, true);
        assert_eq!(targets.homebrew, vec!["git".to_string()]);
        assert_eq!(targets.mise, vec!["node".to_string()]);
        assert_eq!(
            targets.rustup,
            vec!["stable-x86_64-apple-darwin".to_string()]
        );
        assert!(targets.softwareupdate_outdated);
    }

    #[test]
    fn collect_upgrade_all_targets_excludes_pinned_and_deduplicates() {
        let outdated = vec![
            outdated_pkg(ManagerId::HomebrewFormula, "git", false),
            outdated_pkg(ManagerId::HomebrewFormula, "git", false),
            outdated_pkg(ManagerId::Mise, "node", true),
            outdated_pkg(ManagerId::Rustup, "stable-x86_64-apple-darwin", false),
        ];
        let pinned =
            std::collections::HashSet::from(["rustup:stable-x86_64-apple-darwin".to_string()]);

        let targets = collect_upgrade_all_targets(&outdated, &pinned, false);
        assert_eq!(targets.homebrew, vec!["git".to_string()]);
        assert!(targets.mise.is_empty());
        assert!(targets.rustup.is_empty());
        assert!(!targets.softwareupdate_outdated);
    }

    #[test]
    fn upgrade_reason_label_uses_manager_specific_keys() {
        let (homebrew_key, homebrew_args) =
            upgrade_reason_label_for(ManagerId::HomebrewFormula, "git", true);
        assert_eq!(homebrew_key, "service.task.label.upgrade.homebrew_cleanup");
        assert_eq!(homebrew_args, vec![("package", "git".to_string())]);

        let (rustup_key, rustup_args) =
            upgrade_reason_label_for(ManagerId::Rustup, "stable", false);
        assert_eq!(rustup_key, "service.task.label.upgrade.rustup_toolchain");
        assert_eq!(rustup_args, vec![("toolchain", "stable".to_string())]);
    }

    #[test]
    fn upgrade_task_label_includes_plan_step_id_arg() {
        let (label_key, label_args) = upgrade_task_label_for(ManagerId::Npm, "typescript", false);
        assert_eq!(label_key, "service.task.label.upgrade.package");
        assert!(label_args.contains(&("package", "typescript".to_string())));
        assert!(label_args.contains(&("manager", "npm".to_string())));
        assert!(label_args.contains(&(
            "plan_step_id",
            upgrade_plan_step_id(ManagerId::Npm, "typescript"),
        )));
    }

    #[test]
    fn push_upgrade_plan_step_assigns_stable_ids_and_order() {
        let mut steps: Vec<FfiUpgradePlanStep> = Vec::new();
        let mut order_index = 0_u64;

        push_upgrade_plan_step(
            &mut steps,
            ManagerId::Npm,
            "typescript".to_string(),
            false,
            &mut order_index,
        );
        push_upgrade_plan_step(
            &mut steps,
            ManagerId::SoftwareUpdate,
            "__confirm_os_updates__".to_string(),
            false,
            &mut order_index,
        );

        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].step_id, "npm:typescript");
        assert_eq!(steps[0].order_index, 0);
        assert_eq!(steps[0].status, "queued");
        assert_eq!(steps[0].authority, manager_authority_key(ManagerId::Npm));
        assert_eq!(steps[1].step_id, "softwareupdate:__confirm_os_updates__");
        assert_eq!(steps[1].order_index, 1);
    }

    #[test]
    fn manager_status_defaults_disable_optional_managers() {
        let statuses = build_manager_statuses(None, None, &HashMap::new(), &HashMap::new());

        assert!(!status_for(&statuses, ManagerId::Asdf).enabled);
        assert!(!status_for(&statuses, ManagerId::MacPorts).enabled);
        assert!(!status_for(&statuses, ManagerId::NixDarwin).enabled);
        assert!(status_for(&statuses, ManagerId::Mise).enabled);
    }

    #[test]
    fn manager_status_includes_core_install_method_metadata() {
        let statuses = build_manager_statuses(None, None, &HashMap::new(), &HashMap::new());
        let rustup = status_for(&statuses, ManagerId::Rustup);
        let methods = rustup
            .install_method_options
            .iter()
            .map(|method| method.method_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(methods, vec!["rustupInstaller", "homebrew"]);

        let rustup_init = rustup
            .install_method_options
            .iter()
            .find(|method| method.method_id == "rustupInstaller")
            .expect("rustup installer method should exist");
        assert_eq!(rustup_init.recommendation_rank, 0);
        assert_eq!(
            rustup_init.recommendation_reason.as_deref(),
            Some("upstream_recommended")
        );
        assert_eq!(rustup_init.policy_tag, "allowed");
    }

    #[test]
    fn rustup_executable_candidates_include_homebrew_keg_only_paths() {
        let candidates = super::manager_executable_candidates(ManagerId::Rustup);
        assert!(candidates.contains(&"rustup"));
        assert!(candidates.contains(&"/opt/homebrew/opt/rustup/bin/rustup"));
        assert!(candidates.contains(&"/usr/local/opt/rustup/bin/rustup"));
    }

    #[test]
    fn manager_status_preferences_override_default_enabled_policy() {
        let pref_map = HashMap::from([
            (
                ManagerId::Asdf,
                ManagerPreference {
                    manager: ManagerId::Asdf,
                    enabled: true,
                    selected_executable_path: None,
                    selected_install_method: None,
                    timeout_hard_seconds: None,
                    timeout_idle_seconds: None,
                },
            ),
            (
                ManagerId::MacPorts,
                ManagerPreference {
                    manager: ManagerId::MacPorts,
                    enabled: true,
                    selected_executable_path: None,
                    selected_install_method: None,
                    timeout_hard_seconds: None,
                    timeout_idle_seconds: None,
                },
            ),
            (
                ManagerId::Mise,
                ManagerPreference {
                    manager: ManagerId::Mise,
                    enabled: false,
                    selected_executable_path: None,
                    selected_install_method: None,
                    timeout_hard_seconds: None,
                    timeout_idle_seconds: None,
                },
            ),
        ]);
        let statuses = build_manager_statuses(None, None, &HashMap::new(), &pref_map);

        assert!(status_for(&statuses, ManagerId::Asdf).enabled);
        assert!(status_for(&statuses, ManagerId::MacPorts).enabled);
        assert!(!status_for(&statuses, ManagerId::Mise).enabled);
    }

    #[test]
    fn manager_status_exports_detection_only_flags() {
        let statuses = build_manager_statuses(None, None, &HashMap::new(), &HashMap::new());

        assert!(status_for(&statuses, ManagerId::Sparkle).is_detection_only);
        assert!(status_for(&statuses, ManagerId::Setapp).is_detection_only);
        assert!(status_for(&statuses, ManagerId::ParallelsDesktop).is_detection_only);
        assert!(!status_for(&statuses, ManagerId::HomebrewFormula).is_detection_only);
        assert!(!status_for(&statuses, ManagerId::Npm).is_detection_only);
    }

    #[test]
    fn manager_status_marks_alpha2_through_alpha5_slices_as_implemented() {
        let statuses = build_manager_statuses(None, None, &HashMap::new(), &HashMap::new());

        assert!(status_for(&statuses, ManagerId::HomebrewCask).is_implemented);
        assert!(status_for(&statuses, ManagerId::Asdf).is_implemented);
        assert!(status_for(&statuses, ManagerId::MacPorts).is_implemented);
        assert!(status_for(&statuses, ManagerId::NixDarwin).is_implemented);
        assert!(status_for(&statuses, ManagerId::DockerDesktop).is_implemented);
        assert!(status_for(&statuses, ManagerId::Podman).is_implemented);
        assert!(status_for(&statuses, ManagerId::Colima).is_implemented);
        assert!(status_for(&statuses, ManagerId::Sparkle).is_implemented);
        assert!(status_for(&statuses, ManagerId::Setapp).is_implemented);
        assert!(status_for(&statuses, ManagerId::ParallelsDesktop).is_implemented);
        assert!(status_for(&statuses, ManagerId::XcodeCommandLineTools).is_implemented);
        assert!(status_for(&statuses, ManagerId::Rosetta2).is_implemented);
        assert!(status_for(&statuses, ManagerId::FirmwareUpdates).is_implemented);
    }

    #[test]
    fn manager_status_marks_all_0_14_registry_managers_as_implemented() {
        let statuses = build_manager_statuses(None, None, &HashMap::new(), &HashMap::new());

        for manager_id in ManagerId::ALL {
            assert!(
                status_for(&statuses, manager_id).is_implemented,
                "manager {manager_id:?} expected implemented in 0.14 baseline"
            );
        }
    }

    #[test]
    fn manager_status_skips_executable_path_discovery_for_missing_managers() {
        let statuses = build_manager_statuses(None, None, &HashMap::new(), &HashMap::new());
        assert!(
            status_for(&statuses, ManagerId::Npm)
                .executable_paths
                .is_empty()
        );
    }

    #[test]
    fn manager_status_includes_active_executable_path_when_detected() {
        let detection_map = HashMap::from([(
            ManagerId::Npm,
            DetectionInfo {
                installed: true,
                executable_path: Some(std::path::PathBuf::from("/tmp/helm-test-npm")),
                version: Some("1.0.0".to_string()),
            },
        )]);
        let statuses = build_manager_statuses(None, None, &detection_map, &HashMap::new());
        let npm_status = status_for(&statuses, ManagerId::Npm);
        assert!(
            npm_status
                .executable_paths
                .contains(&"/tmp/helm-test-npm".to_string())
        );
    }

    #[test]
    fn manager_status_reports_executable_path_divergence_diagnostics() {
        let detection_map = HashMap::from([(
            ManagerId::Npm,
            DetectionInfo {
                installed: true,
                executable_path: Some(std::path::PathBuf::from("/opt/homebrew/bin/npm")),
                version: Some("10.0.0".to_string()),
            },
        )]);
        let pref_map = HashMap::from([(
            ManagerId::Npm,
            ManagerPreference {
                manager: ManagerId::Npm,
                enabled: true,
                selected_executable_path: Some("/bin/sh".to_string()),
                selected_install_method: None,
                timeout_hard_seconds: None,
                timeout_idle_seconds: None,
            },
        )]);

        let statuses = build_manager_statuses(None, None, &detection_map, &pref_map);
        let npm = status_for(&statuses, ManagerId::Npm);
        assert!(npm.selected_executable_differs_from_default);
        assert_eq!(npm.executable_path_diagnostic, "diverged");
    }

    #[test]
    fn manager_status_reports_executable_path_alignment_diagnostics() {
        let detection_map = HashMap::from([(
            ManagerId::Npm,
            DetectionInfo {
                installed: true,
                executable_path: Some(std::path::PathBuf::from("/opt/homebrew/bin/npm")),
                version: Some("10.0.0".to_string()),
            },
        )]);
        let pref_map = HashMap::from([(
            ManagerId::Npm,
            ManagerPreference {
                manager: ManagerId::Npm,
                enabled: true,
                selected_executable_path: Some("/opt/homebrew/bin/npm".to_string()),
                selected_install_method: None,
                timeout_hard_seconds: None,
                timeout_idle_seconds: None,
            },
        )]);

        let statuses = build_manager_statuses(None, None, &detection_map, &pref_map);
        let npm = status_for(&statuses, ManagerId::Npm);
        assert!(!npm.selected_executable_differs_from_default);
        assert_eq!(npm.executable_path_diagnostic, "aligned");
    }

    #[test]
    fn manager_status_includes_active_install_instance_provenance_summary() {
        let store = temp_sqlite_store("manager-status-provenance");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[sample_rustup_install_instance(
                    StrategyKind::RustupSelf,
                    InstallProvenance::RustupInit,
                    "/Users/test/.cargo/bin/rustup",
                )],
            )
            .expect("install instances should persist");

        let statuses = build_manager_statuses(None, Some(&store), &HashMap::new(), &HashMap::new());
        let rustup = status_for(&statuses, ManagerId::Rustup);
        assert_eq!(rustup.install_instance_count, 1);
        assert_eq!(rustup.install_instances.len(), 1);
        let instance = rustup
            .install_instances
            .first()
            .expect("rustup summary should include instance");
        assert_eq!(instance.provenance, "rustup_init");
        assert_eq!(instance.uninstall_strategy, "rustup_self");
        assert_eq!(rustup.active_provenance.as_deref(), Some("rustup_init"));
        assert_eq!(
            rustup.active_uninstall_strategy.as_deref(),
            Some("rustup_self")
        );
        assert!(rustup.active_confidence.unwrap_or_default() > 0.0);
        assert!(rustup.active_explanation_primary.is_some());

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn manager_status_install_instances_are_sorted_active_first() {
        let store = temp_sqlite_store("manager-status-instance-order");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let mut inactive = sample_rustup_install_instance(
            StrategyKind::HomebrewFormula,
            InstallProvenance::Homebrew,
            "/opt/homebrew/bin/rustup",
        );
        inactive.instance_id = "rustup-homebrew".to_string();
        inactive.identity_value = "rustup-homebrew".to_string();
        inactive.is_active = false;
        inactive.alias_paths = vec![
            std::path::PathBuf::from("/opt/homebrew/opt/rustup/bin/rustup"),
            std::path::PathBuf::from("/opt/homebrew/opt/rustup/bin/rustup"),
        ];

        let mut active = sample_rustup_install_instance(
            StrategyKind::RustupSelf,
            InstallProvenance::RustupInit,
            "/Users/test/.cargo/bin/rustup",
        );
        active.instance_id = "rustup-user".to_string();
        active.identity_value = "rustup-user".to_string();
        active.is_active = true;

        store
            .replace_install_instances(ManagerId::Rustup, &[inactive, active])
            .expect("install instances should persist");

        let statuses = build_manager_statuses(None, Some(&store), &HashMap::new(), &HashMap::new());
        let rustup = status_for(&statuses, ManagerId::Rustup);
        assert_eq!(rustup.install_instances.len(), 2);
        assert!(rustup.install_instances[0].is_active);
        assert_eq!(
            rustup.install_instances[0].display_path,
            "/Users/test/.cargo/bin/rustup"
        );
        assert!(!rustup.install_instances[1].is_active);
        assert_eq!(rustup.install_instances[1].provenance, "homebrew");
        assert_eq!(
            rustup.install_instances[1].alias_paths,
            vec!["/opt/homebrew/opt/rustup/bin/rustup".to_string()]
        );

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn manager_status_no_active_instance_fallback_uses_instance_id_order() {
        let store = temp_sqlite_store("manager-status-no-active-fallback");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let mut first = sample_manager_install_instance(
            ManagerId::Pnpm,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
            InstallProvenance::Homebrew,
            "/opt/homebrew/bin/pnpm",
            "/opt/homebrew/Cellar/pnpm/9.0.0/bin/pnpm",
        );
        first.instance_id = "pnpm-a".to_string();
        first.identity_value = "pnpm-a".to_string();
        first.is_active = false;

        let mut second = sample_manager_install_instance(
            ManagerId::Pnpm,
            StrategyKind::InteractivePrompt,
            StrategyKind::InteractivePrompt,
            InstallProvenance::Unknown,
            "/usr/local/bin/pnpm",
            "/usr/local/bin/pnpm",
        );
        second.instance_id = "pnpm-z".to_string();
        second.identity_value = "pnpm-z".to_string();
        second.is_active = false;

        store
            .replace_install_instances(ManagerId::Pnpm, &[second, first])
            .expect("install instances should persist");

        let statuses = build_manager_statuses(None, Some(&store), &HashMap::new(), &HashMap::new());
        let pnpm = status_for(&statuses, ManagerId::Pnpm);
        assert_eq!(pnpm.install_instances.len(), 2);
        assert_eq!(pnpm.install_instances[0].instance_id, "pnpm-a");
        assert_eq!(pnpm.active_provenance.as_deref(), Some("homebrew"));
        assert_eq!(
            pnpm.active_uninstall_strategy.as_deref(),
            Some("homebrew_formula")
        );

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn manager_status_reports_multi_instance_state_transitions() {
        let store = temp_sqlite_store("manager-status-multi-instance-state");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let mut homebrew = sample_rustup_install_instance(
            StrategyKind::HomebrewFormula,
            InstallProvenance::Homebrew,
            "/opt/homebrew/bin/rustup",
        );
        homebrew.instance_id = "rustup-homebrew".to_string();
        homebrew.identity_value = "rustup-homebrew".to_string();
        homebrew.is_active = false;

        let mut user = sample_rustup_install_instance(
            StrategyKind::RustupSelf,
            InstallProvenance::RustupInit,
            "/Users/test/.cargo/bin/rustup",
        );
        user.instance_id = "rustup-user".to_string();
        user.identity_value = "rustup-user".to_string();
        user.is_active = true;

        store
            .replace_install_instances(ManagerId::Rustup, &[homebrew.clone(), user.clone()])
            .expect("install instances should persist");

        let statuses = build_manager_statuses(None, Some(&store), &HashMap::new(), &HashMap::new());
        let rustup = status_for(&statuses, ManagerId::Rustup);
        assert_eq!(rustup.multi_instance_state, "attention_needed");
        assert!(!rustup.multi_instance_acknowledged);
        let acknowledged_fingerprint = rustup
            .multi_instance_fingerprint
            .as_deref()
            .expect("multi-instance fingerprint should be present")
            .to_string();

        store
            .set_manager_multi_instance_ack_fingerprint(
                ManagerId::Rustup,
                Some(acknowledged_fingerprint.as_str()),
            )
            .expect("multi-instance fingerprint should persist");

        let statuses = build_manager_statuses(None, Some(&store), &HashMap::new(), &HashMap::new());
        let rustup = status_for(&statuses, ManagerId::Rustup);
        assert_eq!(rustup.multi_instance_state, "acknowledged");
        assert!(rustup.multi_instance_acknowledged);
        assert_eq!(
            rustup.multi_instance_fingerprint.as_deref(),
            Some(acknowledged_fingerprint.as_str())
        );

        let mut extra = user;
        extra.instance_id = "rustup-extra".to_string();
        extra.identity_value = "rustup-extra".to_string();
        extra.display_path = std::path::PathBuf::from("/Users/test/.local/bin/rustup");
        extra.canonical_path = Some(std::path::PathBuf::from("/Users/test/.local/bin/rustup"));
        extra.is_active = false;

        store
            .replace_install_instances(ManagerId::Rustup, &[homebrew, extra])
            .expect("updated install instances should persist");

        let statuses = build_manager_statuses(None, Some(&store), &HashMap::new(), &HashMap::new());
        let rustup = status_for(&statuses, ManagerId::Rustup);
        assert_eq!(rustup.multi_instance_state, "attention_needed");
        assert!(!rustup.multi_instance_acknowledged);
        assert_ne!(
            rustup.multi_instance_fingerprint.as_deref(),
            Some(acknowledged_fingerprint.as_str())
        );

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn manager_status_flags_metadata_only_homebrew_formula_install_issues() {
        let store = temp_sqlite_store("manager-status-metadata-only-homebrew");
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[sample_rustup_install_instance(
                    StrategyKind::RustupSelf,
                    InstallProvenance::RustupInit,
                    "/Users/test/.cargo/bin/rustup",
                )],
            )
            .expect("install instances should persist");
        store
            .upsert_installed(&[sample_installed_package(
                ManagerId::HomebrewFormula,
                "rustup",
                Some("1.28.2"),
            )])
            .expect("homebrew installed snapshot should persist");

        let statuses = build_manager_statuses(None, Some(&store), &HashMap::new(), &HashMap::new());
        let rustup = status_for(&statuses, ManagerId::Rustup);
        assert_eq!(rustup.package_state_issues.len(), 1);
        let issue = &rustup.package_state_issues[0];
        assert_eq!(issue.source_manager_id, "homebrew_formula");
        assert_eq!(issue.package_name, "rustup");
        assert_eq!(issue.issue_code, "metadata_only_install");
        assert_eq!(
            issue.finding_code,
            helm_core::doctor::FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL
        );
        assert!(!issue.fingerprint.is_empty());
        assert_eq!(issue.repair_options.len(), 2);
        assert_eq!(
            issue.repair_options[0].option_id,
            helm_core::repair::REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW
        );
        assert_eq!(
            issue.repair_options[1].option_id,
            helm_core::repair::REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY
        );

        let _ = fs::remove_file(store.database_path());
    }

    #[test]
    fn parse_install_options_payload_supports_method_override() {
        let raw = std::ffi::CString::new(r#"{"installMethodOverride":"homebrew"}"#)
            .expect("json options should encode");
        let options = super::parse_install_options_payload(raw.as_ptr())
            .expect("install options payload should parse");
        assert_eq!(options.install_method_override.as_deref(), Some("homebrew"));
    }

    #[test]
    fn parse_legacy_file_coordinator_ipc_flag_accepts_truthy_values() {
        assert!(super::parse_legacy_file_coordinator_ipc_flag(Some("1")));
        assert!(super::parse_legacy_file_coordinator_ipc_flag(Some("true")));
        assert!(super::parse_legacy_file_coordinator_ipc_flag(Some("YES")));
        assert!(super::parse_legacy_file_coordinator_ipc_flag(Some(" on ")));
    }

    #[test]
    fn parse_legacy_file_coordinator_ipc_flag_rejects_missing_or_falsey_values() {
        assert!(!super::parse_legacy_file_coordinator_ipc_flag(None));
        assert!(!super::parse_legacy_file_coordinator_ipc_flag(Some("0")));
        assert!(!super::parse_legacy_file_coordinator_ipc_flag(Some(
            "false"
        )));
        assert!(!super::parse_legacy_file_coordinator_ipc_flag(Some("off")));
        assert!(!super::parse_legacy_file_coordinator_ipc_flag(Some(
            "unexpected"
        )));
    }

    #[test]
    fn coordinator_bridge_external_file_ipc_selection_requires_opt_in_and_ready() {
        assert!(!super::should_use_external_file_coordinator_with_health(
            super::CoordinatorBridgeMode::LocalXpcPreferred,
            true,
        ));
        assert!(!super::should_use_external_file_coordinator_with_health(
            super::CoordinatorBridgeMode::LegacyFileIpc,
            false,
        ));
        assert!(super::should_use_external_file_coordinator_with_health(
            super::CoordinatorBridgeMode::LegacyFileIpc,
            true,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn coordinator_ipc_paths_use_private_modes_and_consistent_ownership() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let state_dir = std::env::temp_dir().join(format!("helm-ffi-coordinator-perms-{nanos}"));
        super::reset_coordinator_state_dir(state_dir.as_path())
            .expect("reset coordinator state dir should succeed");

        let requests_dir = super::coordinator_requests_dir(state_dir.as_path());
        let responses_dir = super::coordinator_responses_dir(state_dir.as_path());
        assert_eq!(unix_mode(state_dir.as_path()), 0o700);
        assert_eq!(unix_mode(requests_dir.as_path()), 0o700);
        assert_eq!(unix_mode(responses_dir.as_path()), 0o700);

        let owner_uid = unix_uid(state_dir.as_path());
        assert_eq!(unix_uid(requests_dir.as_path()), owner_uid);
        assert_eq!(unix_uid(responses_dir.as_path()), owner_uid);

        let request_file = super::coordinator_request_file(state_dir.as_path(), "perm-check");
        let response_file = super::coordinator_response_file(state_dir.as_path(), "perm-check");
        let ready_file = super::coordinator_ready_file(state_dir.as_path());
        super::write_json_file(
            request_file.as_path(),
            &serde_json::json!({ "kind": "ping" }),
        )
        .expect("request file write should succeed");
        super::write_json_file(response_file.as_path(), &serde_json::json!({ "ok": true }))
            .expect("response file write should succeed");
        super::write_json_file(
            ready_file.as_path(),
            &serde_json::json!({ "pid": std::process::id() }),
        )
        .expect("ready file write should succeed");

        assert_eq!(unix_mode(request_file.as_path()), 0o600);
        assert_eq!(unix_mode(response_file.as_path()), 0o600);
        assert_eq!(unix_mode(ready_file.as_path()), 0o600);
        assert_eq!(unix_uid(request_file.as_path()), owner_uid);
        assert_eq!(unix_uid(response_file.as_path()), owner_uid);
        assert_eq!(unix_uid(ready_file.as_path()), owner_uid);

        let _ = fs::remove_dir_all(state_dir);
    }

    #[test]
    fn manager_status_marks_rubygems_ineligible_for_system_executable() {
        let detection_map = HashMap::from([(
            ManagerId::RubyGems,
            DetectionInfo {
                installed: true,
                executable_path: Some(std::path::PathBuf::from("/usr/bin/gem")),
                version: Some("3.4.10".to_string()),
            },
        )]);
        let pref_map = HashMap::from([(
            ManagerId::RubyGems,
            ManagerPreference {
                manager: ManagerId::RubyGems,
                enabled: true,
                selected_executable_path: Some("/usr/bin/gem".to_string()),
                selected_install_method: None,
                timeout_hard_seconds: None,
                timeout_idle_seconds: None,
            },
        )]);

        let statuses = build_manager_statuses(None, None, &detection_map, &pref_map);
        let rubygems = status_for(&statuses, ManagerId::RubyGems);
        assert!(!rubygems.enabled);
        assert!(!rubygems.is_eligible);
        assert_eq!(
            rubygems.ineligible_reason_code.as_deref(),
            Some(RUBYGEMS_SYSTEM_UNMANAGED_REASON_CODE)
        );
    }

    #[test]
    fn manager_status_keeps_rubygems_eligible_for_non_system_executable() {
        let detection_map = HashMap::from([(
            ManagerId::RubyGems,
            DetectionInfo {
                installed: true,
                executable_path: Some(std::path::PathBuf::from("/opt/homebrew/bin/gem")),
                version: Some("3.4.10".to_string()),
            },
        )]);
        let pref_map = HashMap::from([(
            ManagerId::RubyGems,
            ManagerPreference {
                manager: ManagerId::RubyGems,
                enabled: true,
                selected_executable_path: Some("/opt/homebrew/bin/gem".to_string()),
                selected_install_method: None,
                timeout_hard_seconds: None,
                timeout_idle_seconds: None,
            },
        )]);

        let statuses = build_manager_statuses(None, None, &detection_map, &pref_map);
        let rubygems = status_for(&statuses, ManagerId::RubyGems);
        assert!(rubygems.enabled);
        assert!(rubygems.is_eligible);
        assert!(rubygems.ineligible_reason_code.is_none());
    }

    #[test]
    fn manager_status_marks_pip_ineligible_for_system_executable() {
        let detection_map = HashMap::from([(
            ManagerId::Pip,
            DetectionInfo {
                installed: true,
                executable_path: Some(std::path::PathBuf::from("/usr/bin/python3")),
                version: Some("3.9.6".to_string()),
            },
        )]);
        let pref_map = HashMap::from([(
            ManagerId::Pip,
            ManagerPreference {
                manager: ManagerId::Pip,
                enabled: true,
                selected_executable_path: Some("/usr/bin/python3".to_string()),
                selected_install_method: None,
                timeout_hard_seconds: None,
                timeout_idle_seconds: None,
            },
        )]);

        let statuses = build_manager_statuses(None, None, &detection_map, &pref_map);
        let pip = status_for(&statuses, ManagerId::Pip);
        assert!(!pip.enabled);
        assert!(!pip.is_eligible);
        assert_eq!(
            pip.ineligible_reason_code.as_deref(),
            Some(PIP_SYSTEM_UNMANAGED_REASON_CODE)
        );
    }

    #[test]
    fn build_visible_tasks_deduplicates_inflight_rows_by_manager_and_type() {
        let tasks = vec![
            TaskRecord {
                id: TaskId(10),
                manager: ManagerId::HomebrewFormula,
                task_type: TaskType::Refresh,
                status: TaskStatus::Running,
                created_at: std::time::SystemTime::now(),
            },
            TaskRecord {
                id: TaskId(9),
                manager: ManagerId::HomebrewFormula,
                task_type: TaskType::Refresh,
                status: TaskStatus::Queued,
                created_at: std::time::SystemTime::now(),
            },
            TaskRecord {
                id: TaskId(8),
                manager: ManagerId::Npm,
                task_type: TaskType::Upgrade,
                status: TaskStatus::Queued,
                created_at: std::time::SystemTime::now(),
            },
            TaskRecord {
                id: TaskId(7),
                manager: ManagerId::Npm,
                task_type: TaskType::Upgrade,
                status: TaskStatus::Running,
                created_at: std::time::SystemTime::now(),
            },
        ];

        let labels = std::collections::HashMap::new();
        let visible = build_visible_tasks(tasks, &labels);
        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].id, TaskId(10));
        assert_eq!(visible[1].id, TaskId(7));
    }

    #[test]
    fn build_visible_tasks_prefers_newer_inflight_row_when_status_matches() {
        let tasks = vec![
            TaskRecord {
                id: TaskId(21),
                manager: ManagerId::Npm,
                task_type: TaskType::Upgrade,
                status: TaskStatus::Queued,
                created_at: std::time::SystemTime::now(),
            },
            TaskRecord {
                id: TaskId(22),
                manager: ManagerId::Npm,
                task_type: TaskType::Upgrade,
                status: TaskStatus::Queued,
                created_at: std::time::SystemTime::now(),
            },
        ];

        let labels = std::collections::HashMap::new();
        let visible = build_visible_tasks(tasks, &labels);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, TaskId(22));
    }

    #[test]
    fn build_visible_tasks_keeps_terminal_history_bounded() {
        let mut tasks = Vec::new();
        for idx in 0..60 {
            tasks.push(TaskRecord {
                id: TaskId(idx),
                manager: ManagerId::HomebrewFormula,
                task_type: TaskType::Refresh,
                status: TaskStatus::Completed,
                created_at: std::time::SystemTime::now(),
            });
        }

        let labels = std::collections::HashMap::new();
        let visible = build_visible_tasks(tasks, &labels);
        assert_eq!(visible.len(), 50);
        assert_eq!(visible[0].id, TaskId(0));
        assert_eq!(visible[49].id, TaskId(49));
    }

    #[test]
    fn build_visible_tasks_replaces_older_identical_failed_task() {
        let tasks = vec![
            TaskRecord {
                id: TaskId(201),
                manager: ManagerId::Npm,
                task_type: TaskType::Upgrade,
                status: TaskStatus::Running,
                created_at: std::time::SystemTime::now(),
            },
            TaskRecord {
                id: TaskId(200),
                manager: ManagerId::Npm,
                task_type: TaskType::Upgrade,
                status: TaskStatus::Failed,
                created_at: std::time::SystemTime::now(),
            },
        ];
        let labels = std::collections::HashMap::new();
        let visible = build_visible_tasks(tasks, &labels);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, TaskId(201));
        assert_eq!(visible[0].status, TaskStatus::Running);
    }

    #[test]
    fn build_visible_tasks_keeps_distinct_labeled_inflight_rows() {
        let tasks = vec![
            TaskRecord {
                id: TaskId(100),
                manager: ManagerId::Npm,
                task_type: TaskType::Upgrade,
                status: TaskStatus::Queued,
                created_at: std::time::SystemTime::now(),
            },
            TaskRecord {
                id: TaskId(99),
                manager: ManagerId::Npm,
                task_type: TaskType::Upgrade,
                status: TaskStatus::Queued,
                created_at: std::time::SystemTime::now(),
            },
        ];

        let labels = std::collections::HashMap::from([
            (
                100_u64,
                super::TaskLabel {
                    key: "service.task.label.upgrade.package".to_string(),
                    args: std::collections::BTreeMap::from([
                        ("manager".to_string(), "npm".to_string()),
                        ("package".to_string(), "typescript".to_string()),
                    ]),
                },
            ),
            (
                99_u64,
                super::TaskLabel {
                    key: "service.task.label.upgrade.package".to_string(),
                    args: std::collections::BTreeMap::from([
                        ("manager".to_string(), "npm".to_string()),
                        ("package".to_string(), "eslint".to_string()),
                    ]),
                },
            ),
        ]);

        let visible = build_visible_tasks(tasks, &labels);
        assert_eq!(visible.len(), 2);
    }

    #[test]
    fn diagnostics_redaction_masks_non_allowlisted_env_assignments() {
        let redacted = super::redact_diagnostics_text(
            "PATH=/usr/bin API_TOKEN=abc123 TMPDIR=/tmp HELM_LICENSE_KEY=xyz",
        );
        assert!(redacted.contains("PATH=/usr/bin"));
        assert!(redacted.contains("TMPDIR=/tmp"));
        assert!(redacted.contains("API_TOKEN=[REDACTED]"));
        assert!(redacted.contains("HELM_LICENSE_KEY=[REDACTED]"));
        assert!(!redacted.contains("abc123"));
        assert!(!redacted.contains("xyz"));
    }

    #[test]
    fn diagnostics_redaction_masks_sensitive_key_pairs_and_auth_headers() {
        let redacted = super::redact_diagnostics_text(
            "authorization: Bearer token-value\npassword=hunter2 cookie:abc123",
        );
        assert!(redacted.contains("authorization: [REDACTED]"));
        assert!(redacted.contains("password=[REDACTED]"));
        assert!(redacted.contains("cookie:[REDACTED]"));
        assert!(!redacted.contains("token-value"));
        assert!(!redacted.contains("hunter2"));
        assert!(!redacted.contains("abc123"));
    }

    #[test]
    fn build_ffi_task_output_record_redacts_sensitive_fields_by_default() {
        let task_id = TaskId(9_777_001);
        helm_core::execution::task_output_store::record_context(
            task_id,
            Some("API_TOKEN=abc PATH=/usr/bin helm refresh"),
            Some("/tmp/project"),
        );
        helm_core::execution::task_output_store::record_process_context(
            task_id,
            Some("/usr/bin/helm"),
            Some("PATH=/usr/bin:/bin"),
        );
        let started_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(5);
        let finished_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(8);
        helm_core::execution::task_output_store::record_started_at(task_id, started_at);
        helm_core::execution::task_output_store::record_terminal_metadata(
            task_id,
            started_at,
            finished_at,
            Some(1),
            Some("error"),
        );
        helm_core::execution::task_output_store::record_error(
            task_id,
            "spawn_failed",
            "request failed with api_key=abc123",
            Some("error"),
            Some(finished_at),
        );
        helm_core::execution::task_output_store::append_stdout(
            task_id,
            b"authorization: Bearer abc123\nPATH=/usr/bin",
        );
        helm_core::execution::task_output_store::append_stderr(
            task_id,
            b"password=hunter2 TMPDIR=/tmp",
        );

        let record = super::build_ffi_task_output_record(task_id);
        let command = record.command.expect("command should be present");
        let cwd = record.cwd.expect("cwd should be present");
        let program_path = record.program_path.expect("program path should be present");
        let path_snippet = record.path_snippet.expect("path snippet should be present");
        let error_message = record
            .error_message
            .expect("error message should be present");
        let stdout = record.stdout.expect("stdout should be present");
        let stderr = record.stderr.expect("stderr should be present");

        assert!(command.contains("API_TOKEN=[REDACTED]"));
        assert!(command.contains("PATH=/usr/bin"));
        assert!(!command.contains("API_TOKEN=abc"));
        assert_eq!(cwd, "/tmp/project");
        assert_eq!(program_path, "/usr/bin/helm");
        assert_eq!(path_snippet, "PATH=/usr/bin:/bin");
        assert_eq!(record.started_at_unix_ms, Some(5_000));
        assert_eq!(record.finished_at_unix_ms, Some(8_000));
        assert_eq!(record.duration_ms, Some(3_000));
        assert_eq!(record.exit_code, Some(1));
        assert_eq!(record.termination_reason.as_deref(), Some("error"));
        assert_eq!(record.error_code.as_deref(), Some("spawn_failed"));
        assert!(error_message.contains("api_key=[REDACTED]"));
        assert!(!error_message.contains("abc123"));

        assert!(stdout.contains("authorization: [REDACTED]"));
        assert!(stdout.contains("PATH=/usr/bin"));
        assert!(!stdout.contains("Bearer abc123"));

        assert!(stderr.contains("password=[REDACTED]"));
        assert!(stderr.contains("TMPDIR=/tmp"));
        assert!(!stderr.contains("hunter2"));
    }

    #[test]
    fn map_task_log_record_redacts_sensitive_message_payloads() {
        let mapped = super::map_task_log_record(TaskLogRecord {
            id: 42,
            task_id: TaskId(9_777_002),
            manager: ManagerId::Npm,
            task_type: TaskType::Refresh,
            status: Some(TaskStatus::Failed),
            level: helm_core::models::TaskLogLevel::Error,
            message: "AUTH_TOKEN=abc123 PATH=/usr/bin".to_string(),
            created_at: std::time::UNIX_EPOCH + std::time::Duration::from_secs(100),
        });
        assert_eq!(mapped.id, 42);
        assert_eq!(mapped.level, "error");
        assert!(mapped.message.contains("AUTH_TOKEN=[REDACTED]"));
        assert!(mapped.message.contains("PATH=/usr/bin"));
        assert!(!mapped.message.contains("abc123"));
    }

    #[test]
    fn search_label_key_uses_query_variant_when_query_is_present() {
        assert_eq!(
            search_label_key_for_query("openssl"),
            "service.task.label.search.package"
        );
        assert_eq!(
            search_label_key_for_query("   "),
            "service.task.label.search.manager"
        );
    }

    #[test]
    fn search_label_args_include_query_when_present() {
        let with_query = search_label_args(ManagerId::Npm, "typescript");
        assert_eq!(with_query.len(), 2);
        assert_eq!(with_query[0], ("manager", "npm".to_string()));
        assert_eq!(with_query[1], ("query", "typescript".to_string()));

        let without_query = search_label_args(ManagerId::Npm, " ");
        assert_eq!(without_query, vec![("manager", "npm".to_string())]);
    }

    #[test]
    fn individual_package_install_support_is_scoped_to_supported_managers() {
        assert!(manager_allows_individual_package_install(ManagerId::Npm));
        assert!(manager_allows_individual_package_install(
            ManagerId::HomebrewFormula
        ));
        assert!(!manager_allows_individual_package_install(ManagerId::Mas));
        assert!(!manager_allows_individual_package_install(
            ManagerId::SoftwareUpdate
        ));
    }

    fn status_for(
        statuses: &[super::FfiManagerStatus],
        manager_id: ManagerId,
    ) -> &super::FfiManagerStatus {
        statuses
            .iter()
            .find(|status| status.manager_id == manager_id.as_str())
            .expect("manager status should exist")
    }

    fn outdated_pkg(manager: ManagerId, name: &str, pinned: bool) -> OutdatedPackage {
        OutdatedPackage {
            package: PackageRef {
                manager,
                name: name.to_string(),
            },
            installed_version: Some("1.0.0".to_string()),
            candidate_version: "1.1.0".to_string(),
            pinned,
            restart_required: false,
        }
    }
}
