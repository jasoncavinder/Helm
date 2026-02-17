use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::cargo::{
    parse_cargo_installed, parse_cargo_outdated, parse_cargo_search, parse_cargo_version,
};
use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, ManagerAction,
    ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId, PackageRef, SearchQuery,
    TaskId, TaskType,
};

const CARGO_BINSTALL_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const CARGO_BINSTALL_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::CargoBinstall,
    display_name: "cargo-binstall",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: CARGO_BINSTALL_CAPABILITIES,
};

const CARGO_BINSTALL_COMMAND: &str = "cargo-binstall";
const CARGO_COMMAND: &str = "cargo";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CargoBinstallDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait CargoBinstallSource: Send + Sync {
    fn detect(&self) -> AdapterResult<CargoBinstallDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall(&self, name: &str) -> AdapterResult<String>;
    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct CargoBinstallAdapter<S: CargoBinstallSource> {
    source: S,
}

impl<S: CargoBinstallSource> CargoBinstallAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: CargoBinstallSource> ManagerAdapter for CargoBinstallAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &CARGO_BINSTALL_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_cargo_binstall_version(&output.version_output);
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
                let mut packages = parse_cargo_installed(&raw)?;
                for package in &mut packages {
                    package.package.manager = ManagerId::CargoBinstall;
                }
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let mut packages = parse_cargo_outdated(&raw).map_err(|mut error| {
                    error.manager = Some(ManagerId::CargoBinstall);
                    error
                })?;
                for package in &mut packages {
                    package.package.manager = ManagerId::CargoBinstall;
                }
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search(search_request.query.text.as_str())?;
                let mut results = parse_cargo_search(&raw, &search_request.query)?;
                for result in &mut results {
                    result.result.package.manager = ManagerId::CargoBinstall;
                    result.source_manager = ManagerId::CargoBinstall;
                }
                Ok(AdapterResponse::SearchResults(results))
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
                    manager: ManagerId::CargoBinstall,
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
                manager: Some(ManagerId::CargoBinstall),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "cargo-binstall adapter action not implemented in this milestone"
                    .to_string(),
            }),
        }
    }
}

pub fn cargo_binstall_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    cargo_binstall_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(CARGO_BINSTALL_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn cargo_binstall_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    cargo_binstall_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(CARGO_COMMAND).args(["install", "--list"]),
        LIST_TIMEOUT,
    )
}

pub fn cargo_binstall_search_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    cargo_binstall_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(CARGO_COMMAND)
            .args(["search", "--limit", "20", "--color", "never"])
            .arg(query.text.clone()),
        SEARCH_TIMEOUT,
    )
}

pub fn cargo_binstall_search_single_request(
    task_id: Option<TaskId>,
    crate_name: &str,
) -> ProcessSpawnRequest {
    cargo_binstall_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(CARGO_COMMAND)
            .args(["search", "--limit", "1", "--color", "never"])
            .arg(crate_name),
        SEARCH_TIMEOUT,
    )
}

pub fn cargo_binstall_install_request(
    task_id: Option<TaskId>,
    crate_name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let mut command = CommandSpec::new(CARGO_BINSTALL_COMMAND).arg(crate_name);
    if let Some(version) = version
        && !version.trim().is_empty()
    {
        command = command.args(["--version", version.trim()]);
    }

    cargo_binstall_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        command,
        MUTATION_TIMEOUT,
    )
}

pub fn cargo_binstall_uninstall_request(
    task_id: Option<TaskId>,
    crate_name: &str,
) -> ProcessSpawnRequest {
    cargo_binstall_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(CARGO_COMMAND).args(["uninstall", crate_name]),
        MUTATION_TIMEOUT,
    )
}

pub fn cargo_binstall_upgrade_request(
    task_id: Option<TaskId>,
    crate_name: &str,
) -> ProcessSpawnRequest {
    cargo_binstall_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(CARGO_BINSTALL_COMMAND).args(["--force", crate_name]),
        MUTATION_TIMEOUT,
    )
}

fn cargo_binstall_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request =
        ProcessSpawnRequest::new(ManagerId::CargoBinstall, task_type, action, command)
            .requires_elevation(false)
            .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

