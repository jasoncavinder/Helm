use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use tokio::sync::{Mutex, Notify};
use tokio::task::AbortHandle;
use tokio::time::timeout;

use crate::models::{CoreError, CoreErrorKind, ManagerId, TaskId, TaskStatus, TaskType};
use crate::orchestration::{CancellationMode, OrchestrationResult, TaskSubmission};

pub type TaskOperation = Box<
    dyn FnOnce(
            TaskCancellationToken,
        ) -> Pin<Box<dyn Future<Output = OrchestrationResult<()>> + Send>>
        + Send,
>;

#[derive(Clone, Debug)]
pub struct TaskCancellationToken {
    flag: Arc<AtomicBool>,
}

impl TaskCancellationToken {
    fn new(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRuntimeSnapshot {
    pub id: TaskId,
    pub manager: ManagerId,
    pub task_type: TaskType,
    pub status: TaskStatus,
    pub created_at: SystemTime,
    pub started_at: Option<SystemTime>,
    pub finished_at: Option<SystemTime>,
    pub error_message: Option<String>,
}

#[derive(Clone, Default)]
pub struct InMemoryAsyncTaskQueue {
    inner: Arc<Mutex<QueueState>>,
}

#[derive(Default)]
struct QueueState {
    next_task_id: u64,
    tasks: HashMap<TaskId, TaskRuntimeSnapshot>,
    manager_locks: HashMap<ManagerId, Arc<Mutex<()>>>,
    cancellation_flags: HashMap<TaskId, Arc<AtomicBool>>,
    abort_handles: HashMap<TaskId, AbortHandle>,
    completion_notifiers: HashMap<TaskId, Arc<Notify>>,
}

impl InMemoryAsyncTaskQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn spawn(
        &self,
        submission: TaskSubmission,
        operation: TaskOperation,
    ) -> OrchestrationResult<TaskId> {
        let (task_id, manager_lock, cancel_flag, completion_notify) = {
            let mut state = self.inner.lock().await;
            let task_id = TaskId(state.next_task_id);
            state.next_task_id = state.next_task_id.saturating_add(1);

            state.tasks.insert(
                task_id,
                TaskRuntimeSnapshot {
                    id: task_id,
                    manager: submission.manager,
                    task_type: submission.task_type,
                    status: TaskStatus::Queued,
                    created_at: submission.requested_at,
                    started_at: None,
                    finished_at: None,
                    error_message: None,
                },
            );

            let manager_lock = state
                .manager_locks
                .entry(submission.manager)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone();
            let cancel_flag = Arc::new(AtomicBool::new(false));
            let completion_notify = Arc::new(Notify::new());

            state
                .cancellation_flags
                .insert(task_id, cancel_flag.clone());
            state
                .completion_notifiers
                .insert(task_id, completion_notify.clone());

            (task_id, manager_lock, cancel_flag, completion_notify)
        };

        let inner = self.inner.clone();
        let token = TaskCancellationToken::new(cancel_flag);
        let join_handle = tokio::spawn(async move {
            let _manager_guard = manager_lock.lock().await;

            if !set_running_if_possible(&inner, task_id).await {
                finalize_cleanup(&inner, task_id, &completion_notify).await;
                return;
            }

            if token.is_cancelled() {
                set_cancelled_terminal(&inner, task_id, None).await;
                finalize_cleanup(&inner, task_id, &completion_notify).await;
                return;
            }

            let outcome = operation(token.clone()).await;
            match outcome {
                Ok(()) => {
                    if token.is_cancelled() {
                        set_cancelled_terminal(&inner, task_id, None).await;
                    } else {
                        set_terminal(&inner, task_id, TaskStatus::Completed, None).await;
                    }
                }
                Err(error) => {
                    if token.is_cancelled() || error.kind == CoreErrorKind::Cancelled {
                        set_cancelled_terminal(&inner, task_id, Some(error.message)).await;
                    } else {
                        set_terminal(&inner, task_id, TaskStatus::Failed, Some(error.message))
                            .await;
                    }
                }
            }

            finalize_cleanup(&inner, task_id, &completion_notify).await;
        });

        let abort_handle = join_handle.abort_handle();
        let mut state = self.inner.lock().await;
        state.abort_handles.insert(task_id, abort_handle);

        Ok(task_id)
    }

    pub async fn snapshot(&self, task_id: TaskId) -> OrchestrationResult<TaskRuntimeSnapshot> {
        let state = self.inner.lock().await;
        state
            .tasks
            .get(&task_id)
            .cloned()
            .ok_or_else(|| task_lookup_error(task_id))
    }

    pub async fn status(&self, task_id: TaskId) -> OrchestrationResult<TaskStatus> {
        Ok(self.snapshot(task_id).await?.status)
    }

    pub async fn cancel(&self, task_id: TaskId, mode: CancellationMode) -> OrchestrationResult<()> {
        let (abort_handle, notify, prior_status) = {
            let mut state = self.inner.lock().await;
            let prior_status = state
                .tasks
                .get(&task_id)
                .map(|task| task.status)
                .ok_or_else(|| task_lookup_error(task_id))?;
            if is_terminal(prior_status) {
                return Ok(());
            }

            let cancel_flag = state
                .cancellation_flags
                .get(&task_id)
                .ok_or_else(|| task_lookup_error(task_id))?
                .clone();
            cancel_flag.store(true, Ordering::SeqCst);

            if prior_status == TaskStatus::Queued
                && let Some(task) = state.tasks.get_mut(&task_id)
            {
                task.status = TaskStatus::Cancelled;
                task.finished_at = Some(SystemTime::now());
            }

            let abort_handle = state.abort_handles.get(&task_id).cloned();
            let notify = state
                .completion_notifiers
                .get(&task_id)
                .cloned()
                .ok_or_else(|| task_lookup_error(task_id))?;

            (abort_handle, notify, prior_status)
        };

        let mut force_cancelled_state = false;
        match mode {
            CancellationMode::Immediate => {
                if let Some(handle) = abort_handle.clone() {
                    handle.abort();
                }
                force_cancelled_state = true;
            }
            CancellationMode::Graceful { grace_period } => {
                if prior_status == TaskStatus::Running {
                    let wait = notify.notified();
                    if timeout(grace_period, wait).await.is_err()
                        && let Some(handle) = abort_handle.clone()
                    {
                        handle.abort();
                        force_cancelled_state = true;
                    }
                } else if prior_status == TaskStatus::Queued {
                    if let Some(handle) = abort_handle.clone() {
                        handle.abort();
                    }
                    force_cancelled_state = true;
                }
            }
        }

        if force_cancelled_state {
            self.force_mark_cancelled(task_id).await;
        }

        Ok(())
    }

    pub async fn wait_for_terminal(
        &self,
        task_id: TaskId,
        timeout_duration: Option<std::time::Duration>,
    ) -> OrchestrationResult<TaskRuntimeSnapshot> {
        loop {
            let (snapshot, notify) = {
                let state = self.inner.lock().await;
                let snapshot = state
                    .tasks
                    .get(&task_id)
                    .cloned()
                    .ok_or_else(|| task_lookup_error(task_id))?;
                let notify = state
                    .completion_notifiers
                    .get(&task_id)
                    .cloned()
                    .ok_or_else(|| task_lookup_error(task_id))?;
                (snapshot, notify)
            };

            if is_terminal(snapshot.status) {
                return Ok(snapshot);
            }

            if let Some(duration) = timeout_duration {
                timeout(duration, notify.notified())
                    .await
                    .map_err(|_| CoreError {
                        manager: Some(snapshot.manager),
                        task: Some(snapshot.task_type),
                        action: None,
                        kind: CoreErrorKind::Timeout,
                        message: format!("timed out waiting for task '{}' to complete", task_id.0),
                    })?;
            } else {
                notify.notified().await;
            }
        }
    }
}

impl InMemoryAsyncTaskQueue {
    async fn force_mark_cancelled(&self, task_id: TaskId) {
        let notify = {
            let mut state = self.inner.lock().await;
            if let Some(task) = state.tasks.get_mut(&task_id)
                && !is_terminal(task.status)
            {
                task.status = TaskStatus::Cancelled;
                task.finished_at = Some(SystemTime::now());
            }

            state.abort_handles.remove(&task_id);
            state.cancellation_flags.remove(&task_id);
            state.completion_notifiers.get(&task_id).cloned()
        };

        if let Some(notify) = notify {
            notify.notify_waiters();
        }
    }
}

async fn set_running_if_possible(inner: &Arc<Mutex<QueueState>>, task_id: TaskId) -> bool {
    let mut state = inner.lock().await;
    let Some(task) = state.tasks.get_mut(&task_id) else {
        return false;
    };
    if is_terminal(task.status) {
        return false;
    }
    task.status = TaskStatus::Running;
    task.started_at = Some(SystemTime::now());
    true
}

async fn set_terminal(
    inner: &Arc<Mutex<QueueState>>,
    task_id: TaskId,
    status: TaskStatus,
    error_message: Option<String>,
) {
    let mut state = inner.lock().await;
    if let Some(task) = state.tasks.get_mut(&task_id) {
        task.status = status;
        task.finished_at = Some(SystemTime::now());
        task.error_message = error_message;
    }
}

async fn set_cancelled_terminal(
    inner: &Arc<Mutex<QueueState>>,
    task_id: TaskId,
    error_message: Option<String>,
) {
    set_terminal(inner, task_id, TaskStatus::Cancelled, error_message).await;
}

async fn finalize_cleanup(inner: &Arc<Mutex<QueueState>>, task_id: TaskId, notify: &Arc<Notify>) {
    {
        let mut state = inner.lock().await;
        state.abort_handles.remove(&task_id);
        state.cancellation_flags.remove(&task_id);
    }
    notify.notify_waiters();
}

fn is_terminal(status: TaskStatus) -> bool {
    matches!(
        status,
        TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed
    )
}

fn task_lookup_error(task_id: TaskId) -> CoreError {
    CoreError {
        manager: None,
        task: None,
        action: None,
        kind: CoreErrorKind::InvalidInput,
        message: format!("unknown task id '{}'", task_id.0),
    }
}
