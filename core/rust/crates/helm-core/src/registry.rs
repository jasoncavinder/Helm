use crate::models::{Capability, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum InstallMethodRecommendationReason {
    UpstreamRecommended,
    HelmPreferredDefault,
}

impl InstallMethodRecommendationReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UpstreamRecommended => "upstream_recommended",
            Self::HelmPreferredDefault => "helm_preferred_default",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum InstallMethodPolicyTag {
    Allowed,
    ManagedRestricted,
    BlockedByPolicy,
}

impl InstallMethodPolicyTag {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::ManagedRestricted => "managed_restricted",
            Self::BlockedByPolicy => "blocked_by_policy",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerInstallMethodSpec {
    pub id: &'static str,
    pub recommendation_rank: u8,
    pub recommendation_reason: Option<InstallMethodRecommendationReason>,
    pub policy_tag: InstallMethodPolicyTag,
    pub executable_path_hints: &'static [&'static str],
    pub package_hints: &'static [&'static str],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerLifecycleMetadata {
    pub install_method_ids: &'static [&'static str],
    pub install_methods: &'static [ManagerInstallMethodSpec],
    pub participates_in_package_search: bool,
}

const fn method_spec(
    id: &'static str,
    recommendation_rank: u8,
    recommendation_reason: Option<InstallMethodRecommendationReason>,
    policy_tag: InstallMethodPolicyTag,
) -> ManagerInstallMethodSpec {
    ManagerInstallMethodSpec {
        id,
        recommendation_rank,
        recommendation_reason,
        policy_tag,
        executable_path_hints: &[],
        package_hints: &[],
    }
}

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

const MISE_INSTALL_METHOD_IDS: &[&str] =
    &["scriptInstaller", "homebrew", "macports", "cargoInstall"];
