use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const XCODE_CLT_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Upgrade,
];

const XCODE_CLT_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::XcodeCommandLineTools,
    display_name: "Xcode Command Line Tools",
    category: ManagerCategory::SecurityFirmware,
    authority: ManagerAuthority::Guarded,
    capabilities: XCODE_CLT_CAPABILITIES,
};

const PKGUTIL_COMMAND: &str = "/usr/sbin/pkgutil";
const SOFTWAREUPDATE_COMMAND: &str = "/usr/sbin/softwareupdate";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const UPGRADE_TIMEOUT: Duration = Duration::from_secs(1800);
const XCODE_CLT_RECEIPT: &str = "com.apple.pkg.CLTools_Executables";
const XCODE_CLT_DISPLAY_NAME: &str = "Command Line Tools for Xcode";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XcodeCommandLineToolsDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait XcodeCommandLineToolsSource: Send + Sync {
    fn detect(&self) -> AdapterResult<XcodeCommandLineToolsDetectOutput>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn upgrade(&self, label: &str) -> AdapterResult<String>;
}

pub struct XcodeCommandLineToolsAdapter<S: XcodeCommandLineToolsSource> {
    source: S,
}

impl<S: XcodeCommandLineToolsSource> XcodeCommandLineToolsAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: XcodeCommandLineToolsSource> ManagerAdapter for XcodeCommandLineToolsAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &XCODE_CLT_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_xcode_clt_version(&output.version_output);
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
            AdapterRequest::ListInstalled(_) => {
                let output = self.source.detect()?;
                let version = parse_xcode_clt_version(&output.version_output);
                let installed = output.executable_path.is_some() || version.is_some();
                let packages = if installed {
                    vec![InstalledPackage {
                        package: PackageRef {
                            manager: ManagerId::XcodeCommandLineTools,
                            name: XCODE_CLT_DISPLAY_NAME.to_string(),
                        },
                        installed_version: version,
                        pinned: false,
                    }]
                } else {
                    Vec::new()
                };
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_xcode_clt_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.ok_or(CoreError {
                    manager: Some(ManagerId::XcodeCommandLineTools),
                    task: None,
                    action: Some(ManagerAction::Upgrade),
                    kind: CoreErrorKind::InvalidInput,
                    message: "xcode command line tools upgrade requires explicit update label"
                        .to_string(),
                })?;

                let target_label = package.name.trim();
                if target_label.is_empty() {
                    return Err(CoreError {
                        manager: Some(ManagerId::XcodeCommandLineTools),
                        task: None,
                        action: Some(ManagerAction::Upgrade),
                        kind: CoreErrorKind::InvalidInput,
                        message: "xcode command line tools upgrade label cannot be empty"
                            .to_string(),
                    });
                }

                let _ = self.source.upgrade(target_label)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::XcodeCommandLineTools),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message:
                    "xcode command line tools adapter action not implemented in this milestone"
                        .to_string(),
            }),
        }
    }
}

pub fn xcode_command_line_tools_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    xcode_command_line_tools_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(PKGUTIL_COMMAND).args(["--pkg-info", XCODE_CLT_RECEIPT]),
        DETECT_TIMEOUT,
    )
}

pub fn xcode_command_line_tools_list_outdated_request(
    task_id: Option<TaskId>,
) -> ProcessSpawnRequest {
    xcode_command_line_tools_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(SOFTWAREUPDATE_COMMAND).arg("-l"),
        LIST_TIMEOUT,
    )
}

pub fn xcode_command_line_tools_upgrade_request(
    task_id: Option<TaskId>,
    label: &str,
) -> ProcessSpawnRequest {
    xcode_command_line_tools_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(SOFTWAREUPDATE_COMMAND).args(["-i", label]),
        UPGRADE_TIMEOUT,
    )
    .requires_elevation(true)
}

fn xcode_command_line_tools_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request =
        ProcessSpawnRequest::new(ManagerId::XcodeCommandLineTools, task_type, action, command)
            .requires_elevation(false)
            .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_xcode_clt_version(output: &str) -> Option<String> {
    output.lines().map(str::trim).find_map(|line| {
        line.strip_prefix("version:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn parse_xcode_clt_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut packages = Vec::new();
    let mut current_label: Option<String> = None;
    let mut current_title: Option<String> = None;
    let mut current_version: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("* Label:") {
            flush_xcode_clt_update(
                &mut packages,
                current_label.take(),
                current_title.take(),
                current_version.take(),
            );
            current_label = Some(rest.trim().to_string());
            current_title = None;
            current_version = None;
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
                        _ => {}
                    }
                }
            }
        }
    }

    flush_xcode_clt_update(
        &mut packages,
        current_label.take(),
        current_title.take(),
        current_version.take(),
    );

    Ok(packages)
}

fn flush_xcode_clt_update(
    packages: &mut Vec<OutdatedPackage>,
    label: Option<String>,
    title: Option<String>,
    version: Option<String>,
) {
    let Some(label) = label else {
        return;
    };

    let title_matches = title
        .as_deref()
        .is_some_and(|value| value.to_ascii_lowercase().contains("command line tools"));
    let label_matches = label
        .to_ascii_lowercase()
        .contains("command line tools for xcode");
    if !title_matches && !label_matches {
        return;
    }

    let candidate_version = version
        .or_else(|| extract_version_from_text(title.as_deref().unwrap_or_default()))
        .or_else(|| extract_version_from_text(&label));
    let Some(candidate_version) = candidate_version else {
        return;
    };

    packages.push(OutdatedPackage {
        package: PackageRef {
            manager: ManagerId::XcodeCommandLineTools,
            name: label,
        },
        installed_version: None,
        candidate_version,
        pinned: false,
        restart_required: false,
    });
}

