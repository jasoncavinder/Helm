use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, TaskId, TaskType,
};

const ROSETTA2_CAPABILITIES: &[Capability] =
    &[Capability::Detect, Capability::Refresh, Capability::Install];

const ROSETTA2_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Rosetta2,
    display_name: "Rosetta 2",
    category: ManagerCategory::SecurityFirmware,
    authority: ManagerAuthority::Guarded,
    capabilities: ROSETTA2_CAPABILITIES,
};

const PKGUTIL_COMMAND: &str = "/usr/sbin/pkgutil";
const SOFTWAREUPDATE_COMMAND: &str = "/usr/sbin/softwareupdate";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(1800);
const ROSETTA_RECEIPT: &str = "com.apple.pkg.RosettaUpdateAuto";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rosetta2DetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait Rosetta2Source: Send + Sync {
    fn detect(&self) -> AdapterResult<Rosetta2DetectOutput>;
    fn install(&self) -> AdapterResult<String>;
}

pub struct Rosetta2Adapter<S: Rosetta2Source> {
    source: S,
}

impl<S: Rosetta2Source> Rosetta2Adapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: Rosetta2Source> ManagerAdapter for Rosetta2Adapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &ROSETTA2_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_rosetta2_version(&output.version_output);
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
            AdapterRequest::Install(install_request) => {
                let _ = self.source.install()?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: install_request.package,
                    action: ManagerAction::Install,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Rosetta2),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "rosetta2 adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn rosetta2_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rosetta2_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(PKGUTIL_COMMAND).args(["--pkg-info", ROSETTA_RECEIPT]),
        DETECT_TIMEOUT,
    )
}

pub fn rosetta2_install_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rosetta2_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(SOFTWAREUPDATE_COMMAND).args(["--install-rosetta", "--agree-to-license"]),
        INSTALL_TIMEOUT,
    )
    .requires_elevation(true)
}

fn rosetta2_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Rosetta2, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_rosetta2_version(output: &str) -> Option<String> {
    output.lines().map(str::trim).find_map(|line| {
        line.strip_prefix("version:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, InstallRequest,
        ManagerAdapter,
    };
    use crate::adapters::rosetta2::{
        Rosetta2Adapter, Rosetta2DetectOutput, Rosetta2Source, parse_rosetta2_version,
        rosetta2_detect_request, rosetta2_install_request,
    };
    use crate::models::{ManagerAction, ManagerId, PackageRef, TaskType};

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/rosetta2/pkgutil_info.txt");

    #[test]
    fn parses_rosetta2_version_from_pkgutil_output() {
        let version = parse_rosetta2_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("1.0.0.0.1.1700000000"));
    }

    #[test]
    fn detect_request_has_expected_shape() {
        let request = rosetta2_detect_request(None);
        assert_eq!(request.manager, ManagerId::Rosetta2);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program.to_str(), Some("/usr/sbin/pkgutil"));
        assert_eq!(
            request.command.args,
            vec!["--pkg-info", "com.apple.pkg.RosettaUpdateAuto"]
        );
    }

    #[test]
    fn install_request_has_expected_shape_and_elevation() {
        let request = rosetta2_install_request(None);
        assert_eq!(request.manager, ManagerId::Rosetta2);
        assert_eq!(request.task_type, TaskType::Install);
        assert_eq!(request.action, ManagerAction::Install);
        assert_eq!(
            request.command.program.to_str(),
            Some("/usr/sbin/softwareupdate")
        );
        assert_eq!(
            request.command.args,
            vec!["--install-rosetta", "--agree-to-license"]
        );
        assert!(request.requires_elevation);
    }

    #[test]
    fn adapter_detect_reports_not_installed_when_source_empty() {
        let source = FixtureSource {
            detect_result: Ok(Rosetta2DetectOutput {
                executable_path: None,
                version_output: String::new(),
            }),
        };
        let adapter = Rosetta2Adapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();

        let AdapterResponse::Detection(info) = response else {
            panic!("expected detection response");
        };
        assert!(!info.installed);
    }

    #[test]
    fn adapter_install_returns_mutation_response() {
        let source = FixtureSource {
            detect_result: Ok(Rosetta2DetectOutput {
                executable_path: Some(PathBuf::from(
                    "/Library/Apple/usr/libexec/oah/libRosettaRuntime",
                )),
                version_output: VERSION_FIXTURE.to_string(),
            }),
        };
        let adapter = Rosetta2Adapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Rosetta2,
                    name: "rosetta2".to_string(),
                },
                version: None,
            }))
            .unwrap();

        let AdapterResponse::Mutation(result) = response else {
            panic!("expected mutation response");
        };
        assert_eq!(result.action, ManagerAction::Install);
    }

    struct FixtureSource {
        detect_result: AdapterResult<Rosetta2DetectOutput>,
    }

    impl Rosetta2Source for FixtureSource {
        fn detect(&self) -> AdapterResult<Rosetta2DetectOutput> {
            self.detect_result.clone()
        }

        fn install(&self) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
