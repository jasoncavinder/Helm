use crate::models::ManagerId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PackageRef {
    pub manager: ManagerId,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub package: PackageRef,
    pub installed_version: Option<String>,
    pub pinned: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutdatedPackage {
    pub package: PackageRef,
    pub installed_version: Option<String>,
    pub candidate_version: String,
    pub pinned: bool,
    pub restart_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageCandidate {
    pub package: PackageRef,
    pub version: Option<String>,
    pub summary: Option<String>,
}
