use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use helm_core::adapters::manager::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter,
};
use helm_core::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
};
use helm_core::orchestration::AdapterRuntime;

struct TimestampedAdapter {
    descriptor: ManagerDescriptor,
    delay: Duration,
    completion_order: Arc<AtomicU64>,
}

impl TimestampedAdapter {
    fn new(
        id: ManagerId,
        authority: ManagerAuthority,
        delay: Duration,
        completion_order: Arc<AtomicU64>,
    ) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id,
                display_name: "timestamped",
                category: ManagerCategory::ToolRuntime,
                authority,
                capabilities: &[
                    Capability::Detect,
                    Capability::Refresh,
                    Capability::ListInstalled,
                    Capability::ListOutdated,
                ],
            },
            delay,
            completion_order,
        }
    }
}

impl ManagerAdapter for TimestampedAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        // Simulate work with a blocking sleep
        std::thread::sleep(self.delay);

        match request {
            AdapterRequest::Detect(_) => Ok(AdapterResponse::Detection(DetectionInfo {
                installed: true,
                executable_path: None,
                version: Some("1.0.0".to_string()),
            })),
            AdapterRequest::ListInstalled(_) => Ok(AdapterResponse::InstalledPackages(Vec::new())),
            AdapterRequest::ListOutdated(_) => {
                // Record completion order when outdated finishes (last task per phase)
                self.completion_order.fetch_add(1, Ordering::SeqCst);
                Ok(AdapterResponse::OutdatedPackages(Vec::new()))
            }
            _ => Err(CoreError {
                manager: Some(self.descriptor.id),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "not supported".to_string(),
            }),
        }
    }
}

struct FailingAdapter {
    descriptor: ManagerDescriptor,
}

impl FailingAdapter {
    fn new(id: ManagerId, authority: ManagerAuthority) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id,
                display_name: "failing",
                category: ManagerCategory::Language,
                authority,
                capabilities: &[
                    Capability::Detect,
                    Capability::Refresh,
                    Capability::ListInstalled,
                    Capability::ListOutdated,
                ],
            },
        }
    }
}

impl ManagerAdapter for FailingAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        Err(CoreError {
            manager: Some(self.descriptor.id),
            task: None,
            action: Some(request.action()),
            kind: CoreErrorKind::ProcessFailure,
            message: "simulated failure".to_string(),
        })
    }
}

struct CapabilitySparseAdapter {
    descriptor: ManagerDescriptor,
    detect_installed: bool,
    detect_calls: Arc<AtomicU64>,
    list_outdated_calls: Arc<AtomicU64>,
}

impl CapabilitySparseAdapter {
    fn new(
        id: ManagerId,
        authority: ManagerAuthority,
        capabilities: &'static [Capability],
        detect_installed: bool,
        detect_calls: Arc<AtomicU64>,
        list_outdated_calls: Arc<AtomicU64>,
    ) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id,
                display_name: "cap-sparse",
                category: ManagerCategory::SystemOs,
                authority,
                capabilities,
            },
            detect_installed,
            detect_calls,
            list_outdated_calls,
        }
    }
}

impl ManagerAdapter for CapabilitySparseAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        match request {
            AdapterRequest::Detect(_) => {
                self.detect_calls.fetch_add(1, Ordering::SeqCst);
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed: self.detect_installed,
                    executable_path: None,
                    version: if self.detect_installed {
                        Some("1.0.0".to_string())
                    } else {
                        None
                    },
                }))
            }
            AdapterRequest::ListOutdated(_) => {
                self.list_outdated_calls.fetch_add(1, Ordering::SeqCst);
                Ok(AdapterResponse::OutdatedPackages(Vec::new()))
            }
            _ => Err(CoreError {
                manager: Some(self.descriptor.id),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "unsupported".to_string(),
            }),
        }
    }
}

#[tokio::test]
async fn authoritative_phase_completes_before_guarded_phase() {
    let completion_order = Arc::new(AtomicU64::new(0));

    let mise: Arc<dyn ManagerAdapter> = Arc::new(TimestampedAdapter::new(
        ManagerId::Mise,
        ManagerAuthority::Authoritative,
        Duration::from_millis(10),
        completion_order.clone(),
    ));

    let brew: Arc<dyn ManagerAdapter> = Arc::new(TimestampedAdapter::new(
        ManagerId::HomebrewFormula,
        ManagerAuthority::Guarded,
        Duration::from_millis(10),
        completion_order.clone(),
    ));

    let runtime = AdapterRuntime::new([mise, brew]).unwrap();
    let results = runtime.refresh_all_ordered().await;

    assert_eq!(results.len(), 2);
    for (_, result) in &results {
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn failure_isolation_one_manager_failing_does_not_block_others() {
    let completion_order = Arc::new(AtomicU64::new(0));

    let mise: Arc<dyn ManagerAdapter> = Arc::new(TimestampedAdapter::new(
        ManagerId::Mise,
        ManagerAuthority::Authoritative,
        Duration::from_millis(10),
        completion_order.clone(),
    ));

    let failing: Arc<dyn ManagerAdapter> = Arc::new(FailingAdapter::new(
        ManagerId::Npm,
        ManagerAuthority::Standard,
    ));

    let brew: Arc<dyn ManagerAdapter> = Arc::new(TimestampedAdapter::new(
        ManagerId::HomebrewFormula,
        ManagerAuthority::Guarded,
        Duration::from_millis(10),
        completion_order.clone(),
    ));

    let runtime = AdapterRuntime::new([mise, failing, brew]).unwrap();
    let results = runtime.refresh_all_ordered().await;

    // Should have results for at least mise and brew (npm fails)
    let succeeded: Vec<_> = results.iter().filter(|(_, r)| r.is_ok()).collect();
    let failed: Vec<_> = results.iter().filter(|(_, r)| r.is_err()).collect();

    assert_eq!(succeeded.len(), 2);
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].0, ManagerId::Npm);
}

