use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, TaskId, TaskType,
};

const PARALLELS_DESKTOP_CAPABILITIES: &[Capability] = &[Capability::Detect];

const PARALLELS_DESKTOP_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::ParallelsDesktop,
    display_name: "Parallels Desktop",
    category: ManagerCategory::ContainerVm,
    authority: ManagerAuthority::DetectionOnly,
    capabilities: PARALLELS_DESKTOP_CAPABILITIES,
};

const DEFAULTS_COMMAND: &str = "/usr/bin/defaults";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParallelsDesktopDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait ParallelsDesktopSource: Send + Sync {
    fn detect(&self) -> AdapterResult<ParallelsDesktopDetectOutput>;
}

pub struct ParallelsDesktopAdapter<S: ParallelsDesktopSource> {
    source: S,
}

impl<S: ParallelsDesktopSource> ParallelsDesktopAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: ParallelsDesktopSource> ManagerAdapter for ParallelsDesktopAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &PARALLELS_DESKTOP_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_parallels_desktop_version(&output.version_output);
                let has_executable = output.executable_path.is_some();
                let installed = has_executable || version.is_some();
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: output.executable_path,
                    version,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::ParallelsDesktop),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "parallels desktop adapter action not implemented in this milestone"
                    .to_string(),
            }),
        }
    }
}

pub fn parallels_desktop_detect_request(
    task_id: Option<TaskId>,
    info_plist_path: &str,
) -> ProcessSpawnRequest {
    parallels_desktop_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(DEFAULTS_COMMAND).args([
            "read",
            info_plist_path,
            "CFBundleShortVersionString",
        ]),
        DETECT_TIMEOUT,
    )
}

fn parallels_desktop_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request =
        ProcessSpawnRequest::new(ManagerId::ParallelsDesktop, task_type, action, command)
            .requires_elevation(false)
            .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_parallels_desktop_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let sanitized = line.trim_matches(|ch: char| matches!(ch, '"' | '\'' | '(' | ')' | ','));
    let starts_with_digit = sanitized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit());
    if !starts_with_digit {
        return None;
    }
    Some(sanitized.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ManagerAdapter,
        RefreshRequest,
    };
    use crate::adapters::parallels_desktop::{
        ParallelsDesktopAdapter, ParallelsDesktopDetectOutput, ParallelsDesktopSource,
        parallels_desktop_detect_request, parse_parallels_desktop_version,
    };
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, TaskType};

    #[test]
    fn parses_parallels_desktop_version() {
        let version = parse_parallels_desktop_version("20.2.0\n");
        assert_eq!(version.as_deref(), Some("20.2.0"));
    }

    #[test]
    fn detect_request_has_expected_shape() {
        let request = parallels_desktop_detect_request(None, "/Applications/Parallels Desktop.app");
        assert_eq!(request.manager, ManagerId::ParallelsDesktop);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program.to_str(), Some("/usr/bin/defaults"));
        assert_eq!(
            request.command.args,
            vec![
                "read",
                "/Applications/Parallels Desktop.app",
                "CFBundleShortVersionString"
            ]
        );
    }

    #[test]
    fn adapter_detect_reports_installed_when_source_has_executable() {
        let source = FixtureSource {
            detect_result: Ok(ParallelsDesktopDetectOutput {
                executable_path: Some(PathBuf::from("/Applications/Parallels Desktop.app")),
                version_output: "20.2.0".to_string(),
            }),
        };
        let adapter = ParallelsDesktopAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();

        let AdapterResponse::Detection(info) = response else {
            panic!("expected detection response");
        };
        assert!(info.installed);
        assert_eq!(info.version.as_deref(), Some("20.2.0"));
    }

    #[test]
    fn adapter_detect_reports_not_installed_when_source_empty() {
        let source = FixtureSource {
            detect_result: Ok(ParallelsDesktopDetectOutput {
                executable_path: None,
                version_output: String::new(),
            }),
        };
        let adapter = ParallelsDesktopAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();

        let AdapterResponse::Detection(info) = response else {
            panic!("expected detection response");
        };
        assert!(!info.installed);
    }

    #[test]
    fn unsupported_request_is_rejected() {
        let source = FixtureSource {
            detect_result: Ok(ParallelsDesktopDetectOutput {
                executable_path: None,
                version_output: String::new(),
            }),
        };
        let adapter = ParallelsDesktopAdapter::new(source);
        let error = adapter
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .expect_err("refresh should be unsupported");
        assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    }

    struct FixtureSource {
        detect_result: AdapterResult<ParallelsDesktopDetectOutput>,
    }

    impl ParallelsDesktopSource for FixtureSource {
        fn detect(&self) -> AdapterResult<ParallelsDesktopDetectOutput> {
            self.detect_result.clone()
        }
    }
}
