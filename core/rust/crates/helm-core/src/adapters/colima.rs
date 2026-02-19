use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const COLIMA_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

const COLIMA_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Colima,
    display_name: "colima",
    category: ManagerCategory::ContainerVm,
    authority: ManagerAuthority::Standard,
    capabilities: COLIMA_CAPABILITIES,
};

const COLIMA_COMMAND: &str = "colima";
const HOMEBREW_COMMAND: &str = "brew";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const COLIMA_BREW_FORMULA: &str = "colima";
const COLIMA_PACKAGE_LABEL: &str = "colima";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColimaDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait ColimaSource: Send + Sync {
    fn detect(&self) -> AdapterResult<ColimaDetectOutput>;
    fn list_outdated(&self) -> AdapterResult<String>;
}

pub struct ColimaAdapter<S: ColimaSource> {
    source: S,
}

impl<S: ColimaSource> ColimaAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: ColimaSource> ManagerAdapter for ColimaAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &COLIMA_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_colima_version(&output.version_output);
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
                let version = parse_colima_version(&output.version_output);
                let installed = output.executable_path.is_some() || version.is_some();
                let packages = if installed {
                    vec![InstalledPackage {
                        package: PackageRef {
                            manager: ManagerId::Colima,
                            name: COLIMA_PACKAGE_LABEL.to_string(),
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
                let packages = parse_colima_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Colima),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "colima adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn colima_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    colima_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(COLIMA_COMMAND).arg("version"),
        DETECT_TIMEOUT,
    )
}

pub fn colima_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    colima_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(HOMEBREW_COMMAND).args([
            "outdated",
            "--json=v2",
            "--formula",
            COLIMA_BREW_FORMULA,
        ]),
        LIST_TIMEOUT,
    )
}

fn colima_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Colima, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_colima_version(output: &str) -> Option<String> {
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(rest) = line.strip_prefix("colima version ") {
            let token = rest
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .find(|token| looks_like_version_token(token))?;
            return Some(normalize_version_token(token));
        }

        if looks_like_version_token(line) {
            return Some(normalize_version_token(line));
        }

        if let Some(token) = line
            .split(|ch: char| ch.is_whitespace() || ch == ',')
            .find(|token| looks_like_version_token(token))
        {
            return Some(normalize_version_token(token));
        }
    }
    None
}

fn looks_like_version_token(token: &str) -> bool {
    let normalized = normalize_version_token(token);
    let starts_with_digit = normalized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit());
    starts_with_digit
        && normalized
            .chars()
            .any(|ch| ch == '.' || ch.is_ascii_digit())
}

fn normalize_version_token(token: &str) -> String {
    token
        .trim()
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '(' | ')' | ',' | ';'))
        .to_string()
}

fn parse_colima_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let json: Value = serde_json::from_str(trimmed)
        .map_err(|error| parse_error(&format!("invalid brew outdated JSON: {error}")))?;
    let mut packages = Vec::new();

    if let Some(entries) = json.get("formulae").and_then(Value::as_array) {
        for entry in entries {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if name != COLIMA_BREW_FORMULA {
                continue;
            }

            let candidate_version = parse_brew_current_version(entry);
            let Some(candidate_version) = candidate_version else {
                continue;
            };

            packages.push(OutdatedPackage {
                package: PackageRef {
                    manager: ManagerId::Colima,
                    name: COLIMA_PACKAGE_LABEL.to_string(),
                },
                installed_version: parse_brew_installed_version(entry),
                candidate_version,
                pinned: entry
                    .get("pinned")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                restart_required: false,
            });
        }
    }

    Ok(packages)
}

