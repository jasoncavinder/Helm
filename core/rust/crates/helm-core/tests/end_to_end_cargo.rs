use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::time::{Duration, SystemTime};

use helm_core::adapters::cargo::CargoAdapter;
use helm_core::adapters::cargo_process::ProcessCargoSource;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
};
use helm_core::execution::{
    ExecutionResult, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    ProcessTerminationMode, ProcessWaitFuture, RunningProcess,
};
use helm_core::models::{ManagerId, PackageRef, SearchQuery, TaskStatus};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const VERSION_FIXTURE: &str = include_str!("fixtures/cargo/version.txt");
const INSTALLED_FIXTURE: &str = include_str!("fixtures/cargo/install_list.txt");
const SEARCH_FIXTURE: &str = include_str!("fixtures/cargo/search.txt");
const PUBLISHED_MANIFEST: &str = "[package]\nname = 'bat'\nversion = '0.25.0'\n";
const PUBLISHED_LOCK: &str = "version = 4\n[[package]]\nname = 'bat'\nversion = '0.25.0'\n";
const INSTALL_MANIFEST: &str = "[package]\nname = 'rargs'\nversion = '0.3.0'\n";
const INSTALL_LOCK: &str = "version = 4\n[[package]]\nname = 'rargs'\nversion = '0.3.0'\n";

fn installed_fixture_with_bat_version(version: &str) -> String {
    INSTALLED_FIXTURE.replace("bat v0.24.0:", &format!("bat v{version}:"))
}

struct CargoFakeExecutor {
    bat_upgraded: AtomicBool,
    rargs_installed: AtomicBool,
    ripgrep_removed: AtomicBool,
}

impl CargoFakeExecutor {
    fn new() -> Self {
        Self {
            bat_upgraded: AtomicBool::new(false),
            rargs_installed: AtomicBool::new(false),
            ripgrep_removed: AtomicBool::new(false),
        }
    }
}

struct FakeProcess {
    output: ProcessOutput,
}

impl RunningProcess for FakeProcess {
    fn pid(&self) -> Option<u32> {
        Some(9901)
    }

    fn terminate(&self, _mode: ProcessTerminationMode) -> ExecutionResult<()> {
        Ok(())
    }

    fn wait(self: Box<Self>) -> ProcessWaitFuture {
        let output = self.output;
        Box::pin(async move { Ok(output) })
    }
}

