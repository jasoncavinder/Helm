use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use helm_core::adapters::homebrew::HomebrewAdapter;
use helm_core::adapters::homebrew_process::ProcessHomebrewSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, ListInstalledRequest, ListOutdatedRequest,
};
use helm_core::execution::tokio_process::TokioProcessExecutor;
use helm_core::models::ManagerId;
use helm_core::orchestration::AdapterTaskTerminalState;
use helm_core::orchestration::adapter_runtime::AdapterRuntime;
use helm_core::persistence::{PackageStore, TaskStore};
use helm_core::sqlite::SqliteStore;
use lazy_static::lazy_static;

struct HelmState {
    store: Arc<SqliteStore>,
    runtime: Arc<AdapterRuntime>,
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

    // Initialize Homebrew Adapter
    let executor = Arc::new(TokioProcessExecutor);
    let source = ProcessHomebrewSource::new(executor);
    let adapter = Arc::new(HomebrewAdapter::new(source));
    let adapters = vec![adapter as Arc<dyn helm_core::adapters::ManagerAdapter>];

    // Initialize Orchestration
    let runtime = match AdapterRuntime::with_task_store(adapters, store.clone()) {
        Ok(rt) => Arc::new(rt),
        Err(e) => {
            eprintln!("Failed to create adapter runtime: {}", e);
            return false;
        }
    };

    let state = HelmState {
        store,
        runtime,
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
    let store = state.store.clone();

    state._tokio_rt.spawn(async move {
        // Fetch installed packages and persist to store
        if let Some(AdapterResponse::InstalledPackages(pkgs)) = submit_and_wait(
            &runtime,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
            && let Err(e) = store.upsert_installed(&pkgs)
        {
            eprintln!("Failed to persist installed packages: {e}");
        }

        // Fetch outdated packages and persist to store
        if let Some(AdapterResponse::OutdatedPackages(pkgs)) =
            submit_and_wait(&runtime, AdapterRequest::ListOutdated(ListOutdatedRequest)).await
            && let Err(e) = store.upsert_outdated(&pkgs)
        {
            eprintln!("Failed to persist outdated packages: {e}");
        }
    });

    true
}

async fn submit_and_wait(
    runtime: &AdapterRuntime,
    request: AdapterRequest,
) -> Option<AdapterResponse> {
    let task_id = match runtime.submit(ManagerId::HomebrewFormula, request).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to submit task: {e}");
            return None;
        }
    };

    let snapshot = match runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(60)))
        .await
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to wait for task: {e}");
            return None;
        }
    };

    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(response)) => Some(response),
        Some(AdapterTaskTerminalState::Failed(e)) => {
            eprintln!("Task failed: {e}");
            None
        }
        Some(AdapterTaskTerminalState::Cancelled(_)) => {
            eprintln!("Task was cancelled");
            None
        }
        None => None,
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
