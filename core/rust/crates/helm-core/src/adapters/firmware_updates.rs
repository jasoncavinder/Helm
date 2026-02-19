use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, TaskId, TaskType,
};

const FIRMWARE_UPDATES_CAPABILITIES: &[Capability] = &[Capability::Detect, Capability::Refresh];

const FIRMWARE_UPDATES_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::FirmwareUpdates,
    display_name: "Firmware updates",
    category: ManagerCategory::SecurityFirmware,
    authority: ManagerAuthority::Guarded,
    capabilities: FIRMWARE_UPDATES_CAPABILITIES,
};

const SOFTWAREUPDATE_COMMAND: &str = "/usr/sbin/softwareupdate";
const DETECT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareUpdatesDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub history_output: String,
}

pub trait FirmwareUpdatesSource: Send + Sync {
    fn detect(&self) -> AdapterResult<FirmwareUpdatesDetectOutput>;
    fn history(&self) -> AdapterResult<String>;
}

pub struct FirmwareUpdatesAdapter<S: FirmwareUpdatesSource> {
    source: S,
}

impl<S: FirmwareUpdatesSource> FirmwareUpdatesAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: FirmwareUpdatesSource> ManagerAdapter for FirmwareUpdatesAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &FIRMWARE_UPDATES_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_latest_firmware_history_version(&output.history_output);
                let has_executable = output.executable_path.is_some();
                let installed = has_executable;
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: output.executable_path,
                    version,
                }))
            }
            AdapterRequest::Refresh(_) => {
                let _ = self.source.history()?;
                Ok(AdapterResponse::Refreshed)
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::FirmwareUpdates),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "firmware updates adapter action not implemented in this milestone"
                    .to_string(),
            }),
        }
    }
}

pub fn firmware_updates_history_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    firmware_updates_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(SOFTWAREUPDATE_COMMAND).arg("--history"),
        DETECT_TIMEOUT,
    )
}

fn firmware_updates_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request =
        ProcessSpawnRequest::new(ManagerId::FirmwareUpdates, task_type, action, command)
            .requires_elevation(false)
            .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_latest_firmware_history_version(output: &str) -> Option<String> {
    let mut in_firmware_block = false;

    for line in output.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if trimmed.is_empty() {
            in_firmware_block = false;
            continue;
        }

        if lower.contains("firmware") {
            in_firmware_block = true;
            if let Some(version) = extract_version_token(trimmed) {
                return Some(version);
            }
            continue;
        }

        if in_firmware_block && let Some(value) = trimmed.strip_prefix("Version:") {
            let candidate = value.trim();
            if !candidate.is_empty() {
                return Some(candidate.to_string());
            }
        }
    }

    None
}

fn extract_version_token(text: &str) -> Option<String> {
    text.split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ':')
        .map(str::trim)
        .find(|token| {
            token.chars().next().is_some_and(|ch| ch.is_ascii_digit())
                && token.chars().any(|ch| ch == '.')
        })
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapters::firmware_updates::{
        FirmwareUpdatesAdapter, FirmwareUpdatesDetectOutput, FirmwareUpdatesSource,
        firmware_updates_history_request, parse_latest_firmware_history_version,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ManagerAdapter,
        RefreshRequest,
    };
    use crate::models::{ManagerAction, ManagerId, TaskType};

    const HISTORY_FIXTURE: &str = include_str!("../../tests/fixtures/firmware_updates/history.txt");

    #[test]
    fn parses_latest_firmware_version_from_history_fixture() {
        let version = parse_latest_firmware_history_version(HISTORY_FIXTURE);
        assert_eq!(version.as_deref(), Some("2069.0.0.0.1"));
    }

    #[test]
    fn history_request_has_expected_shape() {
        let request = firmware_updates_history_request(None);
        assert_eq!(request.manager, ManagerId::FirmwareUpdates);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(
            request.command.program.to_str(),
            Some("/usr/sbin/softwareupdate")
        );
        assert_eq!(request.command.args, vec!["--history"]);
    }

    #[test]
    fn adapter_detect_reports_installed_when_softwareupdate_exists() {
        let source = FixtureSource {
            detect_result: Ok(FirmwareUpdatesDetectOutput {
                executable_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
                history_output: HISTORY_FIXTURE.to_string(),
            }),
            history_result: Ok(HISTORY_FIXTURE.to_string()),
        };
        let adapter = FirmwareUpdatesAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();

        let AdapterResponse::Detection(info) = response else {
            panic!("expected detection response");
        };
        assert!(info.installed);
        assert_eq!(info.version.as_deref(), Some("2069.0.0.0.1"));
    }

    #[test]
    fn adapter_refresh_runs_history_probe() {
        let source = FixtureSource {
            detect_result: Ok(FirmwareUpdatesDetectOutput {
                executable_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
                history_output: HISTORY_FIXTURE.to_string(),
            }),
            history_result: Ok(HISTORY_FIXTURE.to_string()),
        };
        let adapter = FirmwareUpdatesAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .unwrap();
        assert!(matches!(response, AdapterResponse::Refreshed));
    }

    struct FixtureSource {
        detect_result: AdapterResult<FirmwareUpdatesDetectOutput>,
        history_result: AdapterResult<String>,
    }

    impl FirmwareUpdatesSource for FixtureSource {
        fn detect(&self) -> AdapterResult<FirmwareUpdatesDetectOutput> {
            self.detect_result.clone()
        }

        fn history(&self) -> AdapterResult<String> {
            self.history_result.clone()
        }
    }
}
