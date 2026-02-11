use crate::models::ManagerId;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct PackageRef {
    pub manager: ManagerId,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledPackage {
    pub package: PackageRef,
    pub installed_version: Option<String>,
    pub pinned: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutdatedPackage {
    pub package: PackageRef,
    pub installed_version: Option<String>,
    pub candidate_version: String,
    pub pinned: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageCandidate {
    pub package: PackageRef,
    pub version: Option<String>,
    pub summary: Option<String>,
}
