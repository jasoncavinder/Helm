use super::*;
use crate::external_update::durable::{DurableUpdateError, DurableUpdateSession};
use crate::persistence::{MigrationStore, TaskStore};
use crate::sqlite::SqliteStore;

fn store() -> (tempfile::TempDir, SqliteStore) {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(directory.path().join("ledger.db"));
    store.migrate_to_latest().unwrap();
    (directory, store)
}

fn custom_session(id: usize, path: &str, inode: u64) -> UpdateSession {
    let (mut request, mut target, candidate, boundary) = fixture();
    request.operation_id = format!("550e8400-e29b-41d4-a716-{id:012x}");
    request.target_path = path.into();
    target.canonical_path = path.into();
    target.inode = inode;
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

fn downloaded(store: &SqliteStore) -> DurableUpdateSession<'_> {
    let mut session = DurableUpdateSession::claim(store, session(), 110).unwrap();
    session.event(OPERATION, UpdateEvent::Downloaded).unwrap();
    session
}

fn installing(store: &SqliteStore) -> DurableUpdateSession<'_> {
    let mut session = downloaded(store);
    let (_, target, candidate, boundary) = fixture();
    let permit = session
        .begin_install(target, candidate, boundary, &roots())
        .unwrap();
    assert_eq!(permit.operation_id(), OPERATION);
    assert_eq!(permit.fingerprint(), session.receipt().fingerprint);
    assert_eq!(
        store
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::Installing
    );
    session
}

#[test]
fn claim_is_durable_single_use_and_survives_task_history_deletion() {
    let (_directory, store) = store();
    let mut active = DurableUpdateSession::claim(&store, session(), 110).unwrap();
    assert_eq!(
        store.external_update_receipt(OPERATION).unwrap().as_ref(),
        Some(active.receipt())
    );
    active.event(OPERATION, UpdateEvent::Cancelled).unwrap();
    store.delete_all_tasks().unwrap();
    let reopened = SqliteStore::new(store.database_path());
    assert!(matches!(
        DurableUpdateSession::claim(&reopened, session(), 110),
        Err(DurableUpdateError::Conflict)
    ));
    assert_eq!(
        reopened
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::CancelledBeforeInstall
    );
    assert!(
        DurableUpdateSession::claim(
            &reopened,
            custom_session(1, "/Applications/Example.app", 2),
            110
        )
        .is_ok()
    );
}

#[test]
fn active_path_and_native_identity_are_both_exclusive() {
    let (_directory, store) = store();
    let _first = DurableUpdateSession::claim(&store, session(), 110).unwrap();
    assert!(matches!(
        DurableUpdateSession::claim(
            &store,
            custom_session(1, "/Applications/Example.app", 99),
            110
        ),
        Err(DurableUpdateError::Conflict)
    ));
    assert!(matches!(
        DurableUpdateSession::claim(
            &store,
            custom_session(2, "/Applications/Renamed.app", 2),
            110
        ),
        Err(DurableUpdateError::Conflict)
    ));
    assert!(
        DurableUpdateSession::claim(&store, custom_session(3, "/Applications/Other.app", 3), 110)
            .is_ok()
    );
}

#[test]
fn concurrent_store_handles_cannot_claim_the_same_target() {
    let (_directory, store) = store();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
    let threads: Vec<_> = (0..12)
        .map(|id| {
            let path = store.database_path().to_owned();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let store = SqliteStore::new(path);
                let session = custom_session(id, "/Applications/Example.app", 2);
                barrier.wait();
                DurableUpdateSession::claim(&store, session, 110).is_ok()
            })
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|won| *won)
            .count(),
        1
    );
    assert_eq!(store.pending_external_updates(None, 100).unwrap().len(), 1);
}

#[test]
fn expiration_is_rechecked_when_authorization_is_persisted() {
    let (_directory, store) = store();
    for now in [99, 221, u64::MAX] {
        assert!(matches!(
            DurableUpdateSession::claim(&store, session(), now),
            Err(DurableUpdateError::Policy(Rejection::ReviewExpired))
        ));
    }
    assert!(store.external_update_receipt(OPERATION).unwrap().is_none());
}

