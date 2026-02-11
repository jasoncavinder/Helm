use std::sync::atomic::{AtomicUsize, Ordering};

use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, ManagerAdapter, ensure_request_supported,
    execute_with_capability_check,
};
use helm_core::models::{
    ActionSafety, Capability, CoreErrorKind, ManagerAction, ManagerAuthority, ManagerCategory,
    ManagerDescriptor, ManagerId,
};

struct CountingAdapter {
    descriptor: ManagerDescriptor,
    execute_calls: AtomicUsize,
}

impl CountingAdapter {
    fn new(capabilities: &'static [Capability]) -> Self {
        Self {
            descriptor: ManagerDescriptor {
                id: ManagerId::Npm,
                display_name: "test-adapter",
                category: ManagerCategory::Language,
                authority: ManagerAuthority::Standard,
                capabilities,
            },
            execute_calls: AtomicUsize::new(0),
        }
    }
}

impl ManagerAdapter for CountingAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        &self.descriptor
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(
        &self,
        _request: AdapterRequest,
    ) -> Result<AdapterResponse, helm_core::models::CoreError> {
        self.execute_calls.fetch_add(1, Ordering::SeqCst);
        Ok(AdapterResponse::Refreshed)
    }
}

#[test]
fn ensure_request_supported_returns_structured_error_for_missing_capability() {
    let adapter = CountingAdapter::new(&[Capability::Detect]);
    let request = AdapterRequest::Install(helm_core::adapters::InstallRequest {
        package: helm_core::models::PackageRef {
            manager: ManagerId::Npm,
            name: "ripgrep".to_string(),
        },
        version: None,
    });

    let error = ensure_request_supported(adapter.descriptor(), &request).unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    assert_eq!(error.manager, Some(ManagerId::Npm));
    assert_eq!(error.action, Some(ManagerAction::Install));
}

#[test]
fn execute_with_capability_check_blocks_unsupported_request() {
    let adapter = CountingAdapter::new(&[Capability::Detect]);
    let request = AdapterRequest::Refresh(helm_core::adapters::RefreshRequest);

    let error = execute_with_capability_check(&adapter, request).unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    assert_eq!(adapter.execute_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn execute_with_capability_check_allows_supported_request() {
    let adapter = CountingAdapter::new(&[Capability::Detect]);
    let request = AdapterRequest::Detect(DetectRequest);

    let response = execute_with_capability_check(&adapter, request).unwrap();
    assert_eq!(response, AdapterResponse::Refreshed);
    assert_eq!(adapter.execute_calls.load(Ordering::SeqCst), 1);
}
