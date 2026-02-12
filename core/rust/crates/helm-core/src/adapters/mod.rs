pub(crate) mod detect_utils;
pub mod homebrew;
pub mod homebrew_process;
pub mod manager;
pub mod mise;
pub mod mise_process;
pub(crate) mod process_utils;
pub mod rustup;
pub mod rustup_process;

pub use homebrew::{
    HomebrewAdapter, HomebrewSource, homebrew_detect_request, homebrew_list_installed_request,
    homebrew_list_outdated_request, homebrew_search_local_request,
};
pub use homebrew_process::ProcessHomebrewSource;
pub use manager::{
    AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, InstallRequest,
    ListInstalledRequest, ListOutdatedRequest, ManagerAdapter, MutationResult, PinRequest,
    RefreshRequest, SearchRequest, UninstallRequest, UnpinRequest, UpgradeRequest,
    ensure_action_supported, ensure_request_supported, execute_with_capability_check,
};
pub use mise::{
    MiseAdapter, MiseSource, mise_detect_request, mise_list_installed_request,
    mise_list_outdated_request,
};
pub use mise_process::ProcessMiseSource;
pub use rustup::{
    RustupAdapter, RustupSource, rustup_check_request, rustup_detect_request,
    rustup_toolchain_list_request,
};
pub use rustup_process::ProcessRustupSource;
