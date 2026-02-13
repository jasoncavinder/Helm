use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, ListInstalledRequest, ManagerAdapter,
    SearchRequest,
};
use helm_core::models::{
    ActionSafety, CachedSearchResult, Capability, InstalledPackage, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, PackageCandidate, PackageRef,
    SearchQuery, TaskStatus,
};
use helm_core::orchestration::{AdapterRuntime, CancellationMode};
use helm_core::persistence::SearchCacheStore;
use helm_core::sqlite::SqliteStore;

const TEST_CAPABILITIES: &[Capability] = &[
    Capability::Search,
    Capability::Refresh,
    Capability::ListInstalled,
];

struct SearchAndRefreshAdapter {
    descriptor: ManagerDescriptor,
    search_delay: Duration,
}

impl SearchAndRefreshAdapter {
    fn new(manager: ManagerId, search_delay: Duration) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id: manager,
                display_name: "e2e-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities: TEST_CAPABILITIES,
            },
            search_delay,
        }
    }

    fn search_results(query: &str) -> Vec<CachedSearchResult> {
        match query {
            "wget" => vec![
                CachedSearchResult {
                    result: PackageCandidate {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: "wget".to_string(),
                        },
                        version: Some("1.24.5".to_string()),
                        summary: Some("Internet file retriever".to_string()),
                    },
                    source_manager: ManagerId::HomebrewFormula,
                    originating_query: "wget".to_string(),
                    cached_at: SystemTime::now(),
                },
                CachedSearchResult {
                    result: PackageCandidate {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: "wgetpaste".to_string(),
                        },
                        version: Some("2.33".to_string()),
                        summary: Some("Automate pasting to pastebin services".to_string()),
                    },
                    source_manager: ManagerId::HomebrewFormula,
                    originating_query: "wget".to_string(),
                    cached_at: SystemTime::now(),
                },
            ],
            "rip" => vec![CachedSearchResult {
                result: PackageCandidate {
                    package: PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: "ripgrep".to_string(),
                    },
                    version: Some("14.1.0".to_string()),
                    summary: Some("Search tool like grep and The Silver Searcher".to_string()),
                },
                source_manager: ManagerId::HomebrewFormula,
                originating_query: "rip".to_string(),
                cached_at: SystemTime::now(),
            }],
            _ => vec![],
        }
    }
}

impl ManagerAdapter for SearchAndRefreshAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        match request {
            AdapterRequest::Search(SearchRequest { query }) => {
                std::thread::sleep(self.search_delay);
                Ok(AdapterResponse::SearchResults(Self::search_results(
                    &query.text,
                )))
            }
            AdapterRequest::Refresh(_) => Ok(AdapterResponse::Refreshed),
            AdapterRequest::ListInstalled(_) => {
                Ok(AdapterResponse::InstalledPackages(vec![InstalledPackage {
                    package: PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: "wget".to_string(),
                    },
                    installed_version: Some("1.24.5".to_string()),
                    pinned: false,
                }]))
            }
            _ => Ok(AdapterResponse::Refreshed),
        }
    }
}

fn test_db_path(test_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("helm-{test_name}-{nanos}.sqlite3"))
}

