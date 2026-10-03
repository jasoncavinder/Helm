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
