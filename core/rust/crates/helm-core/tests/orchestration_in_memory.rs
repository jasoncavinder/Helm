use std::time::{Duration, SystemTime, UNIX_EPOCH};

use helm_core::models::{CoreErrorKind, ManagerId, TaskStatus, TaskType};
use helm_core::orchestration::{
    CancellationMode, InMemoryTaskCoordinator, TaskCoordinator, TaskSubmission,
};

fn submission(manager: ManagerId, task_type: TaskType, seconds: u64) -> TaskSubmission {
    TaskSubmission {
        manager,
        task_type,
        requested_at: UNIX_EPOCH + Duration::from_secs(seconds),
    }
}

#[test]
fn serial_per_manager_allows_only_one_running_task_per_manager() {
    let coordinator = InMemoryTaskCoordinator::new();
    let first = coordinator
        .enqueue(submission(ManagerId::Npm, TaskType::Refresh, 1))
        .unwrap();
    let second = coordinator
        .enqueue(submission(ManagerId::Npm, TaskType::Search, 2))
        .unwrap();

    coordinator.start(first.id).unwrap();
    let error = coordinator.start(second.id).unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::InvalidInput);

    coordinator.complete(first.id).unwrap();
    coordinator.start(second.id).unwrap();
    assert_eq!(coordinator.status(second.id).unwrap(), TaskStatus::Running);
}

#[test]
fn different_managers_can_run_in_parallel() {
    let coordinator = InMemoryTaskCoordinator::new();
    let npm = coordinator
        .enqueue(submission(ManagerId::Npm, TaskType::Refresh, 1))
        .unwrap();
    let pip = coordinator
        .enqueue(submission(ManagerId::Pip, TaskType::Refresh, 2))
        .unwrap();

    coordinator.start(npm.id).unwrap();
    coordinator.start(pip.id).unwrap();

    assert_eq!(coordinator.status(npm.id).unwrap(), TaskStatus::Running);
    assert_eq!(coordinator.status(pip.id).unwrap(), TaskStatus::Running);
}

#[test]
fn cancellation_state_transitions_are_enforced() {
    let coordinator = InMemoryTaskCoordinator::new();
    let queued = coordinator
        .enqueue(submission(ManagerId::Cargo, TaskType::Install, 1))
        .unwrap();
    coordinator
        .cancel(queued.id, CancellationMode::Immediate)
        .unwrap();
    assert_eq!(
        coordinator.status(queued.id).unwrap(),
        TaskStatus::Cancelled
    );

    let running = coordinator
        .enqueue(submission(ManagerId::Pnpm, TaskType::Upgrade, 2))
        .unwrap();
    coordinator.start(running.id).unwrap();
    coordinator
        .cancel(
            running.id,
            CancellationMode::Graceful {
                grace_period: Duration::from_millis(250),
            },
        )
        .unwrap();
    assert_eq!(
        coordinator.status(running.id).unwrap(),
        TaskStatus::Cancelled
    );

    let error = coordinator.complete(running.id).unwrap_err();
    assert_eq!(error.kind, CoreErrorKind::InvalidInput);
}

#[test]
fn enqueue_uses_submission_timestamp_as_creation_time() {
    let coordinator = InMemoryTaskCoordinator::new();
    let requested = UNIX_EPOCH + Duration::from_secs(42);
    let record = coordinator
        .enqueue(TaskSubmission {
            manager: ManagerId::Npm,
            task_type: TaskType::Detection,
            requested_at: requested,
        })
        .unwrap();

    assert_eq!(record.status, TaskStatus::Queued);
    assert_eq!(record.created_at, requested);
    assert!(record.created_at <= SystemTime::now());
}