impl ProcessExecutor for CargoFakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let now = SystemTime::now();
        let program = request.command.program.to_string_lossy().to_string();
        let mut args = request.command.args.clone();
        let root = args.iter().position(|arg| arg == "--root").map(|index| {
            let root = PathBuf::from(&args[index + 1]);
            args.drain(index..index + 2);
            root
        });

        let stdout: Vec<u8> = if program.ends_with("which") {
            b"/Users/test/.cargo/bin/cargo".to_vec()
        } else if program == "/usr/bin/tar" {
            assert_eq!(request.private_output_limit, Some(4 * 1024 * 1024));
            match args.last().map(String::as_str) {
                Some("bat-0.25.0/Cargo.toml") => PUBLISHED_MANIFEST.as_bytes().to_vec(),
                Some("bat-0.25.0/Cargo.lock") => PUBLISHED_LOCK.as_bytes().to_vec(),
                Some("rargs-0.3.0/Cargo.toml") => INSTALL_MANIFEST.as_bytes().to_vec(),
                Some("rargs-0.3.0/Cargo.lock") => INSTALL_LOCK.as_bytes().to_vec(),
                _ => panic!("unexpected archive read: {args:?}"),
            }
        } else if program == "cargo" || program.ends_with("/cargo") {
            match args.as_slice() {
                [command, registry, source, color, never, spec]
                    if command == "info"
                        && registry == "--registry"
                        && source == "crates-io"
                        && color == "--color"
                        && never == "never"
                        && (spec == "bat@0.25.0" || spec == "rargs@0.3.0") =>
                {
                    assert_eq!(request.command.working_dir, Some(PathBuf::from("/")));
                    assert!(request.private_output_limit.is_some());
                    Vec::new()
                }
                [arg] if arg == "--version" => VERSION_FIXTURE.as_bytes().to_vec(),
                [arg0, arg1] if arg0 == "install" && arg1 == "--list" => {
                    let mut installed = if self.bat_upgraded.load(Ordering::SeqCst) {
                        installed_fixture_with_bat_version("0.25.0")
                    } else {
                        INSTALLED_FIXTURE.to_string()
                    };
                    if self.rargs_installed.load(Ordering::SeqCst) {
                        installed.push_str("\nrargs v0.3.0:\n    rargs\n");
                    }
                    if self.ripgrep_removed.load(Ordering::SeqCst) {
                        installed = installed
                            .lines()
                            .filter(|line| !line.starts_with("ripgrep v") && line.trim() != "rg")
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                    installed.into_bytes()
                }
                [arg0, arg1, arg2, arg3, arg4, query]
                    if arg0 == "search"
                        && arg1 == "--limit"
                        && arg2 == "20"
                        && arg3 == "--color"
                        && arg4 == "never"
                        && query == "rip" =>
                {
                    SEARCH_FIXTURE.as_bytes().to_vec()
                }
                [arg0, arg1, arg2, arg3, arg4, crate_name]
                    if arg0 == "search"
                        && arg1 == "--limit"
                        && arg2 == "1"
                        && arg3 == "--color"
                        && arg4 == "never" =>
                {
                    match crate_name.as_str() {
                        "bat" => b"bat = \"0.25.0\" # a cat clone with wings\n".to_vec(),
                        "zellij" => b"zellij = \"0.42.1\" # terminal workspace\n".to_vec(),
                        "ripgrep" => b"ripgrep = \"14.1.1\" # search tool\n".to_vec(),
                        _ => Vec::new(),
                    }
                }
                [command, limit, one, color, never, registry, source, name]
                    if command == "search"
                        && limit == "--limit"
                        && one == "1"
                        && color == "--color"
                        && never == "never"
                        && registry == "--registry"
                        && source == "crates-io"
                        && name == "rargs" =>
                {
                    b"rargs = \"0.3.0\" # argument parser\n".to_vec()
                }
                [arg0, crate_name, ..] if arg0 == "install" && crate_name == "rargs" => {
                    assert!(args.iter().any(|arg| arg == "--locked"));
                    assert!(!args.iter().any(|arg| arg == "--force"));
                    assert!(args.windows(2).any(|pair| pair == ["--version", "0.3.0"]));
                    assert!(
                        args.windows(2)
                            .any(|pair| pair == ["--registry", "crates-io"])
                    );
                    assert!(args.windows(2).any(|pair| pair == ["--profile", "release"]));
                    let root = root.as_ref().unwrap();
                    let path = root.join(".crates2.json");
                    let mut receipts: serde_json::Value = std::fs::read(&path)
                        .ok()
                        .map(|bytes| serde_json::from_slice(&bytes).unwrap())
                        .unwrap_or_else(|| serde_json::json!({"installs":{}}));
                    receipts["installs"]["rargs 0.3.0 (registry+https://github.com/rust-lang/crates.io-index)"] = serde_json::json!({
                        "version_req":"=0.3.0", "bins":["rargs"], "features":[], "all_features":false,
                        "no_default_features":false, "profile":"release", "target":"aarch64-apple-darwin", "rustc":"rustc 1.98.1"
                    });
                    std::fs::create_dir_all(root.join("bin")).unwrap();
                    std::fs::write(root.join("bin/rargs"), "installed rargs").unwrap();
                    std::fs::write(path, receipts.to_string()).unwrap();
                    self.rargs_installed.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                [arg0, crate_name] if arg0 == "uninstall" && crate_name == "ripgrep" => {
                    self.ripgrep_removed.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                [arg0, arg1, crate_name, flag, version, ..]
                    if arg0 == "install"
                        && arg1 == "--force"
                        && crate_name == "bat"
                        && flag == "--version"
                        && version == "0.25.0" =>
                {
                    let root = root.as_ref().expect("upgrade binds its installation root");
                    let path = root.join(".crates2.json");
                    let receipt = std::fs::read_to_string(&path).unwrap();
                    std::fs::write(path, receipt.replace("0.24.0", "0.25.0")).unwrap();
                    assert!(args.windows(2).any(|pair| pair == ["--bin", "bat"]));
                    assert!(args.iter().any(|arg| arg == "--locked"));
                    assert!(
                        args.windows(2)
                            .any(|pair| pair == ["--registry", "crates-io"])
                    );
                    self.bat_upgraded.store(true, Ordering::SeqCst);
                    Vec::new()
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };

        Ok(Box::new(FakeProcess {
            output: ProcessOutput {
                status: ProcessExitStatus::ExitCode(0),
                stdout,
                stderr: Vec::new(),
                started_at: now,
                finished_at: now,
            },
        }))
    }
}

fn build_runtime(executor: Arc<dyn ProcessExecutor>, root: PathBuf) -> AdapterRuntime {
    let home = root.join("cargo-home");
    let source_path = home.join("registry/src/index.crates.io-1949cf8c6b5b557f/bat-0.25.0");
    let archive_path = home.join("registry/cache/index.crates.io-1949cf8c6b5b557f");
    std::fs::create_dir_all(&source_path).unwrap();
    std::fs::create_dir_all(&archive_path).unwrap();
    std::fs::write(source_path.join("Cargo.toml"), PUBLISHED_MANIFEST).unwrap();
    std::fs::write(source_path.join("Cargo.lock"), PUBLISHED_LOCK).unwrap();
    std::fs::write(
        archive_path.join("bat-0.25.0.crate"),
        b"fake executor archive",
    )
    .unwrap();
    let install_source = home.join("registry/src/index.crates.io-1949cf8c6b5b557f/rargs-0.3.0");
    std::fs::create_dir_all(&install_source).unwrap();
    std::fs::write(install_source.join("Cargo.toml"), INSTALL_MANIFEST).unwrap();
    std::fs::write(install_source.join("Cargo.lock"), INSTALL_LOCK).unwrap();
    std::fs::write(
        archive_path.join("rargs-0.3.0.crate"),
        b"fake executor archive",
    )
    .unwrap();
    let source = ProcessCargoSource::with_installation_scope(executor, root, home);
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(CargoAdapter::new(source));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

struct UnavailableCargoExecutor {
    failure: AtomicU8,
}

impl ProcessExecutor for UnavailableCargoExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let program = request.command.program.to_string_lossy();
        assert!(
            program.ends_with("which")
                || (program.ends_with("cargo")
                    && (request.command.args == ["--version"]
                        || request.command.args == ["install", "--list"]
                        || request
                            .command
                            .args
                            .first()
                            .is_some_and(|arg| arg == "search"))),
            "read-only recovery must not invoke repair or another installer: {:?}",
            request.command
        );
        let failure = self.failure.load(Ordering::SeqCst);
        if program.ends_with("cargo") && request.command.args == ["--version"] && failure != 0 {
            let now = SystemTime::now();
            let stderr = match failure {
                1 => {
                    "the 'cargo' binary, normally provided by the 'cargo' component, is not applicable to the 'stable-aarch64-apple-darwin' toolchain"
                }
                2 => "",
                3 => "ordinary command failure",
                _ => {
                    "error: 'cargo' is not installed for the toolchain 'stable-aarch64-apple-darwin'"
                }
            };
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(if failure == 2 { 0 } else { 1 }),
                    stdout: Vec::new(),
                    stderr: stderr.as_bytes().to_vec(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }
        CargoFakeExecutor::new().spawn(request)
    }
}

#[tokio::test]
async fn unusable_cargo_proxy_retains_inventory_and_recovers_without_toolchain_changes() {
    use helm_core::adapters::RefreshRequest;
    use helm_core::persistence::PackageStore;
    use helm_core::sqlite::SqliteStore;
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteStore::new(root.path().join("helm.db")));
    store.migrate_to_latest().unwrap();
    let executor = Arc::new(UnavailableCargoExecutor {
        failure: AtomicU8::new(0),
    });
    let adapter: Arc<dyn ManagerAdapter> =
        Arc::new(CargoAdapter::new(ProcessCargoSource::new(executor.clone())));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();
    let mut before = None;
    let mut outdated_before = None;
    for failure in [0, 1, 2, 3, 4, 0] {
        executor.failure.store(failure, Ordering::SeqCst);
        let (task, persistence) = runtime
            .submit_with_persistence(ManagerId::Cargo, AdapterRequest::Refresh(RefreshRequest))
            .await
            .unwrap();
        let snapshot = runtime
            .wait_for_terminal(task, Some(Duration::from_secs(5)))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), persistence.wait_for_completion())
            .await
            .unwrap();
        if failure != 0 {
            let Some(AdapterTaskTerminalState::Failed(error)) = snapshot.terminal_state else {
                panic!("unusable Cargo must fail rather than publishing an empty snapshot");
            };
            if failure == 1 || failure == 4 {
                assert!(error.message.starts_with("[cargo_toolchain_unavailable]"));
                assert!(error.message.contains("stable-aarch64-apple-darwin"));
            } else if failure == 2 {
                assert_eq!(error.kind, helm_core::models::CoreErrorKind::ParseFailure);
            } else {
                assert!(error.message.contains("ordinary command failure"));
                assert!(!error.message.contains("[cargo_toolchain_unavailable]"));
            }
        } else {
            assert!(matches!(
                snapshot.terminal_state,
                Some(AdapterTaskTerminalState::Succeeded(
                    AdapterResponse::SnapshotSync {
                        installed: Some(_),
                        ..
                    }
                ))
            ));
        }
        let installed = store.list_installed().unwrap();
        assert_eq!(installed.len(), 3);
        if let Some(before) = before.as_ref() {
            assert_eq!(before, &installed);
        }
        before = Some(installed);
        let outdated = store.list_outdated().unwrap();
        assert!(!outdated.is_empty());
        if let Some(before) = outdated_before.as_ref() {
            assert_eq!(before, &outdated);
        }
        outdated_before = Some(outdated);
    }
}

