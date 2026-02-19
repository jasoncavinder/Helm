use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::adapters::homebrew::parse_homebrew_version;
use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const HOMEBREW_CASK_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

const HOMEBREW_CASK_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::HomebrewCask,
    display_name: "Homebrew (casks)",
    category: ManagerCategory::GuiApp,
    authority: ManagerAuthority::Standard,
    capabilities: HOMEBREW_CASK_CAPABILITIES,
};

const BREW_COMMAND: &str = "brew";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomebrewCaskDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait HomebrewCaskSource: Send + Sync {
    fn detect(&self) -> AdapterResult<HomebrewCaskDetectOutput>;
    fn list_installed_casks(&self) -> AdapterResult<String>;
    fn list_outdated_casks(&self) -> AdapterResult<String>;
}

pub struct HomebrewCaskAdapter<S: HomebrewCaskSource> {
    source: S,
}

impl<S: HomebrewCaskSource> HomebrewCaskAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: HomebrewCaskSource> ManagerAdapter for HomebrewCaskAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &HOMEBREW_CASK_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_homebrew_version(&output.version_output);
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
                let raw = self.source.list_installed_casks()?;
                let packages = parse_homebrew_cask_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated_casks()?;
                let packages = parse_homebrew_cask_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::HomebrewCask),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "homebrew cask adapter action not implemented in this milestone"
                    .to_string(),
            }),
        }
    }
}

pub fn homebrew_cask_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(BREW_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn homebrew_cask_config_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(BREW_COMMAND).arg("config"),
        DETECT_TIMEOUT,
    )
}

pub fn homebrew_cask_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(BREW_COMMAND).args(["info", "--cask", "--json=v2", "--installed"]),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_cask_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(BREW_COMMAND).args(["outdated", "--cask", "--json=v2"]),
        LIST_TIMEOUT,
    )
}

