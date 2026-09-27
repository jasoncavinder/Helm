use serde::Serialize;

use super::PersistenceResult;

/// A product experience, not a build version. RCs, stable, and patches share it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum FirstRunExperience {
    #[serde(rename = "wayfinder-v0.20")]
    WayfinderV020,
}

impl FirstRunExperience {
    pub const CURRENT: Self = Self::WayfinderV020;

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "wayfinder-v0.20" => Some(Self::WayfinderV020),
            _ => None,
        }
    }

    pub(crate) fn acknowledgment_key(self) -> &'static str {
        match self {
            Self::WayfinderV020 => "first_run.experience.wayfinder-v0.20.acknowledged",
        }
    }
}

/// Failure to load this state must not be interpreted as either completion or
/// permission to start discovery. Acknowledgment grants no operational consent.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FirstRunExperienceState {
    pub schema_version: u32,
    pub experience_id: FirstRunExperience,
    pub acknowledged: bool,
}

impl FirstRunExperienceState {
    pub(crate) fn new(experience_id: FirstRunExperience, acknowledged: bool) -> Self {
        Self {
            schema_version: 1,
            experience_id,
            acknowledged,
        }
    }
}

/// Independent of legacy CLI onboarding, terms acceptance, and setup receipts.
pub trait FirstRunStore: Send + Sync {
    fn first_run_experience_state(
        &self,
        experience: FirstRunExperience,
    ) -> PersistenceResult<FirstRunExperienceState>;

    /// Call only after explicit completion or "Use Helm Now" in the real flow.
    /// Repeated calls are idempotent; previews must never acknowledge it.
    fn acknowledge_first_run_experience(
        &self,
        experience: FirstRunExperience,
    ) -> PersistenceResult<()>;
}
