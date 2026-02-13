use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, OutdatedPackage, PackageRef,
    TaskId, TaskType,
};

const SOFTWAREUPDATE_READ_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListOutdated,
];

const SOFTWAREUPDATE_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::SoftwareUpdate,
    display_name: "softwareupdate",
    category: ManagerCategory::SystemOs,
    authority: ManagerAuthority::Guarded,
    capabilities: SOFTWAREUPDATE_READ_CAPABILITIES,
};

const SW_VERS_COMMAND: &str = "/usr/bin/sw_vers";
const SOFTWAREUPDATE_COMMAND: &str = "/usr/sbin/softwareupdate";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoftwareUpdateDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait SoftwareUpdateSource: Send + Sync {
    fn detect(&self) -> AdapterResult<SoftwareUpdateDetectOutput>;
    fn list_available(&self) -> AdapterResult<String>;
}

pub struct SoftwareUpdateAdapter<S: SoftwareUpdateSource> {
    source: S,
}

impl<S: SoftwareUpdateSource> SoftwareUpdateAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: SoftwareUpdateSource> ManagerAdapter for SoftwareUpdateAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &SOFTWAREUPDATE_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_softwareupdate_version(&output.version_output);
                let has_executable = output.executable_path.is_some();
                let installed = has_executable || version.is_some();
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: output.executable_path,
                    version,
                }))
            }
            AdapterRequest::Refresh(_) => {
                let _ = self.source.detect()?;
                Ok(AdapterResponse::Refreshed)
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_available()?;
                let packages = parse_softwareupdate_list(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::SoftwareUpdate),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "softwareupdate adapter action not implemented in this milestone"
                    .to_string(),
            }),
        }
    }
}

pub fn softwareupdate_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    softwareupdate_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(SW_VERS_COMMAND),
        DETECT_TIMEOUT,
    )
}

pub fn softwareupdate_list_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    softwareupdate_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(SOFTWAREUPDATE_COMMAND).arg("-l"),
        LIST_TIMEOUT,
    )
}

fn softwareupdate_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request =
        ProcessSpawnRequest::new(ManagerId::SoftwareUpdate, task_type, action, command)
            .requires_elevation(false)
            .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_softwareupdate_version(output: &str) -> Option<String> {
    // Parse sw_vers output:
    // ProductName:    macOS
    // ProductVersion: 15.3.1
    // BuildVersion:   24D70
    for line in output.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("ProductVersion:") {
            let version = rest.trim();
            if !version.is_empty() {
                return Some(version.to_owned());
            }
        }
    }
    None
}