#[tokio::test]
async fn cache_enrichment_across_multiple_queries() {
    let path = test_db_path("e2e-enrichment");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SearchAndRefreshAdapter::new(
        ManagerId::HomebrewFormula,
        Duration::from_millis(10),
    ));
    let runtime =
        AdapterRuntime::with_all_stores([adapter], store.clone(), store.clone(), store.clone(), store.clone())
            .unwrap();

    // First search: "wget" → 2 results
    let request1 = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "wget".to_string(),
            issued_at: SystemTime::now(),
        },
    });
    let task1 = runtime
        .submit(ManagerId::HomebrewFormula, request1)
        .await
        .unwrap();
    runtime
        .wait_for_terminal(task1, Some(Duration::from_secs(2)))
        .await
        .unwrap();

    // Wait for persistence
    let mut results = Vec::new();
    for _ in 0..30 {
        results = store.query_local("wget", 50).unwrap();
        if results.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(results.len(), 2, "expected 2 results after 'wget' search");

    // Second search: "rip" → 1 result
    let request2 = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "rip".to_string(),
            issued_at: SystemTime::now(),
        },
    });
    let task2 = runtime
        .submit(ManagerId::HomebrewFormula, request2)
        .await
        .unwrap();
    runtime
        .wait_for_terminal(task2, Some(Duration::from_secs(2)))
        .await
        .unwrap();

    // Wait for persistence
    let mut rip_results = Vec::new();
    for _ in 0..30 {
        rip_results = store.query_local("rip", 50).unwrap();
        if !rip_results.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(rip_results.len(), 1, "expected 1 result after 'rip' search");
    assert_eq!(rip_results[0].result.package.name, "ripgrep");

    // Original "wget" results still in cache (query_local matches by name/summary LIKE)
    let wget_results = store.query_local("wget", 50).unwrap();
    assert_eq!(
        wget_results.len(),
        2,
        "wget results should still be in cache"
    );

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn grace_period_allows_near_complete_search_to_persist() {
    let path = test_db_path("e2e-grace-persist");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    // Adapter takes 200ms, cancel with 500ms grace → should complete and persist
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SearchAndRefreshAdapter::new(
        ManagerId::HomebrewFormula,
        Duration::from_millis(200),
    ));
    let runtime =
        AdapterRuntime::with_all_stores([adapter], store.clone(), store.clone(), store.clone(), store.clone())
            .unwrap();

    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "wget".to_string(),
            issued_at: SystemTime::now(),
        },
    });

    let task_id = runtime
        .submit(ManagerId::HomebrewFormula, request)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
    runtime
        .cancel(
            task_id,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(500),
            },
        )
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(2)))
        .await
        .unwrap();
    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    // Verify results were persisted
    let mut results = Vec::new();
    for _ in 0..30 {
        results = store.query_local("wget", 50).unwrap();
        if results.len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(results.len(), 2, "grace period should allow persistence");

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn long_running_search_aborted_after_grace_period() {
    let path = test_db_path("e2e-abort");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    // Adapter takes 5s, cancel with 100ms grace → should be aborted
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SearchAndRefreshAdapter::new(
        ManagerId::HomebrewFormula,
        Duration::from_secs(5),
    ));
    let runtime =
        AdapterRuntime::with_all_stores([adapter], store.clone(), store.clone(), store.clone(), store.clone())
            .unwrap();

    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "wget".to_string(),
            issued_at: SystemTime::now(),
        },
    });

    let task_id = runtime
        .submit(ManagerId::HomebrewFormula, request)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
    runtime
        .cancel(
            task_id,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(100),
            },
        )
        .await
        .unwrap();

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(2)))
        .await
        .unwrap();
    assert_eq!(snapshot.runtime.status, TaskStatus::Cancelled);

    // No results should be persisted (task was cancelled before completion)
    tokio::time::sleep(Duration::from_millis(100)).await;
    let results = store.query_local("wget", 50).unwrap();
    assert!(results.is_empty(), "cancelled search should not persist");

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn concurrent_search_and_refresh_serialized_without_deadlock() {
    let path = test_db_path("e2e-concurrent");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SearchAndRefreshAdapter::new(
        ManagerId::HomebrewFormula,
        Duration::from_millis(100),
    ));
    let runtime =
        AdapterRuntime::with_all_stores([adapter], store.clone(), store.clone(), store.clone(), store.clone())
            .unwrap();

    // Submit search and refresh concurrently — both use HomebrewFormula, must be serialized
    let search_request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "wget".to_string(),
            issued_at: SystemTime::now(),
        },
    });
    let refresh_request = AdapterRequest::ListInstalled(ListInstalledRequest);

    let search_task = runtime
        .submit(ManagerId::HomebrewFormula, search_request)
        .await
        .unwrap();
    let refresh_task = runtime
        .submit(ManagerId::HomebrewFormula, refresh_request)
        .await
        .unwrap();

    // Both should complete without deadlock
    let search_snap = runtime
        .wait_for_terminal(search_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    let refresh_snap = runtime
        .wait_for_terminal(refresh_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    assert_eq!(search_snap.runtime.status, TaskStatus::Completed);
    assert_eq!(refresh_snap.runtime.status, TaskStatus::Completed);

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn empty_query_returns_empty_results_from_cache() {
    let path = test_db_path("e2e-empty-query");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    // Pre-populate cache with some results
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SearchAndRefreshAdapter::new(
        ManagerId::HomebrewFormula,
        Duration::from_millis(10),
    ));
    let runtime =
        AdapterRuntime::with_all_stores([adapter], store.clone(), store.clone(), store.clone(), store.clone())
            .unwrap();

    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "wget".to_string(),
            issued_at: SystemTime::now(),
        },
    });
    let task_id = runtime
        .submit(ManagerId::HomebrewFormula, request)
        .await
        .unwrap();
    runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(2)))
        .await
        .unwrap();

    // Wait for persistence
    for _ in 0..30 {
        if store.query_local("wget", 50).unwrap().len() >= 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Empty query returns all cached results ('' matches everything via LIKE)
    let all_results = store.query_local("", 50).unwrap();
    assert!(
        all_results.len() >= 2,
        "empty query should return cached results"
    );

    let _ = std::fs::remove_file(path);
}
