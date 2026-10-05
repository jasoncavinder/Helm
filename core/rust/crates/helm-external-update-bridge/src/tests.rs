use super::*;
use helm_core::{
    external_update::{BoundaryObservation, adoption::AdoptionRequest, adoption::ReviewedAdoption},
    sqlite::SqliteStore,
};

pub(super) fn b(value: &[u8]) -> Bytes {
    Bytes {
        data: value.as_ptr(),
        length: value.len(),
    }
}

#[test]
fn revocation_native_handles_are_read_only_single_use_and_revision_bound() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite");
    std::fs::File::create(&path).unwrap();
    assert_eq!(prepare_ledger(&path, 1), 1);
    let request = br#"{"schemaVersion":1,"requestId":"550e8400-e29b-41d4-a716-446655440000","targetPath":"/Applications/Gone.app"}"#;
    let path_bytes = path.to_str().unwrap().as_bytes();
    let before = std::fs::read(&path).unwrap();
    unsafe {
        let first = helm_external_revocation_prepare(b(path_bytes), b(request), b(b""), 100);
        let stale = helm_external_revocation_prepare(b(path_bytes), b(request), b(b""), 100);
        assert!(!first.is_null());
        assert!(!stale.is_null());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(
            helm_external_revocation_confirm(first, b(path_bytes), 110),
            40
        );
        assert_eq!(
            helm_external_revocation_confirm(stale, b(path_bytes), 110),
            41
        );
        let expired = helm_external_revocation_prepare(b(path_bytes), b(request), b(b""), 100);
        assert_eq!(
            helm_external_revocation_confirm(expired, b(path_bytes), 220),
            41
        );
        let discarded = helm_external_revocation_prepare(b(path_bytes), b(request), b(b""), 100);
        helm_external_revocation_free(discarded);
        helm_external_revocation_free(std::ptr::null_mut());
        assert_eq!(
            helm_external_revocation_confirm(std::ptr::null_mut(), b(path_bytes), 110),
            42
        );
        assert!(helm_external_revocation_prepare(b(path_bytes), b(b"{}"), b(b""), 100).is_null());
        let wrong = helm_external_revocation_prepare(b(path_bytes), b(request), b(b""), 100);
        assert_eq!(
            helm_external_revocation_confirm(wrong, b(b"/tmp/helm.db"), 110),
            42
        );
    }
}

pub(super) fn prepare_ledger(path: &std::path::Path, fresh: u8) -> u32 {
    unsafe { helm_external_ledger_prepare(b(path.to_str().unwrap().as_bytes()), fresh) }
}

#[test]
fn helper_ledger_initialization_reopen_and_no_implicit_authority() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite");
    std::fs::File::create(&path).unwrap();
    assert_eq!(prepare_ledger(&path, 1), 1);
    assert_eq!(prepare_ledger(&path, 0), 1);
    let store = SqliteStore::new(&path);
    let target = std::path::Path::new("/Applications/Example.app");
    assert!(store.external_update_adoption(target).unwrap().is_none());
    let revocation = store.revoke_external_update_adoption(target).unwrap();
    assert_eq!(prepare_ledger(&path, 0), 1);
    assert_eq!(
        store.external_update_adoption(target).unwrap().unwrap(),
        revocation
    );
    assert_eq!(preflight(&fixture()), UNRESOLVED);
}

#[test]
fn helper_ledger_never_creates_missing_file_or_parent() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        temp.path().join("ledger.sqlite"),
        temp.path().join("missing/ledger.sqlite"),
    ] {
        for fresh in [0, 1] {
            assert_eq!(prepare_ledger(&path, fresh), 0);
        }
        assert!(!path.exists());
    }
    assert!(!temp.path().join("missing").exists());
}

