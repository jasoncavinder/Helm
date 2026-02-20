use std::cell::Cell;

use crate::models::TaskId;

thread_local! {
    static CURRENT_TASK_ID: Cell<Option<u64>> = const { Cell::new(None) };
}

pub fn current_task_id() -> Option<TaskId> {
    CURRENT_TASK_ID.with(|slot| slot.get().map(TaskId))
}

pub fn with_task_id<R>(task_id: TaskId, operation: impl FnOnce() -> R) -> R {
    CURRENT_TASK_ID.with(|slot| {
        let previous = slot.replace(Some(task_id.0));
        let result = operation();
        slot.set(previous);
        result
    })
}
