use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagerId {
    Mise,
    Asdf,
    Rustup,
    HomebrewFormula,
    #[serde(rename = "softwareupdate")]
    SoftwareUpdate,
    #[serde(rename = "macports")]
    MacPorts,
    NixDarwin,
    Pnpm,
    Npm,
    Yarn,
    Pipx,
    Pip,
    Poetry,
    #[serde(rename = "rubygems")]
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

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mise => "mise",
            Self::Asdf => "asdf",
            Self::Rustup => "rustup",
            Self::HomebrewFormula => "homebrew_formula",
            Self::SoftwareUpdate => "softwareupdate",
            Self::MacPorts => "macports",
            Self::NixDarwin => "nix_darwin",
            Self::Pnpm => "pnpm",
            Self::Npm => "npm",
            Self::Yarn => "yarn",
            Self::Pipx => "pipx",
            Self::Pip => "pip",
            Self::Poetry => "poetry",
            Self::RubyGems => "rubygems",
            Self::Bundler => "bundler",
            Self::Cargo => "cargo",
            Self::CargoBinstall => "cargo_binstall",
            Self::Mas => "mas",
            Self::Sparkle => "sparkle",
            Self::Setapp => "setapp",
            Self::HomebrewCask => "homebrew_cask",
            Self::DockerDesktop => "docker_desktop",
            Self::Podman => "podman",
            Self::Colima => "colima",
            Self::ParallelsDesktop => "parallels_desktop",
            Self::XcodeCommandLineTools => "xcode_command_line_tools",
            Self::Rosetta2 => "rosetta2",
            Self::FirmwareUpdates => "firmware_updates",
        }
    }
}

impl std::str::FromStr for ManagerId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "mise" => Ok(Self::Mise),
            "asdf" => Ok(Self::Asdf),
            "rustup" => Ok(Self::Rustup),
            "homebrew_formula" => Ok(Self::HomebrewFormula),
            "softwareupdate" => Ok(Self::SoftwareUpdate),
            "macports" => Ok(Self::MacPorts),
            "nix_darwin" => Ok(Self::NixDarwin),
            "pnpm" => Ok(Self::Pnpm),
            "npm" => Ok(Self::Npm),
            "yarn" => Ok(Self::Yarn),
            "pipx" => Ok(Self::Pipx),
            "pip" => Ok(Self::Pip),
            "poetry" => Ok(Self::Poetry),
            "rubygems" => Ok(Self::RubyGems),
            "bundler" => Ok(Self::Bundler),
            "cargo" => Ok(Self::Cargo),
            "cargo_binstall" => Ok(Self::CargoBinstall),
            "mas" => Ok(Self::Mas),
            "sparkle" => Ok(Self::Sparkle),
            "setapp" => Ok(Self::Setapp),
            "homebrew_cask" => Ok(Self::HomebrewCask),
            "docker_desktop" => Ok(Self::DockerDesktop),
            "podman" => Ok(Self::Podman),
            "colima" => Ok(Self::Colima),
            "parallels_desktop" => Ok(Self::ParallelsDesktop),
            "xcode_command_line_tools" => Ok(Self::XcodeCommandLineTools),
            "rosetta2" => Ok(Self::Rosetta2),
            "firmware_updates" => Ok(Self::FirmwareUpdates),
            _ => Err(()),
        }
    }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallInstanceIdentityKind {
    DevInode,
    CanonicalPath,
    FallbackHash,
}

impl InstallInstanceIdentityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DevInode => "dev_inode",
            Self::CanonicalPath => "canonical_path",
            Self::FallbackHash => "fallback_hash",
        }
    }
}

impl std::str::FromStr for InstallInstanceIdentityKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "dev_inode" => Ok(Self::DevInode),
            "canonical_path" => Ok(Self::CanonicalPath),
            "fallback_hash" => Ok(Self::FallbackHash),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallProvenance {
    Unknown,
    System,
    Homebrew,
    Macports,
    Nix,
    Asdf,
    Mise,
    RustupInit,
    EnterpriseManaged,
    SourceBuild,
}

impl InstallProvenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::System => "system",
            Self::Homebrew => "homebrew",
            Self::Macports => "macports",
            Self::Nix => "nix",
            Self::Asdf => "asdf",
            Self::Mise => "mise",
            Self::RustupInit => "rustup_init",
            Self::EnterpriseManaged => "enterprise_managed",
            Self::SourceBuild => "source_build",
        }
    }
}

