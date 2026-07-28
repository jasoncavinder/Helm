use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use tokio::sync::{Mutex, Notify};
use tokio::task::AbortHandle;
use tokio::time::timeout;

use crate::models::{CoreError, CoreErrorKind, ManagerId, TaskId, TaskStatus, TaskType};
use crate::orchestration::{
    CancellationMode, OrchestrationResult, TaskSubmission, manager_execution_domain,
};

const WAIT_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const WAIT_HEARTBEAT_LOG_EVERY: u64 = 5;
static HOMEBREW_EXECUTION_LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();

pub type TaskOperation = Box<
    dyn FnOnce(
            TaskId,
            TaskCancellationToken,
        ) -> Pin<Box<dyn Future<Output = OrchestrationResult<()>> + Send>>
        + Send,
>;

#[derive(Clone, Debug)]
pub struct TaskCancellationToken {
    flag: Arc<AtomicBool>,
    forced_flag: Arc<AtomicBool>,
    process_cancellation_key: crate::execution::TaskProcessCancellationKey,
}

impl TaskCancellationToken {
    fn new(
        flag: Arc<AtomicBool>,
        forced_flag: Arc<AtomicBool>,
        process_cancellation_key: crate::execution::TaskProcessCancellationKey,
    ) -> Self {
        Self {
            flag,
            forced_flag,
            process_cancellation_key,
        }
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub fn is_forced_cancelled(&self) -> bool {
        self.forced_flag.load(Ordering::SeqCst)
    }

    pub(crate) fn process_cancellation_key(&self) -> crate::execution::TaskProcessCancellationKey {
        self.process_cancellation_key
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
    forced_cancellation_flags: HashMap<TaskId, Arc<AtomicBool>>,
    preserve_execution_lease_on_cancel: HashMap<TaskId, bool>,
    process_cancellation_keys: HashMap<TaskId, crate::execution::TaskProcessCancellationKey>,
    abort_handles: HashMap<TaskId, AbortHandle>,
    completion_notifiers: HashMap<TaskId, Arc<Notify>>,
}

impl InMemoryAsyncTaskQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_initial_id(start: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(QueueState {
                next_task_id: start,
                ..QueueState::default()
            })),
        }
    }

    pub async fn spawn(
        &self,
        submission: TaskSubmission,
        operation: TaskOperation,
    ) -> OrchestrationResult<TaskId> {
        self.spawn_with_cancellation_policy(submission, operation, false)
            .await
    }

    pub(crate) async fn spawn_preserving_execution_lease(
        &self,
        submission: TaskSubmission,
        operation: TaskOperation,
    ) -> OrchestrationResult<TaskId> {
        self.spawn_with_cancellation_policy(submission, operation, true)
            .await
    }

    async fn spawn_with_cancellation_policy(
        &self,
        submission: TaskSubmission,
        operation: TaskOperation,
        preserve_execution_lease_on_cancel: bool,
    ) -> OrchestrationResult<TaskId> {
        let (
            task_id,
            manager_lock,
            cancel_flag,
            forced_cancel_flag,
            process_cancellation_key,
            completion_notify,
        ) = {
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

            let execution_domain = manager_execution_domain(submission.manager);
            let manager_lock = if execution_domain == ManagerId::HomebrewFormula {
                HOMEBREW_EXECUTION_LOCK
                    .get_or_init(|| Arc::new(Mutex::new(())))
                    .clone()
            } else {
                state
                    .manager_locks
                    .entry(execution_domain)
                    .or_insert_with(|| Arc::new(Mutex::new(())))
                    .clone()
            };
            let cancel_flag = Arc::new(AtomicBool::new(false));
            let forced_cancel_flag = Arc::new(AtomicBool::new(false));
            let process_cancellation_key = crate::execution::next_task_process_cancellation_key();
            let completion_notify = Arc::new(Notify::new());

            state
                .cancellation_flags
                .insert(task_id, cancel_flag.clone());
            state
                .forced_cancellation_flags
                .insert(task_id, forced_cancel_flag.clone());
            state
                .preserve_execution_lease_on_cancel
                .insert(task_id, preserve_execution_lease_on_cancel);
            state
                .process_cancellation_keys
                .insert(task_id, process_cancellation_key);
            state
                .completion_notifiers
                .insert(task_id, completion_notify.clone());

            (
                task_id,
                manager_lock,
                cancel_flag,
                forced_cancel_flag,
                process_cancellation_key,
                completion_notify,
            )
        };

        let inner = self.inner.clone();
        let token =
            TaskCancellationToken::new(cancel_flag, forced_cancel_flag, process_cancellation_key);
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

            let outcome = operation(task_id, token.clone()).await;
            match outcome {
                Ok(()) if token.is_forced_cancelled() => {
                    set_cancelled_terminal(&inner, task_id, None).await;
                }
                Ok(()) => {
                    set_terminal(&inner, task_id, TaskStatus::Completed, None).await;
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
        let (
            abort_handle,
            notify,
            preserve_execution_lease_on_cancel,
            process_cancellation_key,
            prior_status,
            manager,
            task_type,
        ) = {
            let mut state = self.inner.lock().await;
            let task = state
                .tasks
                .get(&task_id)
                .ok_or_else(|| task_lookup_error(task_id))?;
            let prior_status = task.status;
            let manager = task.manager;
            let task_type = task.task_type;
            if is_terminal(prior_status) {
                return Ok(());
            }
            let preserve_execution_lease_on_cancel = state
                .preserve_execution_lease_on_cancel
                .get(&task_id)
                .copied()
                .unwrap_or(false);
            let preserve_running_execution_lease =
                prior_status == TaskStatus::Running && preserve_execution_lease_on_cancel;
            let process_cancellation_key = state
                .process_cancellation_keys
                .get(&task_id)
                .copied()
                .ok_or_else(|| task_lookup_error(task_id))?;

            let cancel_flag = state
                .cancellation_flags
                .get(&task_id)
                .ok_or_else(|| task_lookup_error(task_id))?
                .clone();
            cancel_flag.store(true, Ordering::SeqCst);
            let forced_cancel_flag = state
                .forced_cancellation_flags
                .get(&task_id)
                .ok_or_else(|| task_lookup_error(task_id))?
                .clone();

            if prior_status == TaskStatus::Queued || mode == CancellationMode::Immediate {
                forced_cancel_flag.store(true, Ordering::SeqCst);
            }
            if (prior_status == TaskStatus::Queued
                || (mode == CancellationMode::Immediate && !preserve_running_execution_lease))
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
            (
                abort_handle,
                notify,
                preserve_execution_lease_on_cancel,
                process_cancellation_key,
                prior_status,
                manager,
                task_type,
            )
        };

        tracing::debug!(
            task_id = task_id.0,
            manager = ?manager,
            task_type = ?task_type,
            mode = ?mode,
            prior_status = ?prior_status,
            "cancellation requested"
        );

        let preserve_running_execution_lease =
            prior_status == TaskStatus::Running && preserve_execution_lease_on_cancel;
        let mut force_cancelled_state = false;
        match mode {
            CancellationMode::Immediate => {
                if preserve_running_execution_lease {
                    crate::execution::terminate_task_processes(
                        process_cancellation_key,
                        crate::execution::ProcessTerminationMode::Immediate,
                    )?;
                } else if let Some(handle) = abort_handle.clone() {
                    handle.abort();
                }
                force_cancelled_state = !preserve_running_execution_lease;
                tracing::warn!(
                    task_id = task_id.0,
                    manager = ?manager,
                    task_type = ?task_type,
                    cancellation_path = if preserve_running_execution_lease {
                        "immediate_execution_lease_preserved"
                    } else {
                        "immediate_abort"
                    },
                    execution_lease_preserved = preserve_running_execution_lease,
                    "task cancellation forced"
                );
            }
            CancellationMode::Graceful { grace_period } => {
                if prior_status == TaskStatus::Running {
                    let wait = notify.notified();
                    if timeout(grace_period, wait).await.is_err() {
                        if preserve_running_execution_lease {
                            crate::execution::terminate_task_processes(
                                process_cancellation_key,
                                crate::execution::ProcessTerminationMode::Immediate,
                            )?;
                            self.set_forced_cancellation_flag(task_id).await;
                            self.mark_cancelled_while_reaping(task_id).await;
                            tracing::warn!(
                                task_id = task_id.0,
                                manager = ?manager,
                                task_type = ?task_type,
                                cancellation_path = "graceful_timeout_process_termination",
                                grace_period_ms = grace_period.as_millis(),
                                execution_lease_preserved = true,
                                "graceful cancellation timed out; terminating active process before task becomes cancelled"
                            );
                        } else if !self.force_mark_cancelled(task_id).await {
                            tracing::debug!(
                                task_id = task_id.0,
                                manager = ?manager,
                                task_type = ?task_type,
                                cancellation_path = "graceful_terminal_race",
                                "graceful cancellation observed terminal completion before abort"
                            );
                        } else {
                            if let Some(handle) = abort_handle.clone() {
                                handle.abort();
                            }
                            tracing::warn!(
                                task_id = task_id.0,
                                manager = ?manager,
                                task_type = ?task_type,
                                cancellation_path = if preserve_running_execution_lease {
                                    "graceful_timeout_execution_lease_preserved"
                                } else {
                                    "graceful_timeout_abort"
                                },
                                grace_period_ms = grace_period.as_millis(),
                                execution_lease_preserved = preserve_running_execution_lease,
                                "graceful cancellation timed out; forcing cancellation"
                            );
                        }
                    }
                } else if prior_status == TaskStatus::Queued {
                    if let Some(handle) = abort_handle.clone() {
                        handle.abort();
                    }
                    force_cancelled_state = true;
                    tracing::warn!(
                        task_id = task_id.0,
                        manager = ?manager,
                        task_type = ?task_type,
                        cancellation_path = "graceful_queued_abort",
                        "graceful cancellation aborted queued task"
                    );
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
        let wait_started = tokio::time::Instant::now();
        let mut heartbeat_count = 0_u64;
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

            let wait_slice = match timeout_duration {
                Some(limit) => {
                    let elapsed = wait_started.elapsed();
                    let Some(remaining) = limit.checked_sub(elapsed) else {
                        return Err(wait_timeout_error(
                            task_id,
                            snapshot.manager,
                            snapshot.task_type,
                            limit,
                            elapsed,
                        ));
                    };
                    remaining.min(WAIT_HEARTBEAT_INTERVAL)
                }
                None => WAIT_HEARTBEAT_INTERVAL,
            };

            if timeout(wait_slice, notify.notified()).await.is_err() {
                heartbeat_count = heartbeat_count.saturating_add(1);
                let elapsed = wait_started.elapsed();

                if let Some(limit) = timeout_duration
                    && elapsed >= limit
                {
                    return Err(wait_timeout_error(
                        task_id,
                        snapshot.manager,
                        snapshot.task_type,
                        limit,
                        elapsed,
                    ));
                }

                if heartbeat_count.is_multiple_of(WAIT_HEARTBEAT_LOG_EVERY) {
                    tracing::debug!(
                        task_id = task_id.0,
                        manager = ?snapshot.manager,
                        task_type = ?snapshot.task_type,
                        status = ?snapshot.status,
                        elapsed_ms = elapsed.as_millis(),
                        timeout_ms = timeout_duration.map(|value| value.as_millis()),
                        heartbeat_count,
                        "still waiting for task terminal state"
                    );
                }
            }
        }
    }
}

impl InMemoryAsyncTaskQueue {
    async fn set_forced_cancellation_flag(&self, task_id: TaskId) {
        let state = self.inner.lock().await;
        if let Some(flag) = state.forced_cancellation_flags.get(&task_id) {
            flag.store(true, Ordering::SeqCst);
        }
    }

    async fn mark_cancelled_while_reaping(&self, task_id: TaskId) {
        let notify = {
            let mut state = self.inner.lock().await;
            let transitioned = state
                .tasks
                .get(&task_id)
                .map(|task| !is_terminal(task.status))
                .unwrap_or(false);
            if transitioned && let Some(task) = state.tasks.get_mut(&task_id) {
                task.status = TaskStatus::Cancelled;
                task.finished_at = Some(SystemTime::now());
            }
            transitioned
                .then(|| state.completion_notifiers.get(&task_id).cloned())
                .flatten()
        };

        // Keep the execution lease and process-cancellation state until the task exits and reaps.
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
    }

    async fn force_mark_cancelled(&self, task_id: TaskId) -> bool {
        let (notify, process_cancellation_key, transitioned) = {
            let mut state = self.inner.lock().await;
            let transitioned = state
                .tasks
                .get(&task_id)
                .map(|task| !is_terminal(task.status))
                .unwrap_or(false);
            if transitioned && let Some(flag) = state.forced_cancellation_flags.get(&task_id) {
                flag.store(true, Ordering::SeqCst);
            }
            if transitioned && let Some(task) = state.tasks.get_mut(&task_id) {
                task.status = TaskStatus::Cancelled;
                task.finished_at = Some(SystemTime::now());
            }

            state.abort_handles.remove(&task_id);
            state.cancellation_flags.remove(&task_id);
            state.forced_cancellation_flags.remove(&task_id);
            state.preserve_execution_lease_on_cancel.remove(&task_id);
            let process_cancellation_key = state.process_cancellation_keys.remove(&task_id);
            (
                state.completion_notifiers.get(&task_id).cloned(),
                process_cancellation_key,
                transitioned,
            )
        };

        if let Some(key) = process_cancellation_key {
            crate::execution::clear_task_process_cancellation(key);
        }
        if let Some(notify) = notify {
            notify.notify_waiters();
        }
        transitioned
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
    if let Some(task) = state.tasks.get_mut(&task_id)
        && !is_terminal(task.status)
    {
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
    let process_cancellation_key = {
        let mut state = inner.lock().await;
        state.abort_handles.remove(&task_id);
        state.cancellation_flags.remove(&task_id);
        state.forced_cancellation_flags.remove(&task_id);
        state.preserve_execution_lease_on_cancel.remove(&task_id);
        state.process_cancellation_keys.remove(&task_id)
    };
    if let Some(key) = process_cancellation_key {
        crate::execution::clear_task_process_cancellation(key);
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

fn wait_timeout_error(
    task_id: TaskId,
    manager: ManagerId,
    task_type: TaskType,
    timeout_duration: Duration,
    elapsed: Duration,
) -> CoreError {
    CoreError {
        manager: Some(manager),
        task: Some(task_type),
        action: None,
        kind: CoreErrorKind::Timeout,
        message: format!(
            "timed out waiting for task '{}' to complete after {}ms (timeout={}ms)",
            task_id.0,
            elapsed.as_millis(),
            timeout_duration.as_millis()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn forced_cancellation_cannot_be_overwritten_by_late_success() {
        let queue = InMemoryAsyncTaskQueue::new();
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let operation: TaskOperation = {
            let started = started.clone();
            let release = release.clone();
            Box::new(move |_task_id, _token| {
                Box::pin(async move {
                    started.notify_one();
                    release.notified().await;
                    Ok(())
                })
            })
        };
        let task_id = queue
            .spawn(
                TaskSubmission {
                    manager: ManagerId::Npm,
                    task_type: TaskType::Refresh,
                    requested_at: SystemTime::now(),
                },
                operation,
            )
            .await
            .expect("task should queue");
        started.notified().await;

        queue
            .cancel(task_id, CancellationMode::Immediate)
            .await
            .expect("task should cancel");
        set_terminal(&queue.inner, task_id, TaskStatus::Completed, None).await;

        assert_eq!(
            queue.status(task_id).await.expect("status should load"),
            TaskStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn formula_and_cask_share_execution_lock_across_queues() {
        let formula_queue = InMemoryAsyncTaskQueue::new();
        let cask_queue = InMemoryAsyncTaskQueue::new();
        let formula_started = Arc::new(Notify::new());
        let release_formula = Arc::new(Notify::new());
        let cask_started = Arc::new(Notify::new());

        let formula_operation: TaskOperation = {
            let formula_started = formula_started.clone();
            let release_formula = release_formula.clone();
            Box::new(move |_task_id, _token| {
                Box::pin(async move {
                    formula_started.notify_one();
                    release_formula.notified().await;
                    Ok(())
                })
            })
        };
        let formula_task = formula_queue
            .spawn(
                TaskSubmission {
                    manager: ManagerId::HomebrewFormula,
                    task_type: TaskType::Upgrade,
                    requested_at: SystemTime::now(),
                },
                formula_operation,
            )
            .await
            .expect("formula task should queue");
        formula_started.notified().await;

        let cask_operation: TaskOperation = {
            let cask_started = cask_started.clone();
            Box::new(move |_task_id, _token| {
                Box::pin(async move {
                    cask_started.notify_one();
                    Ok(())
                })
            })
        };
        let cask_task = cask_queue
            .spawn(
                TaskSubmission {
                    manager: ManagerId::HomebrewCask,
                    task_type: TaskType::Upgrade,
                    requested_at: SystemTime::now(),
                },
                cask_operation,
            )
            .await
            .expect("cask task should queue");

        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(
            cask_queue
                .status(cask_task)
                .await
                .expect("cask status should load"),
            TaskStatus::Queued
        );

        release_formula.notify_one();
        assert_eq!(
            formula_queue
                .wait_for_terminal(formula_task, Some(Duration::from_secs(1)))
                .await
                .expect("formula task should complete")
                .status,
            TaskStatus::Completed
        );
        cask_started.notified().await;
        assert_eq!(
            cask_queue
                .wait_for_terminal(cask_task, Some(Duration::from_secs(1)))
                .await
                .expect("cask task should complete")
                .status,
            TaskStatus::Completed
        );
    }
}
