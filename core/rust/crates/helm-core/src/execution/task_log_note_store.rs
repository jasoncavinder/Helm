use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use crate::models::TaskId;

const MAX_TASK_NOTE_RECORDS: usize = 512;
const MAX_NOTES_PER_TASK: usize = 64;
const MAX_NOTE_BYTES: usize = 16 * 1024;

static TASK_NOTES: OnceLock<Mutex<BTreeMap<u64, Vec<String>>>> = OnceLock::new();

fn task_notes() -> &'static Mutex<BTreeMap<u64, Vec<String>>> {
    TASK_NOTES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn truncate_str_to_head_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }

    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end].to_string()
}

fn normalize_note_lines(note: &str) -> Vec<String> {
    let mut lines: Vec<String> = note
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| truncate_str_to_head_bytes(line, MAX_NOTE_BYTES))
        .collect();

    if lines.is_empty() {
        let trimmed = note.trim();
        if !trimmed.is_empty() {
            lines.push(truncate_str_to_head_bytes(trimmed, MAX_NOTE_BYTES));
        }
    }

    lines
}

pub fn append(task_id: TaskId, note: &str) {
    let lines = normalize_note_lines(note);
    if lines.is_empty() {
        return;
    }

    if let Ok(mut notes) = task_notes().lock() {
        if !notes.contains_key(&task_id.0) && notes.len() >= MAX_TASK_NOTE_RECORDS {
            let oldest = notes.keys().next().copied();
            if let Some(oldest) = oldest {
                notes.remove(&oldest);
            }
        }

        let entry = notes.entry(task_id.0).or_default();
        for line in lines {
            if entry.len() >= MAX_NOTES_PER_TASK {
                entry.remove(0);
            }
            entry.push(line);
        }
    }
}

pub fn drain(task_id: TaskId) -> Vec<String> {
    if let Ok(mut notes) = task_notes().lock() {
        return notes.remove(&task_id.0).unwrap_or_default();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::{MAX_NOTE_BYTES, append, drain};
    use crate::models::TaskId;

    #[test]
    fn append_splits_multiline_notes_and_drain_returns_then_clears() {
        let task_id = TaskId(91001);
        append(
            task_id,
            "Helm full-cleanup: removed '/tmp/a'.\n\nHelm full-cleanup: removed '/tmp/b'.",
        );
        let notes = drain(task_id);
        assert_eq!(
            notes,
            vec![
                "Helm full-cleanup: removed '/tmp/a'.".to_string(),
                "Helm full-cleanup: removed '/tmp/b'.".to_string(),
            ]
        );
        assert!(drain(task_id).is_empty());
    }

    #[test]
    fn append_truncates_overlong_lines() {
        let task_id = TaskId(91002);
        let long = "x".repeat(MAX_NOTE_BYTES + 128);
        append(task_id, long.as_str());
        let notes = drain(task_id);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].len(), MAX_NOTE_BYTES);
    }
}