#[tokio::test]
async fn cargo_detect_list_search_and_mutate_through_orchestration() {
    let root = tempfile::tempdir().unwrap();
    let (executor, _) = reviewed_scope_source(root.path());
    let runtime = build_runtime(executor, root.path().into());

    assert_cargo_lifecycle(runtime).await;
}

fn seed_installed_bat(root: &std::path::Path) {
    std::fs::create_dir(root.join("bin")).unwrap();
    std::fs::write(root.join("bin/bat"), "fixture").unwrap();
    std::fs::write(root.join(".crates2.json"), serde_json::json!({"installs": {
        "bat 0.24.0 (registry+https://github.com/rust-lang/crates.io-index)": {
            "version_req": "=0.24.0", "bins":["bat"], "features":[], "all_features":false,
            "no_default_features":false, "profile":"release", "target":"aarch64-apple-darwin", "rustc":"rustc 1.98.1"
        }
    }}).to_string()).unwrap();
}

async fn assert_cargo_lifecycle(runtime: AdapterRuntime) {
    let detect_task = runtime
        .submit(ManagerId::Cargo, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("1.84.1"));
            assert!(info.executable_path.unwrap().ends_with("manager/cargo"));
        }
        other => panic!("expected Detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let installed_snapshot = runtime
        .wait_for_terminal(installed_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match installed_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert_eq!(packages.len(), 3);
            assert_eq!(packages[0].package.name, "bat");
            assert_eq!(packages[0].installed_version.as_deref(), Some("0.24.0"));
        }
        other => panic!("expected InstalledPackages response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();
    let outdated_snapshot = runtime
        .wait_for_terminal(outdated_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match outdated_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 2);
            assert_eq!(packages[0].package.name, "bat");
            assert_eq!(packages[0].candidate_version, "0.25.0");
        }
        other => panic!("expected OutdatedPackages response, got {other:?}"),
    }

    let search_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "rip".to_string(),
                    issued_at: SystemTime::now(),
                },
            }),
        )
        .await
        .unwrap();
    let search_snapshot = runtime
        .wait_for_terminal(search_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match search_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::SearchResults(results))) => {
            assert_eq!(results.len(), 2);
            assert_eq!(results[0].result.package.name, "ripgrep");
        }
        other => panic!("expected SearchResults response, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Cargo,
                    name: "rargs".to_string(),
                },
                target_name: None,
                version: None,
            }),
        )
        .await
        .unwrap();
    let install_snapshot = runtime
        .wait_for_terminal(install_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match install_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "rargs");
            assert_eq!(mutation.action, helm_core::models::ManagerAction::Install);
        }
        other => panic!("expected install mutation, got {other:?}"),
    }

    let uninstall_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Cargo,
                    name: "ripgrep".to_string(),
                },
                target_name: None,
                version: None,
            }),
        )
        .await
        .unwrap();
    let uninstall_snapshot = runtime
        .wait_for_terminal(uninstall_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match uninstall_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "ripgrep");
            assert_eq!(mutation.before_version.as_deref(), Some("14.1.1"));
        }
        other => panic!("expected uninstall mutation, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Cargo,
                    name: "bat".to_string(),
                }),
                target_name: None,
                version: None,
            }),
        )
        .await
        .unwrap();
    let upgrade_snapshot = runtime
        .wait_for_terminal(upgrade_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    assert_eq!(upgrade_snapshot.runtime.status, TaskStatus::Completed);
    match upgrade_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "bat");
            assert_eq!(mutation.before_version.as_deref(), Some("0.24.0"));
            assert_eq!(mutation.after_version.as_deref(), Some("0.25.0"));
        }
        other => panic!("expected upgrade mutation, got {other:?}"),
    }
}

