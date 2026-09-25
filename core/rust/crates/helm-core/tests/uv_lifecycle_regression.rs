use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use helm_core::adapters::manager::*;
use helm_core::adapters::uv_tool_runtime::UvToolAdapter;
use helm_core::execution::*;
use helm_core::models::*;
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};
use helm_core::persistence::PackageStore;
use helm_core::sqlite::SqliteStore;

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

#[derive(Clone, Copy)]
enum Behavior {
    Normal,
    EquivalentVersion,
    Unchanged,
    Failed,
    Cancelled,
    InvalidResolution,
    ReceiptDrift,
}

struct FakeExecutor {
    root: PathBuf,
    behavior: Behavior,
    version: Mutex<Option<String>>,
    commands: Mutex<Vec<ProcessSpawnRequest>>,
}

impl ProcessExecutor for FakeExecutor {
    fn spawn(&self, request: ProcessSpawnRequest) -> ExecutionResult<Box<dyn RunningProcess>> {
        let args = &request.command.args;
        let pair = |first: &str, second: &str| args.windows(2).any(|p| p == [first, second]);
        let mut version = self.version.lock().unwrap();
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut code = 0;
        if args.iter().any(|arg| arg == "--version") {
            stdout = "uv 0.12.18\n".into();
        } else if pair("tool", "dir") {
            stdout = format!(
                "{}\n",
                self.root
                    .join(if args.contains(&"--bin".into()) {
                        "bin"
                    } else {
                        "tools"
                    })
                    .display()
            );
        } else if pair("tool", "list") {
            if let Some(version) = version.as_ref() {
                stdout = format!("black v{version}\n- black\n");
            } else {
                stderr = "No tools installed\n".into();
            }
        } else if pair("pip", "compile") {
            stdout = if matches!(self.behavior, Behavior::InvalidResolution) {
                "black @ https://private:secret@example.invalid/tool\n"
            } else {
                "black==2.0\nclick==8.2\n"
            }
            .into();
            let input = args.iter().position(|a| a == "compile").unwrap() + 1;
            assert_eq!(fs::read_to_string(&args[input]).unwrap(), "black<3\n");
            assert!(args.contains(&"--no-build".into()));
            if matches!(self.behavior, Behavior::ReceiptDrift) {
                fs::write(
                    self.root.join("tools/black/uv-receipt.toml"),
                    "[tool]\nrequirements=[]\n",
                )
                .unwrap();
            }
        } else if pair("tool", "upgrade") {
            assert!(args.contains(&"black==2.0".into()));
            assert_eq!(
                request.command.env["UV_TOOL_BIN_DIR"],
                self.root.join("bin").to_str().unwrap()
            );
            match self.behavior {
                Behavior::Failed => {
                    code = 1;
                    stderr = "private:secret@example.invalid".into();
                }
                Behavior::Cancelled => {
                    return Err(CoreError {
                        manager: None,
                        task: None,
                        action: None,
                        kind: CoreErrorKind::Cancelled,
                        message: "private transport".into(),
                    });
                }
                Behavior::Unchanged => {}
                Behavior::EquivalentVersion => *version = Some("2.0.0".into()),
                _ => *version = Some("2.0".into()),
            }
        } else if pair("tool", "uninstall") {
            *version = None;
            fs::remove_file(self.root.join("bin/black")).unwrap();
        } else {
            panic!("unexpected command: {args:?}");
        }
        self.commands.lock().unwrap().push(request);
        Ok(Box::new(Process(Ok(ProcessOutput {
            status: ProcessExitStatus::ExitCode(code),
            stdout: stdout.into_bytes(),
            stderr: stderr.into_bytes(),
            started_at: SystemTime::UNIX_EPOCH,
            finished_at: SystemTime::UNIX_EPOCH,
        }))))
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    root: PathBuf,
    executor: Arc<FakeExecutor>,
    adapter: UvToolAdapter,
}
impl Fixture {
    fn new(behavior: Behavior) -> Self {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("tools/black/bin")).unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        let executable = root.join("uv");
        fs::write(&executable, b"fake").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join("tools/black/bin/python"), b"fake").unwrap();
        fs::write(root.join("tools/black/bin/black"), b"fake").unwrap();
        symlink(root.join("tools/black/bin/black"), root.join("bin/black")).unwrap();
        fs::write(
            root.join("tools/black/uv-receipt.toml"),
            format!(
                r#"[tool]
requirements = [{{name="black", specifier="<3"}}]
entrypoints = [{{name="black", install-path={:?}}}]
"#,
                root.join("bin/black").to_str().unwrap()
            ),
        )
        .unwrap();
        let executor = Arc::new(FakeExecutor {
            root: root.clone(),
            behavior,
            version: Mutex::new(Some("1.0".into())),
            commands: Mutex::new(Vec::new()),
        });
        let adapter = UvToolAdapter::with_scope(executor.clone(), executable, root.join("tools"));
        Self {
            _directory: directory,
            root,
            executor,
            adapter,
        }
    }
    fn request(&self, version: &str) -> AdapterRequest {
        let context = helm_core::adapters::uv_tool_process::UvToolContext::new(
            self.root.join("uv"),
            self.root.join("tools"),
        )
        .unwrap();
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(context.tool_dir().to_str().unwrap().as_bytes());
        AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(PackageRef {
                manager: ManagerId::Uv,
                name: "black".into(),
            }),
            target_name: Some(format!("uv-tool:{hash:x}:black")),
            version: Some(version.into()),
        })
    }
}

