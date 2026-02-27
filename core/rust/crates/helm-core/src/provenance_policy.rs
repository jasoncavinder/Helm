use crate::models::{AutomationLevel, InstallProvenance};

pub const PROVENANCE_CONFIDENCE_THRESHOLD: f64 = 0.70;
pub const PROVENANCE_MARGIN_THRESHOLD: f64 = 0.15;
pub const AUTOMATIC_CONFIDENCE_THRESHOLD: f64 = 0.85;
pub const UNKNOWN_CONFIRMATION_THRESHOLD: f64 = 0.45;

pub fn automation_level_for(provenance: InstallProvenance, confidence: f64) -> AutomationLevel {
    match provenance {
        InstallProvenance::System
        | InstallProvenance::EnterpriseManaged
        | InstallProvenance::Nix => AutomationLevel::ReadOnly,
        InstallProvenance::Asdf | InstallProvenance::Mise | InstallProvenance::Macports => {
            AutomationLevel::NeedsConfirmation
        }
        InstallProvenance::Unknown => {
            if confidence >= UNKNOWN_CONFIRMATION_THRESHOLD {
                AutomationLevel::NeedsConfirmation
            } else {
                AutomationLevel::ReadOnly
            }
        }
        _ => {
            if confidence >= AUTOMATIC_CONFIDENCE_THRESHOLD {
                AutomationLevel::Automatic
            } else {
                AutomationLevel::NeedsConfirmation
            }
        }
    }
}
