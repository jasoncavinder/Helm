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
//! ## FFI Exports (27 functions)
//!
//! | Function | Category |
//! |----------|----------|
//! | `helm_init` | Lifecycle |
//! | `helm_list_installed_packages` | Package queries |
//! | `helm_list_outdated_packages` | Package queries |
//! | `helm_list_tasks` | Task management |
//! | `helm_trigger_refresh` | Task management |
//! | `helm_cancel_task` | Task management |
//! | `helm_search_local` | Search |
//! | `helm_trigger_remote_search` | Search |
//! | `helm_list_manager_status` | Manager control |
//! | `helm_set_manager_enabled` | Manager control |
//! | `helm_install_manager` | Manager control |
//! | `helm_update_manager` | Manager control |
//! | `helm_uninstall_manager` | Manager control |
//! | `helm_get_safe_mode` | Settings |
//! | `helm_set_safe_mode` | Settings |
//! | `helm_get_homebrew_keg_auto_cleanup` | Settings |
//! | `helm_set_homebrew_keg_auto_cleanup` | Settings |
//! | `helm_list_package_keg_policies` | Keg policies |
//! | `helm_set_package_keg_policy` | Keg policies |
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
use std::os::raw::c_char;
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use std::time::UNIX_EPOCH;

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
use helm_core::models::{
    Capability, DetectionInfo, HomebrewKegPolicy, ManagerAuthority, ManagerId, OutdatedPackage,
    PackageRef, PinKind, PinRecord, SearchQuery, TaskStatus, TaskType,
};
use helm_core::orchestration::adapter_runtime::AdapterRuntime;
use helm_core::orchestration::{AdapterTaskTerminalState, CancellationMode};
use helm_core::persistence::{
    DetectionStore, MigrationStore, PackageStore, PinStore, SearchCacheStore, TaskStore,
};
use helm_core::sqlite::SqliteStore;
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

#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
struct FfiManagerStatus {
    manager_id: String,
    detected: bool,
    version: Option<String>,
    executable_path: Option<String>,
    enabled: bool,
    is_implemented: bool,
    is_optional: bool,
    is_detection_only: bool,
}

fn build_manager_statuses(
    detection_map: &std::collections::HashMap<ManagerId, DetectionInfo>,
    pref_map: &std::collections::HashMap<ManagerId, bool>,
) -> Vec<FfiManagerStatus> {
    ManagerId::ALL
        .iter()
        .map(|&id| {
            let detection = detection_map.get(&id);
            let enabled = pref_map
                .get(&id)
                .copied()
                .unwrap_or_else(|| default_enabled_for_manager(id));
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
            let version = detection.and_then(|d| normalize_nonempty(d.version.clone()));

            FfiManagerStatus {
                manager_id: id.as_str().to_string(),
                detected,
                version,
                executable_path,
                enabled,
                is_implemented,
                is_optional,
                is_detection_only,
            }
        })
        .collect()
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

const TASK_PRUNE_MAX_AGE_SECS: i64 = 300;
const TASK_RECENT_FETCH_LIMIT: usize = 1000;
const TASK_TERMINAL_HISTORY_LIMIT: usize = 50;
const TASK_INFLIGHT_DEDUP_MAX_AGE_SECS: u64 = 1800;

fn is_inflight_status(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Queued | TaskStatus::Running)
}

fn is_recent_inflight_task(task: &helm_core::models::TaskRecord) -> bool {
    std::time::SystemTime::now()
        .duration_since(task.created_at)
        .map(|elapsed| elapsed.as_secs() <= TASK_INFLIGHT_DEDUP_MAX_AGE_SECS)
        .unwrap_or(true)
}

