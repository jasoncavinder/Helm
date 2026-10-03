use super::*;
use crate::external_update::adoption::{AdoptionReceipt, AdoptionToken, identity_fingerprint};
use crate::external_update::{Authority, TargetObservation, normal_app_path};

fn cursor(connection: &Connection, path: &Path) -> rusqlite::Result<AdoptionToken> {
    connection.query_row(
        "SELECT epoch, COALESCE((SELECT MAX(sequence) FROM external_update_adoptions WHERE target_path = ?1), 0)
         FROM external_update_adoption_epoch WHERE singleton = 1",
        [path.to_str()], |row| {
            let epoch: Vec<u8> = row.get(0)?;
            Ok(AdoptionToken {
                epoch: epoch.try_into().map_err(|_| storage_error_sqlite("invalid adoption epoch"))?,
                sequence: row.get(1)?,
            })
        },
    )
}

fn latest(connection: &Connection, path: &Path) -> rusqlite::Result<Option<AdoptionReceipt>> {
    connection
        .query_row(
            "SELECT e.epoch, a.sequence, a.consent_id, a.identity_fingerprint, a.review_fingerprint
         FROM external_update_adoptions a CROSS JOIN external_update_adoption_epoch e
         WHERE a.target_path = ?1 AND e.singleton = 1 ORDER BY a.sequence DESC LIMIT 1",
            [path.to_str()],
            |row| {
                let epoch: Vec<u8> = row.get(0)?;
                Ok(AdoptionReceipt {
                    token: AdoptionToken {
                        epoch: epoch
                            .try_into()
                            .map_err(|_| storage_error_sqlite("invalid adoption epoch"))?,
                        sequence: row.get(1)?,
                    },
                    target_path: path.to_path_buf(),
                    consent_id: row.get(2)?,
                    identity_fingerprint: row.get(3)?,
                    review_fingerprint: row.get(4)?,
                })
            },
        )
        .optional()
}

/// Called within the same immediate transaction as claim/handoff/verification.
/// Cached adoption tokens cannot race revocation across connections.
pub(super) fn authority_is_current(
    connection: &Connection,
    target: &TargetObservation,
) -> rusqlite::Result<bool> {
    match target.authority {
        Authority::Standalone => Ok(true),
        Authority::UserAdopted(token) => Ok(latest(connection, &target.canonical_path)?
            .is_some_and(|receipt| {
                receipt.token == token
                    && !receipt.is_revoked()
                    && receipt.identity_fingerprint.as_deref()
                        == Some(&identity_fingerprint(target))
            })),
        Authority::OtherManager | Authority::Unknown => Ok(false),
    }
}

impl SqliteStore {
    /// Read existing helper history without initialization, migration or consent
    /// mutation. The native caller holds its private-directory lease throughout.
    /// Never use this diagnostic as authority: session admission still resolves
    /// fresh evidence and rechecks the latest grant in its write transaction.
    pub fn inspect_external_update_consent(
        database_path: &Path,
        target: &TargetObservation,
        roots: &[PathBuf],
    ) -> PersistenceResult<crate::external_update::adoption::ConsentStatus> {
        use crate::external_update::adoption::{ConsentStatus, validate_unresolved_target};
        validate_unresolved_target(target, roots).map_err(|_| {
            storage_error_text("inspect_external_update_consent", "target rejected")
        })?;
        (|| -> rusqlite::Result<_> {
            let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX
                | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW;
            let mut connection = Connection::open_with_flags(database_path, flags)?;
            connection.busy_timeout(Duration::from_millis(100))?;
            // Never use immutable mode: committed WAL content must remain visible.
            let transaction = connection.transaction()?;
            let version = read_current_version(&transaction)?;
            if version != current_schema_version()
                || !migration_checksum_column_exists(&transaction)?
            {
                return Err(storage_error_sqlite(
                    "helper ledger requires explicit preparation",
                ));
            }
            validate_migration_manifest().map_err(|error| storage_error_sqlite(&error))?;
            validate_applied_migration_identities(&transaction, version)?;
            let integrity: String =
                transaction.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
            if integrity != "ok" {
                return Err(storage_error_sqlite("helper ledger integrity check failed"));
            }
            cursor(&transaction, &target.canonical_path)?;
            transaction.query_row("SELECT COUNT(*) FROM external_update_sessions", [], |row| {
                row.get::<_, i64>(0)
            })?;
            let status = match latest(&transaction, &target.canonical_path)? {
                None => ConsentStatus::NotRecorded,
                Some(receipt) if receipt.is_revoked() => ConsentStatus::Revoked,
                Some(receipt)
                    if receipt.identity_fingerprint.as_deref()
                        == Some(&identity_fingerprint(target)) =>
                {
                    ConsentStatus::Recorded
                }
                Some(_) => ConsentStatus::IdentityChanged,
            };
            transaction.commit()?;
            Ok(status)
        })()
        .map_err(|error| storage_error("inspect_external_update_consent", error))
    }

