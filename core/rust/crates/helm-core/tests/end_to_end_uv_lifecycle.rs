use std::path::PathBuf;
use std::sync::Arc;

use helm_core::adapters::manager::*;
use helm_core::adapters::uv_tool_runtime::UvToolAdapter;
use helm_core::execution::TokioProcessExecutor;
use helm_core::models::*;

#[tokio::test]
#[ignore = "opt-in: explicit uv and Python; generates offline wheels and mutates only a disposable tool store"]
async fn real_uv_constrained_resolution_upgrade_and_removal() {
    let executable =
        PathBuf::from(std::env::var_os("HELM_UV_CONTRACT_EXECUTABLE").expect("explicit uv"));
    let python =
        PathBuf::from(std::env::var_os("HELM_UV_CONTRACT_PYTHON").expect("explicit Python"));
    assert!(executable.is_absolute() && python.is_absolute());
    if let Some(root) = std::env::var_os("HELM_UV_RUNTIME_CHILD_ROOT") {
        exercise_adapter(executable, PathBuf::from(root)).await;
        return;
    }
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let artifacts = workspace.join("../../../../artifacts/uv-lifecycle");
    let commands = serde_json::json!({
        "installed": ["--color", "never", "--no-progress", "tool", "list", "--show-version-specifiers", "--offline"],
        "latest": ["--color", "never", "--no-progress", "tool", "list", "--show-version-specifiers", "--outdated"]
    });
    let output = std::process::Command::new(&python)
        .arg(workspace.join("tests/fixtures/uv/isolated_lifecycle.py"))
        .arg(&executable)
        .arg(artifacts)
        .arg(commands.to_string())
        .arg("prepare-adapter")
        .output()
        .expect("prepare fixtures");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let prepared: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let root = PathBuf::from(prepared["artifacts"].as_str().unwrap());
    eprintln!("Isolated adapter artifacts: {}", root.display());
    let config = root.join("config/uv.toml");
    let mut options = toml::Table::new();
    options.insert("no-index".into(), toml::Value::Boolean(true));
    options.insert(
        "find-links".into(),
        toml::Value::Array(vec![toml::Value::String(
            root.join("wheels").to_str().unwrap().into(),
        )]),
    );
    // Tool commands ignore pip-only options, so the resolver must ignore this conflicting index too.
    options.insert(
        "pip".into(),
        toml::Value::Table(
            toml::from_str("index-url = 'https://must-not-be-used.invalid/simple'").unwrap(),
        ),
    );
    std::fs::write(&config, toml::to_string(&options).unwrap()).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "real_uv_constrained_resolution_upgrade_and_removal",
            "--nocapture",
        ])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", root.join("home"))
        .env("TMPDIR", root.join("tmp"))
        .env("UV_CONFIG_FILE", config)
        .env("UV_TOOL_BIN_DIR", root.join("bin"))
        .env("UV_CACHE_DIR", root.join("cache"))
        .env("UV_PYTHON", &python)
        .env("UV_PYTHON_DOWNLOADS", "never")
        .env("UV_OFFLINE", "true")
        .env("HELM_UV_CONTRACT_EXECUTABLE", executable)
        .env("HELM_UV_CONTRACT_PYTHON", python)
        .env("HELM_UV_RUNTIME_CHILD_ROOT", root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

async fn exercise_adapter(executable: PathBuf, root: PathBuf) {
    tokio::task::spawn_blocking(move || {
        let adapter = UvToolAdapter::with_scope(
            Arc::new(TokioProcessExecutor),
            executable,
            root.join("tools"),
        );
        let package = PackageRef {
            manager: ManagerId::Uv,
            name: "helm-uv-smoke".into(),
        };
        let receipt = root.join("tools/helm-uv-smoke/uv-receipt.toml");
        let before = std::fs::read_to_string(&receipt).unwrap();
        let response = adapter
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .unwrap();
        let AdapterResponse::SnapshotSync {
            installed: Some(installed),
            outdated: Some(outdated),
        } = response
        else {
            panic!("expected snapshot")
        };
        assert_eq!(installed.len(), 2);
        assert_eq!(outdated.len(), 1, "the exact pin must not become an update");
        assert_eq!(
            outdated[0].candidate_version, "1.1",
            "2.0 violates the stored <2 requirement"
        );
        assert_eq!(
            std::fs::read_to_string(&receipt).unwrap(),
            before,
            "discovery must not mutate receipt"
        );
        let stale = adapter
            .execute(AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(package.clone()),
                target_name: outdated[0].package_identifier.clone(),
                version: Some("2.0".into()),
            }))
            .unwrap_err();
        assert_eq!(stale.kind, CoreErrorKind::InvalidInput);
        let response = adapter
            .execute(AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(package.clone()),
                target_name: outdated[0].package_identifier.clone(),
                version: Some("1.1".into()),
            }))
            .unwrap();
        let AdapterResponse::Mutation(mutation) = response else {
            panic!("expected verified mutation")
        };
        assert_eq!(mutation.before_version.as_deref(), Some("1.0"));
        assert_eq!(mutation.after_version.as_deref(), Some("1.1"));
        assert!(root.join("bin/helm-uv-smoke").is_file());
        assert!(root.join("bin/helm-uv-smoke-alt").is_file());
        let response = adapter
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .unwrap();
        let AdapterResponse::SnapshotSync {
            outdated: Some(outdated),
            ..
        } = response
        else {
            panic!("expected snapshot")
        };
        assert!(outdated.is_empty());
        adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: package.clone(),
                target_name: None,
                version: Some("1.1".into()),
            }))
            .unwrap();
        assert!(!root.join("bin/helm-uv-smoke").exists());
        assert!(root.join("bin/helm-uv-pinned").is_file());
        let response = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: package.clone(),
                target_name: None,
                // PEP 440 release segments are zero-padded for equality. uv
                // satisfies 1.1.0 with our 1.1 wheel and reports its metadata.
                version: Some("1.1.0".into()),
            }))
            .unwrap();
        let AdapterResponse::Mutation(result) = response else {
            panic!("verified install")
        };
        assert_eq!(result.before_version, None);
        assert_eq!(result.after_version.as_deref(), Some("1.1"));
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();
        let AdapterResponse::SnapshotSync {
            outdated: Some(outdated),
            ..
        } = response
        else {
            panic!("snapshot")
        };
        assert!(outdated.is_empty(), "explicit install pin is preserved");
        let failure = adapter.execute(AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: ManagerId::Uv,
                name: "helm-uv-missing".into(),
            },
            target_name: None,
            version: None,
        }));
        assert!(failure.is_err());
        assert!(root.join("bin/helm-uv-pinned").is_file());
        adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package,
                target_name: None,
                version: Some("1.1.0".into()),
            }))
            .unwrap();
        let response = adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Uv,
                    name: "helm-uv-pinned".into(),
                },
                target_name: None,
                version: Some("1.0".into()),
            }))
            .unwrap();
        let AdapterResponse::Mutation(result) = response else {
            panic!("verified last removal")
        };
        assert_eq!(result.after_version, None);
        assert!(!root.join("tools").exists());
        assert!(!root.join("bin/helm-uv-pinned").exists());
    })
    .await
    .unwrap();
}
