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
use helm_core::orchestration::{AdapterRuntime, CancellationMode};
use helm_core::sqlite::SqliteStore;

const TEST_CAPABILITIES: &[Capability] = &[Capability::Search];

struct SlowSearchAdapter {
    descriptor: ManagerDescriptor,
    delay: Duration,
}

impl SlowSearchAdapter {
    fn new(manager: ManagerId, delay: Duration) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id: manager,
                display_name: "slow-search-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities: TEST_CAPABILITIES,
            },
            delay,
        }
    }
}

impl ManagerAdapter for SlowSearchAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, _request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        std::thread::sleep(self.delay);
        let results = vec![CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Npm,
                    name: "slow-package".to_string(),
                },
                version: Some("1.0.0".to_string()),
                summary: Some("A slow package".to_string()),
            },
            source_manager: ManagerId::Npm,
            originating_query: "slow".to_string(),
            cached_at: SystemTime::now(),
        }];
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
async fn graceful_cancel_with_sufficient_grace_period_allows_completion() {
    let path = test_db_path("search-cancel-grace");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    // Adapter takes 200ms, grace period is 500ms → should complete
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SlowSearchAdapter::new(
        ManagerId::Npm,
        Duration::from_millis(200),
    ));
    let runtime =
        AdapterRuntime::with_all_stores([adapter], store.clone(), store.clone(), store.clone())
            .unwrap();

    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "slow".to_string(),
            issued_at: SystemTime::now(),
        },
    });

    let task_id = runtime.submit(ManagerId::Npm, request).await.unwrap();

    // Wait a bit for task to start, then cancel gracefully
    tokio::time::sleep(Duration::from_millis(50)).await;
    let cancel_result = runtime
        .cancel(
            task_id,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(500),
            },
        )
        .await;
    assert!(cancel_result.is_ok());

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(2)))
        .await
        .unwrap();

    // With sufficient grace period, the task completes
    assert_eq!(snapshot.runtime.status, TaskStatus::Completed);

    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn graceful_cancel_with_short_grace_period_cancels_long_task() {
    let path = test_db_path("search-cancel-short");
    let store = Arc::new(SqliteStore::new(&path));
    store.migrate_to_latest().unwrap();

    // Adapter takes 5s, grace period is 100ms → should be cancelled
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(SlowSearchAdapter::new(
        ManagerId::Npm,
        Duration::from_secs(5),
    ));
    let runtime =
        AdapterRuntime::with_all_stores([adapter], store.clone(), store.clone(), store.clone())
            .unwrap();

    let request = AdapterRequest::Search(SearchRequest {
        query: SearchQuery {
            text: "slow".to_string(),
            issued_at: SystemTime::now(),
        },
    });

    let task_id = runtime.submit(ManagerId::Npm, request).await.unwrap();

    // Wait a bit for task to start, then cancel with short grace period
    tokio::time::sleep(Duration::from_millis(50)).await;
    let cancel_result = runtime
        .cancel(
            task_id,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(100),
            },
        )
        .await;
    assert!(cancel_result.is_ok());

    let snapshot = runtime
        .wait_for_terminal(task_id, Some(Duration::from_secs(2)))
        .await
        .unwrap();

    assert_eq!(snapshot.runtime.status, TaskStatus::Cancelled);

    let _ = std::fs::remove_file(path);
}
