use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, OutdatedPackage, PackageRef,
    TaskId, TaskType,
};

const FIRMWARE_UPDATES_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListOutdated,
];

const FIRMWARE_UPDATES_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::FirmwareUpdates,
    display_name: "Firmware updates",
    category: ManagerCategory::SecurityFirmware,
    authority: ManagerAuthority::Guarded,
    capabilities: FIRMWARE_UPDATES_CAPABILITIES,
};

const SOFTWAREUPDATE_COMMAND: &str = "/usr/sbin/softwareupdate";
const DETECT_TIMEOUT: Duration = Duration::from_secs(15);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirmwareUpdatesDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub history_output: String,
}

pub trait FirmwareUpdatesSource: Send + Sync {
    fn detect(&self) -> AdapterResult<FirmwareUpdatesDetectOutput>;
    fn history(&self) -> AdapterResult<String>;
    fn list_available(&self) -> AdapterResult<String>;
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
                let outdated = parse_firmware_updates_list(&self.source.list_available()?)?;
                Ok(AdapterResponse::SnapshotSync {
                    installed: None,
                    outdated: Some(outdated),
                })
            }
            AdapterRequest::ListOutdated(_) => {
                let outdated = parse_firmware_updates_list(&self.source.list_available()?)?;
                Ok(AdapterResponse::OutdatedPackages(outdated))
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

pub fn firmware_updates_list_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    firmware_updates_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(SOFTWAREUPDATE_COMMAND).arg("-l"),
        LIST_TIMEOUT,
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

fn parse_firmware_updates_list(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed.contains("No new software available.") {
        return Ok(Vec::new());
    }

    let mut packages = Vec::new();
    let mut current_label: Option<String> = None;
    let mut current_title: Option<String> = None;
    let mut current_version: Option<String> = None;
    let mut current_restart_required = false;

    for line in output.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("* Label:") {
            flush_firmware_update_block(
                &mut packages,
                current_label.take(),
                current_title.take(),
                current_version.take(),
                current_restart_required,
            );
            current_label = Some(rest.trim().to_string());
            current_title = None;
            current_version = None;
            current_restart_required = false;
            continue;
        }

        if current_label.is_some() && (line.starts_with('\t') || line.starts_with("    ")) {
            for field in trimmed.split(',') {
                let field = field.trim().trim_end_matches(',');
                if let Some((key, value)) = field.split_once(':') {
                    let key = key.trim();
                    let value = value.trim();
                    match key {
                        "Title" => current_title = Some(value.to_string()),
                        "Version" => current_version = Some(value.to_string()),
                        "Action" if value.eq_ignore_ascii_case("restart") => {
                            current_restart_required = true;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    flush_firmware_update_block(
        &mut packages,
        current_label.take(),
        current_title.take(),
        current_version.take(),
        current_restart_required,
    );

    Ok(packages)
}

fn flush_firmware_update_block(
    packages: &mut Vec<OutdatedPackage>,
    label: Option<String>,
    title: Option<String>,
    version: Option<String>,
    restart_required: bool,
) {
    let Some(label) = label else {
        return;
    };

    let title = title.unwrap_or_default();
    let subject = format!("{label} {title}").to_ascii_lowercase();
    let looks_like_firmware = ["firmware", "bridgeos", "efi", "modem", "device firmware"]
        .iter()
        .any(|needle| subject.contains(needle));
    if !looks_like_firmware {
        return;
    }

    let candidate_version = version
        .or_else(|| extract_version_token(&title))
        .or_else(|| extract_version_token(&label));
    let Some(candidate_version) = candidate_version else {
        return;
    };

    packages.push(OutdatedPackage {
        package: PackageRef {
            manager: ManagerId::FirmwareUpdates,
            name: if title.is_empty() {
                label.clone()
            } else {
                title
            },
        },
        package_identifier: Some(label),
        installed_version: None,
        candidate_version,
        pinned: false,
        restart_required,
        runtime_state: Default::default(),
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use crate::adapters::firmware_updates::{
        FirmwareUpdatesAdapter, FirmwareUpdatesDetectOutput, FirmwareUpdatesSource,
        firmware_updates_history_request, firmware_updates_list_request,
        parse_firmware_updates_list, parse_latest_firmware_history_version,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListOutdatedRequest,
        ManagerAdapter, RefreshRequest,
    };
    use crate::models::{ManagerAction, ManagerId, TaskType};

    const HISTORY_FIXTURE: &str = include_str!("../../tests/fixtures/firmware_updates/history.txt");
    const AVAILABLE_FIXTURE: &str = include_str!("../../tests/fixtures/firmware_updates/list.txt");

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
    fn list_request_has_expected_shape() {
        let request = firmware_updates_list_request(None);
        assert_eq!(request.manager, ManagerId::FirmwareUpdates);
        assert_eq!(request.task_type, TaskType::Refresh);
        assert_eq!(request.action, ManagerAction::ListOutdated);
        assert_eq!(
            request.command.program.to_str(),
            Some("/usr/sbin/softwareupdate")
        );
        assert_eq!(request.command.args, vec!["-l"]);
        assert_eq!(request.timeout, Some(Duration::from_secs(120)));
    }

    #[test]
    fn adapter_detect_reports_installed_when_softwareupdate_exists() {
        let source = FixtureSource {
            detect_result: Ok(FirmwareUpdatesDetectOutput {
                executable_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
                history_output: HISTORY_FIXTURE.to_string(),
            }),
            history_result: Ok(HISTORY_FIXTURE.to_string()),
            list_available_result: Ok(AVAILABLE_FIXTURE.to_string()),
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
            list_available_result: Ok(AVAILABLE_FIXTURE.to_string()),
        };
        let adapter = FirmwareUpdatesAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .unwrap();
        let AdapterResponse::SnapshotSync {
            installed,
            outdated,
        } = response
        else {
            panic!("expected snapshot sync response");
        };
        assert!(installed.is_none());
        let outdated = outdated.expect("expected outdated packages");
        assert!(!outdated.is_empty());
        assert_eq!(outdated[0].package.manager, ManagerId::FirmwareUpdates);
    }

    #[test]
    fn adapter_lists_outdated_firmware_updates() {
        let source = FixtureSource {
            detect_result: Ok(FirmwareUpdatesDetectOutput {
                executable_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
                history_output: HISTORY_FIXTURE.to_string(),
            }),
            history_result: Ok(HISTORY_FIXTURE.to_string()),
            list_available_result: Ok(AVAILABLE_FIXTURE.to_string()),
        };
        let adapter = FirmwareUpdatesAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();
        let AdapterResponse::OutdatedPackages(packages) = response else {
            panic!("expected outdated packages response");
        };
        assert_eq!(
            packages,
            parse_firmware_updates_list(AVAILABLE_FIXTURE).expect("fixture should parse")
        );
    }

    struct FixtureSource {
        detect_result: AdapterResult<FirmwareUpdatesDetectOutput>,
        history_result: AdapterResult<String>,
        list_available_result: AdapterResult<String>,
    }

    impl FirmwareUpdatesSource for FixtureSource {
        fn detect(&self) -> AdapterResult<FirmwareUpdatesDetectOutput> {
            self.detect_result.clone()
        }

        fn history(&self) -> AdapterResult<String> {
            self.history_result.clone()
        }

        fn list_available(&self) -> AdapterResult<String> {
            self.list_available_result.clone()
        }
    }
}
