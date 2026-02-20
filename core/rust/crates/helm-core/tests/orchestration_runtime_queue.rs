use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::models::{CoreError, CoreErrorKind, ManagerId, TaskStatus, TaskType};
use helm_core::orchestration::{
    CancellationMode, InMemoryAsyncTaskQueue, OrchestrationResult, TaskCancellationToken,
    TaskOperation, TaskSubmission,
};

fn submission(manager: ManagerId, task_type: TaskType) -> TaskSubmission {
    TaskSubmission {
        manager,
        task_type,
        requested_at: SystemTime::now(),
    }
}

fn operation<F, Fut>(f: F) -> TaskOperation
where
    F: FnOnce(TaskCancellationToken) -> Fut + Send + 'static,
    Fut: Future<Output = OrchestrationResult<()>> + Send + 'static,
{
    Box::new(move |_task_id, token| {
        Box::pin(f(token)) as Pin<Box<dyn Future<Output = OrchestrationResult<()>> + Send>>
    })
}

#[tokio::test]
async fn same_manager_tasks_are_serialized() {
    let queue = InMemoryAsyncTaskQueue::new();
    let current = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    let task1 = queue
        .spawn(
            submission(ManagerId::Npm, TaskType::Refresh),
            operation({
                let current = current.clone();
                let peak = peak.clone();
                move |_| async move {
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            }),
        )
        .await
        .unwrap();

    let task2 = queue
        .spawn(
            submission(ManagerId::Npm, TaskType::Search),
            operation({
                let current = current.clone();
                let peak = peak.clone();
                move |_| async move {
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            }),
        )
        .await
        .unwrap();

    queue
        .wait_for_terminal(task1, Some(Duration::from_secs(1)))
        .await
        .unwrap();
    queue
        .wait_for_terminal(task2, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(peak.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn different_managers_can_run_in_parallel() {
    let queue = InMemoryAsyncTaskQueue::new();
    let current = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));

    let task1 = queue
        .spawn(
            submission(ManagerId::Npm, TaskType::Refresh),
            operation({
                let current = current.clone();
                let peak = peak.clone();
                move |_| async move {
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            }),
        )
        .await
        .unwrap();

    let task2 = queue
        .spawn(
            submission(ManagerId::Pip, TaskType::Refresh),
            operation({
                let current = current.clone();
                let peak = peak.clone();
                move |_| async move {
                    let now = current.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(now, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            }),
        )
        .await
        .unwrap();

    queue
        .wait_for_terminal(task1, Some(Duration::from_secs(1)))
        .await
        .unwrap();
    queue
        .wait_for_terminal(task2, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert!(peak.load(Ordering::SeqCst) >= 2);
}

#[tokio::test]
async fn immediate_cancellation_terminates_running_task_and_unblocks_queue() {
    let queue = InMemoryAsyncTaskQueue::new();
    let second_executed = Arc::new(AtomicBool::new(false));

    let first = queue
        .spawn(
            submission(ManagerId::HomebrewFormula, TaskType::Refresh),
            operation(move |token| async move {
                loop {
                    if token.is_cancelled() {
                        return Err(CoreError {
                            manager: Some(ManagerId::HomebrewFormula),
                            task: Some(TaskType::Refresh),
                            action: None,
                            kind: CoreErrorKind::Cancelled,
                            message: "cancelled".to_string(),
                        });
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }),
        )
        .await
        .unwrap();

    let second = queue
        .spawn(
            submission(ManagerId::HomebrewFormula, TaskType::Search),
            operation({
                let second_executed = second_executed.clone();
                move |_| async move {
                    second_executed.store(true, Ordering::SeqCst);
                    Ok(())
                }
            }),
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(30)).await;
    queue
        .cancel(first, CancellationMode::Immediate)
        .await
        .unwrap();

    let first_snapshot = queue
        .wait_for_terminal(first, Some(Duration::from_secs(1)))
        .await
        .unwrap();
    let second_snapshot = queue
        .wait_for_terminal(second, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(first_snapshot.status, TaskStatus::Cancelled);
    assert_eq!(second_snapshot.status, TaskStatus::Completed);
    assert!(second_executed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn queued_task_can_be_cancelled_before_start() {
    let queue = InMemoryAsyncTaskQueue::new();
    let queued_started = Arc::new(AtomicBool::new(false));

    let first = queue
        .spawn(
            submission(ManagerId::Cargo, TaskType::Refresh),
            operation(move |_| async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                Ok(())
            }),
        )
        .await
        .unwrap();

    let second = queue
        .spawn(
            submission(ManagerId::Cargo, TaskType::Search),
            operation({
                let queued_started = queued_started.clone();
                move |_| async move {
                    queued_started.store(true, Ordering::SeqCst);
                    Ok(())
                }
            }),
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(20)).await;
    queue
        .cancel(second, CancellationMode::Immediate)
        .await
        .unwrap();

    let second_snapshot = queue
        .wait_for_terminal(second, Some(Duration::from_secs(1)))
        .await
        .unwrap();
    queue
        .wait_for_terminal(first, Some(Duration::from_secs(1)))
        .await
        .unwrap();

    assert_eq!(second_snapshot.status, TaskStatus::Cancelled);
    assert!(!queued_started.load(Ordering::SeqCst));
}

#[tokio::test]
async fn graceful_cancel_allows_near_complete_task_to_finish() {
    let queue = InMemoryAsyncTaskQueue::new();
    let task = queue
        .spawn(
            submission(ManagerId::Pnpm, TaskType::Search),
            operation(move |_| async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(())
            }),
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;
    queue
        .cancel(
            task,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(250),
            },
        )
        .await
        .unwrap();

    let snapshot = queue
        .wait_for_terminal(task, Some(Duration::from_secs(1)))
        .await
        .unwrap();
    assert_eq!(snapshot.status, TaskStatus::Completed);
}

#[tokio::test]
async fn graceful_cancel_times_out_and_cancels_long_running_task() {
    let queue = InMemoryAsyncTaskQueue::new();
    let task = queue
        .spawn(
            submission(ManagerId::Yarn, TaskType::Refresh),
            operation(move |_| async move {
                tokio::time::sleep(Duration::from_millis(400)).await;
                Ok(())
            }),
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(10)).await;
    queue
        .cancel(
            task,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(20),
            },
        )
        .await
        .unwrap();

    let snapshot = queue
        .wait_for_terminal(task, Some(Duration::from_secs(1)))
        .await
        .unwrap();
    assert_eq!(snapshot.status, TaskStatus::Cancelled);
}
