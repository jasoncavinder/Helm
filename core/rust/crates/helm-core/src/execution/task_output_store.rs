use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::models::TaskId;

const MAX_TASK_OUTPUT_RECORDS: usize = 512;
const MAX_STREAM_BYTES: usize = 128 * 1024;
const MAX_COMMAND_BYTES: usize = 8 * 1024;
const MAX_WORKING_DIR_BYTES: usize = 8 * 1024;
const MAX_ERROR_CODE_BYTES: usize = 256;
const MAX_ERROR_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TaskOutputRecord {
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub started_at_unix_ms: Option<i64>,
    pub finished_at_unix_ms: Option<i64>,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub termination_reason: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

static TASK_OUTPUTS: OnceLock<Mutex<BTreeMap<u64, TaskOutputRecord>>> = OnceLock::new();

fn task_outputs() -> &'static Mutex<BTreeMap<u64, TaskOutputRecord>> {
    TASK_OUTPUTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn normalize_stream(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }

    let normalized = if bytes.len() > MAX_STREAM_BYTES {
        &bytes[(bytes.len() - MAX_STREAM_BYTES)..]
    } else {
        bytes
    };

    let text = String::from_utf8_lossy(normalized).to_string();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn normalize_command(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        None
    } else {
        let normalized = truncate_str_to_tail_bytes(trimmed, MAX_COMMAND_BYTES);
        Some(normalized.to_string())
    }
}

fn normalize_working_dir(working_dir: &str) -> Option<String> {
    let trimmed = working_dir.trim();
    if trimmed.is_empty() {
        None
    } else {
        let normalized = truncate_str_to_tail_bytes(trimmed, MAX_WORKING_DIR_BYTES);
        Some(normalized.to_string())
    }
}

fn normalize_termination_reason(reason: &str) -> Option<String> {
    let normalized = reason.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "error" | "timeout" | "signal" | "cancelled" => Some(normalized),
        _ => None,
    }
}

fn normalize_error_code(code: &str) -> Option<String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut normalized = String::with_capacity(trimmed.len());
    for character in trimmed.chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            normalized.push(character.to_ascii_lowercase());
        } else if !normalized.ends_with('_') {
            normalized.push('_');
        }
    }
    let normalized = normalized.trim_matches('_');
    if normalized.is_empty() {
        return None;
    }
    let normalized = truncate_str_to_tail_bytes(normalized, MAX_ERROR_CODE_BYTES);
    Some(normalized.to_string())
}

fn normalize_error_message(message: &str) -> Option<String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        None
    } else {
        let normalized = truncate_str_to_tail_bytes(trimmed, MAX_ERROR_MESSAGE_BYTES);
        Some(normalized.to_string())
    }
}

fn truncate_str_to_tail_bytes(input: &str, max_bytes: usize) -> &str {
    if input.len() <= max_bytes {
        return input;
    }
    let min_start = input.len() - max_bytes;
    let start = input
        .char_indices()
        .find_map(|(index, _)| (index >= min_start).then_some(index))
        .unwrap_or(0);
    &input[start..]
}

fn ensure_capacity_for_new_record(outputs: &mut BTreeMap<u64, TaskOutputRecord>, task_id: TaskId) {
    if !outputs.contains_key(&task_id.0)
        && outputs.len() >= MAX_TASK_OUTPUT_RECORDS
        && let Some(oldest_task_id) = outputs.keys().next().copied()
    {
        outputs.remove(&oldest_task_id);
    }
}

fn empty_record() -> TaskOutputRecord {
    TaskOutputRecord::default()
}

fn ensure_entry(
    outputs: &mut BTreeMap<u64, TaskOutputRecord>,
    task_id: TaskId,
) -> &mut TaskOutputRecord {
    ensure_capacity_for_new_record(outputs, task_id);
    outputs.entry(task_id.0).or_insert_with(empty_record)
}

fn system_time_to_unix_ms(value: SystemTime) -> Option<i64> {
    let duration = value.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(duration.as_millis()).ok()
}

fn recalculate_duration_ms(entry: &mut TaskOutputRecord) {
    entry.duration_ms = match (entry.started_at_unix_ms, entry.finished_at_unix_ms) {
        (Some(start), Some(finish)) if finish >= start => Some((finish - start) as u64),
        _ => None,
    };
}

fn append_stream(existing: &mut Option<String>, chunk: &[u8]) {
    if chunk.is_empty() {
        return;
    }

    let mut combined = existing
        .as_ref()
        .map_or_else(Vec::new, |value| value.as_bytes().to_vec());
    combined.extend_from_slice(chunk);

    if combined.len() > MAX_STREAM_BYTES {
        let start = combined.len() - MAX_STREAM_BYTES;
        combined = combined[start..].to_vec();
    }

    let text = String::from_utf8_lossy(&combined).to_string();
    if text.trim().is_empty() {
        *existing = None;
    } else {
        *existing = Some(text);
    }
}