struct InvalidPublishedExecutor {
    inner: CargoFakeExecutor,
    home: PathBuf,
    fault: &'static str,
}

impl ProcessExecutor for InvalidPublishedExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let source = self
            .home
            .join("registry/src/index.crates.io-1949cf8c6b5b557f/bat-0.25.0");
        if request
            .command
            .args
            .first()
            .is_some_and(|arg| arg == "info")
        {
            match self.fault {
                "post-install-drift" => {}
                "missing-lock" => std::fs::remove_file(source.join("Cargo.lock")).unwrap(),
                "stale-root" => std::fs::write(
                    source.join("Cargo.lock"),
                    PUBLISHED_LOCK.replace("0.25.0", "0.24.0"),
                )
                .unwrap(),
                "modified-manifest" => std::fs::write(
                    source.join("Cargo.toml"),
                    PUBLISHED_MANIFEST.replace("0.25.0", "0.25.1"),
                )
                .unwrap(),
                "unknown-cache" => std::fs::create_dir_all(
                    self.home.join("registry/src/unknown-layout/bat-0.25.0"),
                )
                .unwrap(),
                "ambiguous-cache" => std::fs::create_dir_all(
                    self.home
                        .join("registry/src/github.com-1ecc6299db9ec823/bat-0.25.0"),
                )
                .unwrap(),
                "linked-lock" => {
                    std::fs::rename(source.join("Cargo.lock"), source.join("elsewhere.lock"))
                        .unwrap();
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(
                        source.join("elsewhere.lock"),
                        source.join("Cargo.lock"),
                    )
                    .unwrap();
                }
                "download-failed" | "download-cancelled" => {
                    return Err(helm_core::models::CoreError {
                        manager: Some(ManagerId::Cargo),
                        task: None,
                        action: Some(helm_core::models::ManagerAction::Upgrade),
                        kind: if self.fault == "download-cancelled" {
                            helm_core::models::CoreErrorKind::Cancelled
                        } else {
                            helm_core::models::CoreErrorKind::ProcessFailure
                        },
                        message: "candidate metadata unavailable".into(),
                    });
                }
                "source-config-changed" => std::fs::write(
                    self.home.join("config.toml"),
                    "[source.crates-io]\nreplace-with = 'private'\n",
                )
                .unwrap(),
                _ => panic!("unknown fault"),
            }
        }
        if self.fault == "stale-root"
            && request.command.program == std::path::Path::new("/usr/bin/tar")
            && request
                .command
                .args
                .last()
                .is_some_and(|arg| arg.ends_with("Cargo.lock"))
        {
            let now = SystemTime::now();
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(0),
                    stdout: PUBLISHED_LOCK.replace("0.25.0", "0.24.0").into_bytes(),
                    stderr: Vec::new(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }
        let installed = self.fault == "post-install-drift"
            && request
                .command
                .args
                .starts_with(&["install".into(), "--force".into()]);
        let result = self.inner.spawn(request);
        if installed {
            std::fs::write(
                source.join("Cargo.lock"),
                PUBLISHED_LOCK.replace("0.25.0", "0.25.1"),
            )
            .unwrap();
        }
        result
    }
}

