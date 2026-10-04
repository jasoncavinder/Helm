use super::*;
use crate::external_update::adoption::{
    AdoptionReceipt, AdoptionRequest, ConsentStatus, ReviewedLedgerAdoption, resolve,
};
use crate::external_update::durable::DurableUpdateError as Error;
use crate::external_update::revocation::{ReviewedRevocation, RevocationRequest};
use crate::persistence::MigrationStore;
use crate::sqlite::SqliteStore;

fn ledger() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(directory.path().join("ledger.sqlite"));
    store.migrate_to_latest().unwrap();
    (directory, store)
}

fn target() -> TargetObservation {
    let mut target = fixture().1;
    target.authority = Authority::Unknown;
    target
}

fn request(id: usize) -> AdoptionRequest {
    AdoptionRequest {
        schema_version: 1,
        consent_id: format!("550e8400-e29b-41d4-a716-{id:012x}"),
        target_path: target().canonical_path,
        expected_bundle_identifier: target().bundle_identifier,
        expected_installed_build: target().build,
    }
}

fn prepare(path: &Path, id: usize) -> Result<ReviewedLedgerAdoption, Error> {
    ReviewedLedgerAdoption::prepare(path, request(id), target(), fixture().3, &roots(), 100)
}

fn confirm(review: ReviewedLedgerAdoption, path: &Path) -> Result<AdoptionReceipt, Error> {
    review.confirm(path, target(), fixture().3, &roots(), 110)
}

