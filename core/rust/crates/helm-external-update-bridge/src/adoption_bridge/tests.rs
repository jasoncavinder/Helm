use super::*;
use crate::tests::{b, fixture, prepare_ledger};
use helm_core::sqlite::SqliteStore;
use std::path::Path;

const REQUEST: &[u8] = br#"{"schemaVersion":1,"consentId":"550e8400-e29b-41d4-a716-446655440000","targetPath":"/Applications/Example.app","expectedBundleIdentifier":"org.example.App","expectedInstalledBuild":"100"}"#;

fn boundary() -> NativeBoundary {
    NativeBoundary {
        abi_version: 1,
        helper_identifier: b(b"com.jasoncavinder.Helm.SparkleExternalUpdater"),
        helper_team_identifier: b(b"V73WPJR9M4"),
        helper_code_directory_hash: b(&[9; 20]),
        caller_identifier: b(b"com.jasoncavinder.Helm"),
        caller_team_identifier: b(b"V73WPJR9M4"),
        observed_flags: 63,
    }
}

fn ledger() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ledger.sqlite");
    std::fs::File::create(&path).unwrap();
    assert_eq!(prepare_ledger(&path, 1), 1);
    (temp, path)
}

fn prepare(path: &Path) -> *mut AdoptionReview {
    let review = unsafe {
        helm_external_adoption_prepare(
            b(path.to_str().unwrap().as_bytes()),
            b(REQUEST),
            &fixture(),
            &boundary(),
            100,
        )
    };
    assert!(!review.is_null());
    review
}

fn status(path: &Path) -> adoption::ConsentStatus {
    let (target, roots) = unsafe { map_target(&fixture()) }.unwrap();
    SqliteStore::inspect_external_update_consent(path, &target, &roots).unwrap()
}

#[test]
fn boundary_abi_layout_and_mapping_preserve_all_facts() {
    assert_eq!(std::mem::size_of::<NativeBoundary>(), 96);
    assert_eq!(std::mem::align_of::<NativeBoundary>(), 8);
    assert_eq!(
        std::mem::offset_of!(NativeBoundary, helper_code_directory_hash),
        40
    );
    assert_eq!(std::mem::offset_of!(NativeBoundary, observed_flags), 88);
    assert_eq!(
        unsafe { map_boundary(&boundary()) }.unwrap(),
        crate::tests::boundary_fixture()
    );
}

#[test]
fn review_is_read_only_and_confirm_returns_no_update_token() {
    let (_temp, path) = ledger();
    let before = std::fs::read(&path).unwrap();
    let first = prepare(&path);
    let stale = prepare(&path);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(status(&path), adoption::ConsentStatus::NotRecorded);
    let p = b(path.to_str().unwrap().as_bytes());
    unsafe {
        assert_eq!(
            helm_external_adoption_confirm(first, p, &fixture(), &boundary(), 110),
            ADOPTED
        );
        assert_eq!(
            helm_external_adoption_confirm(stale, p, &fixture(), &boundary(), 110),
            REVIEW_CHANGED
        );
    }
    assert_eq!(status(&path), adoption::ConsentStatus::Recorded);
    assert_eq!(
        unsafe { helm_external_target_preflight(&fixture()) },
        UNRESOLVED
    );
}

#[test]
fn rejected_or_absent_boundary_facts_never_prepare_or_confirm() {
    let (_temp, path) = ledger();
    let p = b(path.to_str().unwrap().as_bytes());
    for flags in [0, 62, 61, 59, 55, 47, 31, 127, u32::MAX] {
        let mut input = boundary();
        input.observed_flags = flags;
        unsafe {
            assert!(
                helm_external_adoption_prepare(p, b(REQUEST), &fixture(), &input, 100).is_null()
            );
            assert_eq!(
                helm_external_adoption_confirm(prepare(&path), p, &fixture(), &input, 110),
                REVIEW_CHANGED
            );
        }
    }
    for field in 0..8 {
        let mut input = boundary();
        match field {
            0 => input.abi_version = 2,
            1 => input.helper_identifier = b(b"org.example.Other"),
            2 => input.helper_team_identifier = b(b"OTHER12345"),
            3 => input.helper_code_directory_hash = b(&[0; 20]),
            4 => input.caller_identifier = b(b"com.jasoncavinder.Helm.Other"),
            5 => input.caller_team_identifier = b(b"OTHER12345"),
            6 => input.helper_identifier = b(b"com.jasoncavinder.Helm\0"),
            _ => {
                input.helper_code_directory_hash = Bytes {
                    data: std::ptr::null(),
                    length: 20,
                }
            }
        }
        unsafe {
            assert!(
                helm_external_adoption_prepare(p, b(REQUEST), &fixture(), &input, 100).is_null()
            );
            assert_eq!(
                helm_external_adoption_confirm(prepare(&path), p, &fixture(), &input, 110),
                REVIEW_CHANGED
            );
        }
    }
    assert_eq!(status(&path), adoption::ConsentStatus::NotRecorded);
}

