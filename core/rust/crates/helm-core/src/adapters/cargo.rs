use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const CARGO_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const CARGO_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Cargo,
    display_name: "Cargo",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: CARGO_CAPABILITIES,
};

const CARGO_COMMAND: &str = "cargo";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CargoDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait CargoSource: Send + Sync {
    fn detect(&self) -> AdapterResult<CargoDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall(&self, name: &str) -> AdapterResult<String>;
    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct CargoAdapter<S: CargoSource> {
    source: S,
}

impl<S: CargoSource> CargoAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: CargoSource> ManagerAdapter for CargoAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &CARGO_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_cargo_version(&output.version_output);
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
                let packages = parse_cargo_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_cargo_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search(search_request.query.text.as_str())?;
                let results = parse_cargo_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::Cargo,
                    ManagerAction::Install,
                    install_request.package.name.as_str(),
                )?;
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
                crate::adapters::validate_package_identifier(
                    ManagerId::Cargo,
                    ManagerAction::Uninstall,
                    uninstall_request.package.name.as_str(),
                )?;
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
                    manager: ManagerId::Cargo,
                    name: "__all__".to_string(),
                });

                let target_name = if package.name == "__all__" {
                    None
                } else {
                    crate::adapters::validate_package_identifier(
                        ManagerId::Cargo,
                        ManagerAction::Upgrade,
                        package.name.as_str(),
                    )?;
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
                manager: Some(ManagerId::Cargo),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "cargo adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn cargo_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    cargo_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(CARGO_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn cargo_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    cargo_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(CARGO_COMMAND).args(["install", "--list"]),
        LIST_TIMEOUT,
    )
}

pub fn cargo_search_request(task_id: Option<TaskId>, query: &SearchQuery) -> ProcessSpawnRequest {
    cargo_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(CARGO_COMMAND)
            .args(["search", "--limit", "20", "--color", "never"])
            .arg(query.text.clone()),
        SEARCH_TIMEOUT,
    )
}

pub fn cargo_search_single_request(
    task_id: Option<TaskId>,
    crate_name: &str,
) -> ProcessSpawnRequest {
    cargo_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(CARGO_COMMAND)
            .args(["search", "--limit", "1", "--color", "never"])
            .arg(crate_name),
        SEARCH_TIMEOUT,
    )
}

pub fn cargo_install_request(
    task_id: Option<TaskId>,
    crate_name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let mut command = CommandSpec::new(CARGO_COMMAND).args(["install", crate_name]);
    if let Some(version) = version
        && !version.trim().is_empty()
    {
        command = command.args(["--version", version.trim()]);
    }

    cargo_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        command,
        MUTATION_TIMEOUT,
    )
}

pub fn cargo_uninstall_request(task_id: Option<TaskId>, crate_name: &str) -> ProcessSpawnRequest {
    cargo_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(CARGO_COMMAND).args(["uninstall", crate_name]),
        MUTATION_TIMEOUT,
    )
}

pub fn cargo_upgrade_request(task_id: Option<TaskId>, crate_name: &str) -> ProcessSpawnRequest {
    cargo_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(CARGO_COMMAND).args(["install", "--force", crate_name]),
        MUTATION_TIMEOUT,
    )
}

fn cargo_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Cargo, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

#[derive(Debug, Deserialize)]
struct CargoOutdatedEntry {
    name: String,
    installed_version: String,
    candidate_version: String,
}

pub(crate) fn parse_cargo_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    // cargo 1.84.1 (66221abde 2024-11-19)
    let rest = line.strip_prefix("cargo ")?;
    let version = rest.split_whitespace().next()?.trim();
    if version.is_empty() {
        return None;
    }
    Some(version.to_string())
}

pub(crate) fn parse_cargo_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut packages = Vec::new();

    for line in output.lines().map(str::trim_end) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(' ') {
            continue;
        }

        // Example: "ripgrep v14.1.1:" -> name=ripgrep version=14.1.1
        let Some((name, rest)) = line.split_once(" v") else {
            continue;
        };

        let crate_name = name.trim();
        let version = rest.trim_end_matches(':').trim();
        if crate_name.is_empty() || version.is_empty() {
            continue;
        }

        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Cargo,
                name: crate_name.to_string(),
            },
            installed_version: Some(version.to_string()),
            pinned: false,
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