#[tokio::test]
async fn invalid_published_graph_never_starts_upgrade_or_replaces_existing_binaries() {
    for fault in [
        "missing-lock",
        "stale-root",
        "modified-manifest",
        "unknown-cache",
        "ambiguous-cache",
        "linked-lock",
        "download-failed",
        "download-cancelled",
        "source-config-changed",
    ] {
        let root = tempfile::tempdir().unwrap();
        seed_installed_bat(root.path());
        let before = std::fs::read(root.path().join(".crates2.json")).unwrap();
        let executor = Arc::new(InvalidPublishedExecutor {
            inner: CargoFakeExecutor::new(),
            home: root.path().join("cargo-home"),
            fault,
        });
        let runtime = build_runtime(executor.clone(), root.path().into());
        let task = runtime
            .submit(
                ManagerId::Cargo,
                AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::Cargo,
                        name: "bat".into(),
                    }),
                    target_name: None,
                    version: Some("0.25.0".into()),
                }),
            )
            .await
            .unwrap();
        let snapshot = runtime
            .wait_for_terminal(task, Some(Duration::from_secs(5)))
            .await
            .unwrap();
        assert!(
            !matches!(
                snapshot.terminal_state,
                Some(AdapterTaskTerminalState::Succeeded(_))
            ),
            "{fault}: {snapshot:?}"
        );
        assert!(
            !executor.inner.bat_upgraded.load(Ordering::SeqCst),
            "{fault}"
        );
        assert_eq!(
            std::fs::read(root.path().join(".crates2.json")).unwrap(),
            before,
            "{fault}"
        );
        assert_eq!(
            std::fs::read(root.path().join("bin/bat")).unwrap(),
            b"fixture",
            "{fault}"
        );
    }
}

#[tokio::test]
async fn post_install_cache_drift_reports_unverified_mutation_not_preflight_rejection() {
    let root = tempfile::tempdir().unwrap();
    seed_installed_bat(root.path());
    let executor = Arc::new(InvalidPublishedExecutor {
        inner: CargoFakeExecutor::new(),
        home: root.path().join("cargo-home"),
        fault: "post-install-drift",
    });
    let runtime = build_runtime(executor.clone(), root.path().into());
    let task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Cargo,
                    name: "bat".into(),
                }),
                target_name: None,
                version: Some("0.25.0".into()),
            }),
        )
        .await
        .unwrap();
    let snapshot = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    let Some(AdapterTaskTerminalState::Failed(error)) = snapshot.terminal_state else {
        panic!("changed metadata must not publish a verified mutation");
    };
    assert_eq!(error.kind, helm_core::models::CoreErrorKind::ProcessFailure);
    assert!(error.message.contains("installation may have changed"));
    assert!(executor.inner.bat_upgraded.load(Ordering::SeqCst));
}

