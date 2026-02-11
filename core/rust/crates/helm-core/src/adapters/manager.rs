use crate::models::{
    ActionSafety, CachedSearchResult, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerDescriptor, OutdatedPackage, PackageRef, SearchQuery,
};

pub type AdapterResult<T> = Result<T, CoreError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefreshRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListInstalledRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListOutdatedRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchRequest {
    pub query: SearchQuery,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallRequest {
    pub package: PackageRef,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UninstallRequest {
    pub package: PackageRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeRequest {
    pub package: Option<PackageRef>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinRequest {
    pub package: PackageRef,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnpinRequest {
    pub package: PackageRef,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterRequest {
    Detect(DetectRequest),
    Refresh(RefreshRequest),
    ListInstalled(ListInstalledRequest),
    ListOutdated(ListOutdatedRequest),
    Search(SearchRequest),
    Install(InstallRequest),
    Uninstall(UninstallRequest),
    Upgrade(UpgradeRequest),
    Pin(PinRequest),
    Unpin(UnpinRequest),
}

impl AdapterRequest {
    pub fn action(&self) -> ManagerAction {
        match self {
            Self::Detect(_) => ManagerAction::Detect,
            Self::Refresh(_) => ManagerAction::Refresh,
            Self::ListInstalled(_) => ManagerAction::ListInstalled,
            Self::ListOutdated(_) => ManagerAction::ListOutdated,
            Self::Search(_) => ManagerAction::Search,
            Self::Install(_) => ManagerAction::Install,
            Self::Uninstall(_) => ManagerAction::Uninstall,
            Self::Upgrade(_) => ManagerAction::Upgrade,
            Self::Pin(_) => ManagerAction::Pin,
            Self::Unpin(_) => ManagerAction::Unpin,
        }
    }
}

pub fn ensure_action_supported(
    descriptor: &ManagerDescriptor,
    action: ManagerAction,
) -> AdapterResult<()> {
    let required = action.required_capability();
    if descriptor.supports(required) {
        return Ok(());
    }

    Err(CoreError {
        manager: Some(descriptor.id),
        task: None,
        action: Some(action),
        kind: CoreErrorKind::UnsupportedCapability,
        message: format!(
            "manager '{}' does not support required capability '{:?}' for action '{:?}'",
            descriptor.display_name, required, action
        ),
    })
}

pub fn ensure_request_supported(
    descriptor: &ManagerDescriptor,
    request: &AdapterRequest,
) -> AdapterResult<()> {
    ensure_action_supported(descriptor, request.action())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MutationResult {
    pub package: PackageRef,
    pub action: ManagerAction,
    pub before_version: Option<String>,
    pub after_version: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdapterResponse {
    Detection(DetectionInfo),
    Refreshed,
    InstalledPackages(Vec<InstalledPackage>),
    OutdatedPackages(Vec<OutdatedPackage>),
    SearchResults(Vec<CachedSearchResult>),
    Mutation(MutationResult),
}

pub trait ManagerAdapter: Send + Sync {
    fn descriptor(&self) -> &ManagerDescriptor;

    fn action_safety(&self, action: ManagerAction) -> ActionSafety;

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse>;
}

pub fn execute_with_capability_check(
    adapter: &dyn ManagerAdapter,
    request: AdapterRequest,
) -> AdapterResult<AdapterResponse> {
    ensure_request_supported(adapter.descriptor(), &request)?;
    adapter.execute(request)
}