#[test]
fn fresh_target_boundary_drift_or_expiry_consumes_review_without_grant() {
    let (_temp, path) = ledger();
    let p = b(path.to_str().unwrap().as_bytes());
    for change in 0..12 {
        let review = prepare(&path);
        let mut target = fixture();
        let mut bound = boundary();
        let mut now = 110;
        match change {
            0 => target.inode += 1,
            1 => target.build = b(b"101"),
            2 => target.canonical_path = b(b"/Applications/Other.app"),
            3 => target.manager_exclusions = 2,
            4 => target.manager_exclusions = 16,
            5 => target.writable_by_others = 1,
            6 => target.feed_url = b(b"https://other.example.org/feed"),
            7 => target.ed25519_public_key = b(&[9; 32]),
            8 => bound.helper_code_directory_hash = b(&[5; 20]),
            9 => now = 220,
            10 => now = 99,
            _ => target.user_applications_root = b(b"/tmp"),
        }
        // /Applications remains allowed even if the user root is irrelevant;
        // native account-root drift is additionally bound by the Swift owner.
        if change == 11 {
            target.canonical_path = b(b"/tmp/Example.app");
        }
        assert_eq!(
            unsafe { helm_external_adoption_confirm(review, p, &target, &bound, now) },
            REVIEW_CHANGED
        );
    }
    assert_eq!(status(&path), adoption::ConsentStatus::NotRecorded);
}

#[test]
fn wire_authority_unknown_fields_and_unbounded_inputs_are_rejected() {
    let (_temp, path) = ledger();
    let p = b(path.to_str().unwrap().as_bytes());
    let original = std::str::from_utf8(REQUEST).unwrap();
    for key in [
        "authority",
        "databasePath",
        "boundary",
        "confirmed",
        "epoch",
    ] {
        let extra = format!("{},\"{key}\":true}}", &original[..original.len() - 1]);
        assert!(
            unsafe {
                helm_external_adoption_prepare(p, b(extra.as_bytes()), &fixture(), &boundary(), 100)
            }
            .is_null()
        );
    }
    for request in [b"{}".as_slice(), b"\xff", &[b' '; 8193]] {
        assert!(
            unsafe { helm_external_adoption_prepare(p, b(request), &fixture(), &boundary(), 100) }
                .is_null()
        );
    }
    unsafe {
        assert!(
            helm_external_adoption_prepare(p, b(REQUEST), std::ptr::null(), &boundary(), 100)
                .is_null()
        );
        assert!(
            helm_external_adoption_prepare(p, b(REQUEST), &fixture(), std::ptr::null(), 100)
                .is_null()
        );
        assert_eq!(
            helm_external_adoption_confirm(prepare(&path), p, std::ptr::null(), &boundary(), 110),
            REVIEW_CHANGED
        );
        assert_eq!(
            helm_external_adoption_confirm(prepare(&path), p, &fixture(), std::ptr::null(), 110),
            REVIEW_CHANGED
        );
        assert_eq!(
            helm_external_adoption_confirm(std::ptr::null_mut(), p, &fixture(), &boundary(), 110),
            OUTCOME_UNKNOWN
        );
        helm_external_adoption_free(prepare(&path));
        helm_external_adoption_free(std::ptr::null_mut());
    }
    assert_eq!(status(&path), adoption::ConsentStatus::NotRecorded);
}

#[test]
fn missing_corrupt_or_wrong_database_never_becomes_a_grant() {
    let (temp, path) = ledger();
    let p = b(path.to_str().unwrap().as_bytes());
    let review = prepare(&path);
    let other = temp.path().join("other/ledger.sqlite");
    unsafe {
        assert!(
            helm_external_adoption_prepare(
                b(other.to_str().unwrap().as_bytes()),
                b(REQUEST),
                &fixture(),
                &boundary(),
                100
            )
            .is_null()
        );
        assert_eq!(
            helm_external_adoption_confirm(
                review,
                b(other.to_str().unwrap().as_bytes()),
                &fixture(),
                &boundary(),
                110
            ),
            REVIEW_CHANGED
        );
    }
    assert!(!other.exists());
    let review = prepare(&path);
    std::fs::write(&path, b"corrupt").unwrap();
    unsafe {
        assert!(
            helm_external_adoption_prepare(p, b(REQUEST), &fixture(), &boundary(), 100).is_null()
        );
        assert_eq!(
            helm_external_adoption_confirm(review, p, &fixture(), &boundary(), 110),
            OUTCOME_UNKNOWN
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
}

#[test]
fn revocation_after_native_review_prevents_grant() {
    let (_temp, path) = ledger();
    let review = prepare(&path);
    SqliteStore::new(&path)
        .revoke_external_update_adoption(Path::new("/Applications/Example.app"))
        .unwrap();
    assert_eq!(
        unsafe {
            helm_external_adoption_confirm(
                review,
                b(path.to_str().unwrap().as_bytes()),
                &fixture(),
                &boundary(),
                110,
            )
        },
        REVIEW_CHANGED
    );
    assert_eq!(status(&path), adoption::ConsentStatus::Revoked);
}