pub(crate) fn parse_cargo_search_version(output: &str, crate_name: &str) -> Option<String> {
    let mut exact_match: Option<String> = None;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("note:") {
            continue;
        }

        // Format: name = "version" # summary
        let Some((name_part, rhs)) = line.split_once('=') else {
            continue;
        };
        let name = name_part.trim();
        let Some(start) = rhs.find('"') else {
            continue;
        };
        let remainder = &rhs[start + 1..];
        let Some(end) = remainder.find('"') else {
            continue;
        };
        let version = remainder[..end].trim();
        if version.is_empty() {
            continue;
        }

        if name == crate_name {
            exact_match = Some(version.to_string());
            break;
        }
    }

    exact_match
}

pub(crate) fn parse_cargo_search(
    output: &str,
    query: &SearchQuery,
) -> AdapterResult<Vec<CachedSearchResult>> {
    let mut results = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("note:") {
            continue;
        }

        let Some((name_part, rhs)) = line.split_once('=') else {
            continue;
        };
        let name = name_part.trim();
        if name.is_empty() {
            continue;
        }

        let Some(first_quote) = rhs.find('"') else {
            continue;
        };
        let after_first = &rhs[first_quote + 1..];
        let Some(second_quote) = after_first.find('"') else {
            continue;
        };
        let version = after_first[..second_quote].trim();

        let summary = if let Some(hash_pos) = rhs.find('#') {
            let value = rhs[hash_pos + 1..].trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        } else {
            None
        };

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Cargo,
                    name: name.to_string(),
                },
                version: if version.is_empty() {
                    None
                } else {
                    Some(version.to_string())
                },
                summary,
            },
            source_manager: ManagerId::Cargo,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    Ok(results)
}

