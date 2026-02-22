use crate::adapters::ManagerAdapter;
use crate::models::{ManagerAuthority, ManagerId};

/// Groups registered adapters into execution phases by authority level.
///
/// Returns phases in order: [Authoritative], [Standard], [Guarded].
/// DetectionOnly managers are excluded. Empty phases are omitted.
/// Within each phase, managers execute in parallel.
pub fn authority_phases(adapters: &[&dyn ManagerAdapter]) -> Vec<Vec<ManagerId>> {
    let mut authoritative = Vec::new();
    let mut standard = Vec::new();
    let mut guarded = Vec::new();

    for adapter in adapters {
        let descriptor = adapter.descriptor();
        match descriptor.authority {
            ManagerAuthority::Authoritative => authoritative.push(descriptor.id),
            ManagerAuthority::Standard => standard.push(descriptor.id),
            ManagerAuthority::Guarded => guarded.push(descriptor.id),
            ManagerAuthority::DetectionOnly => {} // skip
        }
    }

    let mut phases = Vec::new();
    if !authoritative.is_empty() {
        phases.push(authoritative);
    }
    if !standard.is_empty() {
        phases.push(standard);
    }
    if !guarded.is_empty() {
        phases.push(guarded);
    }
    phases
}

/// Groups registered adapters into detection phases by authority level.
///
/// Returns phases in order: [Authoritative], [Standard], [Guarded], [DetectionOnly].
/// Empty phases are omitted. Within each phase, managers execute in parallel.
pub fn detection_phases(adapters: &[&dyn ManagerAdapter]) -> Vec<Vec<ManagerId>> {
    let mut authoritative = Vec::new();
    let mut standard = Vec::new();
    let mut guarded = Vec::new();
    let mut detection_only = Vec::new();

    for adapter in adapters {
        let descriptor = adapter.descriptor();
        match descriptor.authority {
            ManagerAuthority::Authoritative => authoritative.push(descriptor.id),
            ManagerAuthority::Standard => standard.push(descriptor.id),
            ManagerAuthority::Guarded => guarded.push(descriptor.id),
            ManagerAuthority::DetectionOnly => detection_only.push(descriptor.id),
        }
    }

    let mut phases = Vec::new();
    if !authoritative.is_empty() {
        phases.push(authoritative);
    }
    if !standard.is_empty() {
        phases.push(standard);
    }
    if !guarded.is_empty() {
        phases.push(guarded);
    }
    if !detection_only.is_empty() {
        phases.push(detection_only);
    }
    phases
}

