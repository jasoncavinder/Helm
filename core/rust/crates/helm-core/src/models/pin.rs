use std::time::SystemTime;

use crate::models::PackageRef;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum PinKind {
    Native,
    Virtual,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinRecord {
    pub package: PackageRef,
    pub kind: PinKind,
    pub pinned_version: Option<String>,
    pub created_at: SystemTime,
}
