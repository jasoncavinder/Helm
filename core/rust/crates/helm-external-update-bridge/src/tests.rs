use super::*;
use helm_core::{
    external_update::{BoundaryObservation, adoption::AdoptionRequest, adoption::ReviewedAdoption},
    sqlite::SqliteStore,
};

fn b(value: &[u8]) -> Bytes {
    Bytes {
        data: value.as_ptr(),
        length: value.len(),
    }
}

fn fixture() -> NativeTarget {
    NativeTarget {
        abi_version: 1,
        canonical_path: b(b"/Applications/Example.app"),
        device: 42,
        inode: 123,
        bundle_identifier: b(b"org.example.App"),
        build: b(b"100"),
        team_identifier: b(b"ABCDE12345"),
        code_directory_hash: b(&[7; 20]),
        ed25519_public_key: b(&[8; 32]),
        feed_url: b(b"https://example.org/feed"),
        framework_major: 2,
        has_store_receipt: 0,
        writable_by_others: 0,
        manager_exclusions: 0,
        user_applications_root: b(b"/Users/agent/Applications"),
    }
}

fn preflight(input: &NativeTarget) -> u32 {
    unsafe { helm_external_target_preflight(input) }
}

#[test]
fn abi_v1_layout_matches_the_native_64_bit_contract() {
    assert_eq!(std::mem::size_of::<NativeTarget>(), 168);
    assert_eq!(std::mem::align_of::<NativeTarget>(), 8);
    assert_eq!(std::mem::offset_of!(NativeTarget, device), 24);
    assert_eq!(std::mem::offset_of!(NativeTarget, inode), 32);
    assert_eq!(std::mem::offset_of!(NativeTarget, ed25519_public_key), 104);
    assert_eq!(std::mem::offset_of!(NativeTarget, manager_exclusions), 144);
    assert_eq!(
        std::mem::offset_of!(NativeTarget, user_applications_root),
        152
    );
}

#[test]
fn mapping_preserves_native_facts_without_inventing_authority() {
    let input = fixture();
    let (target, roots) = unsafe { map_target(&input) }.unwrap();
    assert_eq!(
        target.canonical_path,
        PathBuf::from("/Applications/Example.app")
    );
    assert_eq!((target.device, target.inode), (42, 123));
    assert_eq!(target.bundle_identifier, "org.example.App");
    assert_eq!(target.build, "100");
    assert_eq!(target.team_identifier, "ABCDE12345");
    assert_eq!(target.code_directory_hash, vec![7; 20]);
    assert_eq!(target.ed25519_public_key, vec![8; 32]);
    assert_eq!(target.feed_url, "https://example.org/feed");
    assert_eq!(target.framework_major, 2);
    assert_eq!(target.authority, Authority::Unknown);
    assert_eq!(
        roots,
        vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/Users/agent/Applications")
        ]
    );
    assert_eq!(preflight(&input), UNRESOLVED);
}

#[test]
fn all_exclusion_combinations_remain_other_manager() {
    for flags in 1..=7 {
        let mut input = fixture();
        input.manager_exclusions = flags;
        input.has_store_receipt = u8::from(flags & 1 != 0);
        let (target, _) = unsafe { map_target(&input) }.unwrap();
        assert_eq!(target.authority, Authority::OtherManager);
        assert_eq!(preflight(&input), OTHER_MANAGER);
    }
}

#[test]
fn null_input_is_rejected_before_target_access() {
    assert_eq!(
        unsafe { helm_external_target_preflight(std::ptr::null()) },
        INVALID
    );
}

#[test]
fn malformed_abi_is_not_a_successful_zero_result() {
    for change in 0..9 {
        let mut input = fixture();
        match change {
            0 => input.abi_version = 2,
            1 => input.has_store_receipt = 2,
            2 => input.writable_by_others = 2,
            3 => input.manager_exclusions = 8,
            4 => input.has_store_receipt = 1,
            5 => input.manager_exclusions = 1,
            6 => input.build = b(b"\xff"),
            7 => input.build = b(b"100\0hidden"),
            _ => {
                input.build = Bytes {
                    data: std::ptr::null(),
                    length: 1,
                }
            }
        }
        assert_eq!(preflight(&input), INVALID, "change {change}");
    }
}

#[test]
fn oversized_lengths_are_rejected_before_dereferencing() {
    for change in 0..8 {
        let mut input = fixture();
        let field = match change {
            0 => &mut input.canonical_path,
            1 => &mut input.bundle_identifier,
            2 => &mut input.build,
            3 => &mut input.team_identifier,
            4 => &mut input.code_directory_hash,
            5 => &mut input.ed25519_public_key,
            6 => &mut input.feed_url,
            _ => &mut input.user_applications_root,
        };
        *field = Bytes {
            data: std::ptr::null(),
            length: usize::MAX,
        };
        assert_eq!(preflight(&input), INVALID);
    }
}

