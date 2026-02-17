use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const PIPX_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const PIPX_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Pipx,
    display_name: "pipx",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: PIPX_CAPABILITIES,
};

const PIPX_COMMAND: &str = "pipx";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipxDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait PipxSource: Send + Sync {
    fn detect(&self) -> AdapterResult<PipxDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall(&self, name: &str) -> AdapterResult<String>;
    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct PipxAdapter<S: PipxSource> {
    source: S,
}

impl<S: PipxSource> PipxAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: PipxSource> ManagerAdapter for PipxAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &PIPX_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_pipx_version(&output.version_output);
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
                let raw = self.source.list_installed()?;
                let packages = parse_pipx_list(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_pipx_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Install(install_request) => {
                let _ = self.source.install(
                    &install_request.package.name,
                    install_request.version.as_deref(),
                )?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: install_request.package,
                    action: ManagerAction::Install,
                    before_version: None,
                    after_version: install_request.version,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                let _ = self.source.uninstall(&uninstall_request.package.name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Pipx,
                    name: "__all__".to_string(),
                });
                let target_name = if package.name == "__all__" {
                    None
                } else {
                    Some(package.name.as_str())
                };
                let _ = self.source.upgrade(target_name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Pipx),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "pipx adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn pipx_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    pipx_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(PIPX_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn pipx_list_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    pipx_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(PIPX_COMMAND).args(["list", "--json"]),
        LIST_TIMEOUT,
    )
}

pub fn pipx_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    // pipx does not expose a stable dedicated outdated JSON command across versions.
    // We parse optional latest-version metadata from `pipx list --json` when available.
    pipx_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(PIPX_COMMAND).args(["list", "--json"]),
        LIST_TIMEOUT,
    )
}

pub fn pipx_install_request(
    task_id: Option<TaskId>,
    name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let spec = match version {
        Some(version) if !version.trim().is_empty() => format!("{name}=={}", version.trim()),
        _ => name.to_string(),
    };

    pipx_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(PIPX_COMMAND).args(["install"]).arg(spec),
        MUTATION_TIMEOUT,
    )
}

pub fn pipx_uninstall_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    pipx_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(PIPX_COMMAND).args(["uninstall", name]),
        MUTATION_TIMEOUT,
    )
}

pub fn pipx_upgrade_request(task_id: Option<TaskId>, name: Option<&str>) -> ProcessSpawnRequest {
    let command = if let Some(name) = name {
        CommandSpec::new(PIPX_COMMAND).args(["upgrade", name])
    } else {
        CommandSpec::new(PIPX_COMMAND).arg("upgrade-all")
    };

    pipx_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
}

fn pipx_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Pipx, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_pipx_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let version = line.split_whitespace().next()?.trim();
    if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(version.to_string())
}

#[derive(Debug, Deserialize)]
struct PipxListRoot {
    #[serde(default)]
    venvs: BTreeMap<String, PipxVenv>,
}

#[derive(Debug, Deserialize)]
struct PipxVenv {
    metadata: Option<PipxMetadata>,
}

#[derive(Debug, Deserialize)]
struct PipxMetadata {
    main_package: Option<PipxPackageMetadata>,
}

#[derive(Debug, Deserialize)]
struct PipxPackageMetadata {
    package: Option<String>,
    package_or_url: Option<String>,
    package_version: Option<String>,
    latest_version: Option<String>,
}

fn normalize_name(name: Option<&str>, fallback: &str) -> String {
    let selected = name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback);

    if let Some((left, _right)) = selected.split_once(['=', '@']) {
        let trimmed = left.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    selected.trim().to_string()
}

