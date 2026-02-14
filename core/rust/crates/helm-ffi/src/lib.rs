use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::UNIX_EPOCH;

use helm_core::adapters::homebrew::HomebrewAdapter;
use helm_core::adapters::homebrew_process::ProcessHomebrewSource;
use helm_core::adapters::mas::MasAdapter;
use helm_core::adapters::mas_process::ProcessMasSource;
use helm_core::adapters::mise::MiseAdapter;
use helm_core::adapters::mise_process::ProcessMiseSource;
use helm_core::adapters::rustup::RustupAdapter;
use helm_core::adapters::rustup_process::ProcessRustupSource;
use helm_core::adapters::softwareupdate::SoftwareUpdateAdapter;
use helm_core::adapters::softwareupdate_process::ProcessSoftwareUpdateSource;
use helm_core::adapters::{
    AdapterRequest, InstallRequest, PinRequest, SearchRequest, UninstallRequest, UnpinRequest,
    UpgradeRequest,
};
use helm_core::execution::tokio_process::TokioProcessExecutor;
use helm_core::models::{ManagerId, PackageRef, PinKind, PinRecord, SearchQuery};
use helm_core::orchestration::CancellationMode;
use helm_core::orchestration::adapter_runtime::AdapterRuntime;
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

lazy_static! {
    static ref STATE: Mutex<Option<HelmState>> = Mutex::new(None);
    static ref TASK_LABELS: Mutex<std::collections::HashMap<u64, String>> = Mutex::new(std::collections::HashMap::new());
}

