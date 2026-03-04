use crate::models::{InstallProvenance, ManagerId};

/// Resolve the parent manager dependency implied by install provenance for a manager.
///
/// This describes runtime/lifecycle dependency ownership, not install-method preference.
pub fn provenance_dependency_manager(
    manager: ManagerId,
    provenance: InstallProvenance,
) -> Option<ManagerId> {
    match provenance {
        InstallProvenance::Homebrew => Some(ManagerId::HomebrewFormula),
        InstallProvenance::Macports => Some(ManagerId::MacPorts),
        InstallProvenance::Nix => Some(ManagerId::NixDarwin),
        InstallProvenance::Asdf => Some(ManagerId::Asdf),
        InstallProvenance::Mise if manager != ManagerId::Mise => Some(ManagerId::Mise),
        _ => None,
    }
}

pub fn provenance_requires_manager_dependency(
    manager: ManagerId,
    provenance: InstallProvenance,
) -> bool {
    provenance_dependency_manager(manager, provenance).is_some()
}

#[cfg(test)]
mod tests {
    use super::{provenance_dependency_manager, provenance_requires_manager_dependency};
    use crate::models::{InstallProvenance, ManagerId};

    #[test]
    fn dependency_manager_mapping_matches_expected_parents() {
        assert_eq!(
            provenance_dependency_manager(ManagerId::Rustup, InstallProvenance::Homebrew),
            Some(ManagerId::HomebrewFormula)
        );
        assert_eq!(
            provenance_dependency_manager(ManagerId::Rustup, InstallProvenance::Macports),
            Some(ManagerId::MacPorts)
        );
        assert_eq!(
            provenance_dependency_manager(ManagerId::Rustup, InstallProvenance::Nix),
            Some(ManagerId::NixDarwin)
        );
        assert_eq!(
            provenance_dependency_manager(ManagerId::Rustup, InstallProvenance::Asdf),
            Some(ManagerId::Asdf)
        );
        assert_eq!(
            provenance_dependency_manager(ManagerId::Npm, InstallProvenance::Mise),
            Some(ManagerId::Mise)
        );
        assert_eq!(
            provenance_dependency_manager(ManagerId::Mise, InstallProvenance::Mise),
            None
        );
        assert_eq!(
            provenance_dependency_manager(ManagerId::Rustup, InstallProvenance::RustupInit),
            None
        );
    }

    #[test]
    fn dependency_requirement_tracks_mapping_presence() {
        assert!(provenance_requires_manager_dependency(
            ManagerId::Rustup,
            InstallProvenance::Homebrew
        ));
        assert!(!provenance_requires_manager_dependency(
            ManagerId::Rustup,
            InstallProvenance::RustupInit
        ));
    }
}
