use std::collections::HashSet;

use helm_core::models::{Capability, ManagerAuthority, ManagerCategory, ManagerId};
use helm_core::registry;

#[test]
fn registry_covers_all_manager_ids_exactly_once() {
    let descriptors = registry::managers();

    assert_eq!(descriptors.len(), ManagerId::ALL.len());

    let mut seen = HashSet::new();
    for descriptor in descriptors {
        assert!(
            seen.insert(descriptor.id),
            "duplicate manager descriptor found"
        );
    }

    for id in ManagerId::ALL {
        assert!(
            registry::manager(id).is_some(),
            "missing manager descriptor for {:?}",
            id
        );
    }
}

#[test]
fn language_managers_declare_required_capabilities() {
    let required = [
        Capability::Install,
        Capability::Uninstall,
        Capability::ListInstalled,
        Capability::ListOutdated,
        Capability::Search,
    ];

    for descriptor in registry::managers()
        .iter()
        .filter(|descriptor| descriptor.category == ManagerCategory::Language)
    {
        for capability in required {
            assert!(
                descriptor.supports(capability),
                "language manager {:?} missing {:?}",
                descriptor.id,
                capability
            );
        }
    }
}

#[test]
fn detection_only_managers_are_non_mutating() {
    for descriptor in registry::managers()
        .iter()
        .filter(|descriptor| descriptor.authority == ManagerAuthority::DetectionOnly)
    {
        assert_eq!(descriptor.capabilities, &[Capability::Detect]);
    }
}

#[test]
fn tool_runtime_managers_are_authoritative() {
    for descriptor in registry::managers()
        .iter()
        .filter(|descriptor| descriptor.category == ManagerCategory::ToolRuntime)
    {
        assert_eq!(descriptor.authority, ManagerAuthority::Authoritative);
    }
}

#[test]
fn manager_ids_roundtrip_to_and_from_storage_keys() {
    for manager in ManagerId::ALL {
        let serialized = manager.as_str();
        let parsed: Option<ManagerId> = serialized.parse().ok();
        assert_eq!(parsed, Some(manager));
    }
}