fn homebrew_cask_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::HomebrewCask, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_homebrew_cask_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let payload: Value = serde_json::from_str(trimmed)
        .map_err(|error| parse_error(&format!("invalid homebrew cask installed JSON: {error}")))?;

    let casks = payload
        .get("casks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut packages = Vec::new();
    for cask in casks {
        let Some(name) = cask_identifier(&cask) else {
            continue;
        };

        let installed_version =
            cask.get("installed")
                .and_then(Value::as_array)
                .and_then(|versions| {
                    versions
                        .iter()
                        .rev()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .find(|value| !value.is_empty())
                        .map(str::to_string)
                });

        let Some(installed_version) = installed_version else {
            continue;
        };

        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewCask,
                name,
            },
            installed_version: Some(installed_version),
            pinned: false,
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_homebrew_cask_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return Ok(Vec::new());
    }

    let payload: Value = serde_json::from_str(trimmed)
        .map_err(|error| parse_error(&format!("invalid homebrew cask outdated JSON: {error}")))?;

    let casks = payload
        .get("casks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut packages = Vec::new();
    for cask in casks {
        let Some(name) = cask_identifier(&cask) else {
            continue;
        };

        let candidate_version = cask
            .get("current_version")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let Some(candidate_version) = candidate_version else {
            continue;
        };

        let installed_version = cask
            .get("installed_versions")
            .and_then(Value::as_array)
            .and_then(|versions| {
                versions
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .find(|value| !value.is_empty())
                    .map(str::to_string)
            });

        if installed_version.as_deref() == Some(candidate_version.as_str()) {
            continue;
        }

        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewCask,
                name,
            },
            installed_version,
            candidate_version,
            pinned: false,
            restart_required: false,
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn cask_identifier(cask: &Value) -> Option<String> {
    if let Some(token) = cask
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(token.to_string());
    }

    if let Some(name) = cask
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(name.to_string());
    }

    cask.get("name")
        .and_then(Value::as_array)
        .and_then(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .find(|v| !v.is_empty())
        })
        .map(str::to_string)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::HomebrewCask),
        task: None,
        action: None,
        kind: CoreErrorKind::ParseFailure,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapters::homebrew_cask::{
        HomebrewCaskAdapter, HomebrewCaskDetectOutput, HomebrewCaskSource,
        homebrew_cask_detect_request, homebrew_cask_list_installed_request,
        homebrew_cask_list_outdated_request, parse_homebrew_cask_installed,
        parse_homebrew_cask_outdated,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, InstallRequest,
        ListInstalledRequest, ListOutdatedRequest, ManagerAdapter,
    };
    use crate::models::{ManagerAction, ManagerId, PackageRef, TaskType};

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew_cask/version.txt");
    const INSTALLED_FIXTURE: &str =
        include_str!("../../tests/fixtures/homebrew_cask/installed.json");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew_cask/outdated.json");

    #[test]
    fn parses_installed_casks_from_fixture() {
        let packages = parse_homebrew_cask_installed(INSTALLED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "google-chrome");
        assert_eq!(
            packages[0].installed_version.as_deref(),
            Some("133.0.6943.142")
        );
    }

    #[test]
    fn parses_outdated_casks_from_fixture() {
        let packages = parse_homebrew_cask_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "google-chrome");
        assert_eq!(
            packages[0].installed_version.as_deref(),
            Some("133.0.6943.142")
        );
        assert_eq!(packages[0].candidate_version, "134.0.6998.89");
    }

    #[test]
    fn detect_and_list_request_shapes_match_expected_commands() {
        let detect = homebrew_cask_detect_request(None);
        assert_eq!(detect.manager, ManagerId::HomebrewCask);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.action, ManagerAction::Detect);
        assert_eq!(detect.command.program.to_str(), Some("brew"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let installed = homebrew_cask_list_installed_request(None);
        assert_eq!(installed.task_type, TaskType::Refresh);
        assert_eq!(installed.action, ManagerAction::ListInstalled);
        assert_eq!(
            installed.command.args,
            vec!["info", "--cask", "--json=v2", "--installed"]
        );

        let outdated = homebrew_cask_list_outdated_request(None);
        assert_eq!(outdated.task_type, TaskType::Refresh);
        assert_eq!(outdated.action, ManagerAction::ListOutdated);
        assert_eq!(
            outdated.command.args,
            vec!["outdated", "--cask", "--json=v2"]
        );
    }

    #[test]
    fn adapter_executes_supported_read_only_requests() {
        let source = FixtureSource {
            detect_result: Ok(HomebrewCaskDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok(INSTALLED_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
        };
        let adapter = HomebrewCaskAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let AdapterResponse::Detection(info) = detect else {
            panic!("expected detection response");
        };
        assert!(info.installed);

        let installed = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();
        let AdapterResponse::InstalledPackages(packages) = installed else {
            panic!("expected installed response");
        };
        assert_eq!(packages.len(), 2);

        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();
        let AdapterResponse::OutdatedPackages(packages) = outdated else {
            panic!("expected outdated response");
        };
        assert_eq!(packages.len(), 2);
    }

    #[test]
    fn adapter_rejects_mutating_request() {
        let source = FixtureSource {
            detect_result: Ok(HomebrewCaskDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok(INSTALLED_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
        };
        let adapter = HomebrewCaskAdapter::new(source);
        let result = adapter.execute(AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: ManagerId::HomebrewCask,
                name: "iterm2".to_string(),
            },
            version: None,
        }));

        assert!(result.is_err());
    }

    struct FixtureSource {
        detect_result: AdapterResult<HomebrewCaskDetectOutput>,
        list_installed_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
    }

    impl HomebrewCaskSource for FixtureSource {
        fn detect(&self) -> AdapterResult<HomebrewCaskDetectOutput> {
            self.detect_result.clone()
        }

        fn list_installed_casks(&self) -> AdapterResult<String> {
            self.list_installed_result.clone()
        }

        fn list_outdated_casks(&self) -> AdapterResult<String> {
            self.list_outdated_result.clone()
        }
    }
}