pub fn record(task_id: TaskId, command: Option<&str>, stdout: &[u8], stderr: &[u8]) {
    if let Ok(mut outputs) = task_outputs().lock() {
        let entry = ensure_entry(&mut outputs, task_id);
        if let Some(command) = command.and_then(normalize_command) {
            entry.command = Some(command);
        }
        entry.stdout = normalize_stream(stdout);
        entry.stderr = normalize_stream(stderr);
    }
}

pub fn record_command(task_id: TaskId, command: &str) {
    if let Ok(mut outputs) = task_outputs().lock() {
        let entry = ensure_entry(&mut outputs, task_id);
        if let Some(command) = normalize_command(command) {
            entry.command = Some(command);
        }
    }
}

pub fn record_context(task_id: TaskId, command: Option<&str>, cwd: Option<&str>) {
    if let Ok(mut outputs) = task_outputs().lock() {
        let entry = ensure_entry(&mut outputs, task_id);
        if let Some(command) = command.and_then(normalize_command) {
            entry.command = Some(command);
        }
        if let Some(cwd) = cwd.and_then(normalize_working_dir) {
            entry.cwd = Some(cwd);
        }
    }
}

pub fn record_started_at(task_id: TaskId, started_at: SystemTime) {
    if let Ok(mut outputs) = task_outputs().lock() {
        let entry = ensure_entry(&mut outputs, task_id);
        entry.started_at_unix_ms = system_time_to_unix_ms(started_at);
        recalculate_duration_ms(entry);
    }
}

pub fn record_terminal_metadata(
    task_id: TaskId,
    started_at: SystemTime,
    finished_at: SystemTime,
    exit_code: Option<i32>,
    termination_reason: Option<&str>,
) {
    if let Ok(mut outputs) = task_outputs().lock() {
        let entry = ensure_entry(&mut outputs, task_id);
        entry.started_at_unix_ms = system_time_to_unix_ms(started_at);
        entry.finished_at_unix_ms = system_time_to_unix_ms(finished_at);
        entry.exit_code = exit_code;
        entry.termination_reason = termination_reason.and_then(normalize_termination_reason);
        recalculate_duration_ms(entry);
    }
}

pub fn record_error(
    task_id: TaskId,
    error_code: &str,
    error_message: &str,
    termination_reason: Option<&str>,
    finished_at: Option<SystemTime>,
) {
    if let Ok(mut outputs) = task_outputs().lock() {
        let entry = ensure_entry(&mut outputs, task_id);
        if let Some(code) = normalize_error_code(error_code) {
            entry.error_code = Some(code);
        }
        if let Some(message) = normalize_error_message(error_message) {
            entry.error_message = Some(message);
        }
        if let Some(reason) = termination_reason.and_then(normalize_termination_reason) {
            entry.termination_reason = Some(reason);
        }
        if let Some(finished_at) = finished_at.and_then(system_time_to_unix_ms) {
            entry.finished_at_unix_ms = Some(finished_at);
        }
        recalculate_duration_ms(entry);
    }
}

pub fn append_stdout(task_id: TaskId, chunk: &[u8]) {
    if let Ok(mut outputs) = task_outputs().lock() {
        let entry = ensure_entry(&mut outputs, task_id);
        append_stream(&mut entry.stdout, chunk);
    }
}

pub fn append_stderr(task_id: TaskId, chunk: &[u8]) {
    if let Ok(mut outputs) = task_outputs().lock() {
        let entry = ensure_entry(&mut outputs, task_id);
        append_stream(&mut entry.stderr, chunk);
    }
}

