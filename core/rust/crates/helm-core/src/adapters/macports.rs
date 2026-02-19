use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const MACPORTS_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const MACPORTS_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::MacPorts,
    display_name: "MacPorts",
    category: ManagerCategory::SystemOs,
    authority: ManagerAuthority::Guarded,
    capabilities: MACPORTS_CAPABILITIES,
};

const PORT_COMMAND: &str = "port";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(180);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacPortsDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait MacPortsSource: Send + Sync {
    fn detect(&self) -> AdapterResult<MacPortsDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install(&self, port_name: &str) -> AdapterResult<String>;
    fn uninstall(&self, port_name: &str) -> AdapterResult<String>;
    fn upgrade(&self, port_name: Option<&str>) -> AdapterResult<String>;
}

pub struct MacPortsAdapter<S: MacPortsSource> {
    source: S,
}

impl<S: MacPortsSource> MacPortsAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: MacPortsSource> ManagerAdapter for MacPortsAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &MACPORTS_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_macports_version(&output.version_output);
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
                let packages = parse_macports_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_macports_outdated(&raw);
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search(search_request.query.text.as_str())?;
                let results = parse_macports_search(&raw, &search_request.query);
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::MacPorts,
                    ManagerAction::Install,
                    install_request.package.name.as_str(),
                )?;
                let _ = self.source.install(&install_request.package.name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: install_request.package,
                    action: ManagerAction::Install,
                    before_version: None,
                    after_version: install_request.version,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::MacPorts,
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
                    manager: ManagerId::MacPorts,
                    name: "__all__".to_string(),
                });

                let target = if package.name == "__all__" {
                    None
                } else {
                    crate::adapters::validate_package_identifier(
                        ManagerId::MacPorts,
                        ManagerAction::Upgrade,
                        package.name.as_str(),
                    )?;
                    Some(package.name.as_str())
                };

                let _ = self.source.upgrade(target)?;

                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::MacPorts),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "macports adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn macports_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(PORT_COMMAND).arg("version"),
        DETECT_TIMEOUT,
    )
}

pub fn macports_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(PORT_COMMAND).arg("installed"),
        LIST_TIMEOUT,
    )
}

pub fn macports_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(PORT_COMMAND).arg("outdated"),
        LIST_TIMEOUT,
    )
}

pub fn macports_search_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(PORT_COMMAND).args(["search", query.text.as_str()]),
        SEARCH_TIMEOUT,
    )
}

pub fn macports_install_request(task_id: Option<TaskId>, port_name: &str) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(PORT_COMMAND).args(["install", port_name]),
        MUTATION_TIMEOUT,
    )
    .requires_elevation(true)
}

pub fn macports_uninstall_request(task_id: Option<TaskId>, port_name: &str) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(PORT_COMMAND).args(["uninstall", port_name]),
        MUTATION_TIMEOUT,
    )
    .requires_elevation(true)
}

pub fn macports_upgrade_request(
    task_id: Option<TaskId>,
    port_name: Option<&str>,
) -> ProcessSpawnRequest {
    let command = if let Some(port_name) = port_name {
        CommandSpec::new(PORT_COMMAND).args(["upgrade", port_name])
    } else {
        CommandSpec::new(PORT_COMMAND).args(["upgrade", "outdated"])
    };

    macports_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
    .requires_elevation(true)
}