    pub(crate) fn external_adoption_cursor(&self, path: &Path) -> PersistenceResult<AdoptionToken> {
        self.with_connection("external_adoption_cursor", |connection| {
            ensure_schema_ready(connection)?;
            cursor(connection, path)
        })
    }

    pub(crate) fn commit_external_adoption(
        &self,
        consent_id: &str,
        target: &TargetObservation,
        review_fingerprint: &str,
        prior: AdoptionToken,
    ) -> PersistenceResult<Option<AdoptionReceipt>> {
        self.with_connection("commit_external_adoption", |connection| {
            external_update::prepare(connection)?;
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if !external_update::mutation_allowed(&transaction)?
                || cursor(&transaction, &target.canonical_path)? != prior { return Ok(None); }
            let conflict: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM external_update_adoptions WHERE consent_id = ?1)
                 OR EXISTS(SELECT 1 FROM external_update_sessions WHERE holds_target = 1
                     AND (target_path = ?2 OR (target_device = ?3 AND target_inode = ?4)))",
                params![consent_id, target.canonical_path.to_str(), target.device.to_string(), target.inode.to_string()],
                |row| row.get(0),
            )?;
            if conflict { return Ok(None); }
            let snapshot = serde_json::to_string(target).map_err(|_| storage_error_sqlite("invalid adoption snapshot"))?;
            transaction.execute(
                "INSERT INTO external_update_adoptions (target_path, consent_id, identity_fingerprint, review_fingerprint, reviewed_target_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![target.canonical_path.to_str(), consent_id, identity_fingerprint(target), review_fingerprint, snapshot],
            )?;
            let receipt = latest(&transaction, &target.canonical_path)?;
            transaction.commit()?;
            Ok(receipt)
        })
    }

    /// Read-only permission receipt. The authenticated runtime must resolve it
    /// against new native observations; never deserialize it as authority.
    pub fn external_update_adoption(
        &self,
        path: &Path,
    ) -> PersistenceResult<Option<AdoptionReceipt>> {
        self.with_connection("external_update_adoption", |connection| {
            ensure_schema_ready(connection)?;
            latest(connection, path)
        })
    }

    /// Revocation is allowed in safe mode and during active work. It invalidates
    /// pending reviews even when no grant exists yet. It does not claim to stop
    /// an installer that has already received its durable handoff permit.
    pub fn revoke_external_update_adoption(
        &self,
        path: &Path,
    ) -> PersistenceResult<AdoptionReceipt> {
        self.with_connection("revoke_external_update_adoption", |connection| {
            if !normal_app_path(path) {
                return Err(storage_error_sqlite("invalid adoption target"));
            }
            external_update::prepare(connection)?;
            let transaction =
                connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            transaction.execute(
                "INSERT INTO external_update_adoptions (target_path) VALUES (?1)",
                [path.to_str()],
            )?;
            let receipt = latest(&transaction, path)?
                .ok_or_else(|| storage_error_sqlite("missing revocation receipt"))?;
            transaction.commit()?;
            Ok(receipt)
        })
    }
}
