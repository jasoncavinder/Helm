pub mod manager;

pub use manager::{
    AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, InstallRequest,
    ListInstalledRequest, ListOutdatedRequest, ManagerAdapter, MutationResult, PinRequest,
    RefreshRequest, SearchRequest, UninstallRequest, UnpinRequest, UpgradeRequest,
};