const MISE_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "scriptInstaller",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec("homebrew", 10, None, InstallMethodPolicyTag::Allowed),
    method_spec(
        "macports",
        20,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec(
        "cargoInstall",
        30,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const ASDF_INSTALL_METHOD_IDS: &[&str] = &["scriptInstaller", "homebrew"];
const ASDF_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "scriptInstaller",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const RUSTUP_INSTALL_METHOD_IDS: &[&str] = &["rustupInstaller", "homebrew"];
const RUSTUP_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "rustupInstaller",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const HOMEBREW_FORMULA_INSTALL_METHOD_IDS: &[&str] = &["homebrew", "scriptInstaller"];
const HOMEBREW_FORMULA_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "homebrew",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "scriptInstaller",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const SOFTWAREUPDATE_INSTALL_METHOD_IDS: &[&str] = &["softwareUpdate"];
const SOFTWAREUPDATE_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[method_spec(
    "softwareUpdate",
    0,
    Some(InstallMethodRecommendationReason::UpstreamRecommended),
    InstallMethodPolicyTag::Allowed,
)];

const MACPORTS_INSTALL_METHOD_IDS: &[&str] = &["macports", "officialInstaller"];
const MACPORTS_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "macports",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "officialInstaller",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const NIX_DARWIN_INSTALL_METHOD_IDS: &[&str] = &["scriptInstaller", "homebrew"];
const NIX_DARWIN_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "scriptInstaller",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const NPM_INSTALL_METHOD_IDS: &[&str] = &["mise", "asdf", "homebrew", "officialInstaller"];
const NPM_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "mise",
        0,
        Some(InstallMethodRecommendationReason::HelmPreferredDefault),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec("asdf", 10, None, InstallMethodPolicyTag::ManagedRestricted),
    method_spec(
        "homebrew",
        20,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec(
        "officialInstaller",
        30,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const PNPM_INSTALL_METHOD_IDS: &[&str] = &["corepack", "homebrew", "npm", "scriptInstaller"];
const PNPM_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "corepack",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec("npm", 20, None, InstallMethodPolicyTag::ManagedRestricted),
    method_spec(
        "scriptInstaller",
        30,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const YARN_INSTALL_METHOD_IDS: &[&str] = &["corepack", "homebrew", "npm", "scriptInstaller"];
const YARN_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "corepack",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec("npm", 20, None, InstallMethodPolicyTag::ManagedRestricted),
    method_spec(
        "scriptInstaller",
        30,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const POETRY_INSTALL_METHOD_IDS: &[&str] = &["pipx", "homebrew", "pip", "officialInstaller"];
const POETRY_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "pipx",
        0,
        Some(InstallMethodRecommendationReason::HelmPreferredDefault),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec("pip", 20, None, InstallMethodPolicyTag::ManagedRestricted),
    method_spec(
        "officialInstaller",
        30,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const RUBYGEMS_INSTALL_METHOD_IDS: &[&str] = &["systemProvided", "homebrew", "asdf", "mise"];
const RUBYGEMS_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "systemProvided",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::BlockedByPolicy,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec("asdf", 20, None, InstallMethodPolicyTag::ManagedRestricted),
    method_spec("mise", 30, None, InstallMethodPolicyTag::ManagedRestricted),
];

const BUNDLER_INSTALL_METHOD_IDS: &[&str] = &["gem", "systemProvided", "homebrew", "asdf", "mise"];
const BUNDLER_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "gem",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "systemProvided",
        10,
        None,
        InstallMethodPolicyTag::BlockedByPolicy,
    ),
    method_spec(
        "homebrew",
        20,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec("asdf", 30, None, InstallMethodPolicyTag::ManagedRestricted),
    method_spec("mise", 40, None, InstallMethodPolicyTag::ManagedRestricted),
];

const PIP_INSTALL_METHOD_IDS: &[&str] = &["systemProvided", "homebrew", "asdf", "mise"];
const PIP_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "systemProvided",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::BlockedByPolicy,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec("asdf", 20, None, InstallMethodPolicyTag::ManagedRestricted),
    method_spec("mise", 30, None, InstallMethodPolicyTag::ManagedRestricted),
];

const PIPX_INSTALL_METHOD_IDS: &[&str] = &["homebrew", "pip"];
const PIPX_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "homebrew",
        0,
        Some(InstallMethodRecommendationReason::HelmPreferredDefault),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec("pip", 10, None, InstallMethodPolicyTag::ManagedRestricted),
];

const CARGO_INSTALL_METHOD_IDS: &[&str] = &["rustupInstaller", "homebrew"];
const CARGO_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "rustupInstaller",
        0,
        Some(InstallMethodRecommendationReason::HelmPreferredDefault),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const CARGO_BINSTALL_INSTALL_METHOD_IDS: &[&str] = &["scriptInstaller", "cargoInstall", "homebrew"];
const CARGO_BINSTALL_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "scriptInstaller",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "cargoInstall",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec(
        "homebrew",
        20,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const MAS_INSTALL_METHOD_IDS: &[&str] = &["homebrew", "macports", "appStore", "officialInstaller"];
const MAS_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "homebrew",
        0,
        Some(InstallMethodRecommendationReason::HelmPreferredDefault),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "macports",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec(
        "appStore",
        20,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec(
        "officialInstaller",
        30,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const SPARKLE_INSTALL_METHOD_IDS: &[&str] = &["notManageable"];
const SPARKLE_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[method_spec(
    "notManageable",
    0,
    Some(InstallMethodRecommendationReason::UpstreamRecommended),
    InstallMethodPolicyTag::BlockedByPolicy,
)];

const SETAPP_INSTALL_METHOD_IDS: &[&str] = &["setapp", "notManageable"];
const SETAPP_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "setapp",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "notManageable",
        10,
        None,
        InstallMethodPolicyTag::BlockedByPolicy,
    ),
];

const HOMEBREW_CASK_INSTALL_METHOD_IDS: &[&str] = &["homebrew"];
const HOMEBREW_CASK_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[method_spec(
    "homebrew",
    0,
    Some(InstallMethodRecommendationReason::UpstreamRecommended),
    InstallMethodPolicyTag::Allowed,
)];

const DOCKER_DESKTOP_INSTALL_METHOD_IDS: &[&str] = &["officialInstaller", "homebrew", "setapp"];
const DOCKER_DESKTOP_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "officialInstaller",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec(
        "setapp",
        20,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const PODMAN_INSTALL_METHOD_IDS: &[&str] = &["officialInstaller", "homebrew", "macports"];
const PODMAN_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "officialInstaller",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "homebrew",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec(
        "macports",
        20,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const COLIMA_INSTALL_METHOD_IDS: &[&str] = &["homebrew", "macports", "mise"];
const COLIMA_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "homebrew",
        0,
        Some(InstallMethodRecommendationReason::HelmPreferredDefault),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "macports",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec("mise", 20, None, InstallMethodPolicyTag::ManagedRestricted),
];

const PARALLELS_INSTALL_METHOD_IDS: &[&str] = &["officialInstaller", "setapp", "notManageable"];
const PARALLELS_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "officialInstaller",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "setapp",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
    method_spec(
        "notManageable",
        20,
        None,
        InstallMethodPolicyTag::BlockedByPolicy,
    ),
];

const XCODE_CLT_INSTALL_METHOD_IDS: &[&str] = &["xcodeSelect", "appStore"];
const XCODE_CLT_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[
    method_spec(
        "xcodeSelect",
        0,
        Some(InstallMethodRecommendationReason::UpstreamRecommended),
        InstallMethodPolicyTag::Allowed,
    ),
    method_spec(
        "appStore",
        10,
        None,
        InstallMethodPolicyTag::ManagedRestricted,
    ),
];

const ROSETTA_INSTALL_METHOD_IDS: &[&str] = &["softwareUpdate"];
const ROSETTA_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[method_spec(
    "softwareUpdate",
    0,
    Some(InstallMethodRecommendationReason::UpstreamRecommended),
    InstallMethodPolicyTag::Allowed,
)];

const FIRMWARE_INSTALL_METHOD_IDS: &[&str] = &["systemProvided"];
const FIRMWARE_INSTALL_METHODS: &[ManagerInstallMethodSpec] = &[method_spec(
    "systemProvided",
    0,
    Some(InstallMethodRecommendationReason::UpstreamRecommended),
    InstallMethodPolicyTag::BlockedByPolicy,
)];

const MISE_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: MISE_INSTALL_METHOD_IDS,
    install_methods: MISE_INSTALL_METHODS,
    participates_in_package_search: true,
};
const ASDF_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: ASDF_INSTALL_METHOD_IDS,
    install_methods: ASDF_INSTALL_METHODS,
    participates_in_package_search: true,
};
const RUSTUP_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: RUSTUP_INSTALL_METHOD_IDS,
    install_methods: RUSTUP_INSTALL_METHODS,
    participates_in_package_search: true,
};
const HOMEBREW_FORMULA_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: HOMEBREW_FORMULA_INSTALL_METHOD_IDS,
    install_methods: HOMEBREW_FORMULA_INSTALL_METHODS,
    participates_in_package_search: true,
};
const SOFTWAREUPDATE_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: SOFTWAREUPDATE_INSTALL_METHOD_IDS,
    install_methods: SOFTWAREUPDATE_INSTALL_METHODS,
    participates_in_package_search: true,
};
const MACPORTS_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: MACPORTS_INSTALL_METHOD_IDS,
    install_methods: MACPORTS_INSTALL_METHODS,
    participates_in_package_search: true,
};
const NIX_DARWIN_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: NIX_DARWIN_INSTALL_METHOD_IDS,
    install_methods: NIX_DARWIN_INSTALL_METHODS,
    participates_in_package_search: true,
};
const NPM_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: NPM_INSTALL_METHOD_IDS,
    install_methods: NPM_INSTALL_METHODS,
    participates_in_package_search: true,
};
const PNPM_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: PNPM_INSTALL_METHOD_IDS,
    install_methods: PNPM_INSTALL_METHODS,
    participates_in_package_search: true,
};
const YARN_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: YARN_INSTALL_METHOD_IDS,
    install_methods: YARN_INSTALL_METHODS,
    participates_in_package_search: true,
};
const PIPX_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: PIPX_INSTALL_METHOD_IDS,
    install_methods: PIPX_INSTALL_METHODS,
    participates_in_package_search: true,
};
const PIP_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: PIP_INSTALL_METHOD_IDS,
    install_methods: PIP_INSTALL_METHODS,
    participates_in_package_search: true,
};
const POETRY_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: POETRY_INSTALL_METHOD_IDS,
    install_methods: POETRY_INSTALL_METHODS,
    participates_in_package_search: true,
};
const RUBYGEMS_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: RUBYGEMS_INSTALL_METHOD_IDS,
    install_methods: RUBYGEMS_INSTALL_METHODS,
    participates_in_package_search: true,
};
const BUNDLER_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: BUNDLER_INSTALL_METHOD_IDS,
    install_methods: BUNDLER_INSTALL_METHODS,
    participates_in_package_search: true,
};
const CARGO_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: CARGO_INSTALL_METHOD_IDS,
    install_methods: CARGO_INSTALL_METHODS,
    participates_in_package_search: true,
};
const CARGO_BINSTALL_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: CARGO_BINSTALL_INSTALL_METHOD_IDS,
    install_methods: CARGO_BINSTALL_INSTALL_METHODS,
    participates_in_package_search: true,
};
const MAS_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: MAS_INSTALL_METHOD_IDS,
    install_methods: MAS_INSTALL_METHODS,
    participates_in_package_search: true,
};
const SPARKLE_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: SPARKLE_INSTALL_METHOD_IDS,
    install_methods: SPARKLE_INSTALL_METHODS,
    participates_in_package_search: true,
};
const SETAPP_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: SETAPP_INSTALL_METHOD_IDS,
    install_methods: SETAPP_INSTALL_METHODS,
    participates_in_package_search: true,
};
const HOMEBREW_CASK_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: HOMEBREW_CASK_INSTALL_METHOD_IDS,
    install_methods: HOMEBREW_CASK_INSTALL_METHODS,
    participates_in_package_search: true,
};
const DOCKER_DESKTOP_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: DOCKER_DESKTOP_INSTALL_METHOD_IDS,
    install_methods: DOCKER_DESKTOP_INSTALL_METHODS,
    participates_in_package_search: true,
};
const PODMAN_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: PODMAN_INSTALL_METHOD_IDS,
    install_methods: PODMAN_INSTALL_METHODS,
    participates_in_package_search: true,
};
const COLIMA_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: COLIMA_INSTALL_METHOD_IDS,
    install_methods: COLIMA_INSTALL_METHODS,
    participates_in_package_search: true,
};
const PARALLELS_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: PARALLELS_INSTALL_METHOD_IDS,
    install_methods: PARALLELS_INSTALL_METHODS,
    participates_in_package_search: true,
};
const XCODE_CLT_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: XCODE_CLT_INSTALL_METHOD_IDS,
    install_methods: XCODE_CLT_INSTALL_METHODS,
    participates_in_package_search: true,
};
const ROSETTA_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: ROSETTA_INSTALL_METHOD_IDS,
    install_methods: ROSETTA_INSTALL_METHODS,
    participates_in_package_search: true,
};
const FIRMWARE_LIFECYCLE_METADATA: ManagerLifecycleMetadata = ManagerLifecycleMetadata {
    install_method_ids: FIRMWARE_INSTALL_METHOD_IDS,
    install_methods: FIRMWARE_INSTALL_METHODS,
    participates_in_package_search: true,
};
pub fn managers() -> &'static [ManagerDescriptor] {
    &ALL_MANAGERS
}

