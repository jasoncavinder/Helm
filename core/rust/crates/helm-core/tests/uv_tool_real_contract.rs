#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime};

use helm_core::adapters::uv_tool::{
    UvToolListError, UvToolListMode, UvToolObservation, parse_uv_tool_list, uv_tool_list_command,
};
use helm_core::adapters::uv_tool_eligibility::{UvEligibilityError, UvToolEligibilityPolicy};
use helm_core::execution::{ProcessExitStatus, ProcessOutput};
use serde::Deserialize;

#[derive(Deserialize)]
struct Capture {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Capture {
    fn output(&self) -> ProcessOutput {
        ProcessOutput {
            status: if self.code < 0 {
                ProcessExitStatus::Terminated
            } else {
                ProcessExitStatus::ExitCode(self.code)
            },
            stdout: self.stdout.as_bytes().to_vec(),
            stderr: self.stderr.as_bytes().to_vec(),
            started_at: SystemTime::UNIX_EPOCH,
            finished_at: SystemTime::UNIX_EPOCH,
        }
    }
}

#[derive(Deserialize)]
struct Transcript {
    artifacts: String,
    python: String,
    records: BTreeMap<String, Capture>,
    receipts: BTreeMap<String, String>,
}

impl Transcript {
    fn parsed(
        &self,
        label: &str,
        mode: UvToolListMode,
    ) -> Result<Vec<UvToolObservation>, UvToolListError> {
        parse_uv_tool_list(&self.records[label].output(), mode)
    }
}

fn explicit_executable(key: &str) -> PathBuf {
    let path = PathBuf::from(
        std::env::var_os(key)
            .unwrap_or_else(|| panic!("set {key} to an explicit absolute executable path")),
    );
    assert!(
        path.is_absolute() && path.is_file(),
        "{key} must be an absolute executable file"
    );
    path.canonicalize().unwrap()
}

#[tokio::test]
#[ignore = "opt-in only: set HELM_UV_CONTRACT_EXECUTABLE and HELM_UV_CONTRACT_PYTHON; installs generated test wheels in a disposable store"]
async fn real_uv_output_roundtrips_without_using_host_tool_state() {
    let uv = explicit_executable("HELM_UV_CONTRACT_EXECUTABLE");
    let python = explicit_executable("HELM_UV_CONTRACT_PYTHON");
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository = crate_dir.join("../../../..").canonicalize().unwrap();
    let list_commands = serde_json::json!({
        "installed": uv_tool_list_command(&uv, UvToolListMode::Installed).args,
        "latest": uv_tool_list_command(&uv, UvToolListMode::LatestVersions).args,
    });
    let mut command = tokio::process::Command::new(python);
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .arg("-I")
        .arg(crate_dir.join("tests/fixtures/uv/isolated_lifecycle.py"))
        .arg(uv)
        .arg(repository.join("artifacts/uv-real-contract"))
        .arg(list_commands.to_string())
        .current_dir(&crate_dir)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(600), command.output())
        .await
        .expect("isolated uv fixture harness exceeded its deadline")
        .unwrap();
    assert!(
        output.status.success(),
        "fixture harness failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let transcript: Transcript = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!(
        "uv: {}\nPython: {}\nArtifacts: {}",
        transcript.records["version"].stdout.trim_end(),
        transcript.python,
        transcript.artifacts
    );
    let installed = UvToolListMode::Installed;
    let latest = UvToolListMode::LatestVersions;

    for label in ["empty", "final_empty"] {
        assert!(transcript.parsed(label, installed).unwrap().is_empty());
    }
    for label in ["empty_latest", "final_empty_latest"] {
        assert!(transcript.parsed(label, latest).unwrap().is_empty());
    }
    assert!(
        transcript
            .parsed("current_latest", latest)
            .unwrap()
            .is_empty()
    );
    assert!(transcript.records["current_latest"].stdout.is_empty());
    assert!(transcript.records["current_latest"].stderr.is_empty());
    let before = transcript.parsed("installed", installed).unwrap();
    assert_eq!(before.len(), 2);
    for tool in &before {
        // Real receipts must parse through identity/constraint/entrypoint checks,
        // but this offline wheel source must not become a public-index candidate.
        assert_eq!(
            UvToolEligibilityPolicy::from_receipt(tool, transcript.receipts[&tool.name].as_bytes())
                .unwrap_err(),
            UvEligibilityError::SourceConfigurationRequiresResolution
        );
    }
    assert_eq!(before[0].name, "helm-uv-pinned");
    assert_eq!(before[0].installed_version, "1.0");
    assert_eq!(before[0].requirement.as_deref(), Some("==1.0"));
    assert_eq!(before[1].name, "helm-uv-smoke");
    assert_eq!(before[1].installed_version, "1.0");
    assert_eq!(before[1].requirement.as_deref(), Some("<2"));
    assert_eq!(
        before[1].executables,
        ["helm-uv-smoke", "helm-uv-smoke-alt"]
    );
    let discovered = transcript.parsed("latest", latest).unwrap();
    assert_eq!(discovered.len(), 2);
    for (tool, original) in discovered.iter().zip(&before) {
        assert_eq!(tool.name, original.name);
        assert_eq!(tool.requirement, original.requirement);
        assert_eq!(tool.installed_version, original.installed_version);
        assert_eq!(tool.latest_version.as_deref(), Some("2.0"));
    }
    let after = transcript.parsed("after_upgrade", installed).unwrap();
    assert_eq!(after.len(), 2);
    assert_eq!(after[0], before[0], "exact pin must remain unchanged");
    let mut expected = before[1].clone();
    expected.installed_version = "1.1".to_owned();
    assert_eq!(after[1], expected, "upgrade must stop below the <2 bound");
    let still_newer = transcript.parsed("latest_after_upgrade", latest).unwrap();
    assert_eq!(still_newer.len(), 2);
    assert!(
        still_newer
            .iter()
            .all(|tool| tool.latest_version.as_deref() == Some("2.0"))
    );
    assert_eq!(
        transcript
            .parsed("after_failed_install", installed)
            .unwrap(),
        after
    );
    assert_ne!(transcript.records["failed_install"].code, 0);
    for (label, mode) in [
        ("broken_receipt", installed),
        ("broken_receipt_latest", latest),
    ] {
        assert_eq!(
            transcript.records[label].code, 0,
            "uv skips the broken receipt but exits successfully"
        );
        assert!(transcript.records[label].stdout.contains("helm-uv-smoke"));
        assert_eq!(
            transcript.parsed(label, mode),
            Err(UvToolListError::NonAuthoritative)
        );
    }
    assert_eq!(transcript.parsed("restored", installed).unwrap(), after);
    assert_eq!(
        transcript.parsed("one_remaining", installed).unwrap(),
        vec![before[0].clone()]
    );
}