#[test]
fn safe_mode_blocks_claim_and_is_rechecked_before_handoff() {
    let (_directory, store) = store();
    let connection = rusqlite::Connection::open(store.database_path()).unwrap();
    connection
        .execute(
            "INSERT OR REPLACE INTO app_settings(key, value) VALUES('safe_mode', '1')",
            [],
        )
        .unwrap();
    assert!(DurableUpdateSession::claim(&store, session(), 110).is_err());
    assert!(store.external_update_receipt(OPERATION).unwrap().is_none());
    connection
        .execute(
            "UPDATE app_settings SET value = '0' WHERE key = 'safe_mode'",
            [],
        )
        .unwrap();
    let mut active = downloaded(&store);
    connection
        .execute(
            "UPDATE app_settings SET value = '1' WHERE key = 'safe_mode'",
            [],
        )
        .unwrap();
    let (_, target, candidate, boundary) = fixture();
    assert!(matches!(
        active.begin_install(target, candidate, boundary, &roots()),
        Err(DurableUpdateError::Conflict)
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
        Err(DurableUpdateError::RecoveryRequired)
    ));
}

#[test]
fn handoff_requires_fresh_identity_and_cannot_use_generic_event() {
    let (_directory, store) = store();
    let mut active = downloaded(&store);
    assert!(
        active
            .event(OPERATION, UpdateEvent::InstallationWillBegin)
            .is_err()
    );
    let (_, mut target, candidate, boundary) = fixture();
    target.inode += 1;
    assert!(matches!(
        active.begin_install(target, candidate, boundary, &roots()),
        Err(DurableUpdateError::Policy(Rejection::ReviewChanged))
    ));
    assert_eq!(
        store
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::ReadyToInstall
    );
}