pub fn manager(id: ManagerId) -> Option<&'static ManagerDescriptor> {
    ALL_MANAGERS.iter().find(|descriptor| descriptor.id == id)
}

pub fn manager_lifecycle_metadata(id: ManagerId) -> &'static ManagerLifecycleMetadata {
    match id {
        ManagerId::Mise => &MISE_LIFECYCLE_METADATA,
        ManagerId::Asdf => &ASDF_LIFECYCLE_METADATA,
        ManagerId::Rustup => &RUSTUP_LIFECYCLE_METADATA,
        ManagerId::HomebrewFormula => &HOMEBREW_FORMULA_LIFECYCLE_METADATA,
        ManagerId::SoftwareUpdate => &SOFTWAREUPDATE_LIFECYCLE_METADATA,
        ManagerId::MacPorts => &MACPORTS_LIFECYCLE_METADATA,
        ManagerId::NixDarwin => &NIX_DARWIN_LIFECYCLE_METADATA,
        ManagerId::Pnpm => &PNPM_LIFECYCLE_METADATA,
        ManagerId::Npm => &NPM_LIFECYCLE_METADATA,
        ManagerId::Yarn => &YARN_LIFECYCLE_METADATA,
        ManagerId::Pipx => &PIPX_LIFECYCLE_METADATA,
        ManagerId::Pip => &PIP_LIFECYCLE_METADATA,
        ManagerId::Poetry => &POETRY_LIFECYCLE_METADATA,
        ManagerId::RubyGems => &RUBYGEMS_LIFECYCLE_METADATA,
        ManagerId::Bundler => &BUNDLER_LIFECYCLE_METADATA,
        ManagerId::Cargo => &CARGO_LIFECYCLE_METADATA,
        ManagerId::CargoBinstall => &CARGO_BINSTALL_LIFECYCLE_METADATA,
        ManagerId::Mas => &MAS_LIFECYCLE_METADATA,
        ManagerId::Sparkle => &SPARKLE_LIFECYCLE_METADATA,
        ManagerId::Setapp => &SETAPP_LIFECYCLE_METADATA,
        ManagerId::HomebrewCask => &HOMEBREW_CASK_LIFECYCLE_METADATA,
        ManagerId::DockerDesktop => &DOCKER_DESKTOP_LIFECYCLE_METADATA,
        ManagerId::Podman => &PODMAN_LIFECYCLE_METADATA,
        ManagerId::Colima => &COLIMA_LIFECYCLE_METADATA,
        ManagerId::ParallelsDesktop => &PARALLELS_LIFECYCLE_METADATA,
        ManagerId::XcodeCommandLineTools => &XCODE_CLT_LIFECYCLE_METADATA,
        ManagerId::Rosetta2 => &ROSETTA_LIFECYCLE_METADATA,
        ManagerId::FirmwareUpdates => &FIRMWARE_LIFECYCLE_METADATA,
    }
}

