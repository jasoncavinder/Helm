use crate::models::ManagerId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PackageRef {
    pub manager: ManagerId,
    pub name: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRuntimeState {
    pub is_active: bool,
    pub is_default: bool,
    pub has_override: bool,
}

impl PackageRuntimeState {
    pub const fn is_empty(&self) -> bool {
        !self.is_active && !self.is_default && !self.has_override
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub package: PackageRef,
    pub installed_version: Option<String>,
    pub pinned: bool,
    #[serde(default)]
    pub runtime_state: PackageRuntimeState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutdatedPackage {
    pub package: PackageRef,
    pub installed_version: Option<String>,
    pub candidate_version: String,
    pub pinned: bool,
    pub restart_required: bool,
    #[serde(default)]
    pub runtime_state: PackageRuntimeState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageCandidate {
    pub package: PackageRef,
    pub version: Option<String>,
    pub summary: Option<String>,
}