pub(crate) fn parse_cargo_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    if output.trim().is_empty() {
        return Ok(Vec::new());
    }

    let entries: Vec<CargoOutdatedEntry> = serde_json::from_str(output)
        .map_err(|e| parse_error(&format!("invalid cargo outdated JSON: {e}")))?;

    let mut packages = Vec::new();
    for entry in entries {
        let name = entry.name.trim();
        let installed = entry.installed_version.trim();
        let candidate = entry.candidate_version.trim();
        if name.is_empty() || installed.is_empty() || candidate.is_empty() {
            continue;
        }
        if installed == candidate {
            continue;
        }

        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Cargo,
                name: name.to_string(),
            },
            installed_version: Some(installed.to_string()),
            candidate_version: candidate.to_string(),
            pinned: false,
            restart_required: false,
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Cargo),
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
    use crate::models::{
        CoreErrorKind, ManagerAction, ManagerId, PackageRef, SearchQuery, TaskId, TaskType,
    };

    use super::{
        CargoAdapter, CargoDetectOutput, CargoSource, cargo_detect_request, cargo_install_request,
        cargo_list_installed_request, cargo_search_request, cargo_uninstall_request,
        cargo_upgrade_request, parse_cargo_installed, parse_cargo_outdated, parse_cargo_search,
        parse_cargo_search_version, parse_cargo_version,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/cargo/version.txt");
    const INSTALLED_FIXTURE: &str = include_str!("../../tests/fixtures/cargo/install_list.txt");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/cargo/search.txt");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/cargo/outdated.json");

    #[test]
    fn parses_cargo_version_from_fixture() {
        assert_eq!(
            parse_cargo_version(VERSION_FIXTURE).as_deref(),
            Some("1.84.1")
        );
    }

    #[test]
    fn parses_installed_from_fixture() {
        let packages = parse_cargo_installed(INSTALLED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].package.name, "bat");
        assert_eq!(packages[1].package.name, "ripgrep");
        assert_eq!(packages[2].package.name, "zellij");
    }

    #[test]
    fn parses_search_from_fixture() {
        let query = SearchQuery {
            text: "ripgrep".to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let results = parse_cargo_search(SEARCH_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.package.name, "ripgrep");
        assert_eq!(results[0].result.version.as_deref(), Some("14.1.1"));
        assert_eq!(results[1].result.package.name, "rargs");
    }

    #[test]
    fn parses_search_exact_version() {
        let latest = parse_cargo_search_version(SEARCH_FIXTURE, "ripgrep");
        assert_eq!(latest.as_deref(), Some("14.1.1"));
        assert!(parse_cargo_search_version(SEARCH_FIXTURE, "missing").is_none());
    }

    #[test]
    fn parses_outdated_from_fixture() {
        let outdated = parse_cargo_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(outdated.len(), 2);
        assert_eq!(outdated[0].package.name, "bat");
        assert_eq!(outdated[0].installed_version.as_deref(), Some("0.24.0"));
        assert_eq!(outdated[0].candidate_version, "0.25.0");
    }

    #[test]
    fn request_builders_use_structured_args() {
        let detect = cargo_detect_request(Some(TaskId(9)));
        assert_eq!(detect.manager, ManagerId::Cargo);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.command.program, PathBuf::from("cargo"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let list = cargo_list_installed_request(None);
        assert_eq!(list.command.args, vec!["install", "--list"]);

        let search = cargo_search_request(
            None,
            &SearchQuery {
                text: "ripgrep".to_string(),
                issued_at: std::time::SystemTime::now(),
            },
        );
        assert_eq!(
            search.command.args,
            vec!["search", "--limit", "20", "--color", "never", "ripgrep"]
        );

        let install = cargo_install_request(None, "ripgrep", Some("14.1.1"));
        assert_eq!(
            install.command.args,
            vec!["install", "ripgrep", "--version", "14.1.1"]
        );

        let uninstall = cargo_uninstall_request(None, "ripgrep");
        assert_eq!(uninstall.command.args, vec!["uninstall", "ripgrep"]);

        let upgrade = cargo_upgrade_request(None, "ripgrep");
        assert_eq!(upgrade.command.args, vec!["install", "--force", "ripgrep"]);
    }

    #[derive(Clone)]
    struct StubCargoSource {
        detect_calls: Arc<AtomicUsize>,
        detect_result: AdapterResult<CargoDetectOutput>,
        installed_result: AdapterResult<String>,
        outdated_result: AdapterResult<String>,
        search_result: AdapterResult<String>,
    }

    impl StubCargoSource {
        fn success() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                detect_result: Ok(CargoDetectOutput {
                    executable_path: Some(PathBuf::from("/Users/test/.cargo/bin/cargo")),
                    version_output: VERSION_FIXTURE.to_string(),
                }),
                installed_result: Ok(INSTALLED_FIXTURE.to_string()),
                outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
                search_result: Ok(SEARCH_FIXTURE.to_string()),
            }
        }
    }

    impl CargoSource for StubCargoSource {
        fn detect(&self) -> AdapterResult<CargoDetectOutput> {
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
        let adapter = CargoAdapter::new(StubCargoSource::success());

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
            AdapterResponse::InstalledPackages(packages) => assert_eq!(packages.len(), 3),
            other => panic!("unexpected response: {other:?}"),
        }

        match adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap()
        {
            AdapterResponse::OutdatedPackages(packages) => assert_eq!(packages.len(), 2),
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_returns_mutation() {
        let adapter = CargoAdapter::new(StubCargoSource::success());

        match adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Cargo,
                    name: "ripgrep".to_string(),
                },
                version: Some("14.1.1".to_string()),
            }))
            .unwrap()
        {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Install);
                assert_eq!(mutation.package.manager, ManagerId::Cargo);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn invalid_outdated_json_returns_parse_error() {
        let error = parse_cargo_outdated("{bad-json").expect_err("expected parse failure");
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
        assert_eq!(error.manager, Some(ManagerId::Cargo));
    }
}