fn set_task_label(task_id: helm_core::models::TaskId, label: String) {
    TASK_LABELS.lock().unwrap().insert(task_id.0, label);
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
    if STATE.lock().unwrap().is_some() {
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

    *STATE.lock().unwrap() = Some(state);

    true
}

#[unsafe(no_mangle)]
pub extern "C" fn helm_list_installed_packages() -> *mut c_char {
    let guard = STATE.lock().unwrap();
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
    let guard = STATE.lock().unwrap();
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
    let guard = STATE.lock().unwrap();
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
        label: Option<String>,
    }

    let mut labels = TASK_LABELS.lock().unwrap();
    let active_ids: std::collections::HashSet<u64> = tasks.iter().map(|task| task.id.0).collect();
    labels.retain(|task_id, _| active_ids.contains(task_id));

    let ffi_tasks: Vec<FfiTaskRecord> = tasks
        .iter()
        .map(|task| FfiTaskRecord {
            id: task.id,
            manager: task.manager,
            task_type: task.task_type,
            status: task.status,
            label: labels.get(&task.id.0).cloned(),
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
    let guard = STATE.lock().unwrap();
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
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

    let guard = STATE.lock().unwrap();
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
        let guard = STATE.lock().unwrap();
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
        let guard = STATE.lock().unwrap();
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
    let guard = STATE.lock().unwrap();
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
            FfiManagerStatus {
                manager_id: id.as_str().to_string(),
                detected: detection.map(|d| d.installed).unwrap_or(false),
                version: detection.and_then(|d| d.version.clone()),
                executable_path: detection.and_then(|d| {
                    d.executable_path
                        .as_ref()
                        .map(|p| p.to_string_lossy().to_string())
                }),
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
    let guard = STATE.lock().unwrap();
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };
    state.store.safe_mode().unwrap_or(false)
}

/// Set safe mode state. Returns true on success.
#[unsafe(no_mangle)]
pub extern "C" fn helm_set_safe_mode(enabled: bool) -> bool {
    let guard = STATE.lock().unwrap();
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };
    state.store.set_safe_mode(enabled).is_ok()
}

/// Queue upgrade tasks for supported managers using cached outdated snapshot.
///
/// - `include_pinned`: if false, pinned packages are excluded.
/// - `allow_os_updates`: explicit confirmation gate for `softwareupdate` upgrades.
#[unsafe(no_mangle)]
pub extern "C" fn helm_upgrade_all(include_pinned: bool, allow_os_updates: bool) -> bool {
    let (store, runtime, tokio_rt) = {
        let guard = STATE.lock().unwrap();
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return false,
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

        let mut homebrew_targets = Vec::new();
        let mut seen_homebrew_targets = std::collections::HashSet::new();
        let mut rustup_outdated = false;
        let mut softwareupdate_outdated = false;

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
                    if seen_homebrew_targets.insert(package.package.name.clone()) {
                        homebrew_targets.push(package.package.name);
                    }
                }
                ManagerId::Rustup => rustup_outdated = true,
                ManagerId::SoftwareUpdate => softwareupdate_outdated = true,
                _ => {}
            }
        }

        if runtime.is_manager_enabled(ManagerId::HomebrewFormula) {
            for package_name in homebrew_targets {
                let label = format!("Upgrade {} via Homebrew", package_name);
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: package_name,
                    }),
                });
                match runtime.submit(ManagerId::HomebrewFormula, request).await {
                    Ok(task_id) => set_task_label(task_id, label),
                    Err(error) => {
                        eprintln!("upgrade_all: failed to queue homebrew upgrade task: {error}");
                    }
                }
            }
        }

        if rustup_outdated && runtime.is_manager_enabled(ManagerId::Rustup) {
            let request = AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                }),
            });
            match runtime.submit(ManagerId::Rustup, request).await {
                Ok(task_id) => set_task_label(task_id, "Self-update rustup".to_string()),
                Err(error) => {
                    eprintln!("upgrade_all: failed to queue rustup self-update task: {error}");
                }
            }
        }

        if allow_os_updates
            && softwareupdate_outdated
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
                        set_task_label(task_id, "Upgrade macOS software updates".to_string())
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
/// - "rustup" (only for package "__self__")
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
    if manager_id.is_null() || package_name.is_null() {
        return -1;
    }

    let manager_cstr = unsafe { CStr::from_ptr(manager_id) };
    let manager = match manager_cstr.to_str().ok().and_then(|s| s.parse::<ManagerId>().ok()) {
        Some(manager) => manager,
        None => return -1,
    };

    let package_cstr = unsafe { CStr::from_ptr(package_name) };
    let package_name = match package_cstr.to_str() {
        Ok(name) if !name.trim().is_empty() => name.trim().to_string(),
        _ => return -1,
    };

    let (target_manager, request, label) = match manager {
        ManagerId::HomebrewFormula => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: package_name.clone(),
                }),
            }),
            format!("Upgrade {} via Homebrew", package_name),
        ),
        ManagerId::Rustup if package_name == "__self__" => (
            ManagerId::Rustup,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                }),
            }),
            "Self-update rustup".to_string(),
        ),
        _ => return -1,
    };

    let (runtime, rt_handle) = {
        let guard = STATE.lock().unwrap();
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return -1,
        };
        (state.runtime.clone(), state.rt_handle.clone())
    };

    match rt_handle.block_on(runtime.submit(target_manager, request)) {
        Ok(task_id) => {
            set_task_label(task_id, label);
            task_id.0 as i64
        }
        Err(error) => {
            eprintln!("upgrade_package: failed to queue task: {error}");
            -1
        }
    }
}

