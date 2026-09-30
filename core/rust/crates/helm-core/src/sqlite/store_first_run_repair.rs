use super::*;
use crate::first_run_repair::{
    ACTION_ID, FirstRunRepairPlan, FirstRunRepairReceipt, FirstRunRepairStore, RepairVerification,
};

fn encode<T: serde::Serialize>(value: &T) -> rusqlite::Result<String> {
    serde_json::to_string(value).map_err(|_| storage_error_sqlite("invalid repair receipt"))
}

fn decode(value: &str) -> rusqlite::Result<FirstRunRepairReceipt> {
    serde_json::from_str(value).map_err(|_| storage_error_sqlite("invalid repair receipt"))
}

fn matches_preference(
    connection: &Connection,
    expected: &ManagerPreference,
) -> rusqlite::Result<bool> {
    let hard = expected
        .timeout_hard_seconds
        .map(i64::try_from)
        .transpose()
        .map_err(|_| storage_error_sqlite("invalid timeout"))?;
    let idle = expected
        .timeout_idle_seconds
        .map(i64::try_from)
        .transpose()
        .map_err(|_| storage_error_sqlite("invalid timeout"))?;
    connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM manager_preferences WHERE manager_id = ?1
         AND enabled = ?2 AND selected_executable_path IS ?3
         AND selected_install_method IS ?4 AND timeout_hard_seconds IS ?5
         AND timeout_idle_seconds IS ?6)",
        params![
            expected.manager.as_str(),
            expected.enabled,
            expected.selected_executable_path,
            expected.selected_install_method,
            hard,
            idle
        ],
        |row| row.get(0),
    )
}

fn mutation_allowed(connection: &Connection) -> rusqlite::Result<bool> {
    let safe_mode: Option<String> = connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'safe_mode'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if !matches!(safe_mode.as_deref(), None | Some("0")) {
        return Ok(false);
    }
    connection.query_row(
        "SELECT NOT EXISTS(SELECT 1 FROM task_records WHERE manager_id = 'mise'
         AND status IN ('queued', 'running'))",
        [],
        |row| row.get(0),
    )
}

impl FirstRunRepairStore for SqliteStore {
    fn apply_first_run_repair(
        &self,
        plan: &FirstRunRepairPlan,
    ) -> PersistenceResult<Option<FirstRunRepairReceipt>> {
        self.with_connection("apply_first_run_repair", |connection| {
            ensure_schema_ready(connection)?;
            let transaction = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            if plan.before.manager != ManagerId::Mise || !plan.before.enabled
                || plan.before.selected_executable_path.is_none()
                || !matches_preference(&transaction, &plan.before)? || !mutation_allowed(&transaction)? {
                return Ok(None);
            }
            transaction.execute(
                "UPDATE manager_preferences SET selected_executable_path = NULL WHERE manager_id = 'mise'", [],
            )?;
            let mut receipt = FirstRunRepairReceipt {
                receipt_id: 0, plan_fingerprint: plan.fingerprint().into(), action_id: ACTION_ID.into(),
                previous_path: plan.previous_path().into(), checked_path: plan.executable_path().into(),
                applied: true, verification: RepairVerification::Unverified,
                observed_version: None, reason: None,
            };
            transaction.execute(
                "INSERT INTO first_run_repair_receipts (plan_fingerprint, before_preference_json, receipt_json)
                 VALUES (?1, ?2, ?3)", params![plan.fingerprint(), encode(&plan.before)?, encode(&receipt)?],
            )?;
            receipt.receipt_id = transaction.last_insert_rowid();
            transaction.execute(
                "UPDATE first_run_repair_receipts SET receipt_json = ?1 WHERE receipt_id = ?2",
                params![encode(&receipt)?, receipt.receipt_id],
            )?;
            transaction.commit()?;
            Ok(Some(receipt))
        })
    }

    fn finish_first_run_repair(
        &self,
        plan: &FirstRunRepairPlan,
        proposed: &FirstRunRepairReceipt,
    ) -> PersistenceResult<FirstRunRepairReceipt> {
        self.with_connection("finish_first_run_repair", |connection| {
            ensure_schema_ready(connection)?;
            let transaction =
                connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            let (before, current) = transaction.query_row(
                "SELECT before_preference_json, receipt_json FROM first_run_repair_receipts
                 WHERE receipt_id = ?1 AND plan_fingerprint = ?2",
                params![proposed.receipt_id, plan.fingerprint()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            let original = decode(&current)?;
            let mut immutable = proposed.clone();
            immutable.verification = original.verification.clone();
            immutable.observed_version = original.observed_version.clone();
            immutable.reason = original.reason.clone();
            if before != encode(&plan.before)?
                || immutable != original
                || proposed.verification == RepairVerification::Unverified
                || (proposed.verification == RepairVerification::Verified
                    && (proposed.observed_version.is_none() || proposed.reason.is_some()))
            {
                return Err(storage_error_sqlite(
                    "repair receipt does not match reviewed action",
                ));
            }
            // A late callback cannot overwrite a previously finalized result.
            if original.verification != RepairVerification::Unverified {
                return Ok(original);
            }
            let mut expected = plan.before.clone();
            expected.selected_executable_path = None;
            let mut receipt = proposed.clone();
            if !matches_preference(&transaction, &expected)? || !mutation_allowed(&transaction)? {
                receipt.verification = RepairVerification::Failed;
                receipt.observed_version = None;
                receipt.reason = Some("preference_or_policy_changed".into());
            }
            transaction.execute(
                "UPDATE first_run_repair_receipts SET receipt_json = ?1 WHERE receipt_id = ?2",
                params![encode(&receipt)?, receipt.receipt_id],
            )?;
            transaction.commit()?;
            Ok(receipt)
        })
    }

    fn first_run_repair_receipts(&self) -> PersistenceResult<Vec<FirstRunRepairReceipt>> {
        self.with_connection("first_run_repair_receipts", |connection| {
            ensure_schema_ready(connection)?;
            let mut statement = connection.prepare(
                "SELECT receipt_json FROM first_run_repair_receipts ORDER BY receipt_id DESC LIMIT 100",
            )?;
            statement.query_map([], |row| decode(&row.get::<_, String>(0)?))?.collect()
        })
    }
}