#[cfg(test)]
mod tests {
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter,
    };
    use crate::models::{
        ActionSafety, Capability, ManagerAction, ManagerAuthority, ManagerCategory,
        ManagerDescriptor, ManagerId,
    };

    use super::{authority_phases, detection_phases};

    struct StubAdapter {
        descriptor: ManagerDescriptor,
    }

    impl StubAdapter {
        fn new(id: ManagerId, authority: ManagerAuthority) -> Self {
            Self {
                descriptor: ManagerDescriptor {
                    id,
                    display_name: "stub",
                    category: ManagerCategory::ToolRuntime,
                    authority,
                    capabilities: &[Capability::Detect],
                },
            }
        }
    }

    impl ManagerAdapter for StubAdapter {
        fn descriptor(&self) -> &ManagerDescriptor {
            &self.descriptor
        }

        fn action_safety(&self, action: ManagerAction) -> ActionSafety {
            action.safety()
        }

        fn execute(&self, _request: AdapterRequest) -> AdapterResult<AdapterResponse> {
            unimplemented!()
        }
    }

    #[test]
    fn groups_by_authority_in_correct_order() {
        let mise = StubAdapter::new(ManagerId::Mise, ManagerAuthority::Authoritative);
        let rustup = StubAdapter::new(ManagerId::Rustup, ManagerAuthority::Authoritative);
        let npm = StubAdapter::new(ManagerId::Npm, ManagerAuthority::Standard);
        let brew = StubAdapter::new(ManagerId::HomebrewFormula, ManagerAuthority::Guarded);

        let adapters: Vec<&dyn ManagerAdapter> = vec![&mise, &rustup, &npm, &brew];
        let phases = authority_phases(&adapters);

        assert_eq!(phases.len(), 3);
        assert!(phases[0].contains(&ManagerId::Mise));
        assert!(phases[0].contains(&ManagerId::Rustup));
        assert_eq!(phases[1], vec![ManagerId::Npm]);
        assert_eq!(phases[2], vec![ManagerId::HomebrewFormula]);
    }

    #[test]
    fn skips_detection_only_managers() {
        let sparkle = StubAdapter::new(ManagerId::Sparkle, ManagerAuthority::DetectionOnly);
        let mise = StubAdapter::new(ManagerId::Mise, ManagerAuthority::Authoritative);

        let adapters: Vec<&dyn ManagerAdapter> = vec![&sparkle, &mise];
        let phases = authority_phases(&adapters);

        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0], vec![ManagerId::Mise]);
    }

    #[test]
    fn omits_empty_phases() {
        let brew = StubAdapter::new(ManagerId::HomebrewFormula, ManagerAuthority::Guarded);

        let adapters: Vec<&dyn ManagerAdapter> = vec![&brew];
        let phases = authority_phases(&adapters);

        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0], vec![ManagerId::HomebrewFormula]);
    }

    #[test]
    fn empty_input_returns_no_phases() {
        let adapters: Vec<&dyn ManagerAdapter> = vec![];
        let phases = authority_phases(&adapters);
        assert!(phases.is_empty());
    }

    #[test]
    fn detection_phases_include_detection_only_managers_last() {
        let sparkle = StubAdapter::new(ManagerId::Sparkle, ManagerAuthority::DetectionOnly);
        let mise = StubAdapter::new(ManagerId::Mise, ManagerAuthority::Authoritative);
        let brew = StubAdapter::new(ManagerId::HomebrewFormula, ManagerAuthority::Guarded);

        let adapters: Vec<&dyn ManagerAdapter> = vec![&sparkle, &mise, &brew];
        let phases = detection_phases(&adapters);

        assert_eq!(phases.len(), 3);
        assert_eq!(phases[0], vec![ManagerId::Mise]);
        assert_eq!(phases[1], vec![ManagerId::HomebrewFormula]);
        assert_eq!(phases[2], vec![ManagerId::Sparkle]);
    }

    #[test]
    fn five_adapter_authority_phases() {
        let mise = StubAdapter::new(ManagerId::Mise, ManagerAuthority::Authoritative);
        let rustup = StubAdapter::new(ManagerId::Rustup, ManagerAuthority::Authoritative);
        let mas = StubAdapter::new(ManagerId::Mas, ManagerAuthority::Standard);
        let brew = StubAdapter::new(ManagerId::HomebrewFormula, ManagerAuthority::Guarded);
        let swupd = StubAdapter::new(ManagerId::SoftwareUpdate, ManagerAuthority::Guarded);

        let adapters: Vec<&dyn ManagerAdapter> = vec![&mise, &rustup, &mas, &brew, &swupd];
        let phases = authority_phases(&adapters);

        assert_eq!(phases.len(), 3);
        // Phase 1: Authoritative — mise + rustup
        assert!(phases[0].contains(&ManagerId::Mise));
        assert!(phases[0].contains(&ManagerId::Rustup));
        assert_eq!(phases[0].len(), 2);
        // Phase 2: Standard — mas
        assert_eq!(phases[1], vec![ManagerId::Mas]);
        // Phase 3: Guarded — homebrew + softwareupdate
        assert!(phases[2].contains(&ManagerId::HomebrewFormula));
        assert!(phases[2].contains(&ManagerId::SoftwareUpdate));
        assert_eq!(phases[2].len(), 2);
    }
}