#[test]
fn roots_are_not_expanded_from_home_or_arbitrary_client_scope() {
    let mut input = fixture();
    input.canonical_path = b(b"/Users/agent/Applications/Example.app");
    assert_eq!(preflight(&input), UNRESOLVED);
    input.user_applications_root = Bytes {
        data: std::ptr::null(),
        length: 0,
    };
    assert_eq!(preflight(&input), OUTSIDE_ROOTS);
    input.canonical_path = b(b"/tmp/Example.app");
    input.user_applications_root = b(b"/tmp");
    assert_eq!(preflight(&input), OUTSIDE_ROOTS);
    input.canonical_path = b(b"/Applications/Host.app/Nested.app");
    assert_eq!(preflight(&input), OUTSIDE_ROOTS);
    input.canonical_path = b(b"/Applications/../Example.app");
    assert_eq!(preflight(&input), OUTSIDE_ROOTS);
}

#[test]
fn unsafe_target_fields_use_the_shared_core_policy() {
    for change in 0..8 {
        let mut input = fixture();
        match change {
            0 => input.writable_by_others = 1,
            1 => input.framework_major = 1,
            2 => input.code_directory_hash = b(&[0; 20]),
            3 => input.ed25519_public_key = b(&[8; 31]),
            4 => input.feed_url = b(b"http://example.org/feed"),
            5 => input.team_identifier = b(b"bad"),
            6 => input.bundle_identifier = b(b"invalid"),
            _ => input.build = b(b""),
        }
        assert_eq!(preflight(&input), UNSUPPORTED_TARGET, "change {change}");
    }
    let mut input = fixture();
    input.bundle_identifier = b(b"COM.JASONCAVINDER.HELM.QA");
    assert_eq!(preflight(&input), HELM_SELF_UPDATE);
}

fn boundary_fixture() -> BoundaryObservation {
    BoundaryObservation {
        helper_identifier: "com.jasoncavinder.Helm.SparkleExternalUpdater".into(),
        helper_team_identifier: "V73WPJR9M4".into(),
        helper_code_directory_hash: vec![9; 20],
        caller_identifier: "com.jasoncavinder.Helm".into(),
        caller_team_identifier: "V73WPJR9M4".into(),
        authenticated_live_caller: true,
        developer_id_signature_valid: true,
        notarization_accepted: true,
        helm_sandbox_preserved: true,
        external_helper_unsandboxed: true,
        direct_consumer_channel: true,
    }
}

#[test]
fn fresh_bridge_evidence_fences_saved_adoption() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(directory.path().join("isolated.db"));
    store.migrate_to_latest().unwrap();
    let (native, roots) = unsafe { map_target(&fixture()) }.unwrap();
    assert_eq!(
        adoption::resolve(&store, native.clone(), &roots)
            .unwrap()
            .authority,
        Authority::Unknown
    );
    let request = AdoptionRequest {
        schema_version: 1,
        consent_id: "550e8400-e29b-41d4-a716-446655440000".into(),
        target_path: native.canonical_path.clone(),
        expected_bundle_identifier: native.bundle_identifier.clone(),
        expected_installed_build: native.build.clone(),
    };
    let review = ReviewedAdoption::prepare(
        &store,
        request,
        native.clone(),
        boundary_fixture(),
        &roots,
        100,
    )
    .unwrap();
    let receipt = review
        .confirm(&store, native.clone(), boundary_fixture(), &roots, 110)
        .unwrap();
    assert_eq!(
        adoption::resolve(&store, native, &roots).unwrap().authority,
        Authority::UserAdopted(receipt.token)
    );
    for flags in 1..=7 {
        let mut input = fixture();
        input.manager_exclusions = flags;
        input.has_store_receipt = u8::from(flags & 1 != 0);
        let (fresh, roots) = unsafe { map_target(&input) }.unwrap();
        assert!(adoption::resolve(&store, fresh, &roots).is_err());
        assert_eq!(preflight(&input), OTHER_MANAGER);
    }
    let mut input = fixture();
    input.ed25519_public_key = b(&[9; 32]);
    let (fresh, roots) = unsafe { map_target(&input) }.unwrap();
    assert_eq!(
        adoption::resolve(&store, fresh, &roots).unwrap().authority,
        Authority::Unknown
    );
    // The diagnostic bridge itself never reads or asserts saved adoption.
    assert_eq!(preflight(&fixture()), UNRESOLVED);
    store
        .revoke_external_update_adoption(&PathBuf::from("/Applications/Example.app"))
        .unwrap();
    let (fresh, roots) = unsafe { map_target(&fixture()) }.unwrap();
    assert_eq!(
        adoption::resolve(&store, fresh, &roots).unwrap().authority,
        Authority::Unknown
    );
}
