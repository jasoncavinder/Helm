use super::*;
use crate::external_update::adoption::{self, AdoptionRequest, ReviewedAdoption};
use crate::external_update::durable::{DurableUpdateError as Error, DurableUpdateSession};
use crate::persistence::{MigrationStore, TaskStore};
use crate::sqlite::SqliteStore;

fn store() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(directory.path().join("adoption.db"));
    store.migrate_to_latest().unwrap();
    (directory, store)
}

fn native() -> TargetObservation {
    let mut target = fixture().1;
    target.authority = Authority::Unknown;
    target
}

fn request(id: usize) -> AdoptionRequest {
    AdoptionRequest {
        schema_version: 1,
        consent_id: format!("550e8400-e29b-41d4-a716-{id:012x}"),
        target_path: native().canonical_path,
        expected_bundle_identifier: native().bundle_identifier,
        expected_installed_build: native().build,
    }
}

fn review(store: &SqliteStore, id: usize) -> ReviewedAdoption {
    ReviewedAdoption::prepare(store, request(id), native(), fixture().3, &roots(), 100).unwrap()
}

fn adopt(store: &SqliteStore, id: usize) -> TargetObservation {
    review(store, id)
        .confirm(store, native(), fixture().3, &roots(), 110)
        .unwrap();
    adoption::resolve(store, native(), &roots()).unwrap()
}

fn update(target: TargetObservation) -> UpdateSession {
    let (request, _, candidate, boundary) = fixture();
    ReviewedUpdate::prepare(
        request,
        target.clone(),
        candidate.clone(),
        boundary.clone(),
        &roots(),
        100,
    )
    .unwrap()
    .confirm(target, candidate, boundary, &roots(), 110)
    .unwrap()
}

fn revoke(store: &SqliteStore) {
    store
        .revoke_external_update_adoption(&native().canonical_path)
        .unwrap();
}