#[test]
fn failed_state_commit_never_issues_handoff_and_poisons_session() {
    let (_directory, store) = store();
    let mut active = downloaded(&store);
    let connection = rusqlite::Connection::open(store.database_path()).unwrap();
    connection.execute_batch("CREATE TRIGGER reject_handoff BEFORE UPDATE ON external_update_sessions BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
    let (_, target, candidate, boundary) = fixture();
    assert!(matches!(
        active.begin_install(target, candidate, boundary, &roots()),
        Err(DurableUpdateError::Storage(_))
    ));
    connection
        .execute_batch("DROP TRIGGER reject_handoff;")
        .unwrap();
    assert!(matches!(
        active.event(OPERATION, UpdateEvent::Cancelled),
        Err(DurableUpdateError::RecoveryRequired)
    ));
    assert_eq!(
        store
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::ReadyToInstall
    );
}

#[test]
fn restart_quarantine_fences_old_callbacks_and_retains_target_lock() {
    let (_directory, store) = store();
    let mut active = installing(&store);
    let revision = active.receipt().revision;
    let reopened = SqliteStore::new(store.database_path());
    assert!(
        !reopened
            .quarantine_external_update(OPERATION, revision - 1)
            .unwrap()
    );
    assert!(
        reopened
            .quarantine_external_update(OPERATION, revision)
            .unwrap()
    );
    assert!(matches!(
        active.event(OPERATION, UpdateEvent::InstallerFinished),
        Err(DurableUpdateError::Conflict)
    ));
    assert_eq!(
        reopened
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::Unverified
    );
    assert!(
        DurableUpdateSession::claim(
            &reopened,
            custom_session(2, "/Applications/Example.app", 2),
            110
        )
        .is_err()
    );
    assert!(
        !reopened
            .quarantine_external_update(OPERATION, revision + 1)
            .unwrap()
    );
}

#[test]
fn lost_handoff_is_not_success_or_safe_cancellation() {
    let (_directory, store) = store();
    let mut active = installing(&store);
    active
        .event(OPERATION, UpdateEvent::ConnectionLost)
        .unwrap();
    let (_, mut observed, ..) = fixture();
    observed.build = "101".into();
    assert!(matches!(
        active.verify(OPERATION, &observed),
        Err(DurableUpdateError::RecoveryRequired)
    ));
    assert_eq!(
        store
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::Unverified
    );
}

#[test]
fn verified_version_requires_finished_callback_and_fresh_native_observation() {
    let (_directory, store) = store();
    let mut active = installing(&store);
    let (_, mut observed, ..) = fixture();
    observed.build = "101".into();
    observed.inode = 77;
    assert!(active.verify(OPERATION, &observed).is_err());
    active
        .event(OPERATION, UpdateEvent::InstallerFinished)
        .unwrap();
    assert_eq!(active.receipt().state, UpdateState::AwaitingVerification);
    active.verify(OPERATION, &observed).unwrap();
    assert_eq!(
        store
            .external_update_receipt(OPERATION)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::VersionVerified
    );
    assert!(
        !store
            .quarantine_external_update(OPERATION, active.receipt().revision)
            .unwrap()
    );
    assert!(
        DurableUpdateSession::claim(
            &store,
            custom_session(2, "/Applications/Example.app", 77),
            110
        )
        .is_ok()
    );
}

#[test]
fn mismatched_observation_keeps_unverified_target_reserved() {
    let (_directory, store) = store();
    let mut active = installing(&store);
    active
        .event(OPERATION, UpdateEvent::InstallerFinished)
        .unwrap();
    let (_, observed, ..) = fixture();
    active.verify(OPERATION, &observed).unwrap();
    assert_eq!(active.receipt().state, UpdateState::Unverified);
    assert!(
        DurableUpdateSession::claim(
            &store,
            custom_session(2, "/Applications/Example.app", 2),
            110
        )
        .is_err()
    );
}

#[test]
fn wrong_operation_and_duplicate_callbacks_do_not_advance_receipt() {
    let (_directory, store) = store();
    let mut active = downloaded(&store);
    let before = active.receipt().clone();
    assert!(active.event("wrong", UpdateEvent::Cancelled).is_err());
    assert!(active.event(OPERATION, UpdateEvent::Downloaded).is_err());
    assert_eq!(
        store.external_update_receipt(OPERATION).unwrap().as_ref(),
        Some(&before)
    );
}

#[test]
fn pending_receipts_are_bounded_paginated_and_never_resumed() {
    let (_directory, store) = store();
    for id in 1..4 {
        DurableUpdateSession::claim(
            &store,
            custom_session(id, &format!("/Applications/Example{id}.app"), id as u64),
            110,
        )
        .unwrap();
    }
    let first = store.pending_external_updates(None, 2).unwrap();
    let next = store
        .pending_external_updates(Some(&first[1].operation_id), 2)
        .unwrap();
    assert_eq!((first.len(), next.len()), (2, 1));
    assert!(store.pending_external_updates(None, 0).is_err());
    assert!(store.pending_external_updates(None, 101).is_err());
    for row in first.into_iter().chain(next) {
        assert!(
            store
                .quarantine_external_update(&row.operation_id, row.revision)
                .unwrap()
        );
    }
    assert!(
        store
            .pending_external_updates(None, 100)
            .unwrap()
            .iter()
            .all(|row| row.state == UpdateState::Unverified)
    );
}

#[test]
fn migration_preserves_prior_data_and_explicit_reset_removes_ledger() {
    let directory = tempfile::tempdir().unwrap();
    let store = SqliteStore::new(directory.path().join("upgrade.db"));
    store.apply_migration(22).unwrap();
    let connection = rusqlite::Connection::open(store.database_path()).unwrap();
    connection
        .execute(
            "INSERT INTO app_settings(key, value) VALUES('kept', 'unchanged')",
            [],
        )
        .unwrap();
    store.migrate_to_latest().unwrap();
    let mut active = DurableUpdateSession::claim(&store, session(), 110).unwrap();
    store.migrate_to_latest().unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT value FROM app_settings WHERE key = 'kept'",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "unchanged"
    );
    assert!(store.external_update_receipt(OPERATION).unwrap().is_some());
    assert!(store.apply_migration(0).is_err());
    assert_eq!(
        store.current_version().unwrap(),
        crate::sqlite::migrations::current_schema_version()
    );
    active.event(OPERATION, UpdateEvent::Cancelled).unwrap();
    store.apply_migration(0).unwrap();
    store.migrate_to_latest().unwrap();
    assert!(store.external_update_receipt(OPERATION).unwrap().is_none());
}