fn build_visible_tasks(
    tasks: Vec<helm_core::models::TaskRecord>,
    labels: &std::collections::HashMap<u64, TaskLabel>,
) -> Vec<helm_core::models::TaskRecord> {
    let mut visible = Vec::with_capacity(tasks.len());
    let mut seen_inflight = std::collections::HashSet::new();
    let mut terminal_count = 0usize;

    for task in tasks {
        if is_inflight_status(task.status) {
            let key = labels
                .get(&task.id.0)
                .map(|label| {
                    let mut encoded =
                        format!("{:?}:{:?}:{}", task.manager, task.task_type, label.key);
                    for (arg_key, arg_value) in &label.args {
                        encoded.push('|');
                        encoded.push_str(arg_key);
                        encoded.push('=');
                        encoded.push_str(arg_value);
                    }
                    encoded
                })
                .unwrap_or_else(|| format!("{:?}:{:?}", task.manager, task.task_type));
            if seen_inflight.insert(key) {
                visible.push(task);
            }
            continue;
        }

        if terminal_count < TASK_TERMINAL_HISTORY_LIMIT {
            visible.push(task);
            terminal_count = terminal_count.saturating_add(1);
        }
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
    if !runtime.is_manager_enabled(manager) {
        return false;
    }
    helm_core::registry::manager(manager)
        .map(|descriptor| descriptor.supports(Capability::Search))
        .unwrap_or(false)
}

fn queue_remote_search_task(
    store: &SqliteStore,
    runtime: &AdapterRuntime,
    rt_handle: &tokio::runtime::Handle,
    manager: ManagerId,
    query: &str,
) -> Result<helm_core::models::TaskId, &'static str> {
    if !can_submit_remote_search(runtime, manager) {
        return Err("service.error.unsupported_capability");
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
            Err("service.error.process_failure")
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

fn supports_individual_package_install(manager: ManagerId) -> bool {
    matches!(
        manager,
        ManagerId::HomebrewFormula
            | ManagerId::Mise
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

fn supports_individual_package_uninstall(manager: ManagerId) -> bool {
    supports_individual_package_install(manager)
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

    let state = HelmState {
        store,
        runtime,
        rt_handle,
        _tokio_rt: rt,
    };

    *lock_or_recover(&STATE, "state") = Some(state);

    true
}

#[unsafe(no_mangle)]
pub extern "C" fn helm_list_installed_packages() -> *mut c_char {
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };

    let packages = match state.store.list_installed() {
        Ok(pkgs) => pkgs,
        Err(e) => {
            eprintln!("Failed to list installed packages: {}", e);
            return std::ptr::null_mut();
        }
    };

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

    let packages = match state.store.list_outdated() {
        Ok(pkgs) => pkgs,
        Err(e) => {
            eprintln!("Failed to list outdated packages: {}", e);
            return std::ptr::null_mut();
        }
    };

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

    // Auto-prune completed/failed tasks older than 5 minutes.
    let _ = state.store.prune_completed_tasks(TASK_PRUNE_MAX_AGE_SECS);

    // Fetch a wider snapshot so long-running queued/running tasks do not disappear
    // behind a tight recent-task limit.
    let tasks = match state.store.list_recent_tasks(TASK_RECENT_FETCH_LIMIT) {
        Ok(tasks) => tasks,
        Err(e) => {
            eprintln!("Failed to list tasks: {}", e);
            return std::ptr::null_mut();
        }
    };
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
    let tasks = build_visible_tasks(tasks, &labels);
    let active_ids: std::collections::HashSet<u64> = tasks.iter().map(|task| task.id.0).collect();
    labels.retain(|task_id, _| active_ids.contains(task_id));

    let ffi_tasks: Vec<FfiTaskRecord> = tasks
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

#[unsafe(no_mangle)]
pub extern "C" fn helm_trigger_refresh() -> bool {
    clear_last_error_key();
    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return return_error_bool("service.error.internal"),
    };

    let runtime = state.runtime.clone();
    let store = state.store.clone();

    let has_refresh_or_detection = store
        .list_recent_tasks(TASK_RECENT_FETCH_LIMIT)
        .ok()
        .map(|tasks| {
            tasks.into_iter().any(|task| {
                is_inflight_status(task.status)
                    && is_recent_inflight_task(&task)
                    && matches!(task.task_type, TaskType::Refresh | TaskType::Detection)
            })
        })
        .unwrap_or(false);
    if has_refresh_or_detection {
        return true;
    }

    state._tokio_rt.spawn(async move {
        let results = runtime.refresh_all_ordered().await;
        for (manager, result) in results {
            if let Err(e) = result {
                eprintln!("Refresh failed for {manager:?}: {e}");
            }
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

    let results = match state.store.query_local(query_str, 500) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to query local search cache: {}", e);
            return std::ptr::null_mut();
        }
    };

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
        return return_error_i64("service.error.invalid_input");
    }

    let c_str = unsafe { CStr::from_ptr(query) };
    let query_str = match c_str.to_str() {
        Ok(s) => s.trim(),
        Err(_) => return return_error_i64("service.error.invalid_input"),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
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
        None => return_error_i64(last_error_key.unwrap_or("service.error.unsupported_capability")),
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
        return return_error_i64("service.error.invalid_input");
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => return return_error_i64("service.error.invalid_input"),
    };

    let query_cstr = unsafe { CStr::from_ptr(query) };
    let query_str = match query_cstr.to_str() {
        Ok(query_text) => query_text.trim(),
        Err(_) => return return_error_i64("service.error.invalid_input"),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
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

    let detection_map: std::collections::HashMap<_, _> = detections.into_iter().collect();
    let pref_map: std::collections::HashMap<_, _> = preferences.into_iter().collect();

    let mut statuses = build_manager_statuses(&detection_map, &pref_map);

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
    }

    let json = match serde_json::to_string(&statuses) {
        Ok(j) => j,
        Err(_) => return std::ptr::null_mut(),
    };

    match CString::new(json) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
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

/// Queue upgrade tasks for supported managers using cached outdated snapshot.
///
/// - `include_pinned`: if false, pinned packages are excluded.
/// - `allow_os_updates`: explicit confirmation gate for `softwareupdate` upgrades.
#[unsafe(no_mangle)]
pub extern "C" fn helm_upgrade_all(include_pinned: bool, allow_os_updates: bool) -> bool {
    clear_last_error_key();
    let (store, runtime, tokio_rt) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool("service.error.internal"),
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
                        if cleanup_old_kegs {
                            set_task_label(
                                task_id,
                                "service.task.label.upgrade.homebrew_cleanup",
                                &[("package", package_name.clone())],
                            );
                        } else {
                            set_task_label(
                                task_id,
                                "service.task.label.upgrade.homebrew",
                                &[("package", package_name.clone())],
                            );
                        }
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.mise",
                        &[("package", package_name.clone())],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            ("manager", manager_display_name(ManagerId::Npm).to_string()),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            ("manager", manager_display_name(ManagerId::Pnpm).to_string()),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            ("manager", manager_display_name(ManagerId::Yarn).to_string()),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            (
                                "manager",
                                manager_display_name(ManagerId::Cargo).to_string(),
                            ),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            (
                                "manager",
                                manager_display_name(ManagerId::CargoBinstall).to_string(),
                            ),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            ("manager", manager_display_name(ManagerId::Pip).to_string()),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            ("manager", manager_display_name(ManagerId::Pipx).to_string()),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            (
                                "manager",
                                manager_display_name(ManagerId::Poetry).to_string(),
                            ),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            (
                                "manager",
                                manager_display_name(ManagerId::RubyGems).to_string(),
                            ),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.package",
                        &[
                            ("package", package_name.clone()),
                            (
                                "manager",
                                manager_display_name(ManagerId::Bundler).to_string(),
                            ),
                        ],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.rustup_toolchain",
                        &[("toolchain", toolchain.clone())],
                    ),
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
                    Ok(task_id) => set_task_label(
                        task_id,
                        "service.task.label.upgrade.softwareupdate_all",
                        &[],
                    ),
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
        return return_error_i64("service.error.invalid_input");
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => return return_error_i64("service.error.invalid_input"),
    };

    let package_cstr = unsafe { CStr::from_ptr(package_name) };
    let package_name = match package_cstr.to_str() {
        Ok(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => return return_error_i64("service.error.invalid_input"),
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
                    None => return return_error_i64("service.error.internal"),
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
        _ => return return_error_i64("service.error.unsupported_capability"),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

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
            return_error_i64("service.error.process_failure")
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
        return return_error_i64("service.error.invalid_input");
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => return return_error_i64("service.error.invalid_input"),
    };

    if !supports_individual_package_install(manager) {
        return return_error_i64("service.error.unsupported_capability");
    }

    let package_cstr = unsafe { CStr::from_ptr(package_name) };
    let package_name = match package_cstr.to_str() {
        Ok(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => return return_error_i64("service.error.invalid_input"),
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

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    if !runtime.is_manager_enabled(manager) {
        return return_error_i64("service.error.unsupported_capability");
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
            return_error_i64("service.error.process_failure")
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
        return return_error_i64("service.error.invalid_input");
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr
        .to_str()
        .ok()
        .and_then(|s| s.parse::<ManagerId>().ok())
    {
        Some(manager) => manager,
        None => return return_error_i64("service.error.invalid_input"),
    };

    if !supports_individual_package_uninstall(manager) {
        return return_error_i64("service.error.unsupported_capability");
    }

    let package_cstr = unsafe { CStr::from_ptr(package_name) };
    let package_name = match package_cstr.to_str() {
        Ok(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => return return_error_i64("service.error.invalid_input"),
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

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    if !runtime.is_manager_enabled(manager) {
        return return_error_i64("service.error.unsupported_capability");
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
            return_error_i64("service.error.process_failure")
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
        return return_error_bool("service.error.invalid_input");
    }

    let manager = {
        let c_str = unsafe { CStr::from_ptr(manager_id) };
        match c_str
            .to_str()
            .ok()
            .and_then(|s| s.parse::<ManagerId>().ok())
        {
            Some(id) => id,
            None => return return_error_bool("service.error.invalid_input"),
        }
    };

    let package_name = {
        let c_str = unsafe { CStr::from_ptr(package_name) };
        match c_str.to_str() {
            Ok(value) if !value.trim().is_empty() => value.to_string(),
            _ => return return_error_bool("service.error.invalid_input"),
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
            Err(_) => return return_error_bool("service.error.invalid_input"),
        }
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool("service.error.internal"),
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
        let task_id = match rt_handle.block_on(runtime.submit(manager, request)) {
            Ok(task_id) => task_id,
            Err(_) => return return_error_bool("service.error.process_failure"),
        };

        set_task_label(
            task_id,
            "service.task.label.pin.homebrew",
            &[("package", package.name.clone())],
        );

        let snapshot = match rt_handle.block_on(runtime.wait_for_terminal(task_id, None)) {
            Ok(snapshot) => snapshot,
            Err(_) => return return_error_bool("service.error.process_failure"),
        };

        match snapshot.terminal_state {
            Some(AdapterTaskTerminalState::Succeeded(_)) => {}
            _ => return return_error_bool("service.error.process_failure"),
        }
        PinKind::Native
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
        .map_err(|_| set_last_error_key("service.error.storage_failure"))
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
        return return_error_bool("service.error.invalid_input");
    }

    let manager = {
        let c_str = unsafe { CStr::from_ptr(manager_id) };
        match c_str
            .to_str()
            .ok()
            .and_then(|s| s.parse::<ManagerId>().ok())
        {
            Some(id) => id,
            None => return return_error_bool("service.error.invalid_input"),
        }
    };

    let package_name = {
        let c_str = unsafe { CStr::from_ptr(package_name) };
        match c_str.to_str() {
            Ok(value) if !value.trim().is_empty() => value.to_string(),
            _ => return return_error_bool("service.error.invalid_input"),
        }
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_bool("service.error.internal"),
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
        let task_id = match rt_handle.block_on(runtime.submit(manager, request)) {
            Ok(task_id) => task_id,
            Err(_) => return return_error_bool("service.error.process_failure"),
        };

        set_task_label(
            task_id,
            "service.task.label.unpin.homebrew",
            &[("package", package_name.clone())],
        );

        let snapshot = match rt_handle.block_on(runtime.wait_for_terminal(task_id, None)) {
            Ok(snapshot) => snapshot,
            Err(_) => return return_error_bool("service.error.process_failure"),
        };

        match snapshot.terminal_state {
            Some(AdapterTaskTerminalState::Succeeded(_)) => {}
            _ => return return_error_bool("service.error.process_failure"),
        }
    }

    let package_key = format!("{}:{}", manager.as_str(), package_name);
    store
        .remove_pin(&package_key)
        .map_err(|_| set_last_error_key("service.error.storage_failure"))
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
        return return_error_bool("service.error.invalid_input");
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_bool("service.error.invalid_input"),
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(m) => m,
        Err(_) => return return_error_bool("service.error.invalid_input"),
    };

    let guard = lock_or_recover(&STATE, "state");
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return return_error_bool("service.error.internal"),
    };

    state
        .store
        .set_manager_enabled(manager, enabled)
        .map_err(|_| set_last_error_key("service.error.storage_failure"))
        .is_ok()
}

/// Install a manager tool via Homebrew. Returns the task ID, or -1 on error.
///
/// Supported manager IDs: "mise", "mas".
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_install_manager(manager_id: *const c_char) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_i64("service.error.invalid_input");
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_i64("service.error.invalid_input"),
    };

    // Map manager IDs to the formula name to install via Homebrew
    let formula_name = match id_str {
        "mise" => "mise",
        "mas" => "mas",
        _ => return return_error_i64("service.error.unsupported_capability"),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    // `mise` and `mas` manager installation flows run via Homebrew formula install.
    if !homebrew_dependency_available(&store) {
        return return_error_i64("service.error.homebrew_required");
    }

    let request = AdapterRequest::Install(InstallRequest {
        package: PackageRef {
            manager: ManagerId::HomebrewFormula,
            name: formula_name.to_string(),
        },
        version: None,
    });

    let label_args = [("package", formula_name.to_string())];
    if let Some(existing) = find_matching_inflight_task(
        store.as_ref(),
        ManagerId::HomebrewFormula,
        TaskType::Install,
        Some("service.task.label.install.homebrew_formula"),
        &label_args,
    ) {
        return existing.0 as i64;
    }

    match rt_handle.block_on(runtime.submit(ManagerId::HomebrewFormula, request)) {
        Ok(task_id) => {
            set_task_label(
                task_id,
                "service.task.label.install.homebrew_formula",
                &label_args,
            );
            task_id.0 as i64
        }
        Err(e) => {
            eprintln!("Failed to install manager {}: {}", id_str, e);
            return_error_i64("service.error.process_failure")
        }
    }
}

/// Update a manager tool. Returns the task ID, or -1 on error.
///
/// Supported manager IDs:
/// - "homebrew_formula" -> `brew update`
/// - "mise" -> `brew upgrade mise`
/// - "mas" -> `brew upgrade mas`
/// - "rustup" -> `rustup self update`
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_update_manager(manager_id: *const c_char) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_i64("service.error.invalid_input");
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_i64("service.error.invalid_input"),
    };

    let (target_manager, request, label_key, label_args): (
        ManagerId,
        AdapterRequest,
        &str,
        Vec<(&str, String)>,
    ) = match id_str {
        "homebrew_formula" => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "__self__".to_string(),
                }),
            }),
            "service.task.label.update.homebrew_self",
            Vec::new(),
        ),
        "mise" => {
            let (target_name, label_key) = {
                let guard = lock_or_recover(&STATE, "state");
                let state = match guard.as_ref() {
                    Some(s) => s,
                    None => return return_error_i64("service.error.internal"),
                };
                let policy = effective_homebrew_keg_policy(&state.store, "mise");
                let cleanup_old_kegs = policy == HomebrewKegPolicy::Cleanup;
                let target_name = encode_homebrew_upgrade_target("mise", cleanup_old_kegs);
                let label_key = if cleanup_old_kegs {
                    "service.task.label.update.homebrew_formula_cleanup"
                } else {
                    "service.task.label.update.homebrew_formula"
                };
                (target_name, label_key)
            };
            (
                ManagerId::HomebrewFormula,
                AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: target_name,
                    }),
                }),
                label_key,
                vec![("package", "mise".to_string())],
            )
        }
        "mas" => {
            let (target_name, label_key) = {
                let guard = lock_or_recover(&STATE, "state");
                let state = match guard.as_ref() {
                    Some(s) => s,
                    None => return return_error_i64("service.error.internal"),
                };
                let policy = effective_homebrew_keg_policy(&state.store, "mas");
                let cleanup_old_kegs = policy == HomebrewKegPolicy::Cleanup;
                let target_name = encode_homebrew_upgrade_target("mas", cleanup_old_kegs);
                let label_key = if cleanup_old_kegs {
                    "service.task.label.update.homebrew_formula_cleanup"
                } else {
                    "service.task.label.update.homebrew_formula"
                };
                (target_name, label_key)
            };
            (
                ManagerId::HomebrewFormula,
                AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: target_name,
                    }),
                }),
                label_key,
                vec![("package", "mas".to_string())],
            )
        }
        "rustup" => (
            ManagerId::Rustup,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                }),
            }),
            "service.task.label.update.rustup_self",
            Vec::new(),
        ),
        _ => return return_error_i64("service.error.unsupported_capability"),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

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
            return_error_i64("service.error.process_failure")
        }
    }
}