/// List pin records as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn helm_list_pins() -> *mut c_char {
    let guard = STATE.lock().unwrap();
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
    if manager_id.is_null() || package_name.is_null() {
        return false;
    }

    let manager = {
        let c_str = unsafe { CStr::from_ptr(manager_id) };
        match c_str
            .to_str()
            .ok()
            .and_then(|s| s.parse::<ManagerId>().ok())
        {
            Some(id) => id,
            None => return false,
        }
    };

    let package_name = {
        let c_str = unsafe { CStr::from_ptr(package_name) };
        match c_str.to_str() {
            Ok(value) if !value.trim().is_empty() => value.to_string(),
            _ => return false,
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
            Err(_) => return false,
        }
    };

    let (store, runtime, rt_handle) = {
        let guard = STATE.lock().unwrap();
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return false,
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
        if rt_handle
            .block_on(runtime.submit_refresh_request_response(manager, request))
            .is_err()
        {
            return false;
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
    if manager_id.is_null() || package_name.is_null() {
        return false;
    }

    let manager = {
        let c_str = unsafe { CStr::from_ptr(manager_id) };
        match c_str
            .to_str()
            .ok()
            .and_then(|s| s.parse::<ManagerId>().ok())
        {
            Some(id) => id,
            None => return false,
        }
    };

    let package_name = {
        let c_str = unsafe { CStr::from_ptr(package_name) };
        match c_str.to_str() {
            Ok(value) if !value.trim().is_empty() => value.to_string(),
            _ => return false,
        }
    };

    let (store, runtime, rt_handle) = {
        let guard = STATE.lock().unwrap();
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return false,
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
        if rt_handle
            .block_on(runtime.submit_refresh_request_response(manager, request))
            .is_err()
        {
            return false;
        }
    }

    let package_key = format!("{}:{}", manager.as_str(), package_name);
    store.remove_pin(&package_key).is_ok()
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
    if manager_id.is_null() {
        return false;
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let manager = match id_str.parse::<ManagerId>() {
        Ok(m) => m,
        Err(_) => return false,
    };

    let guard = STATE.lock().unwrap();
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };

    state.store.set_manager_enabled(manager, enabled).is_ok()
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
    if manager_id.is_null() {
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    // Map manager IDs to the formula name to install via Homebrew
    let formula_name = match id_str {
        "mise" => "mise",
        "mas" => "mas",
        _ => return -1, // Not automatable
    };

    let (runtime, rt_handle) = {
        let guard = STATE.lock().unwrap();
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return -1,
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
                format!("Install {} via Homebrew", formula_name),
            );
            task_id.0 as i64
        }
        Err(e) => {
            eprintln!("Failed to install manager {}: {}", id_str, e);
            -1
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
    if manager_id.is_null() {
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let (target_manager, request) = match id_str {
        "homebrew_formula" => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "__self__".to_string(),
                }),
            }),
        ),
        "mise" => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "mise".to_string(),
                }),
            }),
        ),
        "mas" => (
            ManagerId::HomebrewFormula,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "mas".to_string(),
                }),
            }),
        ),
        "rustup" => (
            ManagerId::Rustup,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                }),
            }),
        ),
        _ => return -1,
    };

    let (runtime, rt_handle) = {
        let guard = STATE.lock().unwrap();
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return -1,
        };
        (state.runtime.clone(), state.rt_handle.clone())
    };

    let label = match id_str {
        "homebrew_formula" => "Update Homebrew".to_string(),
        "mise" => "Update mise via Homebrew".to_string(),
        "mas" => "Update mas via Homebrew".to_string(),
        "rustup" => "Self-update rustup".to_string(),
        _ => "Update manager".to_string(),
    };

    match rt_handle.block_on(runtime.submit(target_manager, request)) {
        Ok(task_id) => {
            set_task_label(task_id, label);
            task_id.0 as i64
        }
        Err(e) => {
            eprintln!("Failed to update manager {}: {}", id_str, e);
            -1
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
    if manager_id.is_null() {
        return -1;
    }

    let c_str = unsafe { CStr::from_ptr(manager_id) };
    let id_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let (runtime, rt_handle) = {
        let guard = STATE.lock().unwrap();
        let state = match guard.as_ref() {
            Some(s) => s,
            None => return -1,
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
        _ => return -1, // Not automatable
    };

    let label = match id_str {
        "mise" => "Uninstall mise via Homebrew".to_string(),
        "mas" => "Uninstall mas via Homebrew".to_string(),
        "rustup" => "Uninstall rustup".to_string(),
        _ => "Uninstall manager".to_string(),
    };

    match rt_handle.block_on(runtime.submit(target_manager, request)) {
        Ok(task_id) => {
            set_task_label(task_id, label);
            task_id.0 as i64
        }
        Err(e) => {
            eprintln!("Failed to uninstall manager {}: {}", id_str, e);
            -1
        }
    }
}

/// Reset the database by rolling back all migrations and re-applying them.
/// Returns true on success.
#[unsafe(no_mangle)]
pub extern "C" fn helm_reset_database() -> bool {
    let guard = STATE.lock().unwrap();
    let state = match guard.as_ref() {
        Some(s) => s,
        None => return false,
    };

    // Roll back to version 0 (drops all data tables)
    if let Err(e) = state.store.apply_migration(0) {
        eprintln!("Failed to roll back migrations: {}", e);
        return false;
    }

    // Re-apply all migrations (recreates empty tables)
    if let Err(e) = state.store.migrate_to_latest() {
        eprintln!("Failed to re-apply migrations: {}", e);
        return false;
    }

    // Final cleanup: delete any task records that in-flight persistence
    // watchers may have re-inserted during the brief reset window.
    let _ = state.store.delete_all_tasks();

    true
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