impl std::str::FromStr for InstallProvenance {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "system" => Ok(Self::System),
            "homebrew" => Ok(Self::Homebrew),
            "macports" => Ok(Self::Macports),
            "nix" => Ok(Self::Nix),
            "asdf" => Ok(Self::Asdf),
            "mise" => Ok(Self::Mise),
            "rustup_init" => Ok(Self::RustupInit),
            "enterprise_managed" => Ok(Self::EnterpriseManaged),
            "source_build" => Ok(Self::SourceBuild),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationLevel {
    Automatic,
    NeedsConfirmation,
    ReadOnly,
}

impl AutomationLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::NeedsConfirmation => "needs_confirmation",
            Self::ReadOnly => "read_only",
        }
    }
}

impl std::str::FromStr for AutomationLevel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "automatic" => Ok(Self::Automatic),
            "needs_confirmation" => Ok(Self::NeedsConfirmation),
            "read_only" => Ok(Self::ReadOnly),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrategyKind {
    Unknown,
    InteractivePrompt,
    ReadOnly,
    AsdfSelf,
    RustupSelf,
    HomebrewFormula,
    ManualRemediation,
}

impl StrategyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::InteractivePrompt => "interactive_prompt",
            Self::ReadOnly => "read_only",
            Self::AsdfSelf => "asdf_self",
            Self::RustupSelf => "rustup_self",
            Self::HomebrewFormula => "homebrew_formula",
            Self::ManualRemediation => "manual_remediation",
        }
    }
}

impl std::str::FromStr for StrategyKind {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "interactive_prompt" => Ok(Self::InteractivePrompt),
            "read_only" => Ok(Self::ReadOnly),
            "asdf_self" => Ok(Self::AsdfSelf),
            "rustup_self" => Ok(Self::RustupSelf),
            "homebrew_formula" => Ok(Self::HomebrewFormula),
            "manual_remediation" => Ok(Self::ManualRemediation),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ManagerInstallInstance {
    pub manager: ManagerId,
    pub instance_id: String,
    pub identity_kind: InstallInstanceIdentityKind,
    pub identity_value: String,
    pub display_path: PathBuf,
    pub canonical_path: Option<PathBuf>,
    pub alias_paths: Vec<PathBuf>,
    pub is_active: bool,
    pub version: Option<String>,
    pub provenance: InstallProvenance,
    pub confidence: f64,
    pub decision_margin: Option<f64>,
    pub automation_level: AutomationLevel,
    pub uninstall_strategy: StrategyKind,
    pub update_strategy: StrategyKind,
    pub remediation_strategy: StrategyKind,
    pub explanation_primary: Option<String>,
    pub explanation_secondary: Option<String>,
    pub competing_provenance: Option<InstallProvenance>,
    pub competing_confidence: Option<f64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallImpactPath {
    pub path: String,
    pub exists: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerUninstallPreview {
    pub requested_manager_id: String,
    pub target_manager_id: String,
    pub package_name: String,
    pub strategy: String,
    pub provenance: Option<String>,
    pub automation_level: Option<String>,
    pub confidence: Option<f64>,
    pub decision_margin: Option<f64>,
    pub explanation_primary: Option<String>,
    pub explanation_secondary: Option<String>,
    pub competing_provenance: Option<String>,
    pub competing_confidence: Option<f64>,
    pub files_removed: Vec<UninstallImpactPath>,
    pub directories_removed: Vec<UninstallImpactPath>,
    pub secondary_effects: Vec<String>,
    pub summary_lines: Vec<String>,
    pub blast_radius_score: u32,
    pub requires_yes: bool,
    pub confidence_requires_confirmation: bool,
    pub unknown_provenance: bool,
    pub unknown_override_required: bool,
    pub used_unknown_override: bool,
    pub legacy_fallback_used: bool,
    pub read_only_blocked: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageUninstallPreview {
    pub manager_id: String,
    pub package_name: String,
    pub files_removed: Vec<UninstallImpactPath>,
    pub directories_removed: Vec<UninstallImpactPath>,
    pub secondary_effects: Vec<String>,
    pub summary_lines: Vec<String>,
    pub blast_radius_score: u32,
    pub requires_yes: bool,
    pub confidence_requires_confirmation: bool,
    pub manager_provenance: Option<String>,
    pub manager_automation_level: Option<String>,
    pub manager_uninstall_strategy: Option<String>,
    pub explanation_primary: Option<String>,
    pub explanation_secondary: Option<String>,
    pub competing_provenance: Option<String>,
    pub competing_confidence: Option<f64>,
}