fn macports_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::MacPorts, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_macports_version(output: &str) -> Option<String> {
    output.lines().map(str::trim).find_map(|line| {
        line.strip_prefix("Version:")
            .map(str::trim)
            .or_else(|| {
                line.split_whitespace()
                    .find(|part| part.starts_with(|ch: char| ch.is_ascii_digit()))
            })
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn parse_macports_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut installed: BTreeMap<String, (String, bool)> = BTreeMap::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty()
            || line.starts_with("The following")
            || line.starts_with("No ports are installed")
            || line.starts_with("None of the specified ports")
        {
            continue;
        }

        let Some((name, version)) = parse_macports_entry(line) else {
            continue;
        };
        let active = line.contains("(active)");

        installed
            .entry(name)
            .and_modify(|current| {
                if active || !current.1 {
                    *current = (version.clone(), active);
                }
            })
            .or_insert((version, active));
    }

    Ok(installed
        .into_iter()
        .map(|(name, (version, _active))| InstalledPackage {
            package: PackageRef {
                manager: ManagerId::MacPorts,
                name,
            },
            installed_version: Some(version),
            pinned: false,
        })
        .collect())
}

fn parse_macports_outdated(output: &str) -> Vec<OutdatedPackage> {
    let mut outdated = Vec::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty()
            || line.starts_with("The following")
            || line.starts_with("No installed ports are outdated")
        {
            continue;
        }

        let Some((left, right)) = line.split_once('<') else {
            continue;
        };

        let left = left.trim();
        let right = right.trim();

        let (name, installed_version) = parse_macports_entry(left).unwrap_or_else(|| {
            (
                left.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_string(),
                String::new(),
            )
        });
        if name.is_empty() {
            continue;
        }

        let candidate_version = right
            .split_whitespace()
            .find_map(parse_macports_version_token)
            .or_else(|| parse_macports_version_token(right));
        let Some(candidate_version) = candidate_version else {
            continue;
        };

        outdated.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::MacPorts,
                name,
            },
            installed_version: if installed_version.is_empty() {
                None
            } else {
                Some(installed_version)
            },
            candidate_version,
            pinned: false,
            restart_required: false,
        });
    }

    outdated
}

fn parse_macports_search(output: &str, query: &SearchQuery) -> Vec<CachedSearchResult> {
    let mut results: Vec<CachedSearchResult> = Vec::new();
    let mut pending_summary: Option<usize> = None;

    for raw_line in output.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Found ") || trimmed.starts_with("No match") {
            pending_summary = None;
            continue;
        }

        if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
            if let Some(index) = pending_summary {
                let summary = trimmed.trim_matches('-').trim();
                if !summary.is_empty() {
                    results[index].result.summary = Some(summary.to_string());
                }
            }
            continue;
        }

        let Some((name, version)) = parse_macports_entry(trimmed) else {
            pending_summary = None;
            continue;
        };

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::MacPorts,
                    name,
                },
                version: Some(version),
                summary: None,
            },
            source_manager: ManagerId::MacPorts,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
        pending_summary = Some(results.len() - 1);
    }

    results
}

fn parse_macports_entry(line: &str) -> Option<(String, String)> {
    if let Some((name, rest)) = line.split_once('@') {
        let version = rest
            .split_whitespace()
            .next()
            .map(str::trim)
            .and_then(parse_macports_version_token)?;
        return Some((name.trim().to_string(), version));
    }

    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }

    let name = tokens[0].trim().to_string();
    let version = tokens
        .iter()
        .skip(1)
        .find_map(|token| parse_macports_version_token(token))?;

    Some((name, version))
}