fn count(path: &Path, table: &str) -> i64 {
    let sql = match table {
        "grants" => "SELECT COUNT(*) FROM external_update_adoptions WHERE consent_id IS NOT NULL",
        "sessions" => "SELECT COUNT(*) FROM external_update_sessions",
        _ => panic!("unsupported test table"),
    };
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

#[test]
fn private_review_reads_without_granting_and_confirm_records_only_adoption() {
    let (_directory, store) = ledger();
    let path = store.database_path();
    let before = std::fs::read(path).unwrap();
    let pending = prepare(path, 0).unwrap();
    assert_eq!(std::fs::read(path).unwrap(), before);
    assert_eq!(count(path, "grants"), 0);
    assert_eq!(
        SqliteStore::inspect_external_update_consent(path, &target(), &roots()).unwrap(),
        ConsentStatus::NotRecorded
    );
    let receipt = confirm(pending, path).unwrap();
    assert!(!receipt.is_revoked());
    assert_eq!(
        receipt.consent_id.as_deref(),
        Some(request(0).consent_id.as_str())
    );
    assert_eq!(count(path, "grants"), 1);
    assert_eq!(count(path, "sessions"), 0);
    assert_eq!(
        SqliteStore::inspect_external_update_consent(path, &target(), &roots()).unwrap(),
        ConsentStatus::Recorded
    );
    assert_eq!(
        resolve(&store, target(), &roots()).unwrap().authority,
        Authority::UserAdopted(receipt.token)
    );
}

#[test]
fn private_review_requires_valid_intent_native_target_and_authenticated_boundary() {
    let (_directory, store) = ledger();
    for change in 0..13 {
        let mut native = target();
        let mut boundary = fixture().3;
        let mut intent = request(change);
        match change {
            0 => native.authority = Authority::OtherManager,
            1 => native.has_store_receipt = true,
            2 => native.authority = Authority::Standalone,
            3 => native.writable_by_others = true,
            4 => native.signature_valid = false,
            5 => native.translocated = true,
            6 => native.feed_url = "http://example.org/feed".into(),
            7 => native.build = "different".into(),
            8 => boundary.authenticated_live_caller = false,
            9 => boundary.helm_sandbox_preserved = false,
            10 => boundary.external_helper_unsandboxed = false,
            11 => boundary.notarization_accepted = false,
            _ => intent.schema_version = 99,
        }
        assert!(
            ReviewedLedgerAdoption::prepare(
                store.database_path(),
                intent,
                native,
                boundary,
                &roots(),
                100
            )
            .is_err(),
            "{change}"
        );
    }
    assert!(prepare(Path::new("ledger.sqlite"), 0).is_err());
    assert_eq!(count(store.database_path(), "grants"), 0);
}

#[test]
fn private_confirmation_rechecks_every_identity_and_boundary() {
    let (_directory, store) = ledger();
    for change in 0..12 {
        let pending = prepare(store.database_path(), change).unwrap();
        let mut native = target();
        let mut boundary = fixture().3;
        match change {
            0 => native.inode += 1,
            1 => native.device += 1,
            2 => native.build = "101".into(),
            3 => native.code_directory_hash[0] += 1,
            4 => native.team_identifier = "OTHER12345".into(),
            5 => native.feed_url = "https://other.example.org/feed".into(),
            6 => native.ed25519_public_key[0] += 1,
            7 => native.authority = Authority::OtherManager,
            8 => native.writable_by_others = true,
            9 => boundary.helper_code_directory_hash[0] += 1,
            10 => boundary.authenticated_live_caller = false,
            _ => native.canonical_path = "/Applications/Other.app".into(),
        }
        assert!(
            pending
                .confirm(store.database_path(), native, boundary, &roots(), 110)
                .is_err(),
            "{change}"
        );
    }
    assert_eq!(count(store.database_path(), "grants"), 0);
}

#[test]
fn private_confirmation_binds_path_epoch_and_exclusive_deadline() {
    let (_directory, store) = ledger();
    for now in [99, 220, u64::MAX] {
        assert!(matches!(
            prepare(store.database_path(), 0).unwrap().confirm(
                store.database_path(),
                target(),
                fixture().3,
                &roots(),
                now
            ),
            Err(Error::Policy(Rejection::ReviewExpired))
        ));
    }
    let (_other, other) = ledger();
    assert!(matches!(
        confirm(
            prepare(store.database_path(), 0).unwrap(),
            other.database_path()
        ),
        Err(Error::Policy(Rejection::ReviewChanged))
    ));
    let stale = prepare(store.database_path(), 0).unwrap();
    store.apply_migration(23).unwrap();
    store.migrate_to_latest().unwrap();
    assert!(matches!(
        confirm(stale, store.database_path()),
        Err(Error::Conflict)
    ));
    assert!(
        prepare(store.database_path(), 1)
            .unwrap()
            .confirm(store.database_path(), target(), fixture().3, &roots(), 219)
            .is_ok()
    );
    assert_eq!(count(store.database_path(), "grants"), 1);
    assert_eq!(count(other.database_path(), "grants"), 0);
}

#[test]
fn private_review_and_confirmation_never_recreate_missing_storage() {
    let directory = tempfile::tempdir().unwrap();
    let absent = directory.path().join("absent.sqlite");
    assert!(prepare(&absent, 0).is_err());
    assert!(!absent.exists());
    let (_directory, store) = ledger();
    let pending = prepare(store.database_path(), 0).unwrap();
    std::fs::remove_file(store.database_path()).unwrap();
    assert!(matches!(
        confirm(pending, store.database_path()),
        Err(Error::Storage(_))
    ));
    assert!(!store.database_path().exists());
}

#[test]
fn private_review_and_confirmation_reject_incomplete_ledger_without_repair() {
    for sql in [
        "DELETE FROM external_update_adoption_epoch",
        "DROP TABLE external_update_sessions",
        "UPDATE helm_schema_migrations SET definition_checksum = 'tampered' WHERE version = 24",
        "UPDATE helm_schema_migrations SET name = 'wrong' WHERE version = 24",
    ] {
        let (_directory, store) = ledger();
        let pending = prepare(store.database_path(), 0).unwrap();
        rusqlite::Connection::open(store.database_path())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
        let before = std::fs::read(store.database_path()).unwrap();
        assert!(prepare(store.database_path(), 1).is_err(), "{sql}");
        assert!(
            matches!(
                confirm(pending, store.database_path()),
                Err(Error::Storage(_))
            ),
            "{sql}"
        );
        assert_eq!(
            std::fs::read(store.database_path()).unwrap(),
            before,
            "{sql}"
        );
    }
    let (_directory, store) = ledger();
    let pending = prepare(store.database_path(), 0).unwrap();
    store.apply_migration(23).unwrap();
    let before = std::fs::read(store.database_path()).unwrap();
    assert!(prepare(store.database_path(), 1).is_err());
    assert!(matches!(
        confirm(pending, store.database_path()),
        Err(Error::Storage(_))
    ));
    assert_eq!(std::fs::read(store.database_path()).unwrap(), before);
}

#[test]
fn private_review_and_confirmation_preserve_empty_or_corrupt_storage() {
    for bytes in [Vec::new(), b"not a database".to_vec()] {
        let (_directory, store) = ledger();
        let pending = prepare(store.database_path(), 0).unwrap();
        std::fs::write(store.database_path(), &bytes).unwrap();
        assert!(prepare(store.database_path(), 1).is_err());
        assert!(confirm(pending, store.database_path()).is_err());
        assert_eq!(std::fs::read(store.database_path()).unwrap(), bytes);
    }
}

#[test]
#[cfg(unix)]
fn private_review_and_confirmation_refuse_final_component_symlinks() {
    let (directory, store) = ledger();
    let pending = prepare(store.database_path(), 0).unwrap();
    let original = directory.path().join("saved.sqlite");
    std::fs::rename(store.database_path(), &original).unwrap();
    std::os::unix::fs::symlink(&original, store.database_path()).unwrap();
    let before = std::fs::read(&original).unwrap();
    assert!(prepare(store.database_path(), 1).is_err());
    assert!(confirm(pending, store.database_path()).is_err());
    assert_eq!(std::fs::read(&original).unwrap(), before);
}

#[test]
fn private_confirmation_rechecks_safe_mode_and_existing_install_reservations() {
    for sql in [
        "INSERT INTO app_settings(key,value) VALUES('safe_mode','1')",
        "INSERT INTO app_settings(key,value) VALUES('safe_mode','unknown')",
        "INSERT INTO external_update_sessions VALUES('reserved', 'f', '/Applications/Example.app', '99', '99', 'org.example.App', '100', '101', 'unverified', 0, 1)",
        "INSERT INTO external_update_sessions VALUES('reserved', 'f', '/Applications/Alias.app', '1', '2', 'org.example.App', '100', '101', 'unverified', 0, 1)",
    ] {
        let (_directory, store) = ledger();
        let pending = prepare(store.database_path(), 0).unwrap();
        rusqlite::Connection::open(store.database_path())
            .unwrap()
            .execute_batch(sql)
            .unwrap();
        let before = std::fs::read(store.database_path()).unwrap();
        assert!(
            matches!(
                confirm(pending, store.database_path()),
                Err(Error::Conflict)
            ),
            "{sql}"
        );
        assert_eq!(count(store.database_path(), "grants"), 0);
        assert_eq!(std::fs::read(store.database_path()).unwrap(), before);
    }
}

#[test]
fn private_confirmation_sees_wal_revocation_and_cannot_reuse_consumed_consent() {
    let (_directory, store) = ledger();
    let path = store.database_path();
    let keeper = rusqlite::Connection::open(path).unwrap();
    keeper
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .unwrap();
    let pending = prepare(path, 0).unwrap();
    store
        .revoke_external_update_adoption(&target().canonical_path)
        .unwrap();
    assert!(
        std::fs::metadata(path.with_extension("sqlite-wal"))
            .unwrap()
            .len()
            > 0
    );
    assert!(matches!(confirm(pending, path), Err(Error::Conflict)));
    let current = confirm(prepare(path, 0).unwrap(), path).unwrap();
    assert_eq!(current.token.sequence, 2);
    let stale = prepare(path, 1).unwrap();
    ReviewedRevocation::prepare(
        path,
        RevocationRequest {
            schema_version: 1,
            request_id: OPERATION.into(),
            target_path: target().canonical_path,
        },
        &roots(),
        100,
    )
    .unwrap()
    .confirm(path, 110)
    .unwrap();
    assert!(matches!(confirm(stale, path), Err(Error::Conflict)));
    assert!(matches!(
        confirm(prepare(path, 0).unwrap(), path),
        Err(Error::Conflict)
    ));
    assert_eq!(count(path, "grants"), 1);
    assert_eq!(
        SqliteStore::inspect_external_update_consent(path, &target(), &roots()).unwrap(),
        ConsentStatus::Revoked
    );
}

#[test]
fn private_concurrent_confirmations_commit_at_most_one_grant() {
    let (_directory, store) = ledger();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|id| {
            let pending = prepare(store.database_path(), id).unwrap();
            let path = store.database_path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                confirm(pending, &path).is_ok()
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
    assert_eq!(count(store.database_path(), "grants"), 1);
    assert_eq!(count(store.database_path(), "sessions"), 0);
}

#[test]
fn private_confirmation_rolls_back_failed_writes_and_bounded_lock_contention() {
    let (_directory, store) = ledger();
    let pending = prepare(store.database_path(), 0).unwrap();
    let blocker = rusqlite::Connection::open(store.database_path()).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(matches!(
        confirm(pending, store.database_path()),
        Err(Error::Storage(_))
    ));
    blocker.execute_batch("ROLLBACK").unwrap();
    assert_eq!(count(store.database_path(), "grants"), 0);
    blocker.execute_batch("CREATE TRIGGER reject_grant BEFORE INSERT ON external_update_adoptions BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    assert!(matches!(
        confirm(
            prepare(store.database_path(), 1).unwrap(),
            store.database_path()
        ),
        Err(Error::Storage(_))
    ));
    assert_eq!(count(store.database_path(), "grants"), 0);
}
