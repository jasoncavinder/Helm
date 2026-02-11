use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ManagerId {
    Mise,
    Asdf,
    Rustup,
    HomebrewFormula,
    SoftwareUpdate,
    MacPorts,
    NixDarwin,
    Pnpm,
    Npm,
    Yarn,
    Pipx,
    Pip,
    Poetry,
    RubyGems,
    Bundler,
    Cargo,
    CargoBinstall,
    Mas,
    Sparkle,
    Setapp,
    HomebrewCask,
    DockerDesktop,
    Podman,
    Colima,
    ParallelsDesktop,
    XcodeCommandLineTools,
    Rosetta2,
    FirmwareUpdates,
}

impl ManagerId {
    pub const ALL: [Self; 28] = [
        Self::Mise,
        Self::Asdf,
        Self::Rustup,
        Self::HomebrewFormula,
        Self::SoftwareUpdate,
        Self::MacPorts,
        Self::NixDarwin,
        Self::Pnpm,
        Self::Npm,
        Self::Yarn,
        Self::Pipx,
        Self::Pip,
        Self::Poetry,
        Self::RubyGems,
        Self::Bundler,
        Self::Cargo,
        Self::CargoBinstall,
        Self::Mas,
        Self::Sparkle,
        Self::Setapp,
        Self::HomebrewCask,
        Self::DockerDesktop,
        Self::Podman,
        Self::Colima,
        Self::ParallelsDesktop,
        Self::XcodeCommandLineTools,
        Self::Rosetta2,
        Self::FirmwareUpdates,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ManagerCategory {
    ToolRuntime,
    SystemOs,
    Language,
    GuiApp,
    ContainerVm,
    SecurityFirmware,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ManagerAuthority {
    Authoritative,
    Guarded,
    Standard,
    DetectionOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Capability {
    Detect,
    Refresh,
    Search,
    ListInstalled,
    ListOutdated,
    Install,
    Uninstall,
    Upgrade,
    Pin,
    Unpin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ManagerAction {
    Detect,
    Refresh,
    Search,
    ListInstalled,
    ListOutdated,
    Install,
    Uninstall,
    Upgrade,
    Pin,
    Unpin,
}

impl ManagerAction {
    pub fn required_capability(self) -> Capability {
        match self {
            Self::Detect => Capability::Detect,
            Self::Refresh => Capability::Refresh,
            Self::Search => Capability::Search,
            Self::ListInstalled => Capability::ListInstalled,
            Self::ListOutdated => Capability::ListOutdated,
            Self::Install => Capability::Install,
            Self::Uninstall => Capability::Uninstall,
            Self::Upgrade => Capability::Upgrade,
            Self::Pin => Capability::Pin,
            Self::Unpin => Capability::Unpin,
        }
    }

    pub fn safety(self) -> ActionSafety {
        match self {
            Self::Detect
            | Self::Refresh
            | Self::Search
            | Self::ListInstalled
            | Self::ListOutdated => ActionSafety::ReadOnly,
            Self::Install | Self::Uninstall | Self::Upgrade | Self::Pin | Self::Unpin => {
                ActionSafety::Mutating
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ActionSafety {
    ReadOnly,
    Mutating,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerDescriptor {
    pub id: ManagerId,
    pub display_name: &'static str,
    pub category: ManagerCategory,
    pub authority: ManagerAuthority,
    pub capabilities: &'static [Capability],
}

impl ManagerDescriptor {
    pub fn supports(&self, capability: Capability) -> bool {
        self.capabilities.contains(&capability)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetectionInfo {
    pub installed: bool,
    pub executable_path: Option<PathBuf>,
    pub version: Option<String>,
}
