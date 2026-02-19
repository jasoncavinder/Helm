use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, TaskId, TaskType,
};

const SPARKLE_CAPABILITIES: &[Capability] = &[Capability::Detect];

const SPARKLE_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Sparkle,
    display_name: "Sparkle updater",
    category: ManagerCategory::GuiApp,
    authority: ManagerAuthority::DetectionOnly,
    capabilities: SPARKLE_CAPABILITIES,
};

const DEFAULTS_COMMAND: &str = "/usr/bin/defaults";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SparkleDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait SparkleSource: Send + Sync {
    fn detect(&self) -> AdapterResult<SparkleDetectOutput>;
}

pub struct SparkleAdapter<S: SparkleSource> {
    source: S,
}

impl<S: SparkleSource> SparkleAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: SparkleSource> ManagerAdapter for SparkleAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &SPARKLE_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_sparkle_version(&output.version_output);
                let has_executable = output.executable_path.is_some();
                let installed = has_executable || version.is_some();
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: output.executable_path,
                    version,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Sparkle),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "sparkle adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn sparkle_detect_request(
    task_id: Option<TaskId>,
    info_plist_path: &str,
) -> ProcessSpawnRequest {
    sparkle_request(
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

fn sparkle_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Sparkle, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_sparkle_version(output: &str) -> Option<String> {
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
    use crate::adapters::sparkle::{
        SparkleAdapter, SparkleDetectOutput, SparkleSource, parse_sparkle_version,
        sparkle_detect_request,
    };
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, TaskType};

    #[test]
    fn parses_sparkle_version() {
        let version = parse_sparkle_version("2.6.4\n");
        assert_eq!(version.as_deref(), Some("2.6.4"));
    }

    #[test]
    fn detect_request_has_expected_shape() {
        let request = sparkle_detect_request(
            None,
            "/Applications/Foo.app/Contents/Frameworks/Sparkle.framework/Resources/Info.plist",
        );
        assert_eq!(request.manager, ManagerId::Sparkle);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program.to_str(), Some("/usr/bin/defaults"));
        assert_eq!(
            request.command.args,
            vec![
                "read",
                "/Applications/Foo.app/Contents/Frameworks/Sparkle.framework/Resources/Info.plist",
                "CFBundleShortVersionString"
            ]
        );
    }

    #[test]
    fn adapter_detect_reports_installed_when_source_has_host_app() {
        let source = FixtureSource {
            detect_result: Ok(SparkleDetectOutput {
                executable_path: Some(PathBuf::from("/Applications/Foo.app")),
                version_output: "2.6.4".to_string(),
            }),
        };
        let adapter = SparkleAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();

        let AdapterResponse::Detection(info) = response else {
            panic!("expected detection response");
        };
        assert!(info.installed);
        assert_eq!(info.version.as_deref(), Some("2.6.4"));
    }

    #[test]
    fn adapter_detect_reports_not_installed_when_source_empty() {
        let source = FixtureSource {
            detect_result: Ok(SparkleDetectOutput {
                executable_path: None,
                version_output: String::new(),
            }),
        };
        let adapter = SparkleAdapter::new(source);
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
            detect_result: Ok(SparkleDetectOutput {
                executable_path: None,
                version_output: String::new(),
            }),
        };
        let adapter = SparkleAdapter::new(source);
        let error = adapter
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .expect_err("refresh should be unsupported");
        assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    }

    struct FixtureSource {
        detect_result: AdapterResult<SparkleDetectOutput>,
    }

    impl SparkleSource for FixtureSource {
        fn detect(&self) -> AdapterResult<SparkleDetectOutput> {
            self.detect_result.clone()
        }
    }
}