#[tokio::test]
async fn mutation_requires_observed_version_not_just_zero_exit() {
    tokio::task::spawn_blocking(|| {
        for behavior in [
            Behavior::Normal,
            Behavior::EquivalentVersion,
            Behavior::Unchanged,
            Behavior::Failed,
            Behavior::Cancelled,
            Behavior::InvalidResolution,
            Behavior::ReceiptDrift,
        ] {
            let fixture = Fixture::new(behavior);
            let result = fixture.adapter.execute(fixture.request("2.0"));
            if matches!(behavior, Behavior::Normal | Behavior::EquivalentVersion) {
                let AdapterResponse::Mutation(mutation) = result.unwrap() else {
                    panic!("mutation")
                };
                assert_eq!(
                    mutation.after_version.as_deref(),
                    Some(if matches!(behavior, Behavior::EquivalentVersion) {
                        "2.0.0"
                    } else {
                        "2.0"
                    })
                );
            } else {
                let error = result.unwrap_err();
                assert!(!format!("{error:?}").contains("secret"));
            }
            for command in fixture.executor.commands.lock().unwrap().iter() {
                assert!(command.private_output_limit.is_some());
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn stale_target_wrong_store_and_blanket_upgrade_never_mutate() {
    tokio::task::spawn_blocking(|| {
        let fixture = Fixture::new(Behavior::Normal);
        let mut wrong_store = fixture.request("2.0");
        if let AdapterRequest::Upgrade(r) = &mut wrong_store {
            r.target_name = Some("uv-tool:wrong:black".into());
        }
        for request in [
            fixture.request("3.0"),
            wrong_store,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: None,
                target_name: None,
                version: None,
            }),
        ] {
            assert!(fixture.adapter.execute(request).is_err());
        }
        assert!(
            !fixture
                .executor
                .commands
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.command.args.contains(&"upgrade".into()))
        );
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn untrusted_store_and_redirected_entrypoints_block_mutations() {
    tokio::task::spawn_blocking(|| {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let fixture = Fixture::new(Behavior::Normal);
        fs::set_permissions(
            fixture.root.join("tools"),
            fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        assert!(fixture.adapter.execute(fixture.request("2.0")).is_err());
        fs::set_permissions(
            fixture.root.join("tools"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        fs::remove_file(fixture.root.join("bin/black")).unwrap();
        symlink("/bin/ls", fixture.root.join("bin/black")).unwrap();
        assert!(fixture.adapter.execute(fixture.request("2.0")).is_err());
        assert!(
            !fixture
                .executor
                .commands
                .lock()
                .unwrap()
                .iter()
                .any(|r| r.command.args.contains(&"upgrade".into()))
        );
    })
    .await
    .unwrap();
}

struct RecordingAdapter(Mutex<Vec<AdapterRequest>>);

#[tokio::test]
async fn uninstall_accepts_equivalent_versions_but_rejects_different_or_invalid_versions() {
    tokio::task::spawn_blocking(|| {
        for version in ["1.0.0", "2.0", "1.0rc1", "1.0+local", "invalid"] {
            let fixture = Fixture::new(Behavior::Normal);
            let result = fixture
                .adapter
                .execute(AdapterRequest::Uninstall(UninstallRequest {
                    package: PackageRef {
                        manager: ManagerId::Uv,
                        name: "black".into(),
                    },
                    target_name: None,
                    version: Some(version.into()),
                }));
            if version == "1.0.0" {
                let AdapterResponse::Mutation(mutation) = result.unwrap() else {
                    panic!("verified removal")
                };
                assert_eq!(mutation.before_version.as_deref(), Some("1.0"));
                assert_eq!(mutation.after_version, None);
            } else {
                assert!(result.is_err());
                assert!(
                    !fixture
                        .executor
                        .commands
                        .lock()
                        .unwrap()
                        .iter()
                        .any(|r| { r.command.args.contains(&"uninstall".into()) })
                );
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn runtime_persists_only_verified_mutations_and_preserves_failed_inventory() {
    for behavior in [
        Behavior::Normal,
        Behavior::EquivalentVersion,
        Behavior::Unchanged,
        Behavior::Failed,
    ] {
        let fixture = Fixture::new(behavior);
        let request = fixture.request("2.0");
        let store = Arc::new(SqliteStore::new(fixture.root.join("test.sqlite")));
        store.migrate_to_latest().unwrap();
        let runtime = AdapterRuntime::with_all_stores(
            [Arc::new(fixture.adapter) as Arc<dyn ManagerAdapter>],
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
        )
        .unwrap();
        let (task, persistence) = runtime
            .submit_with_persistence(ManagerId::Uv, AdapterRequest::Refresh(RefreshRequest))
            .await
            .unwrap();
        let refresh = runtime
            .wait_for_terminal(task, Some(Duration::from_secs(5)))
            .await
            .unwrap();
        assert!(matches!(
            refresh.terminal_state,
            Some(AdapterTaskTerminalState::Succeeded(_))
        ));
        persistence.wait_for_completion().await;
        assert_eq!(
            store.list_installed().unwrap()[0]
                .installed_version
                .as_deref(),
            Some("1.0")
        );
        assert_eq!(store.list_outdated().unwrap().len(), 1);
        let (task, persistence) = runtime
            .submit_with_persistence(ManagerId::Uv, request)
            .await
            .unwrap();
        let terminal = runtime
            .wait_for_terminal(task, Some(Duration::from_secs(5)))
            .await
            .unwrap();
        persistence.wait_for_completion().await;
        let success = matches!(behavior, Behavior::Normal | Behavior::EquivalentVersion);
        assert_eq!(
            matches!(
                terminal.terminal_state,
                Some(AdapterTaskTerminalState::Succeeded(_))
            ),
            success
        );
        assert_eq!(
            store.list_installed().unwrap()[0]
                .installed_version
                .as_deref(),
            Some(match behavior {
                Behavior::Normal => "2.0",
                Behavior::EquivalentVersion => "2.0.0",
                _ => "1.0",
            })
        );
        assert_eq!(store.list_outdated().unwrap().len(), usize::from(!success));
    }
}

impl ManagerAdapter for RecordingAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        helm_core::registry::manager(ManagerId::Uv).unwrap()
    }
    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }
    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        self.0.lock().unwrap().push(request);
        Ok(AdapterResponse::Refreshed)
    }
}

#[tokio::test]
async fn runtime_binds_cached_candidate_and_scope_and_allows_offline_local_search() {
    let directory = tempfile::tempdir().unwrap();
    let store = Arc::new(SqliteStore::new(directory.path().join("test.sqlite")));
    store.migrate_to_latest().unwrap();
    let adapter = Arc::new(RecordingAdapter(Mutex::new(Vec::new())));
    let runtime = AdapterRuntime::with_all_stores(
        [adapter.clone() as Arc<dyn ManagerAdapter>],
        store.clone(),
        store.clone(),
        store.clone(),
        store.clone(),
    )
    .unwrap();
    let package = PackageRef {
        manager: ManagerId::Uv,
        name: "black".into(),
    };
    let outdated = OutdatedPackage {
        package: package.clone(),
        package_identifier: Some("uv-tool:scope:black".into()),
        installed_version: Some("1.0".into()),
        candidate_version: "2.0".into(),
        pinned: false,
        restart_required: false,
        runtime_state: Default::default(),
    };
    store
        .replace_outdated_snapshot(ManagerId::Uv, std::slice::from_ref(&outdated))
        .unwrap();
    let request = AdapterRequest::Upgrade(UpgradeRequest {
        package: Some(package),
        target_name: None,
        version: None,
    });
    let task = runtime
        .submit(ManagerId::Uv, request.clone())
        .await
        .unwrap();
    let result = runtime
        .wait_for_terminal(task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    assert!(matches!(
        result.terminal_state,
        Some(AdapterTaskTerminalState::Succeeded(_))
    ));
    let requests = adapter.0.lock().unwrap().clone();
    let AdapterRequest::Upgrade(bound) = &requests[0] else {
        panic!("upgrade")
    };
    assert_eq!(bound.version.as_deref(), Some("2.0"));
    assert_eq!(bound.target_name.as_deref(), Some("uv-tool:scope:black"));
    let pinned = OutdatedPackage {
        pinned: true,
        ..outdated
    };
    store
        .replace_outdated_snapshot(ManagerId::Uv, &[pinned])
        .unwrap();
    assert!(
        runtime
            .submit(ManagerId::Uv, request.clone())
            .await
            .is_err()
    );
    runtime.set_network_available(false);
    assert!(runtime.submit(ManagerId::Uv, request).await.is_err());
    assert!(
        runtime
            .submit(
                ManagerId::Uv,
                AdapterRequest::Search(SearchRequest {
                    query: SearchQuery {
                        text: "black".into(),
                        issued_at: SystemTime::now()
                    }
                })
            )
            .await
            .is_ok()
    );
}
