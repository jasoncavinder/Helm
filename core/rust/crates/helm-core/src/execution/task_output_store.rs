use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use crate::models::TaskId;

const MAX_TASK_OUTPUT_RECORDS: usize = 512;
const MAX_STREAM_BYTES: usize = 128 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskOutputRecord {
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

pub fn record(task_id: TaskId, stdout: &[u8], stderr: &[u8]) {
    if let Ok(mut outputs) = task_outputs().lock() {
        if !outputs.contains_key(&task_id.0)
            && outputs.len() >= MAX_TASK_OUTPUT_RECORDS
            && let Some(oldest_task_id) = outputs.keys().next().copied()
        {
            outputs.remove(&oldest_task_id);
        }

        outputs.insert(
            task_id.0,
            TaskOutputRecord {
                stdout: normalize_stream(stdout),
                stderr: normalize_stream(stderr),
            },
        );
    }
}

pub fn get(task_id: TaskId) -> Option<TaskOutputRecord> {
    task_outputs().lock().ok()?.get(&task_id.0).cloned()
}

#[cfg(test)]
mod tests {
    use super::{MAX_STREAM_BYTES, get, record};
    use crate::models::TaskId;

    #[test]
    fn record_and_get_round_trip_output() {
        let task_id = TaskId(9001);
        record(task_id, b"hello\n", b"warn\n");

        let output = get(task_id).expect("expected output to be recorded");
        assert_eq!(output.stdout.as_deref(), Some("hello\n"));
        assert_eq!(output.stderr.as_deref(), Some("warn\n"));
    }

    #[test]
    fn output_is_truncated_to_tail_window() {
        let task_id = TaskId(9002);
        let input = vec![b'a'; MAX_STREAM_BYTES + 32];
        record(task_id, &input, b"");

        let output = get(task_id).expect("expected output to be recorded");
        let stdout = output.stdout.expect("expected stdout text");
        assert_eq!(stdout.len(), MAX_STREAM_BYTES);
    }
}
