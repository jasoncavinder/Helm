use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const ASDF_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const ASDF_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Asdf,
    display_name: "asdf",
    category: ManagerCategory::ToolRuntime,
    authority: ManagerAuthority::Authoritative,
    capabilities: ASDF_CAPABILITIES,
};

const ASDF_COMMAND: &str = "asdf";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsdfDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait AsdfSource: Send + Sync {
    fn detect(&self) -> AdapterResult<AsdfDetectOutput>;
    fn list_current(&self) -> AdapterResult<String>;
    fn list_plugins(&self) -> AdapterResult<String>;
    fn list_all_plugins(&self) -> AdapterResult<String>;
    fn latest_version(&self, plugin: &str) -> AdapterResult<String>;
    fn install(&self, plugin: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall(&self, plugin: &str, version: &str) -> AdapterResult<String>;
    fn upgrade(&self, plugin: Option<&str>) -> AdapterResult<String>;
}

pub struct AsdfAdapter<S: AsdfSource> {
    source: S,
}

impl<S: AsdfSource> AsdfAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: AsdfSource> ManagerAdapter for AsdfAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &ASDF_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_asdf_version(&output.version_output);
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
                let raw = self.source.list_current()?;
                let packages = parse_asdf_current(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_current()?;
                let installed = parse_asdf_current(&raw)?;
                let mut outdated = Vec::new();

                for package in installed {
                    let Some(installed_version) = package.installed_version.clone() else {
                        continue;
                    };
                    // Compatibility mode: skip tools that fail latest-version lookup instead of
                    // failing the full outdated scan.
                    let latest_raw = match self.source.latest_version(&package.package.name) {
                        Ok(output) => output,
                        Err(_) => continue,
                    };
                    let Some(latest_version) = parse_asdf_latest_version(&latest_raw) else {
                        continue;
                    };
                    if latest_version == installed_version {
                        continue;
                    }
                    outdated.push(OutdatedPackage {
                        package: package.package,
                        installed_version: Some(installed_version),
                        candidate_version: latest_version,
                        pinned: false,
                        restart_required: false,
                    });
                }

                Ok(AdapterResponse::OutdatedPackages(outdated))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.list_all_plugins()?;
                let results = parse_asdf_search(&raw, &search_request.query);
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::Asdf,
                    ManagerAction::Install,
                    install_request.package.name.as_str(),
                )?;
                let _ = self.source.install(
                    install_request.package.name.as_str(),
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
                    ManagerId::Asdf,
                    ManagerAction::Uninstall,
                    uninstall_request.package.name.as_str(),
                )?;

                let installed_raw = self.source.list_current()?;
                let installed = parse_asdf_current(&installed_raw)?;
                let installed_version = installed
                    .iter()
                    .find(|pkg| pkg.package.name == uninstall_request.package.name)
                    .and_then(|pkg| pkg.installed_version.clone())
                    .ok_or(CoreError {
                        manager: Some(ManagerId::Asdf),
                        task: None,
                        action: Some(ManagerAction::Uninstall),
                        kind: CoreErrorKind::InvalidInput,
                        message: format!(
                            "asdf tool '{}' is not installed or has no resolvable version",
                            uninstall_request.package.name
                        ),
                    })?;

                let _ = self
                    .source
                    .uninstall(uninstall_request.package.name.as_str(), &installed_version)?;

                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: Some(installed_version),
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Asdf,
                    name: "__all__".to_string(),
                });

                let target_name = if package.name == "__all__" {
                    None
                } else {
                    crate::adapters::validate_package_identifier(
                        ManagerId::Asdf,
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
                manager: Some(ManagerId::Asdf),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "asdf adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn asdf_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(ASDF_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn asdf_list_current_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(ASDF_COMMAND).arg("current"),
        LIST_TIMEOUT,
    )
}

pub fn asdf_list_plugins_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::Refresh,
        CommandSpec::new(ASDF_COMMAND).args(["plugin", "list"]),
        LIST_TIMEOUT,
    )
}

pub fn asdf_list_all_plugins_request(
    task_id: Option<TaskId>,
    _query: &SearchQuery,
) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(ASDF_COMMAND).args(["plugin", "list", "all"]),
        SEARCH_TIMEOUT,
    )
}

