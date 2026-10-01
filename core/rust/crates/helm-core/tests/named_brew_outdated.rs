use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use helm_core::adapters::docker_desktop::DockerDesktopDetectOutput;
use helm_core::adapters::*;
use helm_core::execution::*;
use helm_core::models::*;
use serde_json::{Value, json};

const MANAGERS: [ManagerId; 3] = [
    ManagerId::Podman,
    ManagerId::Colima,
    ManagerId::DockerDesktop,
];

fn target(manager: ManagerId) -> (&'static str, &'static str, &'static str) {
    match manager {
        ManagerId::Podman => ("--formula", "formulae", "podman"),
        ManagerId::Colima => ("--formula", "formulae", "colima"),
        ManagerId::DockerDesktop => ("--cask", "casks", "docker-desktop"),
        _ => unreachable!(),
    }
}

fn payload(manager: ManagerId) -> Value {
    let (_, collection, name) = target(manager);
    let mut json = json!({"formulae": [], "casks": []});
    json[collection] = json!([{
        "name": name, "installed_versions": ["6.1.2"], "current_version": "6.1.3",
        "pinned": false, "pinned_version": null
    }]);
    json
}

struct Process(ExecutionResult<ProcessOutput>);

impl RunningProcess for Process {
    fn pid(&self) -> Option<u32> {
        None
    }
    fn terminate(&self, _: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }
    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        Box::pin(async move { self.0 })
    }
}

struct Executor {
    result: ExecutionResult<ProcessOutput>,
    info_fails: bool,
    spawn_fails: bool,
    requests: Mutex<Vec<ProcessSpawnRequest>>,
}

fn output(status: ProcessExitStatus, stdout: &str, stderr: &str) -> ProcessOutput {
    ProcessOutput {
        status,
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        started_at: SystemTime::UNIX_EPOCH,
        finished_at: SystemTime::UNIX_EPOCH,
    }
}