fn parse_softwareupdate_list(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut packages = Vec::new();
    let mut current_label: Option<String> = None;
    let mut current_title: Option<String> = None;
    let mut current_version: Option<String> = None;
    let mut current_restart_required = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // New update block: "* Label: macOS Sequoia 15.3.2-15.3.2"
        if let Some(rest) = trimmed.strip_prefix("* Label:") {
            // Flush previous block
            if let (Some(label), Some(version)) = (current_label.take(), current_version.take()) {
                let _title = current_title.take();
                packages.push(build_outdated_package(
                    &label,
                    &version,
                    current_restart_required,
                ));
            }
            current_label = Some(rest.trim().to_owned());
            current_title = None;
            current_version = None;
            current_restart_required = false;
            continue;
        }

        // Indented metadata line within a block
        if current_label.is_some() && (line.starts_with('\t') || line.starts_with("    ")) {
            // Parse comma-separated fields like:
            // "Title: macOS Sequoia 15.3.2, Version: 15.3.2, Size: 1803133KiB, Recommended: YES, Action: restart,"
            for field in trimmed.split(',') {
                let field = field.trim().trim_end_matches(',');
                if let Some((key, value)) = field.split_once(':') {
                    let key = key.trim();
                    let value = value.trim();
                    match key {
                        "Title" => current_title = Some(value.to_owned()),
                        "Version" => current_version = Some(value.to_owned()),
                        "Action" if value.eq_ignore_ascii_case("restart") => {
                            current_restart_required = true;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    // Flush final block
    if let (Some(label), Some(version)) = (current_label.take(), current_version.take()) {
        let _title = current_title.take();
        packages.push(build_outdated_package(
            &label,
            &version,
            current_restart_required,
        ));
    }

    Ok(packages)
}

fn build_outdated_package(
    label: &str,
    candidate_version: &str,
    restart_required: bool,
) -> OutdatedPackage {
    OutdatedPackage {
        package: PackageRef {
            manager: ManagerId::SoftwareUpdate,
            name: label.to_owned(),
        },
        installed_version: None,
        candidate_version: candidate_version.to_owned(),
        pinned: false,
        restart_required,
    }
}

fn _parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::SoftwareUpdate),
        task: None,
        action: None,
        kind: CoreErrorKind::ParseFailure,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter,
    };
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, TaskId, TaskType};

    use super::{
        SoftwareUpdateAdapter, SoftwareUpdateDetectOutput, SoftwareUpdateSource,
        parse_softwareupdate_list, parse_softwareupdate_version, softwareupdate_detect_request,
        softwareupdate_list_request,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/softwareupdate/version.txt");
    const LIST_AVAILABLE_FIXTURE: &str =
        include_str!("../../tests/fixtures/softwareupdate/list_available.txt");
    const LIST_AVAILABLE_EMPTY_FIXTURE: &str =
        include_str!("../../tests/fixtures/softwareupdate/list_available_empty.txt");

    #[test]
    fn parses_version_from_sw_vers_output() {
        let version = parse_softwareupdate_version(
            "ProductName:\t\tmacOS\nProductVersion:\t\t15.3.1\nBuildVersion:\t\t24D70\n",
        );
        assert_eq!(version.as_deref(), Some("15.3.1"));
    }

    #[test]
    fn parses_version_from_fixture() {
        let version = parse_softwareupdate_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("15.3.1"));
    }

    #[test]
    fn version_parse_returns_none_for_empty_input() {
        assert!(parse_softwareupdate_version("").is_none());
        assert!(parse_softwareupdate_version("   \n  ").is_none());
    }

    #[test]
    fn version_parse_returns_none_for_unrecognized_format() {
        assert!(parse_softwareupdate_version("macOS 15.3.1").is_none());
    }

    #[test]
    fn parses_list_available_from_fixture() {
        let packages = parse_softwareupdate_list(LIST_AVAILABLE_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);

        assert_eq!(packages[0].package.name, "macOS Sequoia 15.3.2-15.3.2");
        assert_eq!(packages[0].candidate_version, "15.3.2");
        assert!(packages[0].installed_version.is_none());
        assert!(packages[0].restart_required);

        assert_eq!(packages[1].package.name, "Safari 18.3.1-18.3.1");
        assert_eq!(packages[1].candidate_version, "18.3.1");
        assert!(!packages[1].restart_required);
    }

    #[test]
    fn parses_empty_list_available() {
        let packages = parse_softwareupdate_list(LIST_AVAILABLE_EMPTY_FIXTURE).unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn parses_empty_string_as_no_updates() {
        let packages = parse_softwareupdate_list("").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn adapter_executes_supported_read_only_requests() {
        let source = FixtureSource::default();
        let adapter = SoftwareUpdateAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        assert!(matches!(detect, AdapterResponse::Detection(_)));
        assert!(matches!(outdated, AdapterResponse::OutdatedPackages(_)));
    }

    #[test]
    fn adapter_rejects_unsupported_action() {
        let source = FixtureSource::default();
        let adapter = SoftwareUpdateAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    }

    #[test]
    fn adapter_rejects_install_request() {
        let source = FixtureSource::default();
        let adapter = SoftwareUpdateAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::SoftwareUpdate,
                    name: "macOS Sequoia".to_string(),
                },
                version: None,
            }))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    }

    #[test]
    fn detect_command_spec_uses_structured_args() {
        let request = softwareupdate_detect_request(Some(TaskId(42)));
        assert_eq!(request.manager, ManagerId::SoftwareUpdate);
        assert_eq!(request.task_id, Some(TaskId(42)));
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program, PathBuf::from("/usr/bin/sw_vers"));
        assert!(request.command.args.is_empty());
        assert!(request.timeout.is_some());
    }

    #[test]
    fn list_command_spec_uses_structured_args() {
        let request = softwareupdate_list_request(None);
        assert_eq!(
            request.command.program,
            PathBuf::from("/usr/sbin/softwareupdate")
        );
        assert_eq!(request.command.args, vec!["-l".to_string()]);
        assert_eq!(request.action, ManagerAction::ListOutdated);
        assert_eq!(request.task_type, TaskType::Refresh);
    }

    #[derive(Default, Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
    }

    impl SoftwareUpdateSource for FixtureSource {
        fn detect(&self) -> AdapterResult<SoftwareUpdateDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(SoftwareUpdateDetectOutput {
                executable_path: Some(PathBuf::from("/usr/bin/sw_vers")),
                version_output: VERSION_FIXTURE.to_string(),
            })
        }

        fn list_available(&self) -> AdapterResult<String> {
            Ok(LIST_AVAILABLE_FIXTURE.to_string())
        }
    }
}