#[test]
fn helper_ledger_rejects_existing_empty_corrupt_or_wrong_freshness() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite");
    std::fs::write(&path, []).unwrap();
    assert_eq!(prepare_ledger(&path, 0), 0);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
    std::fs::write(&path, b"corrupt").unwrap();
    assert_eq!(prepare_ledger(&path, 0), 0);
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
    std::fs::write(&path, []).unwrap();
    assert_eq!(prepare_ledger(&path, 1), 1);
    assert_eq!(prepare_ledger(&path, 1), 0);
    assert_eq!(prepare_ledger(&path, 2), 0);
    assert_eq!(prepare_ledger(&path, 0), 1);
}

#[cfg(unix)]
#[test]
fn helper_ledger_sqlite_nofollow_refuses_final_alias() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    std::fs::write(&target, []).unwrap();
    let path = temp.path().join("ledger.sqlite");
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert_eq!(prepare_ledger(&path, 1), 0);
    assert_eq!(std::fs::read(&target).unwrap(), b"");
}

#[test]
fn helper_ledger_rejects_invalid_abi_input() {
    for path in [
        b"".as_slice(),
        b"ledger.sqlite",
        b"/tmp/helm.db",
        b"/tmp/ledger.sqlite\0",
        b"\xff",
    ] {
        assert_eq!(unsafe { helm_external_ledger_prepare(b(path), 0) }, 0);
    }
    assert_eq!(
        unsafe {
            helm_external_ledger_prepare(
                Bytes {
                    data: std::ptr::null(),
                    length: 1,
                },
                0,
            )
        },
        0
    );
    assert_eq!(
        unsafe { helm_external_ledger_prepare(b(&[b'a'; 4097]), 0) },
        0
    );
}

pub(super) fn fixture() -> NativeTarget {
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

fn request() -> Vec<u8> {
    br#"{"schemaVersion":1,"requestId":"550e8400-e29b-41d4-a716-446655440000","targetPath":"/Applications/Example.app","expectedBundleIdentifier":"org.example.App","expectedInstalledBuild":"100"}"#.to_vec()
}

#[test]
fn consent_bridge_distinguishes_history_without_granting_authority() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite");
    let query = |input: &NativeTarget| unsafe {
        helm_external_consent_status(input, b(&request()), b(path.to_str().unwrap().as_bytes()))
    };
    assert_eq!(query(&fixture()), CONSENT_LEDGER_UNAVAILABLE);
    assert!(!path.exists());
    std::fs::File::create(&path).unwrap();
    assert_eq!(prepare_ledger(&path, 1), 1);
    assert_eq!(query(&fixture()), CONSENT_NOT_RECORDED);
    let store = SqliteStore::new(&path);
    let (native, roots) = unsafe { map_target(&fixture()) }.unwrap();
    ReviewedAdoption::prepare(
        &store,
        AdoptionRequest {
            schema_version: 1,
            consent_id: "550e8400-e29b-41d4-a716-446655440099".into(),
            target_path: native.canonical_path.clone(),
            expected_bundle_identifier: native.bundle_identifier.clone(),
            expected_installed_build: native.build.clone(),
        },
        native.clone(),
        boundary_fixture(),
        &roots,
        100,
    )
    .unwrap()
    .confirm(&store, native.clone(), boundary_fixture(), &roots, 110)
    .unwrap();
    assert_eq!(query(&fixture()), CONSENT_RECORDED);
    let mut changed = fixture();
    changed.ed25519_public_key = b(&[5; 32]);
    assert_eq!(query(&changed), CONSENT_IDENTITY_CHANGED);
    for flags in 1..=63 {
        let mut excluded = fixture();
        excluded.manager_exclusions = flags;
        excluded.has_store_receipt = u8::from(flags & 1 != 0);
        assert_eq!(query(&excluded), CONSENT_TARGET_REJECTED);
    }
    changed = fixture();
    changed.build = b(b"changed");
    assert_eq!(query(&changed), CONSENT_TARGET_REJECTED);
    store
        .revoke_external_update_adoption(&native.canonical_path)
        .unwrap();
    assert_eq!(query(&fixture()), CONSENT_REVOKED);
    assert_eq!(preflight(&fixture()), UNRESOLVED);
}