pub fn asdf_latest_request(task_id: Option<TaskId>, plugin: &str) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(ASDF_COMMAND).args(["latest", plugin]),
        LIST_TIMEOUT,
    )
}

pub fn asdf_install_request(
    task_id: Option<TaskId>,
    plugin: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let target = version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest");

    asdf_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(ASDF_COMMAND).args(["install", plugin, target]),
        MUTATION_TIMEOUT,
    )
}

pub fn asdf_uninstall_request(
    task_id: Option<TaskId>,
    plugin: &str,
    version: &str,
) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(ASDF_COMMAND).args(["uninstall", plugin, version]),
        MUTATION_TIMEOUT,
    )
}

pub fn asdf_upgrade_request(task_id: Option<TaskId>, plugin: &str) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(ASDF_COMMAND).args(["install", plugin, "latest"]),
        MUTATION_TIMEOUT,
    )
}

fn asdf_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Asdf, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_asdf_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;

    let token = line.split_whitespace().find(|piece| {
        piece
            .chars()
            .next()
            .is_some_and(|ch| ch == 'v' || ch.is_ascii_digit())
    })?;
    let normalized = token.strip_prefix('v').unwrap_or(token);
    if normalized.is_empty() || !normalized.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }

    Some(normalized.to_string())
}

fn parse_asdf_current(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut installed = BTreeMap::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty()
            || line.starts_with("No ")
            || line.starts_with("not installed")
            || line.starts_with("Name")
        {
            continue;
        }

        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }

        let name = tokens[0];
        let version = tokens[1];
        if version == "system" || version.starts_with('(') {
            continue;
        }

        installed.insert(name.to_string(), version.to_string());
    }

    Ok(installed
        .into_iter()
        .map(|(name, version)| InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Asdf,
                name,
            },
            installed_version: Some(version),
            pinned: false,
        })
        .collect())
}

fn parse_asdf_latest_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let token = line.split_whitespace().find(|piece| {
        piece
            .chars()
            .next()
            .is_some_and(|ch| ch == 'v' || ch.is_ascii_digit())
    })?;
    let normalized = token.strip_prefix('v').unwrap_or(token).trim();
    if normalized.is_empty() || !normalized.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    Some(normalized.to_string())
}