impl Executor {
    fn new(status: ProcessExitStatus, stdout: &str, stderr: &str) -> Self {
        Self {
            result: Ok(output(status, stdout, stderr)),
            info_fails: false,
            spawn_fails: false,
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl ProcessExecutor for Executor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        self.requests.lock().unwrap().push(request.clone());
        let args = &request.command.args;
        let stdout = if request.command.program.to_str() == Some("/usr/bin/which") {
            format!("/opt/homebrew/bin/{}\n", args[0])
        } else if args == &["--version"] || args == &["version"] {
            "6.1.2\n".into()
        } else if args.first().is_some_and(|arg| arg == "info") {
            if self.info_fails {
                return Ok(Box::new(Process(Ok(output(
                    ProcessExitStatus::ExitCode(1),
                    "{}",
                    "info failed",
                )))));
            }
            let (_, collection, name) = target(request.manager);
            if request.manager == ManagerId::DockerDesktop {
                json!({"formulae": [], "casks": [{"token": name, "name": ["Docker Desktop"], "installed": "6.1.2"}]}).to_string()
            } else {
                json!({collection: [{"name": name, "installed": [{"version": "6.1.2"}]}]})
                    .to_string()
            }
        } else if args.first().is_some_and(|arg| arg == "outdated") {
            let (flag, _, name) = target(request.manager);
            assert_eq!(args, &["outdated", "--json=v2", flag, name]);
            assert_eq!(request.action, ManagerAction::ListOutdated);
            if self.spawn_fails {
                return Err(self.result.clone().unwrap_err());
            }
            return Ok(Box::new(Process(self.result.clone())));
        } else {
            panic!("unexpected command: {request:?}");
        };
        Ok(Box::new(Process(Ok(output(
            ProcessExitStatus::ExitCode(0),
            &stdout,
            "",
        )))))
    }
}

fn collect(manager: ManagerId, executor: Arc<Executor>) -> AdapterResult<String> {
    match manager {
        ManagerId::Podman => ProcessPodmanSource::new(executor).list_outdated(),
        ManagerId::Colima => ProcessColimaSource::new(executor).list_outdated(),
        ManagerId::DockerDesktop => ProcessDockerDesktopSource::new(executor).list_outdated(),
        _ => unreachable!(),
    }
}

// Native Docker.app discovery is independent of the Homebrew process contract.
// Supply detection only; keep both brew methods on the real process source.
struct DetectedDocker(ProcessDockerDesktopSource);
impl DockerDesktopSource for DetectedDocker {
    fn detect(&self) -> AdapterResult<DockerDesktopDetectOutput> {
        Ok(DockerDesktopDetectOutput {
            executable_path: Some("/Applications/Docker.app".into()),
            version_output: "6.1.2".into(),
        })
    }
    fn homebrew_info(&self) -> AdapterResult<String> {
        self.0.homebrew_info()
    }
    fn list_outdated(&self) -> AdapterResult<String> {
        self.0.list_outdated()
    }
}

fn adapter(manager: ManagerId, executor: Arc<Executor>) -> Box<dyn ManagerAdapter> {
    match manager {
        ManagerId::Podman => Box::new(PodmanAdapter::new(ProcessPodmanSource::new(executor))),
        ManagerId::Colima => Box::new(ColimaAdapter::new(ProcessColimaSource::new(executor))),
        ManagerId::DockerDesktop => Box::new(DockerDesktopAdapter::new(DetectedDocker(
            ProcessDockerDesktopSource::new(executor),
        ))),
        _ => unreachable!(),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn named_brew_exit_one_survives_process_source_and_adapter_refresh_and_list() {
    let runtime = runtime();
    let _guard = runtime.enter();
    for manager in MANAGERS {
        for code in [0, 1] {
            for request in [
                AdapterRequest::Refresh(RefreshRequest),
                AdapterRequest::ListOutdated(ListOutdatedRequest),
            ] {
                let executor = Arc::new(Executor::new(
                    ProcessExitStatus::ExitCode(code),
                    &payload(manager).to_string(),
                    "",
                ));
                let response = adapter(manager, executor.clone()).execute(request).unwrap();
                let packages = match response {
                    AdapterResponse::SnapshotSync {
                        installed,
                        outdated,
                    } => {
                        assert_eq!(installed.unwrap().len(), 1);
                        outdated.unwrap()
                    }
                    AdapterResponse::OutdatedPackages(packages) => packages,
                    _ => panic!("unexpected response"),
                };
                assert_eq!(packages.len(), 1, "{manager:?}");
                assert_eq!(packages[0].package.manager, manager);
                assert_eq!(packages[0].installed_version.as_deref(), Some("6.1.2"));
                assert_eq!(packages[0].candidate_version, "6.1.3");
                assert_eq!(
                    executor
                        .requests
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|r| r.action == ManagerAction::ListOutdated)
                        .count(),
                    1
                );
            }
        }
    }
}

#[test]
fn empty_success_remains_current_but_empty_nonzero_remains_a_failure() {
    let runtime = runtime();
    let _guard = runtime.enter();
    for manager in MANAGERS {
        let empty = r#"{"formulae":[],"casks":[]}"#;
        let executor = Arc::new(Executor::new(ProcessExitStatus::ExitCode(0), empty, ""));
        let result = adapter(manager, executor)
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();
        assert!(matches!(result, AdapterResponse::OutdatedPackages(p) if p.is_empty()));
        for stdout in [empty, "", "{}", "null", "not JSON", "{\"formulae\":"] {
            let executor = Arc::new(Executor::new(ProcessExitStatus::ExitCode(1), stdout, ""));
            let error = collect(manager, executor).unwrap_err();
            assert_eq!(
                error.kind,
                CoreErrorKind::ProcessFailure,
                "{manager:?}: {stdout}"
            );
            assert!(error.message.contains("process exited with code 1"));
        }
    }
}

#[test]
fn exit_one_requires_exact_collection_name_and_complete_version_evidence() {
    let runtime = runtime();
    let _guard = runtime.enter();
    for manager in MANAGERS {
        let (_, collection, _) = target(manager);
        let other = if collection == "formulae" {
            "casks"
        } else {
            "formulae"
        };
        let valid = payload(manager);
        let mut variants = Vec::new();
        for (field, value) in [
            ("name", json!("unrelated")),
            ("name", Value::Null),
            ("current_version", json!(" ")),
            ("current_version", json!(42)),
            ("installed_versions", json!([])),
            ("installed_versions", json!([""])),
            ("installed_versions", json!(["6.1.2", null])),
            ("installed_versions", json!("6.1.2")),
        ] {
            let mut variant = valid.clone();
            variant[collection][0][field] = value;
            variants.push(variant);
        }
        let mut swapped = valid.clone();
        swapped[other] = swapped[collection].take();
        swapped[collection] = json!([]);
        variants.push(swapped);
        let mut mixed = valid.clone();
        mixed[other] = json!([{"name":"other"}]);
        variants.push(mixed);
        let mut duplicate = valid.clone();
        duplicate[collection]
            .as_array_mut()
            .unwrap()
            .push(valid[collection][0].clone());
        variants.push(duplicate);
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(other);
        variants.push(missing);
        for variant in variants {
            let executor = Arc::new(Executor::new(
                ProcessExitStatus::ExitCode(1),
                &variant.to_string(),
                "",
            ));
            assert_eq!(
                collect(manager, executor).unwrap_err().kind,
                CoreErrorKind::ProcessFailure,
                "{manager:?}: {variant}"
            );
        }
    }
}

#[test]
fn process_failures_diagnostics_signals_and_spawn_wait_errors_are_preserved() {
    let runtime = runtime();
    let _guard = runtime.enter();
    for manager in MANAGERS {
        for status in [
            ProcessExitStatus::ExitCode(1),
            ProcessExitStatus::ExitCode(2),
            ProcessExitStatus::ExitCode(127),
            ProcessExitStatus::Terminated,
        ] {
            let executor = Arc::new(Executor::new(
                status,
                &payload(manager).to_string(),
                "Error: command failed",
            ));
            let error = collect(manager, executor).unwrap_err();
            assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
            assert_eq!(error.manager, Some(manager));
            assert_eq!(error.task, Some(TaskType::Refresh));
            assert_eq!(error.action, Some(ManagerAction::ListOutdated));
            if status != ProcessExitStatus::Terminated {
                assert!(error.message.ends_with("Error: command failed"));
            }
        }
        // A valid payload without stderr must not admit other nonzero exit codes.
        let executor = Arc::new(Executor::new(
            ProcessExitStatus::ExitCode(2),
            &payload(manager).to_string(),
            "",
        ));
        assert!(collect(manager, executor).is_err());
        for kind in [
            CoreErrorKind::Timeout,
            CoreErrorKind::Cancelled,
            CoreErrorKind::ProcessFailure,
        ] {
            for spawn_fails in [false, true] {
                let expected = CoreError {
                    manager: Some(manager),
                    task: Some(TaskType::Refresh),
                    action: Some(ManagerAction::ListOutdated),
                    kind,
                    message: "original error".into(),
                };
                let mut executor = Executor::new(ProcessExitStatus::ExitCode(0), "", "");
                executor.result = Err(expected.clone());
                executor.spawn_fails = spawn_fails;
                assert_eq!(collect(manager, Arc::new(executor)).unwrap_err(), expected);
            }
        }
    }
}

#[test]
fn brew_info_failure_is_not_relaxed_by_the_outdated_exception() {
    let runtime = runtime();
    let _guard = runtime.enter();
    for manager in MANAGERS {
        let mut executor = Executor::new(
            ProcessExitStatus::ExitCode(1),
            &payload(manager).to_string(),
            "",
        );
        executor.info_fails = true;
        let executor = Arc::new(executor);
        let error = adapter(manager, executor.clone())
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
        assert!(error.message.ends_with("info failed"));
        assert!(
            !executor
                .requests
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.action == ManagerAction::ListOutdated)
        );
    }
}