fn extract_version_from_text(text: &str) -> Option<String> {
    text.split(|ch: char| ch.is_whitespace() || ch == '-' || ch == '(' || ch == ')')
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

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter,
    };
    use crate::adapters::xcode_command_line_tools::{
        XcodeCommandLineToolsAdapter, XcodeCommandLineToolsDetectOutput,
        XcodeCommandLineToolsSource, parse_xcode_clt_outdated, parse_xcode_clt_version,
        xcode_command_line_tools_detect_request, xcode_command_line_tools_list_outdated_request,
        xcode_command_line_tools_upgrade_request,
    };
    use crate::models::{ManagerAction, ManagerId, TaskType};

    const PKGUTIL_FIXTURE: &str =
        include_str!("../../tests/fixtures/xcode_command_line_tools/pkgutil_info.txt");
    const OUTDATED_FIXTURE: &str =
        include_str!("../../tests/fixtures/xcode_command_line_tools/list_available.txt");

    #[test]
    fn parses_xcode_clt_version_from_pkgutil_output() {
        let version = parse_xcode_clt_version(PKGUTIL_FIXTURE);
        assert_eq!(version.as_deref(), Some("16.3.0.0.1.1742423573"));
    }

    #[test]
    fn parses_xcode_clt_outdated_from_fixture() {
        let packages = parse_xcode_clt_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(
            packages[0].package.manager,
            ManagerId::XcodeCommandLineTools
        );
        assert_eq!(
            packages[0].package.name,
            "Command Line Tools for Xcode-16.3"
        );
        assert_eq!(packages[0].candidate_version, "16.3");
    }

    #[test]
    fn detect_request_has_expected_shape() {
        let request = xcode_command_line_tools_detect_request(None);
        assert_eq!(request.manager, ManagerId::XcodeCommandLineTools);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program.to_str(), Some("/usr/sbin/pkgutil"));
        assert_eq!(
            request.command.args,
            vec!["--pkg-info", "com.apple.pkg.CLTools_Executables"]
        );
    }

    #[test]
    fn list_outdated_request_has_expected_shape() {
        let request = xcode_command_line_tools_list_outdated_request(None);
        assert_eq!(request.manager, ManagerId::XcodeCommandLineTools);
        assert_eq!(request.task_type, TaskType::Refresh);
        assert_eq!(request.action, ManagerAction::ListOutdated);
        assert_eq!(
            request.command.program.to_str(),
            Some("/usr/sbin/softwareupdate")
        );
        assert_eq!(request.command.args, vec!["-l"]);
    }

    #[test]
    fn upgrade_request_has_expected_shape_and_elevation() {
        let request =
            xcode_command_line_tools_upgrade_request(None, "Command Line Tools for Xcode-16.3");
        assert_eq!(request.manager, ManagerId::XcodeCommandLineTools);
        assert_eq!(request.task_type, TaskType::Upgrade);
        assert_eq!(request.action, ManagerAction::Upgrade);
        assert_eq!(
            request.command.program.to_str(),
            Some("/usr/sbin/softwareupdate")
        );
        assert_eq!(
            request.command.args,
            vec!["-i", "Command Line Tools for Xcode-16.3"]
        );
        assert!(request.requires_elevation);
    }

    #[test]
    fn adapter_list_installed_returns_single_package_when_detected() {
        let source = FixtureSource {
            detect_result: Ok(XcodeCommandLineToolsDetectOutput {
                executable_path: Some(PathBuf::from(
                    "/Library/Developer/CommandLineTools/usr/bin/clang",
                )),
                version_output: PKGUTIL_FIXTURE.to_string(),
            }),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
        };
        let adapter = XcodeCommandLineToolsAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();

        let AdapterResponse::InstalledPackages(packages) = response else {
            panic!("expected installed packages response");
        };
        assert_eq!(packages.len(), 1);
        assert_eq!(
            packages[0].package.manager,
            ManagerId::XcodeCommandLineTools
        );
        assert_eq!(
            packages[0].installed_version.as_deref(),
            Some("16.3.0.0.1.1742423573")
        );
    }

    #[test]
    fn adapter_list_outdated_parses_softwareupdate_payload() {
        let source = FixtureSource {
            detect_result: Ok(XcodeCommandLineToolsDetectOutput {
                executable_path: Some(PathBuf::from(
                    "/Library/Developer/CommandLineTools/usr/bin/clang",
                )),
                version_output: PKGUTIL_FIXTURE.to_string(),
            }),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
        };
        let adapter = XcodeCommandLineToolsAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        let AdapterResponse::OutdatedPackages(packages) = response else {
            panic!("expected outdated packages response");
        };
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].candidate_version, "16.3");
    }

    #[test]
    fn adapter_detect_reports_not_installed_when_source_empty() {
        let source = FixtureSource {
            detect_result: Ok(XcodeCommandLineToolsDetectOutput {
                executable_path: None,
                version_output: String::new(),
            }),
            list_outdated_result: Ok(String::new()),
        };
        let adapter = XcodeCommandLineToolsAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();

        let AdapterResponse::Detection(info) = response else {
            panic!("expected detection response");
        };
        assert!(!info.installed);
    }

    struct FixtureSource {
        detect_result: AdapterResult<XcodeCommandLineToolsDetectOutput>,
        list_outdated_result: AdapterResult<String>,
    }

    impl XcodeCommandLineToolsSource for FixtureSource {
        fn detect(&self) -> AdapterResult<XcodeCommandLineToolsDetectOutput> {
            self.detect_result.clone()
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            self.list_outdated_result.clone()
        }

        fn upgrade(&self, _label: &str) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
