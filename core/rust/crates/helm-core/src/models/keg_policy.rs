use serde::{Deserialize, Serialize};

use crate::models::PackageRef;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomebrewKegPolicy {
    Keep,
    Cleanup,
}

impl HomebrewKegPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Cleanup => "cleanup",
        }
    }
}

impl std::str::FromStr for HomebrewKegPolicy {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "keep" => Ok(Self::Keep),
            "cleanup" => Ok(Self::Cleanup),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageKegPolicy {
    pub package: PackageRef,
    pub policy: HomebrewKegPolicy,
}