#[test]
fn claim_process_helper() {
    let Some(root) = std::env::var_os("HELM_LEDGER_TEST_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let id: usize = std::env::var("HELM_LEDGER_TEST_INDEX")
        .unwrap()
        .parse()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !root.join("start").is_file() {
        assert!(
            std::time::Instant::now() < deadline,
            "start handshake timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let store = SqliteStore::new(root.join("ledger.db"));
    let result = DurableUpdateSession::claim(
        &store,
        custom_session(id, "/Applications/Example.app", 2),
        110,
    );
    match result {
        Ok(mut session) => {
            if std::env::var_os("HELM_LEDGER_TEST_WAIT").is_some() {
                session
                    .event(
                        session.receipt().operation_id.clone().as_str(),
                        UpdateEvent::Downloaded,
                    )
                    .unwrap();
                std::fs::write(root.join("committed"), b"ready").unwrap();
                std::thread::sleep(std::time::Duration::from_secs(30));
            }
        }
        Err(DurableUpdateError::Conflict) => std::process::exit(42),
        Err(error) => panic!("unexpected claim failure: {error}"),
    }
}

fn child(root: &Path, id: usize, wait: bool) -> std::process::Child {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "external_update::tests::durable::claim_process_helper",
            "--test-threads=1",
        ])
        .env("HELM_LEDGER_TEST_ROOT", root)
        .env("HELM_LEDGER_TEST_INDEX", id.to_string())
        .env_remove("HELM_LEDGER_TEST_WAIT")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if wait {
        command.env("HELM_LEDGER_TEST_WAIT", "1");
    }
    command.spawn().unwrap()
}

#[test]
fn independent_processes_serialize_claims_without_reusing_authorization() {
    let (directory, store) = store();
    let children: Vec<_> = (0..8)
        .map(|id| child(directory.path(), id, false))
        .collect();
    std::fs::write(directory.path().join("start"), b"go").unwrap();
    let mut winners = 0;
    for child in children {
        let output = child.wait_with_output().unwrap();
        match output.status.code() {
            Some(0) => winners += 1,
            Some(42) => {}
            status => panic!(
                "child failed: {status:?} {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        }
    }
    assert_eq!(winners, 1);
    assert_eq!(store.pending_external_updates(None, 100).unwrap().len(), 1);
}

#[test]
fn terminated_process_receipt_is_quarantined_without_resumption() {
    let (directory, store) = store();
    let mut process = child(directory.path(), 0, true);
    std::fs::write(directory.path().join("start"), b"go").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !directory.path().join("committed").is_file() {
        if std::time::Instant::now() >= deadline {
            let _ = process.kill();
            panic!(
                "commit handshake timed out: {:?}",
                process.wait_with_output().unwrap()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    process.kill().unwrap();
    assert!(!process.wait().unwrap().success());
    let operation = "550e8400-e29b-41d4-a716-000000000000";
    let record = store.external_update_receipt(operation).unwrap().unwrap();
    assert_eq!(record.state, UpdateState::ReadyToInstall);
    assert!(
        store
            .quarantine_external_update(operation, record.revision)
            .unwrap()
    );
    assert_eq!(
        store
            .external_update_receipt(operation)
            .unwrap()
            .unwrap()
            .state,
        UpdateState::Unverified
    );
    assert!(DurableUpdateSession::claim(&store, session(), 110).is_err());
    assert!(store.apply_migration(0).is_err());
    let connection = rusqlite::Connection::open(store.database_path()).unwrap();
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
}