fn parse_asdf_search(output: &str, query: &SearchQuery) -> Vec<CachedSearchResult> {
    let needle = query.text.trim().to_ascii_lowercase();
    let mut seen = BTreeSet::new();
    let mut results = Vec::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        if name.is_empty() {
            continue;
        }

        if !needle.is_empty() && !name.to_ascii_lowercase().contains(&needle) {
            continue;
        }

        if !seen.insert(name.to_string()) {
            continue;
        }

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: name.to_string(),
                },
                version: None,
                summary: Some("asdf plugin".to_string()),
            },
            source_manager: ManagerId::Asdf,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    results.sort_by(|a, b| a.result.package.name.cmp(&b.result.package.name));
    results
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    use crate::adapters::asdf::{
        AsdfAdapter, AsdfDetectOutput, AsdfSource, asdf_detect_request, asdf_install_request,
        asdf_list_all_plugins_request, asdf_list_current_request, asdf_uninstall_request,
        asdf_upgrade_request, parse_asdf_current, parse_asdf_latest_version, parse_asdf_search,
        parse_asdf_version,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, ListInstalledRequest, ListOutdatedRequest,
        ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
    };
    use crate::models::{ManagerAction, ManagerId, PackageRef, SearchQuery, TaskType};

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/asdf/version.txt");
    const CURRENT_FIXTURE: &str = include_str!("../../tests/fixtures/asdf/current.txt");
    const PLUGINS_FIXTURE: &str = include_str!("../../tests/fixtures/asdf/plugin_list_all.txt");

    #[test]
    fn parses_asdf_version_from_fixture() {
        let parsed = parse_asdf_version(VERSION_FIXTURE);
        assert_eq!(parsed.as_deref(), Some("0.16.0"));
    }

    #[test]
    fn parses_asdf_installed_packages_from_current_output() {
        let packages = parse_asdf_current(CURRENT_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "nodejs");
        assert_eq!(packages[0].installed_version.as_deref(), Some("20.12.2"));
        assert_eq!(packages[1].package.name, "python");
        assert_eq!(packages[1].installed_version.as_deref(), Some("3.12.2"));
    }

    #[test]
    fn parses_latest_version_line() {
        assert_eq!(
            parse_asdf_latest_version("20.12.3\n").as_deref(),
            Some("20.12.3")
        );
        assert_eq!(
            parse_asdf_latest_version("v3.13.0\n").as_deref(),
            Some("3.13.0")
        );
    }

    #[test]
    fn parses_plugin_search_results() {
        let query = SearchQuery {
            text: "py".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let results = parse_asdf_search(PLUGINS_FIXTURE, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "python");
        assert_eq!(results[0].originating_query, "py");
    }

    #[test]
    fn detect_request_has_expected_shape() {
        let request = asdf_detect_request(None);
        assert_eq!(request.manager, ManagerId::Asdf);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program.to_str(), Some("asdf"));
        assert_eq!(request.command.args, vec!["--version"]);
        assert!(!request.requires_elevation);
    }

    #[test]
    fn list_current_request_has_expected_shape() {
        let request = asdf_list_current_request(None);
        assert_eq!(request.manager, ManagerId::Asdf);
        assert_eq!(request.task_type, TaskType::Refresh);
        assert_eq!(request.action, ManagerAction::ListInstalled);
        assert_eq!(request.command.args, vec!["current"]);
    }

    #[test]
    fn list_all_plugins_request_has_expected_shape() {
        let query = SearchQuery {
            text: "node".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let request = asdf_list_all_plugins_request(None, &query);
        assert_eq!(request.task_type, TaskType::Search);
        assert_eq!(request.action, ManagerAction::Search);
        assert_eq!(request.command.args, vec!["plugin", "list", "all"]);
    }

    #[test]
    fn install_uninstall_upgrade_requests_have_expected_shapes() {
        let install = asdf_install_request(None, "python", Some("3.12.2"));
        assert_eq!(install.task_type, TaskType::Install);
        assert_eq!(install.command.args, vec!["install", "python", "3.12.2"]);

        let uninstall = asdf_uninstall_request(None, "python", "3.12.2");
        assert_eq!(uninstall.task_type, TaskType::Uninstall);
        assert_eq!(
            uninstall.command.args,
            vec!["uninstall", "python", "3.12.2"]
        );

        let upgrade = asdf_upgrade_request(None, "python");
        assert_eq!(upgrade.task_type, TaskType::Upgrade);
        assert_eq!(upgrade.command.args, vec!["install", "python", "latest"]);
    }

    #[test]
    fn adapter_list_outdated_compares_latest_per_tool() {
        let source = FixtureSource {
            detect_result: Ok(AsdfDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/asdf")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_current_result: Ok(CURRENT_FIXTURE.to_string()),
            list_plugins_result: Ok("nodejs\npython\n".to_string()),
            list_all_plugins_result: Ok(PLUGINS_FIXTURE.to_string()),
            latest_by_plugin: HashMap::from([
                ("nodejs".to_string(), "20.12.2\n".to_string()),
                ("python".to_string(), "3.13.0\n".to_string()),
            ]),
        };
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        let AdapterResponse::OutdatedPackages(packages) = response else {
            panic!("expected outdated packages response");
        };
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.name, "python");
        assert_eq!(packages[0].installed_version.as_deref(), Some("3.12.2"));
        assert_eq!(packages[0].candidate_version, "3.13.0");
    }

    #[test]
    fn adapter_list_outdated_skips_tools_with_latest_probe_failures() {
        let source = FixtureSource {
            detect_result: Ok(AsdfDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/asdf")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_current_result: Ok(CURRENT_FIXTURE.to_string()),
            list_plugins_result: Ok("nodejs\npython\n".to_string()),
            list_all_plugins_result: Ok(PLUGINS_FIXTURE.to_string()),
            latest_by_plugin: HashMap::from([("python".to_string(), "3.13.0\n".to_string())]),
        };
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        let AdapterResponse::OutdatedPackages(packages) = response else {
            panic!("expected outdated packages response");
        };
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.name, "python");
    }

    #[test]
    fn adapter_uninstall_uses_current_tool_version() {
        let source = FixtureSource {
            detect_result: Ok(AsdfDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/asdf")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_current_result: Ok(CURRENT_FIXTURE.to_string()),
            list_plugins_result: Ok("nodejs\npython\n".to_string()),
            list_all_plugins_result: Ok(PLUGINS_FIXTURE.to_string()),
            latest_by_plugin: HashMap::new(),
        };
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: "python".to_string(),
                },
            }))
            .unwrap();

        let AdapterResponse::Mutation(result) = response else {
            panic!("expected mutation response");
        };
        assert_eq!(result.before_version.as_deref(), Some("3.12.2"));
        assert_eq!(result.action, ManagerAction::Uninstall);
    }

    #[test]
    fn adapter_search_filters_plugin_catalog() {
        let source = FixtureSource {
            detect_result: Ok(AsdfDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/asdf")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_current_result: Ok(CURRENT_FIXTURE.to_string()),
            list_plugins_result: Ok("nodejs\npython\n".to_string()),
            list_all_plugins_result: Ok(PLUGINS_FIXTURE.to_string()),
            latest_by_plugin: HashMap::new(),
        };
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "node".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .unwrap();

        let AdapterResponse::SearchResults(results) = response else {
            panic!("expected search response");
        };
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "nodejs");
    }

    #[test]
    fn adapter_list_installed_returns_packages() {
        let source = FixtureSource {
            detect_result: Ok(AsdfDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/asdf")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_current_result: Ok(CURRENT_FIXTURE.to_string()),
            list_plugins_result: Ok("nodejs\npython\n".to_string()),
            list_all_plugins_result: Ok(PLUGINS_FIXTURE.to_string()),
            latest_by_plugin: HashMap::new(),
        };
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();

        let AdapterResponse::InstalledPackages(packages) = response else {
            panic!("expected installed packages response");
        };
        assert_eq!(packages.len(), 2);
    }

    #[test]
    fn adapter_upgrade_all_uses_default_placeholder_package() {
        let source = FixtureSource {
            detect_result: Ok(AsdfDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/asdf")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_current_result: Ok(CURRENT_FIXTURE.to_string()),
            list_plugins_result: Ok("nodejs\npython\n".to_string()),
            list_all_plugins_result: Ok(PLUGINS_FIXTURE.to_string()),
            latest_by_plugin: HashMap::new(),
        };
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Upgrade(UpgradeRequest { package: None }))
            .unwrap();

        let AdapterResponse::Mutation(result) = response else {
            panic!("expected mutation response");
        };
        assert_eq!(result.package.name, "__all__");
    }

    struct FixtureSource {
        detect_result: AdapterResult<AsdfDetectOutput>,
        list_current_result: AdapterResult<String>,
        list_plugins_result: AdapterResult<String>,
        list_all_plugins_result: AdapterResult<String>,
        latest_by_plugin: HashMap<String, String>,
    }

    impl AsdfSource for FixtureSource {
        fn detect(&self) -> AdapterResult<AsdfDetectOutput> {
            self.detect_result.clone()
        }

        fn list_current(&self) -> AdapterResult<String> {
            self.list_current_result.clone()
        }

        fn list_plugins(&self) -> AdapterResult<String> {
            self.list_plugins_result.clone()
        }

        fn list_all_plugins(&self) -> AdapterResult<String> {
            self.list_all_plugins_result.clone()
        }

        fn latest_version(&self, plugin: &str) -> AdapterResult<String> {
            self.latest_by_plugin
                .get(plugin)
                .cloned()
                .ok_or(crate::models::CoreError {
                    manager: Some(ManagerId::Asdf),
                    task: None,
                    action: None,
                    kind: crate::models::CoreErrorKind::ParseFailure,
                    message: "missing latest fixture".to_string(),
                })
        }

        fn install(&self, _plugin: &str, _version: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn uninstall(&self, _plugin: &str, _version: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade(&self, _plugin: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
