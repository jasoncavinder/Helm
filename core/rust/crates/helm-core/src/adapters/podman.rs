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

const PODMAN_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

const PODMAN_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Podman,
    display_name: "podman",
    category: ManagerCategory::ContainerVm,
    authority: ManagerAuthority::Standard,
    capabilities: PODMAN_CAPABILITIES,
};

const PODMAN_COMMAND: &str = "podman";
const HOMEBREW_COMMAND: &str = "brew";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const PODMAN_BREW_FORMULA: &str = "podman";
const PODMAN_PACKAGE_LABEL: &str = "podman";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PodmanDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait PodmanSource: Send + Sync {
    fn detect(&self) -> AdapterResult<PodmanDetectOutput>;
    fn homebrew_info(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
}

pub struct PodmanAdapter<S: PodmanSource> {
    source: S,
}

impl<S: PodmanSource> PodmanAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: PodmanSource> ManagerAdapter for PodmanAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &PODMAN_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_podman_version(&output.version_output);
                let installed = version.is_some();
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: output.executable_path,
                    version,
                }))
            }
            AdapterRequest::Refresh(_) => {
                let output = self.source.detect()?;
                let version = parse_podman_version(&output.version_output);
                if version.is_none() {
                    return Ok(AdapterResponse::SnapshotSync {
                        installed: Some(Vec::new()),
                        outdated: Some(Vec::new()),
                    });
                }

                let outdated = if parse_homebrew_formula_installed(&self.source.homebrew_info()?)? {
                    parse_podman_outdated(&self.source.list_outdated()?)?
                } else {
                    Vec::new()
                };
                Ok(AdapterResponse::SnapshotSync {
                    installed: Some(vec![InstalledPackage {
                        package: PackageRef {
                            manager: ManagerId::Podman,
                            name: PODMAN_PACKAGE_LABEL.to_string(),
                        },
                        package_identifier: None,
                        installed_version: version,
                        pinned: false,
                        runtime_state: Default::default(),
                    }]),
                    outdated: Some(outdated),
                })
            }
            AdapterRequest::ListInstalled(_) => {
                let output = self.source.detect()?;
                let version = parse_podman_version(&output.version_output);
                let installed = version.is_some();
                let packages = if installed {
                    vec![InstalledPackage {
                        package: PackageRef {
                            manager: ManagerId::Podman,
                            name: PODMAN_PACKAGE_LABEL.to_string(),
                        },
                        package_identifier: None,
                        installed_version: version,
                        pinned: false,
                        runtime_state: Default::default(),
                    }]
                } else {
                    Vec::new()
                };
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let output = self.source.detect()?;
                if parse_podman_version(&output.version_output).is_none() {
                    return Ok(AdapterResponse::OutdatedPackages(Vec::new()));
                }

                let packages = if parse_homebrew_formula_installed(&self.source.homebrew_info()?)? {
                    let raw = self.source.list_outdated()?;
                    parse_podman_outdated(&raw)?
                } else {
                    Vec::new()
                };
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Podman),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "podman adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn podman_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    podman_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(PODMAN_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn podman_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    podman_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(HOMEBREW_COMMAND).args([
            "outdated",
            "--json=v2",
            "--formula",
            PODMAN_BREW_FORMULA,
        ]),
        LIST_TIMEOUT,
    )
}

pub fn podman_homebrew_info_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    podman_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(HOMEBREW_COMMAND).args([
            "info",
            "--formula",
            "--json=v2",
            PODMAN_BREW_FORMULA,
        ]),
        LIST_TIMEOUT,
    )
}

fn podman_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Podman, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_podman_version(output: &str) -> Option<String> {
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(rest) = line.strip_prefix("podman version ") {
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

fn parse_podman_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
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
            if name != PODMAN_BREW_FORMULA {
                continue;
            }

            let candidate_version = parse_brew_current_version(entry);
            let Some(candidate_version) = candidate_version else {
                continue;
            };

            packages.push(OutdatedPackage {
                package: PackageRef {
                    manager: ManagerId::Podman,
                    name: PODMAN_PACKAGE_LABEL.to_string(),
                },
                package_identifier: None,
                installed_version: parse_brew_installed_version(entry),
                candidate_version,
                pinned: entry
                    .get("pinned")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                restart_required: false,
                runtime_state: Default::default(),
            });
        }
    }

    Ok(packages)
}

fn parse_homebrew_formula_installed(output: &str) -> AdapterResult<bool> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }

    let json: Value = serde_json::from_str(trimmed)
        .map_err(|error| parse_error(&format!("invalid brew info JSON: {error}")))?;

    Ok(json
        .get("formulae")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(homebrew_formula_entry_is_installed))
}

