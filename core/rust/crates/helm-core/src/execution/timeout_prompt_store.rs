use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::models::{ManagerAction, ManagerId, TaskId, TaskType};

const MAX_PROMPTS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeoutPromptDecision {
    Wait,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskTimeoutPromptRecord {
    pub task_id: TaskId,
    pub manager: ManagerId,
    pub task_type: TaskType,
    pub action: ManagerAction,
    pub requested_at_unix_ms: i64,
    pub grace_seconds: u64,
    pub suggested_extension_seconds: u64,
}

#[derive(Clone, Debug)]
struct TimeoutPromptState {
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    requested_at_unix_ms: i64,
    grace_seconds: u64,
    suggested_extension_seconds: u64,
    decision: Option<TimeoutPromptDecision>,
}

static TIMEOUT_PROMPTS: OnceLock<Mutex<BTreeMap<u64, TimeoutPromptState>>> = OnceLock::new();

fn timeout_prompts() -> &'static Mutex<BTreeMap<u64, TimeoutPromptState>> {
    TIMEOUT_PROMPTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn system_time_to_unix_ms(value: SystemTime) -> i64 {
    value.duration_since(UNIX_EPOCH).map_or(0_i64, |duration| {
        i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
    })
}

fn normalize_secs(value: Duration) -> u64 {
    value.as_secs().max(1)
}

fn state_to_record(state: &TimeoutPromptState) -> TaskTimeoutPromptRecord {
    TaskTimeoutPromptRecord {
        task_id: state.task_id,
        manager: state.manager,
        task_type: state.task_type,
        action: state.action,
        requested_at_unix_ms: state.requested_at_unix_ms,
        grace_seconds: state.grace_seconds,
        suggested_extension_seconds: state.suggested_extension_seconds,
    }
}

fn enforce_capacity(map: &mut BTreeMap<u64, TimeoutPromptState>, incoming_task_id: TaskId) {
    if map.contains_key(&incoming_task_id.0) || map.len() < MAX_PROMPTS {
        return;
    }
    if let Some(oldest_task_id) = map.keys().next().copied() {
        map.remove(&oldest_task_id);
    }
}

pub fn upsert_prompt(
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    action: ManagerAction,
    grace_period: Duration,
    suggested_extension: Duration,
) -> TaskTimeoutPromptRecord {
    if let Ok(mut prompts) = timeout_prompts().lock() {
        if let Some(existing) = prompts.get_mut(&task_id.0) {
            existing.manager = manager;
            existing.task_type = task_type;
            existing.action = action;
            existing.grace_seconds = normalize_secs(grace_period);
            existing.suggested_extension_seconds = normalize_secs(suggested_extension);
            return state_to_record(existing);
        }

        enforce_capacity(&mut prompts, task_id);
        let state = TimeoutPromptState {
            task_id,
            manager,
            task_type,
            action,
            requested_at_unix_ms: system_time_to_unix_ms(SystemTime::now()),
            grace_seconds: normalize_secs(grace_period),
            suggested_extension_seconds: normalize_secs(suggested_extension),
            decision: None,
        };
        prompts.insert(task_id.0, state.clone());
        return state_to_record(&state);
    }

    TaskTimeoutPromptRecord {
        task_id,
        manager,
        task_type,
        action,
        requested_at_unix_ms: system_time_to_unix_ms(SystemTime::now()),
        grace_seconds: normalize_secs(grace_period),
        suggested_extension_seconds: normalize_secs(suggested_extension),
    }
}

pub fn list_prompts() -> Vec<TaskTimeoutPromptRecord> {
    timeout_prompts()
        .lock()
        .ok()
        .map(|prompts| prompts.values().map(state_to_record).collect::<Vec<_>>())
        .unwrap_or_default()
}

pub fn respond(task_id: TaskId, decision: TimeoutPromptDecision) -> bool {
    if let Ok(mut prompts) = timeout_prompts().lock()
        && let Some(state) = prompts.get_mut(&task_id.0)
    {
        state.decision = Some(decision);
        return true;
    }
    false
}

pub fn take_decision(task_id: TaskId) -> Option<TimeoutPromptDecision> {
    timeout_prompts().lock().ok().and_then(|mut prompts| {
        prompts
            .get_mut(&task_id.0)
            .and_then(|state| state.decision.take())
    })
}

pub fn clear_prompt(task_id: TaskId) {
    if let Ok(mut prompts) = timeout_prompts().lock() {
        prompts.remove(&task_id.0);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TimeoutPromptDecision, clear_prompt, list_prompts, respond, take_decision, upsert_prompt,
    };
    use crate::models::{ManagerAction, ManagerId, TaskId, TaskType};
    use std::sync::{Mutex, OnceLock};
    use std::time::Duration;

    fn prompt_store_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("timeout prompt test lock should be available")
    }

    #[test]
    fn upsert_list_respond_and_clear_prompt() {
        let _guard = prompt_store_test_lock();
        let task_id = TaskId(777_001);
        clear_prompt(task_id);

        let inserted = upsert_prompt(
            task_id,
            ManagerId::Rustup,
            TaskType::Uninstall,
            ManagerAction::Uninstall,
            Duration::from_secs(30),
            Duration::from_secs(600),
        );
        assert_eq!(inserted.task_id, task_id);
        assert_eq!(inserted.manager, ManagerId::Rustup);
        assert_eq!(inserted.task_type, TaskType::Uninstall);
        assert_eq!(inserted.action, ManagerAction::Uninstall);
        assert_eq!(inserted.grace_seconds, 30);
        assert_eq!(inserted.suggested_extension_seconds, 600);
        assert!(inserted.requested_at_unix_ms > 0);

        let listed = list_prompts();
        assert!(listed.iter().any(|entry| entry.task_id == task_id));

        assert!(respond(task_id, TimeoutPromptDecision::Wait));
        assert_eq!(take_decision(task_id), Some(TimeoutPromptDecision::Wait));
        assert_eq!(take_decision(task_id), None);

        assert!(respond(task_id, TimeoutPromptDecision::Stop));
        assert_eq!(take_decision(task_id), Some(TimeoutPromptDecision::Stop));

        clear_prompt(task_id);
        assert!(!list_prompts().iter().any(|entry| entry.task_id == task_id));
    }
}
