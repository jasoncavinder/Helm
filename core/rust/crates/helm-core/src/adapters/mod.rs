pub mod asdf;
pub mod asdf_process;
pub mod bundler;
pub mod bundler_process;
pub mod cargo;
pub mod cargo_binstall;
pub mod cargo_binstall_process;
pub(crate) mod cargo_outdated;
pub mod cargo_process;
pub mod colima;
pub mod colima_process;
pub(crate) mod detect_utils;
pub mod docker_desktop;
pub mod docker_desktop_process;
pub mod firmware_updates;
pub mod firmware_updates_process;
pub mod homebrew;
pub mod homebrew_cask;
pub mod homebrew_cask_process;
pub mod homebrew_process;
pub mod macports;
pub mod macports_process;
pub mod manager;
pub mod mas;
pub mod mas_process;
pub mod mise;
pub mod mise_process;
pub mod nix_darwin;
pub mod nix_darwin_process;
pub mod npm;
pub mod npm_process;
pub mod parallels_desktop;
pub mod parallels_desktop_process;
pub mod pip;
pub mod pip_process;
pub mod pipx;
pub mod pipx_process;
pub mod pnpm;
pub mod pnpm_process;
pub mod podman;
pub mod podman_process;
pub mod poetry;
pub mod poetry_process;
pub(crate) mod process_utils;
pub mod rosetta2;
pub mod rosetta2_process;
pub mod rubygems;
pub mod rubygems_process;
pub mod rustup;
pub mod rustup_process;
pub mod setapp;
pub mod setapp_process;
pub mod softwareupdate;
pub mod softwareupdate_process;
pub mod sparkle;
pub mod sparkle_process;
pub mod xcode_command_line_tools;
pub mod xcode_command_line_tools_process;
pub mod yarn;
pub mod yarn_process;

