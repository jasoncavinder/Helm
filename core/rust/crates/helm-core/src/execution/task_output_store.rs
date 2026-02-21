use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use crate::models::TaskId;

const MAX_TASK_OUTPUT_RECORDS: usize = 512;
const MAX_STREAM_BYTES: usize = 128 * 1024;
const MAX_COMMAND_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskOutputRecord {
    pub command: Option<String>,
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
        ensure_capacity_for_new_record(&mut outputs, task_id);

        outputs.insert(
            task_id.0,
            TaskOutputRecord {
                command: command.and_then(normalize_command),
                stdout: normalize_stream(stdout),
                stderr: normalize_stream(stderr),
            },
        );
    }
}

pub fn record_command(task_id: TaskId, command: &str) {
    if let Ok(mut outputs) = task_outputs().lock() {
        ensure_capacity_for_new_record(&mut outputs, task_id);
        let entry = outputs.entry(task_id.0).or_insert(TaskOutputRecord {
            command: None,
            stdout: None,
            stderr: None,
        });
        if let Some(command) = normalize_command(command) {
            entry.command = Some(command);
        }
    }
}

pub fn append_stdout(task_id: TaskId, chunk: &[u8]) {
    if let Ok(mut outputs) = task_outputs().lock() {
        ensure_capacity_for_new_record(&mut outputs, task_id);
        let entry = outputs.entry(task_id.0).or_insert(TaskOutputRecord {
            command: None,
            stdout: None,
            stderr: None,
        });
        append_stream(&mut entry.stdout, chunk);
    }
}

pub fn append_stderr(task_id: TaskId, chunk: &[u8]) {
    if let Ok(mut outputs) = task_outputs().lock() {
        ensure_capacity_for_new_record(&mut outputs, task_id);
        let entry = outputs.entry(task_id.0).or_insert(TaskOutputRecord {
            command: None,
            stdout: None,
            stderr: None,
        });
        append_stream(&mut entry.stderr, chunk);
    }
}

pub fn get(task_id: TaskId) -> Option<TaskOutputRecord> {
    task_outputs().lock().ok()?.get(&task_id.0).cloned()
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_STREAM_BYTES, MAX_TASK_OUTPUT_RECORDS, append_stderr, append_stdout, get, record,
        record_command, task_outputs,
    };
    use crate::models::TaskId;

    fn clear_store() {
        if let Ok(mut outputs) = task_outputs().lock() {
            outputs.clear();
        }
    }

    #[test]
    fn record_and_get_round_trip_output() {
        clear_store();
        let task_id = TaskId(9001);
        record(task_id, Some("brew upgrade ripgrep"), b"hello\n", b"warn\n");

        let output = get(task_id).expect("expected output to be recorded");
        assert_eq!(output.command.as_deref(), Some("brew upgrade ripgrep"));
        assert_eq!(output.stdout.as_deref(), Some("hello\n"));
        assert_eq!(output.stderr.as_deref(), Some("warn\n"));
    }

    #[test]
    fn output_is_truncated_to_tail_window() {
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
    fn record_capacity_prunes_oldest_task_records() {
        clear_store();
        for offset in 0..=MAX_TASK_OUTPUT_RECORDS {
            let task_id = TaskId((10_000 + offset) as u64);
            record(task_id, None, b"ok\n", b"");
        }

        assert!(get(TaskId(10_000)).is_none(), "oldest record should be pruned");
        assert!(
            get(TaskId((10_000 + MAX_TASK_OUTPUT_RECORDS) as u64)).is_some(),
            "newest record should be retained"
        );
    }
}