fn parse_macports_version_token(token: &str) -> Option<String> {
    let cleaned = token.trim().trim_start_matches('@').trim_end_matches(',');
    if cleaned.is_empty() || !cleaned.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    Some(cleaned.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    use crate::adapters::macports::{
        MacPortsAdapter, MacPortsDetectOutput, MacPortsSource, macports_detect_request,
        macports_install_request, macports_list_installed_request, macports_list_outdated_request,
        macports_search_request, macports_uninstall_request, macports_upgrade_request,
        parse_macports_installed, parse_macports_outdated, parse_macports_search,
        parse_macports_version,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter, SearchRequest,
    };
    use crate::models::{ManagerAction, ManagerId, SearchQuery, TaskType};

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/macports/version.txt");
    const INSTALLED_FIXTURE: &str = include_str!("../../tests/fixtures/macports/installed.txt");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/macports/outdated.txt");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/macports/search.txt");

    #[test]
    fn parses_macports_version() {
        assert_eq!(
            parse_macports_version(VERSION_FIXTURE).as_deref(),
            Some("2.8.1")
        );
    }

    #[test]
    fn parses_macports_installed_fixture() {
        let installed = parse_macports_installed(INSTALLED_FIXTURE).unwrap();
        assert_eq!(installed.len(), 2);
        assert_eq!(installed[0].package.name, "git");
        assert_eq!(installed[0].installed_version.as_deref(), Some("2.48.1_0"));
    }

    #[test]
    fn parses_macports_outdated_fixture() {
        let outdated = parse_macports_outdated(OUTDATED_FIXTURE);
        assert_eq!(outdated.len(), 2);
        assert_eq!(outdated[0].package.name, "git");
        assert_eq!(outdated[0].candidate_version, "2.49.0_0");
    }

    #[test]
    fn parses_macports_search_fixture() {
        let query = SearchQuery {
            text: "ripgrep".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let results = parse_macports_search(SEARCH_FIXTURE, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "ripgrep");
        assert_eq!(results[0].result.version.as_deref(), Some("14.1.1_0"));
    }

    #[test]
    fn request_shapes_match_expected_commands() {
        let detect = macports_detect_request(None);
        assert_eq!(detect.manager, ManagerId::MacPorts);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.command.args, vec!["version"]);

        let list_installed = macports_list_installed_request(None);
        assert_eq!(list_installed.task_type, TaskType::Refresh);
        assert_eq!(list_installed.command.args, vec!["installed"]);

        let list_outdated = macports_list_outdated_request(None);
        assert_eq!(list_outdated.action, ManagerAction::ListOutdated);
        assert_eq!(list_outdated.command.args, vec!["outdated"]);

        let query = SearchQuery {
            text: "git".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let search = macports_search_request(None, &query);
        assert_eq!(search.task_type, TaskType::Search);
        assert_eq!(search.command.args, vec!["search", "git"]);

        let install = macports_install_request(None, "git");
        assert!(install.requires_elevation);
        assert_eq!(install.command.args, vec!["install", "git"]);

        let uninstall = macports_uninstall_request(None, "git");
        assert!(uninstall.requires_elevation);
        assert_eq!(uninstall.command.args, vec!["uninstall", "git"]);

        let upgrade_one = macports_upgrade_request(None, Some("git"));
        assert!(upgrade_one.requires_elevation);
        assert_eq!(upgrade_one.command.args, vec!["upgrade", "git"]);

        let upgrade_all = macports_upgrade_request(None, None);
        assert_eq!(upgrade_all.command.args, vec!["upgrade", "outdated"]);
    }

    #[test]
    fn adapter_detect_and_listing_paths_work() {
        let source = FixtureSource {
            detect_result: Ok(MacPortsDetectOutput {
                executable_path: Some(PathBuf::from("/opt/local/bin/port")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok(INSTALLED_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
            search_result: Ok(SEARCH_FIXTURE.to_string()),
        };
        let adapter = MacPortsAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let AdapterResponse::Detection(info) = detect else {
            panic!("expected detection response");
        };
        assert!(info.installed);
        assert_eq!(info.version.as_deref(), Some("2.8.1"));

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

        let search = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "ripgrep".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .unwrap();
        let AdapterResponse::SearchResults(results) = search else {
            panic!("expected search response");
        };
        assert_eq!(results.len(), 1);
    }

    struct FixtureSource {
        detect_result: AdapterResult<MacPortsDetectOutput>,
        list_installed_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
        search_result: AdapterResult<String>,
    }

    impl MacPortsSource for FixtureSource {
        fn detect(&self) -> AdapterResult<MacPortsDetectOutput> {
            self.detect_result.clone()
        }

        fn list_installed(&self) -> AdapterResult<String> {
            self.list_installed_result.clone()
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            self.list_outdated_result.clone()
        }

        fn search(&self, _query: &str) -> AdapterResult<String> {
            self.search_result.clone()
        }

        fn install(&self, _port_name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn uninstall(&self, _port_name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade(&self, _port_name: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