#[test]
fn adoption_request_cannot_supply_authority_or_installation_arguments() {
    let valid = request(0);
    assert_eq!(
        AdoptionRequest::decode(&serde_json::to_vec(&valid).unwrap()).unwrap(),
        valid
    );
    for field in [
        "authority",
        "token",
        "feedURL",
        "command",
        "candidate",
        "roots",
        "signatureValid",
        "expectedCandidateBuild",
    ] {
        let mut value = serde_json::to_value(&valid).unwrap();
        value[field] = serde_json::json!(true);
        assert_eq!(
            AdoptionRequest::decode(&serde_json::to_vec(&value).unwrap()),
            Err(Rejection::MalformedRequest)
        );
    }
    assert!(AdoptionRequest::decode(&vec![b' '; MAX_REQUEST_BYTES + 1]).is_err());
    for change in 0..6 {
        let mut invalid = valid.clone();
        match change {
            0 => invalid.schema_version = 2,
            1 => invalid.consent_id = "not-consent".into(),
            2 => invalid.target_path = "/Applications/../Example.app".into(),
            3 => invalid.expected_bundle_identifier = "invalid".into(),
            4 => invalid.expected_installed_build = "\n100".into(),
            _ => invalid.target_path = "/Applications/Host.app/Nested.app".into(),
        }
        assert!(AdoptionRequest::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
}

#[test]
fn review_is_read_only_and_consent_is_not_standalone_provenance_or_update_approval() {
    let (_directory, store) = store();
    let review = review(&store, 0);
    assert!(
        store
            .external_update_adoption(&native().canonical_path)
            .unwrap()
            .is_none()
    );
    let unresolved = adoption::resolve(&store, native(), &roots()).unwrap();
    assert_eq!(unresolved.authority, Authority::Unknown);
    let (request, _, candidate, boundary) = fixture();
    assert!(
        ReviewedUpdate::prepare(
            request.clone(),
            unresolved,
            candidate.clone(),
            boundary.clone(),
            &roots(),
            100
        )
        .is_err()
    );
    let receipt = review
        .confirm(&store, native(), boundary.clone(), &roots(), 110)
        .unwrap();
    assert!(!receipt.is_revoked());
    store.delete_all_tasks().unwrap();
    let reopened = SqliteStore::new(store.database_path());
    assert_eq!(
        reopened
            .external_update_adoption(&native().canonical_path)
            .unwrap(),
        Some(receipt.clone())
    );
    let resolved = adoption::resolve(&reopened, native(), &roots()).unwrap();
    assert_eq!(resolved.authority, Authority::UserAdopted(receipt.token));
    assert!(store.external_update_receipt(OPERATION).unwrap().is_none());
    assert!(
        store
            .pending_external_updates(None, 100)
            .unwrap()
            .is_empty()
    );
    let mut unsupported = candidate;
    unsupported.sparkle_accepts_upgrade = false;
    assert_eq!(
        ReviewedUpdate::prepare(request, resolved, unsupported, boundary, &roots(), 100)
            .unwrap_err(),
        Rejection::UnsupportedCandidate
    );
}

#[test]
fn competing_claims_unsafe_targets_and_failed_boundary_cannot_be_adopted() {
    let (_directory, store) = store();
    for change in 0..19 {
        let mut target = native();
        let mut request = request(change);
        let mut boundary = fixture().3;
        match change {
            0 => target.authority = Authority::OtherManager,
            1 => target.has_store_receipt = true,
            2 => target.writable_by_others = true,
            3 => target.translocated = true,
            4 => target.signature_valid = false,
            5 => target.code_directory_hash.clear(),
            6 => target.framework_major = 1,
            7 => target.ed25519_public_key.clear(),
            8 => target.feed_url = "http://example.org/updates".into(),
            9 => target.team_identifier = "adhoc".into(),
            10 => target.authority = Authority::Standalone,
            11 => {
                target.bundle_identifier = HELM_IDENTIFIER.into();
                request.expected_bundle_identifier = target.bundle_identifier.clone();
            }
            12 => {
                target.canonical_path = "/System/Applications/Example.app".into();
                request.target_path = target.canonical_path.clone();
            }
            13 => boundary.authenticated_live_caller = false,
            14 => boundary.direct_consumer_channel = false,
            15 => boundary.helm_sandbox_preserved = false,
            16 => boundary.notarization_accepted = false,
            17 => boundary.helper_identifier = "org.impostor.App".into(),
            _ => target.build = "99".into(),
        }
        assert!(
            ReviewedAdoption::prepare(&store, request, target, boundary, &roots(), 100).is_err(),
            "{change}"
        );
    }
    assert!(
        store
            .external_update_adoption(&native().canonical_path)
            .unwrap()
            .is_none()
    );
}

#[test]
fn adoption_confirmation_reobserves_expires_and_binds_exact_review() {
    let (_directory, store) = store();
    for now in [99, 221, u64::MAX] {
        assert!(matches!(
            review(&store, 0).confirm(&store, native(), fixture().3, &roots(), now),
            Err(Error::Policy(Rejection::ReviewExpired))
        ));
    }
    for change in 0..11 {
        let mut target = native();
        let mut boundary = fixture().3;
        match change {
            0 => target.inode += 1,
            1 => target.device += 1,
            2 => target.build = "101".into(),
            3 => target.code_directory_hash[0] += 1,
            4 => target.ed25519_public_key[0] += 1,
            5 => target.feed_url = "https://other.example/feed".into(),
            6 => target.team_identifier = "OTHER12345".into(),
            7 => target.authority = Authority::OtherManager,
            8 => target.writable_by_others = true,
            9 => boundary.helper_code_directory_hash[0] += 1,
            _ => boundary.authenticated_live_caller = false,
        }
        assert!(
            review(&store, change)
                .confirm(&store, target, boundary, &roots(), 110)
                .is_err(),
            "{change}"
        );
    }
    assert!(
        store
            .external_update_adoption(&native().canonical_path)
            .unwrap()
            .is_none()
    );
    assert!(
        review(&store, 99)
            .confirm(&store, native(), fixture().3, &roots(), 220)
            .is_ok()
    );
}

#[test]
fn persistent_adoption_is_bound_to_vendor_path_and_update_configuration_not_version() {
    let (_directory, store) = store();
    let target = adopt(&store, 0);
    for change in 0..5 {
        let mut changed = native();
        match change {
            0 => changed.canonical_path = "/Applications/Other.app".into(),
            1 => changed.bundle_identifier = "org.other.App".into(),
            2 => changed.team_identifier = "OTHER12345".into(),
            3 => changed.ed25519_public_key[0] += 1,
            _ => changed.feed_url = "https://other.example/feed".into(),
        }
        assert_eq!(
            adoption::resolve(&store, changed, &roots())
                .unwrap()
                .authority,
            Authority::Unknown
        );
    }
    for change in 0..4 {
        let mut blocked = native();
        match change {
            0 => blocked.authority = Authority::OtherManager,
            1 => blocked.has_store_receipt = true,
            2 => blocked.writable_by_others = true,
            _ => blocked.signature_valid = false,
        }
        assert!(adoption::resolve(&store, blocked, &roots()).is_err());
    }
    assert!(
        adoption::resolve(&store, target.clone(), &roots()).is_err(),
        "a cached resolved observation is not fresh native evidence"
    );
    let mut replaced = native();
    replaced.build = "101".into();
    replaced.inode += 1;
    replaced.code_directory_hash[0] += 1;
    assert_eq!(
        adoption::resolve(&store, replaced, &roots())
            .unwrap()
            .authority,
        target.authority
    );
}

#[test]
fn revocation_fences_pending_reviews_even_before_the_first_adoption() {
    let (_directory, store) = store();
    let pending = review(&store, 0);
    let other = SqliteStore::new(store.database_path());
    revoke(&other);
    assert!(matches!(
        pending.confirm(&store, native(), fixture().3, &roots(), 110),
        Err(Error::Conflict)
    ));
    let target = adopt(&store, 1);
    let pending = review(&store, 2);
    revoke(&other);
    assert!(matches!(
        pending.confirm(&store, native(), fixture().3, &roots(), 110),
        Err(Error::Conflict)
    ));
    assert_eq!(
        adoption::resolve(&store, native(), &roots())
            .unwrap()
            .authority,
        Authority::Unknown
    );
    let readopted = adopt(&store, 3);
    assert_ne!(readopted.authority, target.authority);
}

#[test]
fn previously_consumed_consent_id_is_not_reusable_after_revocation() {
    let (_directory, store) = store();
    adopt(&store, 0);
    revoke(&store);
    assert!(matches!(
        review(&store, 0).confirm(&store, native(), fixture().3, &roots(), 110),
        Err(Error::Conflict)
    ));
    assert!(
        store
            .external_update_adoption(&native().canonical_path)
            .unwrap()
            .unwrap()
            .is_revoked()
    );
}

#[test]
fn concurrent_adoption_reviews_cannot_overwrite_each_other() {
    let (_directory, store) = store();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|id| {
            let path = store.database_path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = SqliteStore::new(path);
                let pending = review(&store, id);
                barrier.wait();
                pending
                    .confirm(&store, native(), fixture().3, &roots(), 110)
                    .is_ok()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .filter_map(|thread| thread.join().unwrap().then_some(()))
            .count(),
        1
    );
}

#[test]
fn readoption_changes_the_individual_update_review_fingerprint() {
    let (_directory, store) = store();
    let target = adopt(&store, 0);
    let (request, _, candidate, boundary) = fixture();
    let pending = ReviewedUpdate::prepare(
        request,
        target,
        candidate.clone(),
        boundary.clone(),
        &roots(),
        100,
    )
    .unwrap();
    revoke(&store);
    let fresh = adopt(&store, 1);
    assert_eq!(
        pending
            .confirm(fresh, candidate, boundary, &roots(), 110)
            .unwrap_err(),
        Rejection::ReviewChanged
    );
}

#[test]
fn durable_claim_does_not_accept_a_token_transferred_to_another_identity() {
    let (_directory, store) = store();
    let target = adopt(&store, 0);
    for change in 0..3 {
        let mut substituted = target.clone();
        let (mut request, _, mut candidate, boundary) = fixture();
        match change {
            0 => {
                substituted.canonical_path = "/Applications/Other.app".into();
                request.target_path = substituted.canonical_path.clone();
            }
            1 => substituted.ed25519_public_key[0] += 1,
            _ => {
                substituted.feed_url = "https://other.example/feed".into();
                candidate.feed_url = substituted.feed_url.clone();
            }
        }
        let pending = ReviewedUpdate::prepare(
            request,
            substituted.clone(),
            candidate.clone(),
            boundary.clone(),
            &roots(),
            100,
        )
        .unwrap();
        let session = pending
            .confirm(substituted, candidate, boundary, &roots(), 110)
            .unwrap();
        assert!(matches!(
            DurableUpdateSession::claim(&store, session, 110),
            Err(Error::Conflict)
        ));
    }
}

#[test]
fn repeat_revocation_fences_a_new_pending_review_and_rejects_invalid_paths() {
    let (_directory, store) = store();
    revoke(&store);
    let pending = review(&store, 0);
    revoke(&store);
    assert!(matches!(
        pending.confirm(&store, native(), fixture().3, &roots(), 110),
        Err(Error::Conflict)
    ));
    for path in [
        "relative.app",
        "/Applications/../Example.app",
        "/Applications/Host.app/Nested.app",
    ] {
        assert!(
            store
                .revoke_external_update_adoption(Path::new(path))
                .is_err()
        );
    }
}

#[test]
fn safe_mode_blocks_new_consent_but_not_revocation() {
    let (_directory, store) = store();
    adopt(&store, 0);
    let pending = review(&store, 1);
    let connection = rusqlite::Connection::open(store.database_path()).unwrap();
    connection
        .execute(
            "INSERT OR REPLACE INTO app_settings(key, value) VALUES('safe_mode', '1')",
            [],
        )
        .unwrap();
    assert!(matches!(
        pending.confirm(&store, native(), fixture().3, &roots(), 110),
        Err(Error::Conflict)
    ));
    revoke(&store);
    assert_eq!(
        adoption::resolve(&store, native(), &roots())
            .unwrap()
            .authority,
        Authority::Unknown
    );
}

#[test]
fn revocation_after_confirmation_blocks_durable_update_claim() {
    let (_directory, store) = store();
    let target = adopt(&store, 0);
    let session = update(target);
    revoke(&SqliteStore::new(store.database_path()));
    assert!(matches!(
        DurableUpdateSession::claim(&store, session, 110),
        Err(Error::Conflict)
    ));
    assert!(store.external_update_receipt(OPERATION).unwrap().is_none());
}

#[test]
fn download_revocation_rejects_cached_authority_at_atomic_install_handoff() {
    let (_directory, store) = store();
    let target = adopt(&store, 0);
    let mut active = DurableUpdateSession::claim(&store, update(target.clone()), 110).unwrap();
    active.event(OPERATION, UpdateEvent::Downloaded).unwrap();
    revoke(&SqliteStore::new(store.database_path()));
    assert!(matches!(
        active.begin_install(target, fixture().2, fixture().3, &roots()),
        Err(Error::Conflict)
    ));
    assert_eq!(
        store
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::ReadyToInstall
    );
    assert!(matches!(
        active.event(OPERATION, UpdateEvent::Cancelled),
        Err(Error::RecoveryRequired)
    ));
    assert!(
        matches!(
            review(&store, 1).confirm(&store, native(), fixture().3, &roots(), 110),
            Err(Error::Conflict)
        ),
        "active target reservation cannot be overwritten by adoption"
    );
}

#[test]
fn version_verification_rechecks_current_consent_even_with_a_cached_token() {
    let (_directory, store) = store();
    let target = adopt(&store, 0);
    let mut active = DurableUpdateSession::claim(&store, update(target.clone()), 110).unwrap();
    active.event(OPERATION, UpdateEvent::Downloaded).unwrap();
    active
        .begin_install(target.clone(), fixture().2, fixture().3, &roots())
        .unwrap();
    active
        .event(OPERATION, UpdateEvent::InstallerFinished)
        .unwrap();
    revoke(&SqliteStore::new(store.database_path()));
    let mut stale = target;
    stale.build = "101".into();
    stale.inode += 1;
    assert!(matches!(
        active.verify(OPERATION, &stale),
        Err(Error::Conflict)
    ));
    assert_eq!(
        store
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::AwaitingVerification
    );
    assert_eq!(store.pending_external_updates(None, 100).unwrap().len(), 1);
}

#[test]
fn adopted_update_still_requires_candidate_review_handoff_and_observed_replacement() {
    let (_directory, store) = store();
    let target = adopt(&store, 0);
    let mut active = DurableUpdateSession::claim(&store, update(target.clone()), 110).unwrap();
    active.event(OPERATION, UpdateEvent::Downloaded).unwrap();
    active
        .begin_install(target, fixture().2, fixture().3, &roots())
        .unwrap();
    active
        .event(OPERATION, UpdateEvent::InstallerFinished)
        .unwrap();
    let mut installed = native();
    installed.build = "101".into();
    installed.inode += 1;
    installed.code_directory_hash[0] += 1;
    let installed = adoption::resolve(&store, installed, &roots()).unwrap();
    active.verify(OPERATION, &installed).unwrap();
    assert_eq!(active.receipt().state, UpdateState::VersionVerified);
    assert!(
        store
            .pending_external_updates(None, 100)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn reset_and_downgrade_do_not_resurrect_reviews_or_old_authority() {
    let (_directory, store) = store();
    let before_first_grant = review(&store, 0);
    store.apply_migration(23).unwrap();
    store.migrate_to_latest().unwrap();
    assert!(matches!(
        before_first_grant.confirm(&store, native(), fixture().3, &roots(), 110),
        Err(Error::Conflict)
    ));
    let old = adopt(&store, 1);
    let pending = review(&store, 2);
    store.apply_migration(0).unwrap();
    store.migrate_to_latest().unwrap();
    assert!(
        store
            .external_update_adoption(&native().canonical_path)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        pending.confirm(&store, native(), fixture().3, &roots(), 110),
        Err(Error::Conflict)
    ));
    let new = adopt(&store, 1);
    assert_ne!(old.authority, new.authority);
    assert!(matches!(
        DurableUpdateSession::claim(&store, update(old), 110),
        Err(Error::Conflict)
    ));
}

#[test]
fn a_review_from_another_database_cannot_grant_consent() {
    let (_one, first) = store();
    let (_two, second) = store();
    assert!(matches!(
        review(&first, 0).confirm(&second, native(), fixture().3, &roots(), 110),
        Err(Error::Conflict)
    ));
    assert!(
        second
            .external_update_adoption(&native().canonical_path)
            .unwrap()
            .is_none()
    );
}

#[test]
fn adoption_write_failure_returns_no_permission_and_preserves_the_ledger() {
    let (_directory, store) = store();
    let connection = rusqlite::Connection::open(store.database_path()).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER reject_adoption BEFORE INSERT ON external_update_adoptions
        BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
        )
        .unwrap();
    assert!(matches!(
        review(&store, 0).confirm(&store, native(), fixture().3, &roots(), 110),
        Err(Error::Storage(_))
    ));
    assert!(
        store
            .external_update_adoption(&native().canonical_path)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        adoption::resolve(&store, native(), &roots())
            .unwrap()
            .authority,
        Authority::Unknown
    );
}

#[test]
fn migration24_preserves_existing_sessions_and_failed_downgrade_preserves_adoption() {
    let (_directory, store) = store();
    store.apply_migration(23).unwrap();
    let connection = rusqlite::Connection::open(store.database_path()).unwrap();
    connection.execute("INSERT INTO external_update_sessions VALUES (?1, 'fingerprint', '/Applications/Other.app', '1', '99', 'org.other.App', '1', '2', 'unverified', 0, 1)", [OPERATION]).unwrap();
    store.migrate_to_latest().unwrap();
    adopt(&store, 0);
    let before = store
        .external_update_adoption(&native().canonical_path)
        .unwrap();
    store.migrate_to_latest().unwrap();
    assert!(store.apply_migration(0).is_err());
    assert_eq!(
        store
            .external_update_adoption(&native().canonical_path)
            .unwrap(),
        before
    );
    assert_eq!(
        store
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::Unverified
    );
}