struct FreshInstallFaultExecutor {
    inner: Arc<ReviewedScopeExecutor>,
    root: PathBuf,
    fault: &'static str,
}

impl ProcessExecutor for FreshInstallFaultExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let info = request
            .command
            .args
            .first()
            .is_some_and(|arg| arg == "info");
        let install = request
            .command
            .args
            .starts_with(&["install".into(), "rargs".into()]);
        let source = self
            .root
            .join("cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/rargs-0.3.0");
        if info {
            match self.fault {
                "missing-lock" => std::fs::remove_file(source.join("Cargo.lock")).unwrap(),
                "stale-lock" => std::fs::write(
                    source.join("Cargo.lock"),
                    INSTALL_LOCK.replace("0.3.0", "0.2.0"),
                )
                .unwrap(),
                "manifest-drift" => std::fs::write(source.join("Cargo.toml"), "changed").unwrap(),
                "binary-drift" => std::fs::write(self.root.join("bin/bat"), "changed").unwrap(),
                "executable-drift" => {
                    std::fs::write(self.inner.program.lock().unwrap().as_path(), "changed").unwrap()
                }
                "metadata-failure" | "cancelled" => {
                    return Err(helm_core::models::CoreError {
                        manager: Some(ManagerId::Cargo),
                        task: Some(helm_core::models::TaskType::Install),
                        action: Some(helm_core::models::ManagerAction::Install),
                        kind: if self.fault == "cancelled" {
                            helm_core::models::CoreErrorKind::Cancelled
                        } else {
                            helm_core::models::CoreErrorKind::ProcessFailure
                        },
                        message: "controlled metadata failure".into(),
                    });
                }
                _ => {}
            }
        }
        let result = self.inner.spawn(request);
        if install {
            match self.fault {
                "post-lock-drift" => std::fs::write(source.join("Cargo.lock"), "changed").unwrap(),
                "post-binary-drift" => {
                    std::fs::write(self.root.join("bin/bat"), "changed").unwrap()
                }
                "post-executable-drift" => {
                    std::fs::write(self.inner.program.lock().unwrap().as_path(), "changed").unwrap()
                }
                _ => {}
            }
        }
        result
    }
}

#[test]
fn fresh_install_rejects_preflight_drift_and_does_not_retry_after_uncertain_mutation() {
    use helm_core::adapters::cargo::CargoSource;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    for fault in [
        "missing-lock",
        "stale-lock",
        "manifest-drift",
        "binary-drift",
        "executable-drift",
        "metadata-failure",
        "cancelled",
        "post-lock-drift",
        "post-binary-drift",
        "post-executable-drift",
    ] {
        let root = tempfile::tempdir().unwrap();
        let (inner, _) = reviewed_scope_source(root.path());
        let executor = Arc::new(FreshInstallFaultExecutor {
            inner: inner.clone(),
            root: root.path().into(),
            fault,
        });
        let source = ProcessCargoSource::with_installation_scope(
            executor,
            root.path().into(),
            root.path().join("cargo-home"),
        );
        let result = source.install("rargs", Some("0.3.0")).unwrap_err();
        assert_eq!(
            result.action,
            Some(helm_core::models::ManagerAction::Install)
        );
        if fault.starts_with("post-") {
            assert!(inner.inner.rargs_installed.load(Ordering::SeqCst));
            assert_eq!(
                result.kind,
                helm_core::models::CoreErrorKind::ProcessFailure
            );
            assert!(result.message.contains("may have changed"));
        } else {
            assert!(
                !inner.inner.rargs_installed.load(Ordering::SeqCst),
                "{fault}"
            );
            assert!(!root.path().join("bin/rargs").exists());
            if fault == "cancelled" {
                assert_eq!(result.kind, helm_core::models::CoreErrorKind::Cancelled);
            }
        }
    }
}

#[test]
fn fresh_install_pins_native_candidate_and_reinstall_preserves_existing_options() {
    use helm_core::adapters::cargo::CargoSource;
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let root = tempfile::tempdir().unwrap();
    let (executor, source) = reviewed_scope_source(root.path());
    source.install("rargs", None).unwrap();
    assert!(executor.inner.rargs_installed.load(Ordering::SeqCst));
    let before = std::fs::read_to_string(root.path().join(".crates2.json")).unwrap();
    source.install("bat", Some("0.25.0")).unwrap();
    assert!(executor.inner.bat_upgraded.load(Ordering::SeqCst));
    let after = std::fs::read_to_string(root.path().join(".crates2.json")).unwrap();
    assert_eq!(after, before.replace("0.24.0", "0.25.0"));
}

struct RustupBoundExecutor {
    inner: CargoFakeExecutor,
    proxy: PathBuf,
    bound_info: AtomicBool,
    bound_install: AtomicBool,
}