pub(crate) fn parse_cargo_binstall_version(output: &str) -> Option<String> {
    parse_cargo_version(output).or_else(|| {
        let line = output
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())?;
        let rest = line.strip_prefix("cargo-binstall ")?;
        let version = rest.split_whitespace().next()?.trim();
        if version.is_empty() {
            return None;
        }
        Some(version.to_string())
    })
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
    use crate::models::{
        CoreErrorKind, ManagerAction, ManagerId, PackageRef, SearchQuery, TaskId, TaskType,
    };

    use super::{
        CargoBinstallAdapter, CargoBinstallDetectOutput, CargoBinstallSource,
        cargo_binstall_detect_request, cargo_binstall_install_request,
        cargo_binstall_list_installed_request, cargo_binstall_search_request,
        cargo_binstall_uninstall_request, cargo_binstall_upgrade_request,
        parse_cargo_binstall_version,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/cargo_binstall/version.txt");
    const INSTALLED_FIXTURE: &str =
        include_str!("../../tests/fixtures/cargo_binstall/install_list.txt");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/cargo_binstall/search.txt");
    const OUTDATED_FIXTURE: &str =
        include_str!("../../tests/fixtures/cargo_binstall/outdated.json");

    #[test]
    fn parses_cargo_binstall_version_from_fixture() {
        assert_eq!(
            parse_cargo_binstall_version(VERSION_FIXTURE).as_deref(),
            Some("1.12.1")
        );
    }

    #[test]
    fn request_builders_use_structured_args() {
        let detect = cargo_binstall_detect_request(Some(TaskId(9)));
        assert_eq!(detect.manager, ManagerId::CargoBinstall);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.command.program, PathBuf::from("cargo-binstall"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let list = cargo_binstall_list_installed_request(None);
        assert_eq!(list.command.program, PathBuf::from("cargo"));
        assert_eq!(list.command.args, vec!["install", "--list"]);

        let search = cargo_binstall_search_request(
            None,
            &SearchQuery {
                text: "ripgrep".to_string(),
                issued_at: std::time::SystemTime::now(),
            },
        );
        assert_eq!(search.command.program, PathBuf::from("cargo"));
        assert_eq!(
            search.command.args,
            vec!["search", "--limit", "20", "--color", "never", "ripgrep"]
        );

        let install = cargo_binstall_install_request(None, "ripgrep", Some("14.1.1"));
        assert_eq!(install.command.args, vec!["ripgrep", "--version", "14.1.1"]);

        let uninstall = cargo_binstall_uninstall_request(None, "ripgrep");
        assert_eq!(uninstall.command.program, PathBuf::from("cargo"));
        assert_eq!(uninstall.command.args, vec!["uninstall", "ripgrep"]);

        let upgrade = cargo_binstall_upgrade_request(None, "ripgrep");
        assert_eq!(upgrade.command.args, vec!["--force", "ripgrep"]);
    }

    #[derive(Clone)]
    struct StubCargoBinstallSource {
        detect_calls: Arc<AtomicUsize>,
        detect_result: AdapterResult<CargoBinstallDetectOutput>,
        installed_result: AdapterResult<String>,
        outdated_result: AdapterResult<String>,
        search_result: AdapterResult<String>,
    }

    impl StubCargoBinstallSource {
        fn success() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                detect_result: Ok(CargoBinstallDetectOutput {
                    executable_path: Some(PathBuf::from("/Users/test/.cargo/bin/cargo-binstall")),
                    version_output: VERSION_FIXTURE.to_string(),
                }),
                installed_result: Ok(INSTALLED_FIXTURE.to_string()),
                outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
                search_result: Ok(SEARCH_FIXTURE.to_string()),
            }
        }
    }

    impl CargoBinstallSource for StubCargoBinstallSource {
        fn detect(&self) -> AdapterResult<CargoBinstallDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            self.detect_result.clone()
        }

        fn list_installed(&self) -> AdapterResult<String> {
            self.installed_result.clone()
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            self.outdated_result.clone()
        }

        fn search(&self, _query: &str) -> AdapterResult<String> {
            self.search_result.clone()
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
    fn execute_supported_requests() {
        let adapter = CargoBinstallAdapter::new(StubCargoBinstallSource::success());

        match adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap()
        {
            AdapterResponse::Detection(info) => assert!(info.installed),
            other => panic!("unexpected response: {other:?}"),
        }

        match adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap()
        {
            AdapterResponse::InstalledPackages(packages) => {
                assert_eq!(packages.len(), 3);
                assert_eq!(packages[0].package.manager, ManagerId::CargoBinstall);
            }
            other => panic!("unexpected response: {other:?}"),
        }

        match adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap()
        {
            AdapterResponse::OutdatedPackages(packages) => {
                assert_eq!(packages.len(), 2);
                assert_eq!(packages[0].package.manager, ManagerId::CargoBinstall);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_returns_mutation() {
        let adapter = CargoBinstallAdapter::new(StubCargoBinstallSource::success());

        match adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::CargoBinstall,
                    name: "ripgrep".to_string(),
                },
                version: Some("14.1.1".to_string()),
            }))
            .unwrap()
        {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Install);
                assert_eq!(mutation.package.manager, ManagerId::CargoBinstall);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn invalid_outdated_json_returns_parse_error() {
        let adapter = CargoBinstallAdapter::new(StubCargoBinstallSource {
            detect_calls: Arc::new(AtomicUsize::new(0)),
            detect_result: Ok(CargoBinstallDetectOutput {
                executable_path: Some(PathBuf::from("/Users/test/.cargo/bin/cargo-binstall")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            installed_result: Ok(INSTALLED_FIXTURE.to_string()),
            outdated_result: Ok("{bad-json".to_string()),
            search_result: Ok(SEARCH_FIXTURE.to_string()),
        });

        let error = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .expect_err("expected parse failure");
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
        assert_eq!(error.manager, Some(ManagerId::CargoBinstall));
    }
}
