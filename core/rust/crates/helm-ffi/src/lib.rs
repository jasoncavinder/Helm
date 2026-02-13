use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use helm_core::adapters::homebrew::HomebrewAdapter;
use helm_core::adapters::homebrew_process::ProcessHomebrewSource;
use helm_core::adapters::mise::MiseAdapter;
use helm_core::adapters::mise_process::ProcessMiseSource;
use helm_core::adapters::rustup::RustupAdapter;
use helm_core::adapters::rustup_process::ProcessRustupSource;
use helm_core::adapters::{AdapterRequest, SearchRequest};
use helm_core::execution::tokio_process::TokioProcessExecutor;
use helm_core::models::{ManagerId, SearchQuery};
use helm_core::orchestration::CancellationMode;
use helm_core::orchestration::adapter_runtime::AdapterRuntime;
use helm_core::persistence::{DetectionStore, PackageStore, SearchCacheStore, TaskStore};
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

    let adapters: Vec<Arc<dyn helm_core::adapters::ManagerAdapter>> =
        vec![homebrew_adapter, mise_adapter, rustup_adapter];

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

    // List recent 50 tasks
    let tasks = match state.store.list_recent_tasks(50) {
        Ok(tasks) => tasks,
        Err(e) => {
            eprintln!("Failed to list tasks: {}", e);
            return std::ptr::null_mut();
        }
    };

    let json = match serde_json::to_string(&tasks) {
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

    let results = match state.store.query_local(query_str, 50) {
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
                executable_path: detection
                    .and_then(|d| d.executable_path.as_ref().map(|p| p.to_string_lossy().to_string())),
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
