use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter, SearchRequest,
};
use helm_core::models::{
    ActionSafety, CachedSearchResult, Capability, ManagerAction, ManagerAuthority, ManagerCategory,
    ManagerDescriptor, ManagerId, PackageCandidate, PackageRef, SearchQuery, TaskStatus,
};
use helm_core::orchestration::AdapterRuntime;
use helm_core::persistence::SearchCacheStore;
use helm_core::sqlite::SqliteStore;

const TEST_CAPABILITIES: &[Capability] = &[Capability::Search];

struct SearchTestAdapter {
    descriptor: ManagerDescriptor,
}

impl SearchTestAdapter {
    fn new(manager: ManagerId) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id: manager,
                display_name: "search-test-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities: TEST_CAPABILITIES,
            },
        }
    }
}

impl ManagerAdapter for SearchTestAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, _request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        let results = vec![
            CachedSearchResult {
                result: PackageCandidate {
                    package: PackageRef {
                        manager: ManagerId::Npm,
                        name: "ripgrep".to_string(),
                    },
                    version: Some("14.1.0".to_string()),
                    summary: Some("A fast search tool".to_string()),
                },
                source_manager: ManagerId::Npm,
                originating_query: "rip".to_string(),
                cached_at: SystemTime::now(),
            },
            CachedSearchResult {
                result: PackageCandidate {
                    package: PackageRef {
                        manager: ManagerId::Npm,
                        name: "rip-csv".to_string(),
                    },
                    version: Some("1.0.0".to_string()),
                    summary: None,
                },
                source_manager: ManagerId::Npm,
                originating_query: "rip".to_string(),
                cached_at: SystemTime::now(),
            },
        ];
        Ok(AdapterResponse::SearchResults(results))
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
async fn search_results_are_persisted_to_cache_via_watcher() {
    let path = test_db_path("search-persistence-watcher");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SearchTestAdapter::new(ManagerId::Npm));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();

    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "rip".to_string(),
            issued_at: SystemTime::now(),
        },
    });

    let task_id = runtime.submit(ManagerId::Npm, request).await.unwrap();
    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(2)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    // Wait for the persistence watcher to flush search results to cache
    let mut cached = Vec::new();
    for _ in 0..30 {
        cached = store.query_local("rip", 50).unwrap();
        if cached.len() == 2 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(cached.len(), 2, "expected 2 cached search results");
    let names: Vec<&str> = cached
        .iter()
        .map(|r| r.result.package.name.as_str())
        .collect();
    assert!(names.contains(&"ripgrep"));
    assert!(names.contains(&"rip-csv"));

    let _ = std::fs::remove_file(path);
}
