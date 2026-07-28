use std::cell::Cell;

use crate::execution::TaskProcessCancellationKey;
use crate::models::TaskId;

thread_local! {
    static CURRENT_TASK_ID: Cell<Option<u64>> = const { Cell::new(None) };
    static CURRENT_PROCESS_CANCELLATION_KEY: Cell<Option<u64>> = const { Cell::new(None) };
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

pub(crate) fn current_process_cancellation_key() -> Option<TaskProcessCancellationKey> {
    CURRENT_PROCESS_CANCELLATION_KEY.with(|slot| slot.get().map(TaskProcessCancellationKey))
}

pub(crate) fn with_task_process_cancellation_key<R>(
    key: TaskProcessCancellationKey,
    operation: impl FnOnce() -> R,
) -> R {
    CURRENT_PROCESS_CANCELLATION_KEY.with(|slot| {
        let previous = slot.replace(Some(key.0));
        let result = operation();
        slot.set(previous);
        result
    })
}