/// Uninstall a manager tool. Returns the task ID, or -1 on error.
///
/// Supported manager IDs: "mise", "mas" (via Homebrew), "rustup" (self uninstall).
///
/// # Safety
///
/// `manager_id` must be a valid, non-null pointer to a NUL-terminated UTF-8 C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn helm_uninstall_manager(manager_id: *const c_char) -> i64 {
    clear_last_error_key();
    if manager_id.is_null() {
        return return_error_i64("service.error.invalid_input");
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return return_error_i64("service.error.invalid_input"),
    };

    let (store, runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (
            state.store.clone(),
            state.runtime.clone(),
            state.rt_handle.clone(),
        )
    };

    let (target_manager, request) = match id_str {
        "mise" => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "mise".to_string(),
                },
            }),
        ),
        "mas" => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "mas".to_string(),
                },
            }),
        ),
        "rustup" => (
            ManagerId::Rustup,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                },
            }),
        ),
        _ => return return_error_i64("service.error.unsupported_capability"),
    };

    let (label_key, label_args): (&str, Vec<(&str, String)>) = match id_str {
        "mise" => (
            "service.task.label.uninstall.homebrew_formula",
            vec![("package", "mise".to_string())],
        ),
        "mas" => (
            "service.task.label.uninstall.homebrew_formula",
            vec![("package", "mas".to_string())],
        ),
        "rustup" => ("service.task.label.uninstall.rustup_self", Vec::new()),
        _ => ("service.task.label.uninstall.homebrew_formula", Vec::new()),
    };

    if let Some(existing) = find_matching_inflight_task(
        store.as_ref(),
        target_manager,
        TaskType::Uninstall,
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
            eprintln!("Failed to uninstall manager {}: {}", id_str, e);
            return_error_i64("service.error.process_failure")
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
        None => return return_error_bool("service.error.internal"),
    };

    // Roll back to version 0 (drops all data tables)
    if let Err(e) = state.store.apply_migration(0) {
        eprintln!("Failed to roll back migrations: {}", e);
        return return_error_bool("service.error.storage_failure");
    }

    // Re-apply all migrations (recreates empty tables)
    if let Err(e) = state.store.migrate_to_latest() {
        eprintln!("Failed to re-apply migrations: {}", e);
        return return_error_bool("service.error.storage_failure");
    }

    // Final cleanup: delete any task records that in-flight persistence
    // watchers may have re-inserted during the brief reset window.
    let _ = state.store.delete_all_tasks();

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
        build_manager_statuses, build_visible_tasks, collect_upgrade_all_targets,
        homebrew_probe_candidates, parse_homebrew_config_version, search_label_args,
        search_label_key_for_query, supports_individual_package_install,
    };
    use helm_core::models::{
        ManagerId, OutdatedPackage, PackageRef, TaskId, TaskRecord, TaskStatus, TaskType,
    };
    use std::collections::HashMap;
    use std::path::Path;

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
    fn manager_status_defaults_disable_optional_managers() {
        let statuses = build_manager_statuses(&HashMap::new(), &HashMap::new());

        assert!(!status_for(&statuses, ManagerId::Asdf).enabled);
        assert!(!status_for(&statuses, ManagerId::MacPorts).enabled);
        assert!(!status_for(&statuses, ManagerId::NixDarwin).enabled);
        assert!(status_for(&statuses, ManagerId::Mise).enabled);
    }

    #[test]
    fn manager_status_preferences_override_default_enabled_policy() {
        let pref_map = HashMap::from([
            (ManagerId::Asdf, true),
            (ManagerId::MacPorts, true),
            (ManagerId::Mise, false),
        ]);
        let statuses = build_manager_statuses(&HashMap::new(), &pref_map);

        assert!(status_for(&statuses, ManagerId::Asdf).enabled);
        assert!(status_for(&statuses, ManagerId::MacPorts).enabled);
        assert!(!status_for(&statuses, ManagerId::Mise).enabled);
    }

    #[test]
    fn manager_status_exports_detection_only_flags() {
        let statuses = build_manager_statuses(&HashMap::new(), &HashMap::new());

        assert!(status_for(&statuses, ManagerId::Sparkle).is_detection_only);
        assert!(status_for(&statuses, ManagerId::Setapp).is_detection_only);
        assert!(status_for(&statuses, ManagerId::ParallelsDesktop).is_detection_only);
        assert!(!status_for(&statuses, ManagerId::HomebrewFormula).is_detection_only);
        assert!(!status_for(&statuses, ManagerId::Npm).is_detection_only);
    }

    #[test]
    fn manager_status_marks_alpha2_through_alpha5_slices_as_implemented() {
        let statuses = build_manager_statuses(&HashMap::new(), &HashMap::new());

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
        let statuses = build_manager_statuses(&HashMap::new(), &HashMap::new());

        for manager_id in ManagerId::ALL {
            assert!(
                status_for(&statuses, manager_id).is_implemented,
                "manager {manager_id:?} expected implemented in 0.14 baseline"
            );
        }
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
        assert_eq!(visible[1].id, TaskId(8));
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
        assert!(supports_individual_package_install(ManagerId::Npm));
        assert!(supports_individual_package_install(
            ManagerId::HomebrewFormula
        ));
        assert!(!supports_individual_package_install(ManagerId::Mas));
        assert!(!supports_individual_package_install(
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