struct ReviewedScopeExecutor {
    inner: CargoFakeExecutor,
    program: std::sync::Mutex<PathBuf>,
    change_on_info: AtomicBool,
    change_after_install: AtomicBool,
}

impl ProcessExecutor for ReviewedScopeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        if request.command.program.ends_with("which") {
            let now = SystemTime::now();
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(0),
                    stdout: self
                        .program
                        .lock()
                        .unwrap()
                        .to_string_lossy()
                        .as_bytes()
                        .to_vec(),
                    stderr: Vec::new(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }
        let info = request
            .command
            .args
            .first()
            .is_some_and(|arg| arg == "info");
        let install = request
            .command
            .args
            .starts_with(&["install".into(), "--force".into()]);
        let result = self.inner.spawn(request);
        if (info && self.change_on_info.load(Ordering::SeqCst))
            || (install && self.change_after_install.load(Ordering::SeqCst))
        {
            std::fs::write(self.program.lock().unwrap().as_path(), "changed executable").unwrap();
        }
        result
    }
}

fn reviewed_scope_source(
    root: &std::path::Path,
) -> (Arc<ReviewedScopeExecutor>, ProcessCargoSource) {
    seed_installed_bat(root);
    let program = root.join("manager/cargo");
    std::fs::create_dir(program.parent().unwrap()).unwrap();
    std::fs::write(&program, "original executable").unwrap();
    let executor = Arc::new(ReviewedScopeExecutor {
        inner: CargoFakeExecutor::new(),
        program: std::sync::Mutex::new(program),
        change_on_info: AtomicBool::new(false),
        change_after_install: AtomicBool::new(false),
    });
    // Seed the packaged candidate/cache fixtures using the existing orchestration fixture.
    let _ = build_runtime(executor.clone(), root.into());
    let source = ProcessCargoSource::with_installation_scope(
        executor.clone(),
        root.into(),
        root.join("cargo-home"),
    );
    (executor, source)
}

#[test]
fn reviewed_scope_upgrade_preserves_the_native_receipt_contract() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    use helm_core::adapters::cargo::CargoSource;
    let root = tempfile::tempdir().unwrap();
    let (executor, source) = reviewed_scope_source(root.path());
    let token = source.review_upgrade_token("bat", "0.25.0").unwrap();
    assert!(token.starts_with("cargo-review-v1:"));
    let payload: serde_json::Value =
        serde_json::from_str(&source.list_outdated().unwrap()).unwrap();
    assert_eq!(payload[0]["package_identifier"], token);
    let result = CargoAdapter::new(source)
        .execute(AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(PackageRef {
                manager: ManagerId::Cargo,
                name: "bat".into(),
            }),
            version: Some("0.25.0".into()),
            target_name: Some(token),
        }))
        .unwrap();
    assert!(matches!(result, AdapterResponse::Mutation(_)));
    assert!(executor.inner.bat_upgraded.load(Ordering::SeqCst));
}

#[test]
fn changed_reviewed_scope_never_starts_the_install() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    use helm_core::adapters::cargo::CargoSource;
    for change in [
        "binary",
        "features",
        "executable",
        "selection",
        "candidate",
        "missing",
        "unavailable",
    ] {
        let root = tempfile::tempdir().unwrap();
        let (executor, source) = reviewed_scope_source(root.path());
        let mut token = source.review_upgrade_token("bat", "0.25.0").unwrap();
        let mut version = "0.25.0";
        match change {
            "binary" => std::fs::write(root.path().join("bin/bat"), "changed binary").unwrap(),
            "features" => {
                let path = root.path().join(".crates2.json");
                let mut receipt: serde_json::Value =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                receipt["installs"]
                    .as_object_mut()
                    .unwrap()
                    .values_mut()
                    .next()
                    .unwrap()["features"] = serde_json::json!(["color"]);
                std::fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
            }
            "executable" => {
                std::fs::write(executor.program.lock().unwrap().as_path(), "changed").unwrap()
            }
            "selection" => {
                let other = root.path().join("other/cargo");
                std::fs::create_dir(other.parent().unwrap()).unwrap();
                std::fs::write(&other, "other").unwrap();
                *executor.program.lock().unwrap() = other;
            }
            "candidate" => version = "0.26.0",
            "missing" => std::fs::remove_file(root.path().join(".crates2.json")).unwrap(),
            "unavailable" => token = helm_core::adapters::cargo_review_scope::UNAVAILABLE.into(),
            _ => unreachable!(),
        }
        assert!(
            source
                .upgrade_reviewed("bat", version, Some(&token))
                .is_err(),
            "{change}"
        );
        assert!(
            !executor.inner.bat_upgraded.load(Ordering::SeqCst),
            "{change}"
        );
    }
}