pub fn manager_install_method_candidates(id: ManagerId) -> &'static [&'static str] {
    manager_lifecycle_metadata(id).install_method_ids
}

pub fn manager_install_method_specs(id: ManagerId) -> &'static [ManagerInstallMethodSpec] {
    manager_lifecycle_metadata(id).install_methods
}

pub fn manager_install_method_spec(
    id: ManagerId,
    method_id: &str,
) -> Option<&'static ManagerInstallMethodSpec> {
    manager_install_method_specs(id)
        .iter()
        .find(|spec| spec.id == method_id)
}

pub fn manager_participates_in_package_search(id: ManagerId) -> bool {
    manager_lifecycle_metadata(id).participates_in_package_search
}

#[cfg(test)]
mod tests {
    use super::{
        InstallMethodRecommendationReason, manager_install_method_candidates,
        manager_install_method_specs, manager_participates_in_package_search,
    };
    use crate::models::ManagerId;

    #[test]
    fn rustup_install_methods_include_rustup_installer_and_homebrew() {
        assert_eq!(
            manager_install_method_candidates(ManagerId::Rustup),
            ["rustupInstaller", "homebrew"]
        );
    }

    #[test]
    fn rustup_install_method_is_marked_upstream_recommended() {
        let methods = manager_install_method_specs(ManagerId::Rustup);
        let rustup_installer = methods
            .iter()
            .find(|spec| spec.id == "rustupInstaller")
            .expect("expected rustupInstaller method");
        assert_eq!(
            rustup_installer.recommendation_reason,
            Some(InstallMethodRecommendationReason::UpstreamRecommended)
        );
    }

    #[test]
    fn rustup_participates_in_package_search_policy() {
        assert!(manager_participates_in_package_search(ManagerId::Rustup));
        assert!(manager_participates_in_package_search(
            ManagerId::HomebrewFormula
        ));
    }
}