fn parse_brew_current_version(entry: &Value) -> Option<String> {
    entry
        .get("current_version")
        .and_then(Value::as_str)
        .or_else(|| entry.get("version").and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_brew_installed_version(entry: &Value) -> Option<String> {
    if let Some(version) = entry.get("installed_version").and_then(Value::as_str) {
        let version = version.trim();
        if !version.is_empty() {
            return Some(version.to_string());
        }
    }

    if let Some(version) = entry.get("installed_versions").and_then(Value::as_str) {
        let version = version.trim();
        if !version.is_empty() {
            return Some(version.to_string());
        }
    }

    entry
        .get("installed_versions")
        .and_then(Value::as_array)
        .and_then(|versions| versions.first())
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Colima),
        task: None,
        action: None,
        kind: CoreErrorKind::ParseFailure,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapters::colima::{
        COLIMA_PACKAGE_LABEL, ColimaAdapter, ColimaDetectOutput, ColimaSource,
        colima_detect_request, colima_list_outdated_request, parse_colima_outdated,
        parse_colima_version,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter,
    };
    use crate::models::{ManagerAction, ManagerId, TaskType};

    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/colima/outdated_brew.json");

    #[test]
    fn parses_colima_version_from_standard_output() {
        let version = parse_colima_version("colima version 0.8.1\n");
        assert_eq!(version.as_deref(), Some("0.8.1"));
    }

    #[test]
    fn parses_outdated_colima_from_fixture() {
        let packages = parse_colima_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.manager, ManagerId::Colima);
        assert_eq!(packages[0].package.name, COLIMA_PACKAGE_LABEL);
        assert_eq!(packages[0].installed_version.as_deref(), Some("0.8.0"));
        assert_eq!(packages[0].candidate_version, "0.8.1");
    }

    #[test]
    fn detect_request_has_expected_shape() {
        let request = colima_detect_request(None);
        assert_eq!(request.manager, ManagerId::Colima);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program.to_str(), Some("colima"));
        assert_eq!(request.command.args, vec!["version"]);
    }

    #[test]
    fn list_outdated_request_has_expected_shape() {
        let request = colima_list_outdated_request(None);
        assert_eq!(request.manager, ManagerId::Colima);
        assert_eq!(request.task_type, TaskType::Refresh);
        assert_eq!(request.action, ManagerAction::ListOutdated);
        assert_eq!(request.command.program.to_str(), Some("brew"));
        assert_eq!(
            request.command.args,
            vec!["outdated", "--json=v2", "--formula", "colima"]
        );
    }

    #[test]
    fn adapter_list_installed_returns_single_status_package_when_detected() {
        let source = FixtureSource {
            detect_result: Ok(ColimaDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/colima")),
                version_output: "colima version 0.8.1".to_string(),
            }),
            list_outdated_result: Ok(String::new()),
        };
        let adapter = ColimaAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();

        let AdapterResponse::InstalledPackages(packages) = response else {
            panic!("expected installed packages response");
        };

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.manager, ManagerId::Colima);
        assert_eq!(packages[0].package.name, COLIMA_PACKAGE_LABEL);
        assert_eq!(packages[0].installed_version.as_deref(), Some("0.8.1"));
    }

    #[test]
    fn adapter_list_outdated_parses_brew_payload() {
        let source = FixtureSource {
            detect_result: Ok(ColimaDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/colima")),
                version_output: "colima version 0.8.1".to_string(),
            }),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
        };
        let adapter = ColimaAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        let AdapterResponse::OutdatedPackages(packages) = response else {
            panic!("expected outdated packages response");
        };

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].candidate_version, "0.8.1");
    }

    #[test]
    fn adapter_detect_marks_not_installed_when_source_reports_nothing() {
        let source = FixtureSource {
            detect_result: Ok(ColimaDetectOutput {
                executable_path: None,
                version_output: String::new(),
            }),
            list_outdated_result: Ok(String::new()),
        };
        let adapter = ColimaAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();

        let AdapterResponse::Detection(info) = response else {
            panic!("expected detection response");
        };
        assert!(!info.installed);
    }

    struct FixtureSource {
        detect_result: AdapterResult<ColimaDetectOutput>,
        list_outdated_result: AdapterResult<String>,
    }

    impl ColimaSource for FixtureSource {
        fn detect(&self) -> AdapterResult<ColimaDetectOutput> {
            self.detect_result.clone()
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            self.list_outdated_result.clone()
        }
    }
}