#[test]
fn scope_drift_during_metadata_download_fails_before_mutation() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    use helm_core::adapters::cargo::CargoSource;
    let root = tempfile::tempdir().unwrap();
    let (executor, source) = reviewed_scope_source(root.path());
    let token = source.review_upgrade_token("bat", "0.25.0").unwrap();
    executor.change_on_info.store(true, Ordering::SeqCst);
    assert!(
        source
            .upgrade_reviewed("bat", "0.25.0", Some(&token))
            .is_err()
    );
    assert!(!executor.inner.bat_upgraded.load(Ordering::SeqCst));
}

#[test]
fn scope_drift_after_install_reports_possible_mutation_not_verified_success() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    use helm_core::adapters::cargo::CargoSource;
    let root = tempfile::tempdir().unwrap();
    let (executor, source) = reviewed_scope_source(root.path());
    let token = source.review_upgrade_token("bat", "0.25.0").unwrap();
    executor.change_after_install.store(true, Ordering::SeqCst);
    let error = source
        .upgrade_reviewed("bat", "0.25.0", Some(&token))
        .unwrap_err();
    assert!(error.message.contains("installation may have changed"));
    assert!(executor.inner.bat_upgraded.load(Ordering::SeqCst));
}

#[test]
fn unrelated_receipt_changes_do_not_invalidate_a_remaining_reviewed_package() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let _entered = runtime.enter();
    let root = tempfile::tempdir().unwrap();
    let (_, source) = reviewed_scope_source(root.path());
    let token = source.review_upgrade_token("bat", "0.25.0").unwrap();
    let path = root.path().join(".crates2.json");
    let mut receipt: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut other = receipt["installs"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .clone();
    other["bins"] = serde_json::json!(["other"]);
    receipt["installs"]["other 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)"] =
        other;
    std::fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
    std::fs::write(root.path().join("bin/other"), "unrelated").unwrap();
    assert_eq!(source.review_upgrade_token("bat", "0.25.0").unwrap(), token);
}

impl ProcessExecutor for RustupBoundExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let program = request.command.program.to_string_lossy();
        if program.ends_with("which") || program.ends_with("rustup") {
            let now = SystemTime::now();
            let stdout = if program.ends_with("which") {
                self.proxy.to_string_lossy().into_owned()
            } else {
                assert_eq!(request.command.args, ["show", "active-toolchain"]);
                assert_eq!(request.command.working_dir, None);
                "stable-aarch64-apple-darwin (directory override for '/project')".into()
            };
            return Ok(Box::new(FakeProcess {
                output: ProcessOutput {
                    status: ProcessExitStatus::ExitCode(0),
                    stdout: stdout.into_bytes(),
                    stderr: Vec::new(),
                    started_at: now,
                    finished_at: now,
                },
            }));
        }
        if program.ends_with("cargo") {
            let info = request
                .command
                .args
                .first()
                .is_some_and(|arg| arg == "info");
            let install = request
                .command
                .args
                .starts_with(&["install".into(), "--force".into()]);
            if info || install {
                assert_eq!(
                    request
                        .command
                        .env
                        .get("RUSTUP_TOOLCHAIN")
                        .map(String::as_str),
                    Some("stable-aarch64-apple-darwin")
                );
                if info {
                    self.bound_info.store(true, Ordering::SeqCst);
                }
                if install {
                    self.bound_install.store(true, Ordering::SeqCst);
                }
            }
        }
        self.inner.spawn(request)
    }
}

#[tokio::test]
async fn rustup_project_selection_survives_metadata_working_directory_change() {
    let root = tempfile::tempdir().unwrap();
    seed_installed_bat(root.path());
    let proxies = root.path().join("proxies");
    std::fs::create_dir(&proxies).unwrap();
    std::fs::write(proxies.join("rustup"), b"rustup proxy").unwrap();
    std::fs::hard_link(proxies.join("rustup"), proxies.join("cargo")).unwrap();
    let executor = Arc::new(RustupBoundExecutor {
        inner: CargoFakeExecutor::new(),
        proxy: proxies.join("cargo"),
        bound_info: AtomicBool::new(false),
        bound_install: AtomicBool::new(false),
    });
    let runtime = build_runtime(executor.clone(), root.path().into());
    let task = runtime
        .submit(
            ManagerId::Cargo,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Cargo,
                    name: "bat".into(),
                }),
                target_name: None,
                version: Some("0.25.0".into()),
            }),
        )
        .await
        .unwrap();
    let snapshot = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    assert!(
        matches!(
            snapshot.terminal_state,
            Some(AdapterTaskTerminalState::Succeeded(_))
        ),
        "{snapshot:?}"
    );
    assert!(executor.bound_info.load(Ordering::SeqCst));
    assert!(executor.bound_install.load(Ordering::SeqCst));
}
