use super::*;
use crate::external_update::UpdateState;
use crate::external_update::durable::{
    ExternalUpdateReceipt, holds_target, parse_state, state_name,
};

fn prepare(connection: &Connection) -> rusqlite::Result<()> {
    ensure_schema_ready(connection)?;
    // External side effects cannot be rolled back with SQLite. Unlike ordinary
    // cache writes, every acknowledged authorization/state commit must sync.
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "fullfsync", "ON")?;
    Ok(())
}

fn mutation_allowed(connection: &Connection) -> rusqlite::Result<bool> {
    let value: Option<String> = connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'safe_mode'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    Ok(matches!(value.as_deref(), None | Some("0")))
}

fn receipt(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExternalUpdateReceipt> {
    let parse_number = |index| -> rusqlite::Result<u64> {
        row.get::<_, String>(index)?
            .parse()
            .map_err(|_| storage_error_sqlite("invalid external update identity"))
    };
    Ok(ExternalUpdateReceipt {
        operation_id: row.get(0)?,
        fingerprint: row.get(1)?,
        target_path: PathBuf::from(row.get::<_, String>(2)?),
        target_device: parse_number(3)?,
        target_inode: parse_number(4)?,
        bundle_identifier: row.get(5)?,
        installed_build: row.get(6)?,
        candidate_build: row.get(7)?,
        state: parse_state(&row.get::<_, String>(8)?)
            .ok_or_else(|| storage_error_sqlite("invalid external update state"))?,
        revision: row.get(9)?,
    })
}

impl SqliteStore {
    pub(crate) fn claim_external_update(
        &self,
        record: &ExternalUpdateReceipt,
    ) -> PersistenceResult<bool> {
        self.with_connection("claim_external_update", |connection| {
            prepare(connection)?;
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if !mutation_allowed(&transaction)? { return Ok(false); }
            let conflict: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM external_update_sessions WHERE operation_id = ?1
                 OR (holds_target = 1 AND (target_path = ?2 OR (target_device = ?3 AND target_inode = ?4))))",
                params![record.operation_id, record.target_path.to_str(), record.target_device.to_string(), record.target_inode.to_string()],
                |row| row.get(0),
            )?;
            if conflict { return Ok(false); }
            transaction.execute(
                "INSERT INTO external_update_sessions
                 (operation_id, fingerprint, target_path, target_device, target_inode, bundle_identifier,
                  installed_build, candidate_build, state, revision, holds_target)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'downloading', 0, 1)",
                params![record.operation_id, record.fingerprint, record.target_path.to_str(),
                    record.target_device.to_string(), record.target_inode.to_string(), record.bundle_identifier,
                    record.installed_build, record.candidate_build],
            )?;
            transaction.commit()?;
            Ok(true)
        })
    }

    pub(crate) fn transition_external_update(
        &self,
        before: &ExternalUpdateReceipt,
        after: UpdateState,
        check_safe_mode: bool,
    ) -> PersistenceResult<bool> {
        self.with_connection("transition_external_update", |connection| {
            prepare(connection)?;
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if check_safe_mode && !mutation_allowed(&transaction)? { return Ok(false); }
            let changed = transaction.execute(
                "UPDATE external_update_sessions SET state = ?1, revision = revision + 1, holds_target = ?2
                 WHERE operation_id = ?3 AND fingerprint = ?4 AND revision = ?5 AND state = ?6",
                params![state_name(after), holds_target(after), before.operation_id, before.fingerprint,
                    before.revision, state_name(before.state)],
            )?;
            transaction.commit()?;
            Ok(changed == 1)
        })
    }

    /// Read by exact operation ID. Receipts are not reusable authorization.
    pub fn external_update_receipt(
        &self,
        operation_id: &str,
    ) -> PersistenceResult<Option<ExternalUpdateReceipt>> {
        self.with_connection("external_update_receipt", |connection| {
            ensure_schema_ready(connection)?;
            connection.query_row(
                "SELECT operation_id, fingerprint, target_path, target_device, target_inode, bundle_identifier,
                 installed_build, candidate_build, state, revision FROM external_update_sessions WHERE operation_id = ?1",
                [operation_id], receipt,
            ).optional()
        })
    }

    /// Bounded cursor-based recovery inventory, never automatic resumed work.
    pub fn pending_external_updates(
        &self,
        after_operation: Option<&str>,
        limit: usize,
    ) -> PersistenceResult<Vec<ExternalUpdateReceipt>> {
        self.with_connection("pending_external_updates", |connection| {
            ensure_schema_ready(connection)?;
            if !(1..=100).contains(&limit) {
                return Err(storage_error_sqlite("invalid external update receipt limit"));
            }
            let mut statement = connection.prepare(
                "SELECT operation_id, fingerprint, target_path, target_device, target_inode, bundle_identifier,
                 installed_build, candidate_build, state, revision FROM external_update_sessions
                 WHERE holds_target = 1 AND operation_id > ?1 ORDER BY operation_id LIMIT ?2",
            )?;
            statement.query_map(params![after_operation.unwrap_or(""), limit as i64], receipt)?.collect()
        })
    }

    /// Conservative restart/loss quarantine. The expected revision fences late
    /// callbacks; a successful quarantine retains the target lock indefinitely.
    /// This never authorizes retry, resumes an installer, or establishes success.
    pub fn quarantine_external_update(
        &self,
        operation_id: &str,
        expected_revision: i64,
    ) -> PersistenceResult<bool> {
        self.with_connection("quarantine_external_update", |connection| {
            prepare(connection)?;
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let changed = transaction.execute(
                "UPDATE external_update_sessions SET state = 'unverified', revision = revision + 1
                 WHERE operation_id = ?1 AND revision = ?2 AND revision < 9223372036854775807
                 AND state IN ('downloading', 'ready_to_install', 'installing', 'awaiting_verification')",
                params![operation_id, expected_revision],
            )?;
            transaction.commit()?;
            Ok(changed == 1)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_update_writes_use_full_sync_without_changing_other_connections() {
        let directory = tempfile::tempdir().unwrap();
        let store = SqliteStore::new(directory.path().join("durability.db"));
        store.migrate_to_latest().unwrap();
        let connection = open_connection(store.database_path()).unwrap();
        prepare(&connection).unwrap();
        assert_eq!(
            connection
                .pragma_query_value(None, "synchronous", |row| row.get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            connection
                .pragma_query_value(None, "fullfsync", |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        let ordinary = open_connection(store.database_path()).unwrap();
        assert_eq!(
            ordinary
                .pragma_query_value(None, "synchronous", |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