pub fn get(task_id: TaskId) -> Option<TaskOutputRecord> {
    task_outputs().lock().ok()?.get(&task_id.0).cloned()
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use std::sync::{Mutex, OnceLock};

    use super::{
        MAX_STREAM_BYTES, MAX_TASK_OUTPUT_RECORDS, append_stderr, append_stdout, get, record,
        record_command, record_context, record_error, record_started_at, record_terminal_metadata,
        task_outputs,
    };
    use crate::models::TaskId;

    static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn acquire_test_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("task output store test lock should not be poisoned")
    }

    fn clear_store() {
        if let Ok(mut outputs) = task_outputs().lock() {
            outputs.clear();
        }
    }

    #[test]
    fn record_and_get_round_trip_output() {
        let _guard = acquire_test_lock();
        clear_store();
        let task_id = TaskId(9001);
        record(task_id, Some("brew upgrade ripgrep"), b"hello\n", b"warn\n");

        let output = get(task_id).expect("expected output to be recorded");
        assert_eq!(output.command.as_deref(), Some("brew upgrade ripgrep"));
        assert_eq!(output.stdout.as_deref(), Some("hello\n"));
        assert_eq!(output.stderr.as_deref(), Some("warn\n"));
        assert_eq!(output.cwd, None);
        assert_eq!(output.started_at_unix_ms, None);
        assert_eq!(output.finished_at_unix_ms, None);
        assert_eq!(output.duration_ms, None);
        assert_eq!(output.exit_code, None);
        assert_eq!(output.termination_reason, None);
        assert_eq!(output.error_code, None);
        assert_eq!(output.error_message, None);
    }

    #[test]
    fn output_is_truncated_to_tail_window() {
        let _guard = acquire_test_lock();
        clear_store();
        let task_id = TaskId(9002);
        let input = vec![b'a'; MAX_STREAM_BYTES + 32];
        record(task_id, None, &input, b"");

        let output = get(task_id).expect("expected output to be recorded");
        let stdout = output.stdout.expect("expected stdout text");
        assert_eq!(stdout.len(), MAX_STREAM_BYTES);
    }

    #[test]
    fn append_stdout_updates_live_output_tail() {
        let _guard = acquire_test_lock();
        clear_store();
        let task_id = TaskId(9003);
        record_command(task_id, "brew update");
        append_stdout(task_id, b"first\n");
        append_stdout(task_id, b"second\n");

        let output = get(task_id).expect("expected output to be recorded");
        assert_eq!(output.command.as_deref(), Some("brew update"));
        assert_eq!(output.stdout.as_deref(), Some("first\nsecond\n"));
    }

    #[test]
    fn append_stderr_is_truncated_to_tail_window() {
        let _guard = acquire_test_lock();
        clear_store();
        let task_id = TaskId(9004);
        record_command(task_id, "brew update");

        let large_chunk = vec![b'e'; MAX_STREAM_BYTES + 64];
        append_stderr(task_id, &large_chunk);

        let output = get(task_id).expect("expected output to be recorded");
        let stderr = output.stderr.expect("expected stderr text");
        assert_eq!(stderr.len(), MAX_STREAM_BYTES);
    }

    #[test]
    fn context_and_terminal_metadata_are_persisted() {
        let _guard = acquire_test_lock();
        clear_store();
        let task_id = TaskId(9005);
        let started_at = UNIX_EPOCH + Duration::from_millis(2_000);
        let finished_at = UNIX_EPOCH + Duration::from_millis(2_450);
        record_context(
            task_id,
            Some("npm outdated --json"),
            Some("/Users/test/work"),
        );
        record_started_at(task_id, started_at);
        record_terminal_metadata(task_id, started_at, finished_at, Some(0), None);

        let output = get(task_id).expect("expected output to be recorded");
        assert_eq!(output.command.as_deref(), Some("npm outdated --json"));
        assert_eq!(output.cwd.as_deref(), Some("/Users/test/work"));
        assert_eq!(output.started_at_unix_ms, Some(2_000));
        assert_eq!(output.finished_at_unix_ms, Some(2_450));
        assert_eq!(output.duration_ms, Some(450));
        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.termination_reason, None);
    }

    #[test]
    fn error_metadata_is_persisted() {
        let _guard = acquire_test_lock();
        clear_store();
        let task_id = TaskId(9006);
        let started_at = UNIX_EPOCH + Duration::from_millis(4_000);
        let finished_at = UNIX_EPOCH + Duration::from_millis(4_120);
        record_started_at(task_id, started_at);
        record_error(
            task_id,
            "Spawn Failed",
            "failed to spawn process: No such file or directory",
            Some("error"),
            Some(finished_at),
        );

        let output = get(task_id).expect("expected output to be recorded");
        assert_eq!(output.error_code.as_deref(), Some("spawn_failed"));
        assert_eq!(
            output.error_message.as_deref(),
            Some("failed to spawn process: No such file or directory")
        );
        assert_eq!(output.termination_reason.as_deref(), Some("error"));
        assert_eq!(output.finished_at_unix_ms, Some(4_120));
        assert_eq!(output.duration_ms, Some(120));
    }

    #[test]
    fn record_capacity_prunes_oldest_task_records() {
        let _guard = acquire_test_lock();
        clear_store();
        for offset in 0..=MAX_TASK_OUTPUT_RECORDS {
            let task_id = TaskId((10_000 + offset) as u64);
            record(task_id, None, b"ok\n", b"");
        }

        assert!(
            get(TaskId(10_000)).is_none(),
            "oldest record should be pruned"
        );
        assert!(
            get(TaskId((10_000 + MAX_TASK_OUTPUT_RECORDS) as u64)).is_some(),
            "newest record should be retained"
        );
    }
}