pub use asdf::{
    AsdfAdapter, AsdfSource, asdf_detect_request, asdf_install_request, asdf_latest_request,
    asdf_list_all_plugins_request, asdf_list_current_request, asdf_list_plugins_request,
    asdf_uninstall_request, asdf_upgrade_request,
};
pub use asdf_process::ProcessAsdfSource;
pub use bundler::{
    BundlerAdapter, BundlerSource, bundler_detect_request, bundler_install_request,
    bundler_list_installed_request, bundler_list_outdated_request, bundler_uninstall_request,
    bundler_upgrade_request,
};
pub use bundler_process::ProcessBundlerSource;
pub use cargo::{
    CargoAdapter, CargoSource, cargo_detect_request, cargo_install_request,
    cargo_list_installed_request, cargo_search_request, cargo_uninstall_request,
    cargo_upgrade_request,
};
pub use cargo_binstall::{
    CargoBinstallAdapter, CargoBinstallSource, cargo_binstall_detect_request,
    cargo_binstall_install_request, cargo_binstall_list_installed_request,
    cargo_binstall_search_request, cargo_binstall_uninstall_request,
    cargo_binstall_upgrade_request,
};
pub use cargo_binstall_process::ProcessCargoBinstallSource;
pub use cargo_process::ProcessCargoSource;
pub use colima::{
    ColimaAdapter, ColimaSource, colima_detect_request, colima_list_outdated_request,
};
pub use colima_process::ProcessColimaSource;
pub use docker_desktop::{
    DockerDesktopAdapter, DockerDesktopSource, docker_desktop_detect_request,
    docker_desktop_list_outdated_request,
};
pub use docker_desktop_process::ProcessDockerDesktopSource;
pub use firmware_updates::{
    FirmwareUpdatesAdapter, FirmwareUpdatesSource, firmware_updates_history_request,
};
pub use firmware_updates_process::ProcessFirmwareUpdatesSource;
pub use homebrew::{
    HomebrewAdapter, HomebrewSource, homebrew_detect_request, homebrew_list_installed_request,
    homebrew_list_outdated_request, homebrew_pin_request, homebrew_search_local_request,
    homebrew_unpin_request,
};
pub use homebrew_cask::{
    HomebrewCaskAdapter, HomebrewCaskSource, homebrew_cask_detect_request,
    homebrew_cask_list_installed_request, homebrew_cask_list_outdated_request,
};
pub use homebrew_cask_process::ProcessHomebrewCaskSource;
pub use homebrew_process::ProcessHomebrewSource;
pub use macports::{
    MacPortsAdapter, MacPortsSource, macports_detect_request, macports_install_request,
    macports_list_installed_request, macports_list_outdated_request, macports_search_request,
    macports_uninstall_request, macports_upgrade_request,
};
pub use macports_process::ProcessMacPortsSource;
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
pub use nix_darwin::{
    NixDarwinAdapter, NixDarwinSource, nix_darwin_detect_request, nix_darwin_install_request,
    nix_darwin_list_installed_request, nix_darwin_list_outdated_request, nix_darwin_search_request,
    nix_darwin_uninstall_request, nix_darwin_upgrade_request,
};
pub use nix_darwin_process::ProcessNixDarwinSource;
pub use npm::{
    NpmAdapter, NpmSource, npm_detect_request, npm_install_request, npm_list_installed_request,
    npm_list_outdated_request, npm_search_request, npm_uninstall_request, npm_upgrade_request,
};
pub use npm_process::ProcessNpmSource;
pub use parallels_desktop::{
    ParallelsDesktopAdapter, ParallelsDesktopSource, parallels_desktop_detect_request,
};
pub use parallels_desktop_process::ProcessParallelsDesktopSource;
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
pub use pnpm::{
    PnpmAdapter, PnpmSource, pnpm_detect_request, pnpm_install_request,
    pnpm_list_installed_request, pnpm_list_outdated_request, pnpm_search_request,
    pnpm_uninstall_request, pnpm_upgrade_request,
};
pub use pnpm_process::ProcessPnpmSource;
pub use podman::{
    PodmanAdapter, PodmanSource, podman_detect_request, podman_list_outdated_request,
};
pub use podman_process::ProcessPodmanSource;
pub use poetry::{
    PoetryAdapter, PoetrySource, poetry_detect_request, poetry_install_plugin_request,
    poetry_list_installed_request, poetry_list_outdated_plugins_request,
    poetry_uninstall_plugin_request, poetry_upgrade_plugins_request,
};
pub use poetry_process::ProcessPoetrySource;
pub use rosetta2::{
    Rosetta2Adapter, Rosetta2Source, rosetta2_detect_request, rosetta2_install_request,
};
pub use rosetta2_process::ProcessRosetta2Source;
pub use rubygems::{
    RubyGemsAdapter, RubyGemsSource, rubygems_detect_request, rubygems_install_request,
    rubygems_list_installed_request, rubygems_list_outdated_request, rubygems_search_request,
    rubygems_uninstall_request, rubygems_upgrade_request,
};
pub use rubygems_process::ProcessRubyGemsSource;
pub use rustup::{
    RustupAdapter, RustupSource, rustup_check_request, rustup_detect_request,
    rustup_toolchain_list_request,
};
pub use rustup_process::ProcessRustupSource;
pub use setapp::{SetappAdapter, SetappSource, setapp_detect_request};
pub use setapp_process::ProcessSetappSource;
pub use softwareupdate::{
    SoftwareUpdateAdapter, SoftwareUpdateSource, softwareupdate_detect_request,
    softwareupdate_list_request, softwareupdate_upgrade_request,
};
pub use softwareupdate_process::ProcessSoftwareUpdateSource;
pub use sparkle::{SparkleAdapter, SparkleSource, sparkle_detect_request};
pub use sparkle_process::ProcessSparkleSource;
pub use xcode_command_line_tools::{
    XcodeCommandLineToolsAdapter, XcodeCommandLineToolsSource,
    xcode_command_line_tools_detect_request, xcode_command_line_tools_list_outdated_request,
    xcode_command_line_tools_upgrade_request,
};
pub use xcode_command_line_tools_process::ProcessXcodeCommandLineToolsSource;
pub use yarn::{
    YarnAdapter, YarnSource, yarn_detect_request, yarn_install_request,
    yarn_list_installed_request, yarn_list_outdated_request, yarn_search_request,
    yarn_uninstall_request, yarn_upgrade_request,
};
pub use yarn_process::ProcessYarnSource;

pub(crate) fn validate_package_identifier(
    manager: crate::models::ManagerId,
    action: crate::models::ManagerAction,
    name: &str,
) -> crate::adapters::manager::AdapterResult<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(crate::models::CoreError {
            manager: Some(manager),
            task: None,
            action: Some(action),
            kind: crate::models::CoreErrorKind::InvalidInput,
            message: "package identifier cannot be empty".to_string(),
        });
    }

    if trimmed.starts_with('-') {
        return Err(crate::models::CoreError {
            manager: Some(manager),
            task: None,
            action: Some(action),
            kind: crate::models::CoreErrorKind::InvalidInput,
            message: "package identifier cannot start with '-'".to_string(),
        });
    }

    if trimmed.chars().any(char::is_whitespace) {
        return Err(crate::models::CoreError {
            manager: Some(manager),
            task: None,
            action: Some(action),
            kind: crate::models::CoreErrorKind::InvalidInput,
            message: "package identifier cannot contain whitespace".to_string(),
        });
    }

    if trimmed.len() > 256 {
        return Err(crate::models::CoreError {
            manager: Some(manager),
            task: None,
            action: Some(action),
            kind: crate::models::CoreErrorKind::InvalidInput,
            message: "package identifier exceeds 256 characters".to_string(),
        });
    }

    Ok(())
}
