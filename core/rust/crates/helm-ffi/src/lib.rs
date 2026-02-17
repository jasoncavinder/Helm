use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use std::time::UNIX_EPOCH;

use helm_core::adapters::cargo::CargoAdapter;
use helm_core::adapters::cargo_binstall::CargoBinstallAdapter;
use helm_core::adapters::cargo_binstall_process::ProcessCargoBinstallSource;
use helm_core::adapters::cargo_process::ProcessCargoSource;
use helm_core::adapters::homebrew::HomebrewAdapter;
use helm_core::adapters::homebrew_process::ProcessHomebrewSource;
use helm_core::adapters::mas::MasAdapter;
use helm_core::adapters::mas_process::ProcessMasSource;
use helm_core::adapters::mise::MiseAdapter;
use helm_core::adapters::mise_process::ProcessMiseSource;
use helm_core::adapters::npm::NpmAdapter;
use helm_core::adapters::npm_process::ProcessNpmSource;
use helm_core::adapters::pip::PipAdapter;
use helm_core::adapters::pip_process::ProcessPipSource;
use helm_core::adapters::pipx::PipxAdapter;
use helm_core::adapters::pipx_process::ProcessPipxSource;
use helm_core::adapters::pnpm::PnpmAdapter;
use helm_core::adapters::pnpm_process::ProcessPnpmSource;
use helm_core::adapters::poetry::PoetryAdapter;
use helm_core::adapters::poetry_process::ProcessPoetrySource;
use helm_core::adapters::rubygems::RubyGemsAdapter;
use helm_core::adapters::rubygems_process::ProcessRubyGemsSource;
use helm_core::adapters::rustup::RustupAdapter;
use helm_core::adapters::rustup_process::ProcessRustupSource;
use helm_core::adapters::softwareupdate::SoftwareUpdateAdapter;
use helm_core::adapters::softwareupdate_process::ProcessSoftwareUpdateSource;
use helm_core::adapters::yarn::YarnAdapter;
use helm_core::adapters::yarn_process::ProcessYarnSource;
use helm_core::adapters::{
    AdapterRequest, InstallRequest, PinRequest, SearchRequest, UninstallRequest, UnpinRequest,
    UpgradeRequest,
};
use helm_core::execution::tokio_process::TokioProcessExecutor;
use helm_core::models::{
    DetectionInfo, HomebrewKegPolicy, ManagerId, OutdatedPackage, PackageRef, PinKind, PinRecord,
    SearchQuery,
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
    let mise_adapter = Arc::new(MiseAdapter::new(ProcessMiseSource::new(executor.clone())));
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
    let rustup_adapter = Arc::new(RustupAdapter::new(ProcessRustupSource::new(
        executor.clone(),
    )));
    let softwareupdate_adapter = Arc::new(SoftwareUpdateAdapter::new(
        ProcessSoftwareUpdateSource::new(executor.clone()),
    ));
    let mas_adapter = Arc::new(MasAdapter::new(ProcessMasSource::new(executor.clone())));

    let adapters: Vec<Arc<dyn helm_core::adapters::ManagerAdapter>> = vec![
        homebrew_adapter,
        mise_adapter,
        npm_adapter,
        pnpm_adapter,
        yarn_adapter,
        cargo_adapter,
        cargo_binstall_adapter,
        pip_adapter,
        pipx_adapter,
        poetry_adapter,
        rubygems_adapter,
        rustup_adapter,
        softwareupdate_adapter,
        mas_adapter,
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

    // Auto-prune completed/failed/cancelled tasks older than 5 minutes
    let _ = state.store.prune_completed_tasks(300);

    // List recent 50 tasks
    let tasks = match state.store.list_recent_tasks(50) {
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
    if query.is_null() {
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(query) };
    let query_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let (runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return -1,
        };
        (state.runtime.clone(), state.rt_handle.clone())
    };

    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: query_str.to_string(),
            issued_at: std::time::SystemTime::now(),
        },
    });

    match rt_handle.block_on(runtime.submit(ManagerId::HomebrewFormula, request)) {
        Ok(task_id) => task_id.0 as i64,
        Err(e) => {
            eprintln!("Failed to submit remote search: {}", e);
            -1
        }
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

    let implemented_ids: &[ManagerId] = &[
        ManagerId::HomebrewFormula,
        ManagerId::Mise,
        ManagerId::Npm,
        ManagerId::Pnpm,
        ManagerId::Yarn,
        ManagerId::Cargo,
        ManagerId::CargoBinstall,
        ManagerId::Pip,
        ManagerId::Pipx,
        ManagerId::Poetry,
        ManagerId::RubyGems,
        ManagerId::Rustup,
        ManagerId::SoftwareUpdate,
        ManagerId::Mas,
    ];

    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct FfiManagerStatus {
        manager_id: String,
        detected: bool,
        version: Option<String>,
        executable_path: Option<String>,
        enabled: bool,
        is_implemented: bool,
    }

    let statuses: Vec<FfiManagerStatus> = ManagerId::ALL
        .iter()
        .map(|&id| {
            let detection = detection_map.get(&id);
            let enabled = pref_map.get(&id).copied().unwrap_or(true);
            let is_implemented = implemented_ids.contains(&id);
            let mut detected = detection.map(|d| d.installed).unwrap_or(false);
            let executable_path = detection.and_then(|d| {
                normalize_nonempty(
                    d.executable_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string()),
                )
            });
            let mut version = detection.and_then(|d| normalize_nonempty(d.version.clone()));

            // Homebrew detection/version probing is occasionally flaky during first detection.
            // If status is missing or incomplete, probe directly from brew.
            if id == ManagerId::HomebrewFormula
                && (version.is_none() || !detected)
                && let Some(probed) =
                    probe_homebrew_version(detection.and_then(|d| d.executable_path.as_deref()))
            {
                version = Some(probed.clone());
                detected = true;
                if let Some(existing) = detection {
                    let refreshed = DetectionInfo {
                        installed: true,
                        executable_path: existing.executable_path.clone(),
                        version: Some(probed),
                    };
                    let _ = state.store.upsert_detection(id, &refreshed);
                } else {
                    let refreshed = DetectionInfo {
                        installed: true,
                        executable_path: None,
                        version: Some(probed),
                    };
                    let _ = state.store.upsert_detection(id, &refreshed);
                }
            }
            FfiManagerStatus {
                manager_id: id.as_str().to_string(),
                detected,
                version,
                executable_path,
                enabled,
                is_implemented,
            }
        })
        .collect();

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
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::Npm, request).await {
                    eprintln!("upgrade_all: failed to queue npm upgrade task: {error}");
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Pnpm) {
            for package_name in targets.pnpm {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Pnpm,
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::Pnpm, request).await {
                    eprintln!("upgrade_all: failed to queue pnpm upgrade task: {error}");
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Yarn) {
            for package_name in targets.yarn {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Yarn,
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::Yarn, request).await {
                    eprintln!("upgrade_all: failed to queue yarn upgrade task: {error}");
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Cargo) {
            for package_name in targets.cargo {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Cargo,
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::Cargo, request).await {
                    eprintln!("upgrade_all: failed to queue cargo upgrade task: {error}");
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::CargoBinstall) {
            for package_name in targets.cargo_binstall {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::CargoBinstall,
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::CargoBinstall, request).await {
                    eprintln!("upgrade_all: failed to queue cargo-binstall upgrade task: {error}");
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Pip) {
            for package_name in targets.pip {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Pip,
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::Pip, request).await {
                    eprintln!("upgrade_all: failed to queue pip upgrade task: {error}");
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Pipx) {
            for package_name in targets.pipx {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Pipx,
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::Pipx, request).await {
                    eprintln!("upgrade_all: failed to queue pipx upgrade task: {error}");
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::Poetry) {
            for package_name in targets.poetry {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Poetry,
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::Poetry, request).await {
                    eprintln!("upgrade_all: failed to queue poetry upgrade task: {error}");
                }
            }
        }

        if runtime.is_manager_enabled(ManagerId::RubyGems) {
            for package_name in targets.rubygems {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::RubyGems,
                        name: package_name,
                    }),
                });
                if let Err(error) = runtime.submit(ManagerId::RubyGems, request).await {
                    eprintln!("upgrade_all: failed to queue rubygems upgrade task: {error}");
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
            None,
            Vec::new(),
        ),
        ManagerId::Pnpm => (
            ManagerId::Pnpm,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pnpm,
                    name: package_name.clone(),
                }),
            }),
            None,
            Vec::new(),
        ),
        ManagerId::Yarn => (
            ManagerId::Yarn,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Yarn,
                    name: package_name.clone(),
                }),
            }),
            None,
            Vec::new(),
        ),
        ManagerId::Cargo => (
            ManagerId::Cargo,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Cargo,
                    name: package_name.clone(),
                }),
            }),
            None,
            Vec::new(),
        ),
        ManagerId::CargoBinstall => (
            ManagerId::CargoBinstall,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::CargoBinstall,
                    name: package_name.clone(),
                }),
            }),
            None,
            Vec::new(),
        ),
        ManagerId::Pip => (
            ManagerId::Pip,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pip,
                    name: package_name.clone(),
                }),
            }),
            None,
            Vec::new(),
        ),
        ManagerId::Pipx => (
            ManagerId::Pipx,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Pipx,
                    name: package_name.clone(),
                }),
            }),
            None,
            Vec::new(),
        ),
        ManagerId::Poetry => (
            ManagerId::Poetry,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Poetry,
                    name: package_name.clone(),
                }),
            }),
            None,
            Vec::new(),
        ),
        ManagerId::RubyGems => (
            ManagerId::RubyGems,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::RubyGems,
                    name: package_name.clone(),
                }),
            }),
            None,
            Vec::new(),
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

    let (runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (state.runtime.clone(), state.rt_handle.clone())
    };

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

    let (runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (state.runtime.clone(), state.rt_handle.clone())
    };

    let request = AdapterRequest::Install(InstallRequest {
        package: PackageRef {
            manager: ManagerId::HomebrewFormula,
            name: formula_name.to_string(),
        },
        version: None,
    });

    match rt_handle.block_on(runtime.submit(ManagerId::HomebrewFormula, request)) {
        Ok(task_id) => {
            set_task_label(
                task_id,
                "service.task.label.install.homebrew_formula",
                &[("package", formula_name.to_string())],
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

    let (runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (state.runtime.clone(), state.rt_handle.clone())
    };

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

    let (runtime, rt_handle) = {
        let guard = lock_or_recover(&STATE, "state");
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return return_error_i64("service.error.internal"),
        };
        (state.runtime.clone(), state.rt_handle.clone())
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
        collect_upgrade_all_targets, homebrew_probe_candidates, parse_homebrew_config_version,
    };
    use helm_core::models::{ManagerId, OutdatedPackage, PackageRef};
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