fn parse_pipx_list(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let root: PipxListRoot = serde_json::from_str(output)
        .map_err(|e| parse_error(&format!("invalid pipx list JSON: {e}")))?;

    let mut packages = Vec::new();
    for (venv_name, venv) in root.venvs {
        let main = venv.metadata.and_then(|metadata| metadata.main_package);
        let name = normalize_name(
            main.as_ref()
                .and_then(|main| main.package.as_deref().or(main.package_or_url.as_deref())),
            &venv_name,
        );
        if name.is_empty() {
            continue;
        }

        let installed_version = main
            .as_ref()
            .and_then(|main| main.package_version.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Pipx,
                name,
            },
            installed_version,
            pinned: false,
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_pipx_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let root: PipxListRoot = serde_json::from_str(output)
        .map_err(|e| parse_error(&format!("invalid pipx list JSON: {e}")))?;

    let mut packages = Vec::new();
    for (venv_name, venv) in root.venvs {
        let Some(main) = venv.metadata.and_then(|metadata| metadata.main_package) else {
            continue;
        };

        let installed_version = main
            .package_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let candidate_version = main
            .latest_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let (Some(installed_version), Some(candidate_version)) =
            (installed_version, candidate_version)
        else {
            continue;
        };

        if installed_version == candidate_version {
            continue;
        }

        let name = normalize_name(
            main.package.as_deref().or(main.package_or_url.as_deref()),
            &venv_name,
        );
        if name.is_empty() {
            continue;
        }

        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Pipx,
                name,
            },
            installed_version: Some(installed_version),
            candidate_version,
            pinned: false,
            restart_required: false,
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Pipx),
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
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, PackageRef, TaskId, TaskType};

    use super::{
        PipxAdapter, PipxDetectOutput, PipxSource, parse_pipx_list, parse_pipx_outdated,
        parse_pipx_version, pipx_detect_request, pipx_install_request, pipx_list_outdated_request,
        pipx_list_request, pipx_uninstall_request, pipx_upgrade_request,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/pipx/version.txt");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/pipx/list_global.json");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/pipx/list_outdated.json");

    #[test]
    fn parses_pipx_version_from_fixture() {
        let version = parse_pipx_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("1.7.1"));
    }

    #[test]
    fn parses_installed_from_fixture() {
        let packages = parse_pipx_list(LIST_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "black");
        assert_eq!(packages[0].installed_version.as_deref(), Some("24.10.0"));
        assert_eq!(packages[1].package.name, "httpie");
    }

    #[test]
    fn parses_outdated_when_latest_version_is_available() {
        let packages = parse_pipx_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.name, "httpie");
        assert_eq!(packages[0].installed_version.as_deref(), Some("3.2.2"));
        assert_eq!(packages[0].candidate_version, "3.2.4");
    }

    #[test]
    fn request_builders_use_expected_commands() {
        let detect = pipx_detect_request(Some(TaskId(12)));
        assert_eq!(detect.manager, ManagerId::Pipx);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.action, ManagerAction::Detect);
        assert_eq!(detect.command.program, PathBuf::from("pipx"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let list = pipx_list_request(None);
        assert_eq!(list.command.args, vec!["list", "--json"]);

        let outdated = pipx_list_outdated_request(None);
        assert_eq!(outdated.command.args, vec!["list", "--json"]);

        let install = pipx_install_request(None, "black", Some("24.10.0"));
        assert_eq!(install.command.args, vec!["install", "black==24.10.0"]);

        let uninstall = pipx_uninstall_request(None, "black");
        assert_eq!(uninstall.command.args, vec!["uninstall", "black"]);

        let upgrade = pipx_upgrade_request(None, Some("black"));
        assert_eq!(upgrade.command.args, vec!["upgrade", "black"]);

        let upgrade_all = pipx_upgrade_request(None, None);
        assert_eq!(upgrade_all.command.args, vec!["upgrade-all"]);
    }

    #[derive(Clone)]
    struct StubPipxSource {
        detect_calls: Arc<AtomicUsize>,
        detect_result: AdapterResult<PipxDetectOutput>,
        list_result: AdapterResult<String>,
        outdated_result: AdapterResult<String>,
    }

    impl StubPipxSource {
        fn success() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                detect_result: Ok(PipxDetectOutput {
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/pipx")),
                    version_output: "1.7.1\n".to_string(),
                }),
                list_result: Ok(LIST_FIXTURE.to_string()),
                outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
            }
        }
    }

    impl PipxSource for StubPipxSource {
        fn detect(&self) -> AdapterResult<PipxDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            self.detect_result.clone()
        }

        fn list_installed(&self) -> AdapterResult<String> {
            self.list_result.clone()
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            self.outdated_result.clone()
        }

        fn install(&self, _name: &str, _version: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn uninstall(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade(&self, _name: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }
    }

    #[test]
    fn execute_detect_returns_detection_response() {
        let source = StubPipxSource::success();
        let calls = source.detect_calls.clone();
        let adapter = PipxAdapter::new(source);

        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .expect("detect should succeed");

        match response {
            AdapterResponse::Detection(detection) => {
                assert!(detection.installed);
                assert_eq!(detection.version.as_deref(), Some("1.7.1"));
            }
            other => panic!("unexpected response: {other:?}"),
        }

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn execute_list_requests_return_parsed_payloads() {
        let adapter = PipxAdapter::new(StubPipxSource::success());

        let installed = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .expect("list installed should succeed");
        match installed {
            AdapterResponse::InstalledPackages(packages) => assert_eq!(packages.len(), 2),
            other => panic!("unexpected response: {other:?}"),
        }

        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .expect("list outdated should succeed");
        match outdated {
            AdapterResponse::OutdatedPackages(packages) => assert_eq!(packages.len(), 1),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_returns_mutation_response() {
        let adapter = PipxAdapter::new(StubPipxSource::success());

        let response = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pipx,
                    name: "black".to_string(),
                },
                version: Some("24.10.0".to_string()),
            }))
            .expect("install should succeed");

        match response {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Install);
                assert_eq!(mutation.package.manager, ManagerId::Pipx);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn invalid_json_produces_parse_failure() {
        let error = parse_pipx_list("{not-json").expect_err("expected parse failure");
        assert_eq!(error.manager, Some(ManagerId::Pipx));
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
    }
}
