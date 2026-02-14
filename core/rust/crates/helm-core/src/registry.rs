use crate::models::{Capability, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId};

const DETECT_ONLY_CAPABILITIES: &[Capability] = &[Capability::Detect];
const STATUS_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];
const SEARCHABLE_PACKAGE_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];
const HOMEBREW_FORMULA_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
    Capability::Pin,
    Capability::Unpin,
];
const SYSTEM_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Upgrade,
];
const SOFTWARE_UPDATE_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListOutdated,
    Capability::Upgrade,
];
const ROSETTA_CAPABILITIES: &[Capability] =
    &[Capability::Detect, Capability::Refresh, Capability::Install];
const FIRMWARE_CAPABILITIES: &[Capability] = &[Capability::Detect, Capability::Refresh];
const MAS_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

const ALL_MANAGERS: [ManagerDescriptor; 28] = [
    ManagerDescriptor {
        id: ManagerId::Mise,
        display_name: "mise",
        category: ManagerCategory::ToolRuntime,
        authority: ManagerAuthority::Authoritative,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Asdf,
        display_name: "asdf",
        category: ManagerCategory::ToolRuntime,
        authority: ManagerAuthority::Authoritative,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Rustup,
        display_name: "rustup",
        category: ManagerCategory::ToolRuntime,
        authority: ManagerAuthority::Authoritative,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::HomebrewFormula,
        display_name: "Homebrew (formulae)",
        category: ManagerCategory::SystemOs,
        authority: ManagerAuthority::Guarded,
        capabilities: HOMEBREW_FORMULA_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::SoftwareUpdate,
        display_name: "softwareupdate",
        category: ManagerCategory::SystemOs,
        authority: ManagerAuthority::Guarded,
        capabilities: SOFTWARE_UPDATE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::MacPorts,
        display_name: "MacPorts",
        category: ManagerCategory::SystemOs,
        authority: ManagerAuthority::Guarded,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::NixDarwin,
        display_name: "nix-darwin",
        category: ManagerCategory::SystemOs,
        authority: ManagerAuthority::Guarded,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Pnpm,
        display_name: "pnpm",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Npm,
        display_name: "npm",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Yarn,
        display_name: "yarn",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Pipx,
        display_name: "pipx",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Pip,
        display_name: "pip",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Poetry,
        display_name: "poetry",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::RubyGems,
        display_name: "RubyGems",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Bundler,
        display_name: "bundler",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Cargo,
        display_name: "Cargo",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::CargoBinstall,
        display_name: "cargo-binstall",
        category: ManagerCategory::Language,
        authority: ManagerAuthority::Standard,
        capabilities: SEARCHABLE_PACKAGE_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Mas,
        display_name: "mas",
        category: ManagerCategory::GuiApp,
        authority: ManagerAuthority::Standard,
        capabilities: MAS_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Sparkle,
        display_name: "Sparkle updater",
        category: ManagerCategory::GuiApp,
        authority: ManagerAuthority::DetectionOnly,
        capabilities: DETECT_ONLY_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Setapp,
        display_name: "Setapp",
        category: ManagerCategory::GuiApp,
        authority: ManagerAuthority::DetectionOnly,
        capabilities: DETECT_ONLY_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::HomebrewCask,
        display_name: "Homebrew (casks)",
        category: ManagerCategory::GuiApp,
        authority: ManagerAuthority::Standard,
        capabilities: STATUS_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::DockerDesktop,
        display_name: "Docker Desktop",
        category: ManagerCategory::ContainerVm,
        authority: ManagerAuthority::Standard,
        capabilities: STATUS_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Podman,
        display_name: "podman",
        category: ManagerCategory::ContainerVm,
        authority: ManagerAuthority::Standard,
        capabilities: STATUS_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Colima,
        display_name: "colima",
        category: ManagerCategory::ContainerVm,
        authority: ManagerAuthority::Standard,
        capabilities: STATUS_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::ParallelsDesktop,
        display_name: "Parallels Desktop",
        category: ManagerCategory::ContainerVm,
        authority: ManagerAuthority::DetectionOnly,
        capabilities: DETECT_ONLY_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::XcodeCommandLineTools,
        display_name: "Xcode Command Line Tools",
        category: ManagerCategory::SecurityFirmware,
        authority: ManagerAuthority::Guarded,
        capabilities: SYSTEM_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::Rosetta2,
        display_name: "Rosetta 2",
        category: ManagerCategory::SecurityFirmware,
        authority: ManagerAuthority::Guarded,
        capabilities: ROSETTA_CAPABILITIES,
    },
    ManagerDescriptor {
        id: ManagerId::FirmwareUpdates,
        display_name: "Firmware updates",
        category: ManagerCategory::SecurityFirmware,
        authority: ManagerAuthority::Guarded,
        capabilities: FIRMWARE_CAPABILITIES,
    },
];

pub fn managers() -> &'static [ManagerDescriptor] {
    &ALL_MANAGERS
}

pub fn manager(id: ManagerId) -> Option<&'static ManagerDescriptor> {
    ALL_MANAGERS.iter().find(|descriptor| descriptor.id == id)
}