#[tokio::test]
async fn parallel_within_authoritative_phase() {
    let completion_order = Arc::new(AtomicU64::new(0));

    let mise: Arc<dyn ManagerAdapter> = Arc::new(TimestampedAdapter::new(
        ManagerId::Mise,
        ManagerAuthority::Authoritative,
        Duration::from_millis(50),
        completion_order.clone(),
    ));

    let rustup: Arc<dyn ManagerAdapter> = Arc::new(TimestampedAdapter::new(
        ManagerId::Rustup,
        ManagerAuthority::Authoritative,
        Duration::from_millis(50),
        completion_order.clone(),
    ));

    let runtime = AdapterRuntime::new([mise, rustup]).unwrap();

    let start = std::time::Instant::now();
    let results = runtime.refresh_all_ordered().await;
    let elapsed = start.elapsed();

    assert_eq!(results.len(), 2);
    for (_, result) in &results {
        assert!(result.is_ok());
    }

    // If truly parallel, both should complete in roughly 50ms * 2 tasks per manager
    // (ListInstalled + ListOutdated, serialized per manager), not 50ms * 4 tasks
    // Give generous margin for CI
    assert!(
        elapsed < Duration::from_millis(1000),
        "Expected parallel execution within single phase, took {elapsed:?}"
    );
}

#[tokio::test]
async fn refresh_all_ordered_skips_missing_list_installed_capability() {
    const SWUPD_CAPS: &[Capability] = &[
        Capability::Detect,
        Capability::Refresh,
        Capability::ListOutdated,
    ];
    let detect_calls = Arc::new(AtomicU64::new(0));
    let list_outdated_calls = Arc::new(AtomicU64::new(0));
    let swupd: Arc<dyn ManagerAdapter> = Arc::new(CapabilitySparseAdapter::new(
        ManagerId::SoftwareUpdate,
        ManagerAuthority::Guarded,
        SWUPD_CAPS,
        true,
        detect_calls,
        list_outdated_calls.clone(),
    ));

    let runtime = AdapterRuntime::new([swupd]).unwrap();
    let results = runtime.refresh_all_ordered().await;

    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_ok());
    assert_eq!(list_outdated_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn refresh_all_ordered_skips_list_actions_for_not_installed_manager() {
    const SWUPD_CAPS: &[Capability] = &[
        Capability::Detect,
        Capability::Refresh,
        Capability::ListOutdated,
    ];
    let detect_calls = Arc::new(AtomicU64::new(0));
    let list_outdated_calls = Arc::new(AtomicU64::new(0));
    let swupd: Arc<dyn ManagerAdapter> = Arc::new(CapabilitySparseAdapter::new(
        ManagerId::SoftwareUpdate,
        ManagerAuthority::Guarded,
        SWUPD_CAPS,
        false,
        detect_calls,
        list_outdated_calls.clone(),
    ));

    let runtime = AdapterRuntime::new([swupd]).unwrap();
    let results = runtime.refresh_all_ordered().await;

    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_ok());
    assert_eq!(list_outdated_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn detect_all_ordered_runs_detect_without_refresh_actions() {
    const SWUPD_CAPS: &[Capability] = &[
        Capability::Detect,
        Capability::Refresh,
        Capability::ListOutdated,
    ];
    let detect_calls = Arc::new(AtomicU64::new(0));
    let list_outdated_calls = Arc::new(AtomicU64::new(0));
    let swupd: Arc<dyn ManagerAdapter> = Arc::new(CapabilitySparseAdapter::new(
        ManagerId::SoftwareUpdate,
        ManagerAuthority::Guarded,
        SWUPD_CAPS,
        true,
        detect_calls.clone(),
        list_outdated_calls.clone(),
    ));

    let runtime = AdapterRuntime::new([swupd]).unwrap();
    let results = runtime.detect_all_ordered().await;

    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_ok());
    assert_eq!(detect_calls.load(Ordering::SeqCst), 1);
    assert_eq!(list_outdated_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn detect_all_ordered_includes_detection_only_authority() {
    const DETECT_ONLY_CAPS: &[Capability] = &[Capability::Detect];
    let detect_calls = Arc::new(AtomicU64::new(0));
    let list_outdated_calls = Arc::new(AtomicU64::new(0));
    let sparkle: Arc<dyn ManagerAdapter> = Arc::new(CapabilitySparseAdapter::new(
        ManagerId::Sparkle,
        ManagerAuthority::DetectionOnly,
        DETECT_ONLY_CAPS,
        false,
        detect_calls.clone(),
        list_outdated_calls.clone(),
    ));

    let runtime = AdapterRuntime::new([sparkle]).unwrap();
    let results = runtime.detect_all_ordered().await;

    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_ok());
    assert_eq!(detect_calls.load(Ordering::SeqCst), 1);
    assert_eq!(list_outdated_calls.load(Ordering::SeqCst), 0);
}