fn homebrew_formula_entry_is_installed(entry: &Value) -> bool {
    entry.get("name").and_then(Value::as_str) == Some(PODMAN_BREW_FORMULA)
        && entry
            .get("installed")
            .and_then(Value::as_array)
            .is_some_and(|installed| !installed.is_empty())
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
        manager: Some(ManagerId::Podman),
        task: None,
        action: None,
        kind: CoreErrorKind::ParseFailure,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter,
    };
    use crate::adapters::podman::{
        PODMAN_PACKAGE_LABEL, PodmanAdapter, PodmanDetectOutput, PodmanSource,
        parse_podman_outdated, parse_podman_version, podman_detect_request,
        podman_homebrew_info_request, podman_list_outdated_request,
    };
    use crate::models::{ManagerAction, ManagerId, TaskType};

    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/podman/outdated_brew.json");
    const INSTALLED_FIXTURE: &str = r#"{
  "formulae": [
    {
      "name": "podman",
      "installed": [
        { "version": "5.4.0" }
      ]
    }
  ],
  "casks": []
}"#;

    #[test]
    fn parses_podman_version_from_standard_output() {
        let version = parse_podman_version("podman version 5.4.0\n");
        assert_eq!(version.as_deref(), Some("5.4.0"));
    }

    #[test]
    fn parses_outdated_podman_from_fixture() {
        let packages = parse_podman_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.manager, ManagerId::Podman);
        assert_eq!(packages[0].package.name, PODMAN_PACKAGE_LABEL);
        assert_eq!(packages[0].installed_version.as_deref(), Some("5.4.0"));
        assert_eq!(packages[0].candidate_version, "5.5.0");
    }

    #[test]
    fn detect_request_has_expected_shape() {
        let request = podman_detect_request(None);
        assert_eq!(request.manager, ManagerId::Podman);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program.to_str(), Some("podman"));
        assert_eq!(request.command.args, vec!["--version"]);
    }

    #[test]
    fn list_outdated_request_has_expected_shape() {
        let request = podman_list_outdated_request(None);
        assert_eq!(request.manager, ManagerId::Podman);
        assert_eq!(request.task_type, TaskType::Refresh);
        assert_eq!(request.action, ManagerAction::ListOutdated);
        assert_eq!(request.command.program.to_str(), Some("brew"));
        assert_eq!(
            request.command.args,
            vec!["outdated", "--json=v2", "--formula", "podman"]
        );
    }

    #[test]
    fn homebrew_info_request_has_expected_shape() {
        let request = podman_homebrew_info_request(None);
        assert_eq!(request.manager, ManagerId::Podman);
        assert_eq!(request.task_type, TaskType::Refresh);
        assert_eq!(request.action, ManagerAction::ListInstalled);
        assert_eq!(request.command.program.to_str(), Some("brew"));
        assert_eq!(
            request.command.args,
            vec!["info", "--formula", "--json=v2", "podman"]
        );
    }

    #[test]
    fn adapter_list_installed_returns_single_status_package_when_detected() {
        let source = FixtureSource {
            detect_result: Ok(PodmanDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/podman")),
                version_output: "podman version 5.4.0".to_string(),
            }),
            homebrew_info_result: Ok(INSTALLED_FIXTURE.to_string()),
            list_outdated_result: Ok(String::new()),
        };
        let adapter = PodmanAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();

        let AdapterResponse::InstalledPackages(packages) = response else {
            panic!("expected installed packages response");
        };

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.manager, ManagerId::Podman);
        assert_eq!(packages[0].package.name, PODMAN_PACKAGE_LABEL);
        assert_eq!(packages[0].installed_version.as_deref(), Some("5.4.0"));
    }

    #[test]
    fn adapter_list_outdated_parses_brew_payload() {
        let source = FixtureSource {
            detect_result: Ok(PodmanDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/podman")),
                version_output: "podman version 5.4.0".to_string(),
            }),
            homebrew_info_result: Ok(INSTALLED_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
        };
        let adapter = PodmanAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        let AdapterResponse::OutdatedPackages(packages) = response else {
            panic!("expected outdated packages response");
        };

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].candidate_version, "5.5.0");
    }

    #[test]
    fn adapter_list_outdated_skips_non_homebrew_installations() {
        let source = FixtureSource {
            detect_result: Ok(PodmanDetectOutput {
                executable_path: Some(PathBuf::from("/usr/local/bin/podman")),
                version_output: "podman version 5.4.0".to_string(),
            }),
            homebrew_info_result: Ok(r#"{"formulae":[],"casks":[]}"#.to_string()),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
        };
        let adapter = PodmanAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        let AdapterResponse::OutdatedPackages(packages) = response else {
            panic!("expected outdated packages response");
        };

        assert!(packages.is_empty());
    }

    #[test]
    fn adapter_detect_marks_not_installed_when_source_reports_nothing() {
        let source = FixtureSource {
            detect_result: Ok(PodmanDetectOutput {
                executable_path: None,
                version_output: String::new(),
            }),
            homebrew_info_result: Ok(String::new()),
            list_outdated_result: Ok(String::new()),
        };
        let adapter = PodmanAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();

        let AdapterResponse::Detection(info) = response else {
            panic!("expected detection response");
        };
        assert!(!info.installed);
    }

    struct FixtureSource {
        detect_result: AdapterResult<PodmanDetectOutput>,
        homebrew_info_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
    }

    impl PodmanSource for FixtureSource {
        fn detect(&self) -> AdapterResult<PodmanDetectOutput> {
            self.detect_result.clone()
        }

        fn homebrew_info(&self) -> AdapterResult<String> {
            self.homebrew_info_result.clone()
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            self.list_outdated_result.clone()
        }
    }
}