#[test]
fn consent_bridge_rejects_bad_intent_and_paths_without_io() {
    for path in [
        b"".as_slice(),
        b"relative/ledger.sqlite",
        b"/tmp/helm.db",
        b"/tmp/ledger.sqlite\0",
    ] {
        assert_eq!(
            unsafe { helm_external_consent_status(&fixture(), b(&request()), b(path)) },
            INVALID
        );
    }
    assert_eq!(
        unsafe {
            helm_external_consent_status(std::ptr::null(), b(&request()), b(b"/tmp/ledger.sqlite"))
        },
        INVALID
    );
    for data in [b"{}".as_slice(), b"{\"authority\":\"Standalone\"}"] {
        assert_eq!(
            unsafe { helm_external_consent_status(&fixture(), b(data), b(b"/tmp/ledger.sqlite")) },
            INVALID
        );
    }
}

#[test]
fn strict_request_contract_rejects_authority_paths_and_unknown_fields() {
    let valid = request();
    let validate = |data: &[u8]| unsafe {
        helm_external_preflight_request(b(data), b(b"/Users/agent/Applications"))
    };
    assert_eq!(validate(&valid), UNRESOLVED);
    let text = String::from_utf8(valid).unwrap();
    for (from, to) in [
        ("\"schemaVersion\":1", "\"schemaVersion\":2"),
        (
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
        ),
        (
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"authority\":\"Standalone\"",
        ),
        (
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"databasePath\":\"/tmp/user.db\"",
        ),
        (
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"feedURL\":\"https://example.org\"",
        ),
        ("/Applications/Example.app", "/tmp/Example.app"),
        (
            "/Applications/Example.app",
            "/Users/other/Applications/Example.app",
        ),
        (
            "/Applications/Example.app",
            "/Applications/Host.app/Nested.app",
        ),
        ("/Applications/Example.app", "/Applications/../Example.app"),
        ("/Applications/Example.app", "/Applications//Example.app"),
        ("org.example.App", "COM.JASONCAVINDER.HELM.QA"),
        ("446655440000", "44665544000Z"),
        ("\"100\"", "\"\""),
    ] {
        assert_ne!(
            validate(text.replace(from, to).as_bytes()),
            UNRESOLVED,
            "{to}"
        );
    }
    assert_eq!(validate(&[b' '; 8193]), INVALID);
    assert_eq!(validate(b"\xff"), INVALID);
    assert_eq!(validate(b""), INVALID);
}

#[test]
fn requested_native_target_requires_exact_path_identifier_and_build() {
    let request = request();
    let check =
        |input: &NativeTarget| unsafe { helm_external_requested_preflight(input, b(&request)) };
    assert_eq!(check(&fixture()), UNRESOLVED);
    for change in 0..3 {
        let mut input = fixture();
        match change {
            0 => input.canonical_path = b(b"/Applications/Other.app"),
            1 => input.bundle_identifier = b(b"org.example.Other"),
            _ => input.build = b(b"101"),
        }
        assert_eq!(check(&input), TARGET_CHANGED);
    }
    let mut input = fixture();
    input.manager_exclusions = 2;
    assert_eq!(check(&input), OTHER_MANAGER);
    input.manager_exclusions = 0;
    input.writable_by_others = 1;
    assert_eq!(check(&input), UNSUPPORTED_TARGET);
}

#[test]
fn requested_preflight_null_and_oversized_buffers_fail_closed() {
    let invalid = Bytes {
        data: std::ptr::null(),
        length: usize::MAX,
    };
    assert_eq!(
        unsafe { helm_external_requested_preflight(std::ptr::null(), b(&request())) },
        INVALID
    );
    assert_eq!(
        unsafe { helm_external_requested_preflight(&fixture(), invalid) },
        INVALID
    );
    assert_eq!(
        unsafe { helm_external_preflight_request(invalid, b(b"")) },
        INVALID
    );
    assert_eq!(
        unsafe { helm_external_preflight_request(b(&request()), invalid) },
        INVALID
    );
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
    for flags in 1..=63 {
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
            3 => input.manager_exclusions = 64,
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

pub(super) fn boundary_fixture() -> BoundaryObservation {
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
    for flags in 1..=63 {
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
