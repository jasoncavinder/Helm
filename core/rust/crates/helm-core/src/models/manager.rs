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
    pub capabilities: Vec<Capability>,
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
