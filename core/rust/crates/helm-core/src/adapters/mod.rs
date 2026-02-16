pub mod cargo;
pub mod cargo_process;
pub(crate) mod detect_utils;
pub mod homebrew;
pub mod homebrew_process;
pub mod manager;
pub mod mas;
pub mod mas_process;
pub mod mise;
pub mod mise_process;
pub mod npm;
pub mod npm_process;
pub mod pip;
pub mod pip_process;
pub mod pipx;
pub mod pipx_process;
pub(crate) mod process_utils;
pub mod rustup;
pub mod rustup_process;
pub mod softwareupdate;
pub mod softwareupdate_process;

pub use cargo::{
    CargoAdapter, CargoSource, cargo_detect_request, cargo_install_request,
    cargo_list_installed_request, cargo_search_request, cargo_uninstall_request,
    cargo_upgrade_request,
};
pub use cargo_process::ProcessCargoSource;
pub use homebrew::{
    HomebrewAdapter, HomebrewSource, homebrew_detect_request, homebrew_list_installed_request,
    homebrew_list_outdated_request, homebrew_pin_request, homebrew_search_local_request,
    homebrew_unpin_request,
};
pub use homebrew_process::ProcessHomebrewSource;
pub use manager::{
    AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, InstallRequest,
    ListInstalledRequest, ListOutdatedRequest, ManagerAdapter, MutationResult, PinRequest,
    RefreshRequest, SearchRequest, UninstallRequest, UnpinRequest, UpgradeRequest,
    ensure_action_supported, ensure_request_supported, execute_with_capability_check,
};
pub use mas::{
    MasAdapter, MasSource, mas_detect_request, mas_list_installed_request,
    mas_list_outdated_request,
};
pub use mas_process::ProcessMasSource;
pub use mise::{
    MiseAdapter, MiseSource, mise_detect_request, mise_list_installed_request,
    mise_list_outdated_request,
};
pub use mise_process::ProcessMiseSource;
pub use npm::{
    NpmAdapter, NpmSource, npm_detect_request, npm_install_request, npm_list_installed_request,
    npm_list_outdated_request, npm_search_request, npm_uninstall_request, npm_upgrade_request,
};
pub use npm_process::ProcessNpmSource;
pub use pip::{
    PipAdapter, PipSource, pip_detect_request, pip_install_request, pip_list_outdated_request,
    pip_list_request, pip_search_request, pip_uninstall_request, pip_upgrade_request,
};
pub use pip_process::ProcessPipSource;
pub use pipx::{
    PipxAdapter, PipxSource, pipx_detect_request, pipx_install_request, pipx_list_outdated_request,
    pipx_list_request, pipx_uninstall_request, pipx_upgrade_request,
};
pub use pipx_process::ProcessPipxSource;
pub use rustup::{
    RustupAdapter, RustupSource, rustup_check_request, rustup_detect_request,
    rustup_toolchain_list_request,
};
pub use rustup_process::ProcessRustupSource;
pub use softwareupdate::{
    SoftwareUpdateAdapter, SoftwareUpdateSource, softwareupdate_detect_request,
    softwareupdate_list_request, softwareupdate_upgrade_request,
};
pub use softwareupdate_process::ProcessSoftwareUpdateSource;
