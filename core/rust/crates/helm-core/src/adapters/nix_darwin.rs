use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const NIX_DARWIN_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const NIX_DARWIN_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::NixDarwin,
    display_name: "nix-darwin",
    category: ManagerCategory::SystemOs,
    authority: ManagerAuthority::Guarded,
    capabilities: NIX_DARWIN_CAPABILITIES,
};

const DARWIN_REBUILD_COMMAND: &str = "darwin-rebuild";
const NIX_ENV_COMMAND: &str = "nix-env";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(180);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(120);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NixDarwinDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait NixDarwinSource: Send + Sync {
    fn detect(&self) -> AdapterResult<NixDarwinDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install(&self, package_name: &str) -> AdapterResult<String>;
    fn uninstall(&self, package_name: &str) -> AdapterResult<String>;
    fn upgrade(&self, package_name: Option<&str>) -> AdapterResult<String>;
}

pub struct NixDarwinAdapter<S: NixDarwinSource> {
    source: S,
}

impl<S: NixDarwinSource> NixDarwinAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: NixDarwinSource> ManagerAdapter for NixDarwinAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &NIX_DARWIN_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_nix_darwin_version(&output.version_output);
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
                let packages = parse_nix_darwin_installed(&raw);
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_nix_darwin_outdated(&raw);
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search(search_request.query.text.as_str())?;
                let results = parse_nix_darwin_search(&raw, &search_request.query);
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::NixDarwin,
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
                    ManagerId::NixDarwin,
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
                    manager: ManagerId::NixDarwin,
                    name: "__all__".to_string(),
                });

                let target = if package.name == "__all__" {
                    None
                } else {
                    crate::adapters::validate_package_identifier(
                        ManagerId::NixDarwin,
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
                manager: Some(ManagerId::NixDarwin),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "nix-darwin adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn nix_darwin_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    nix_darwin_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(DARWIN_REBUILD_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn nix_darwin_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    nix_darwin_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(NIX_ENV_COMMAND).args(["-q", "--installed"]),
        LIST_TIMEOUT,
    )
}

pub fn nix_darwin_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    nix_darwin_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(NIX_ENV_COMMAND).args(["-u", "--dry-run"]),
        LIST_TIMEOUT,
    )
}

pub fn nix_darwin_search_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    nix_darwin_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(NIX_ENV_COMMAND).args(["-qaP", query.text.as_str()]),
        SEARCH_TIMEOUT,
    )
}

pub fn nix_darwin_install_request(
    task_id: Option<TaskId>,
    package_name: &str,
) -> ProcessSpawnRequest {
    let attr = if package_name.contains('.') {
        package_name.to_string()
    } else {
        format!("nixpkgs.{package_name}")
    };

    nix_darwin_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(NIX_ENV_COMMAND).args(["-iA", attr.as_str()]),
        MUTATION_TIMEOUT,
    )
}

pub fn nix_darwin_uninstall_request(
    task_id: Option<TaskId>,
    package_name: &str,
) -> ProcessSpawnRequest {
    nix_darwin_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(NIX_ENV_COMMAND).args(["-e", package_name]),
        MUTATION_TIMEOUT,
    )
}

pub fn nix_darwin_upgrade_request(
    task_id: Option<TaskId>,
    package_name: Option<&str>,
) -> ProcessSpawnRequest {
    let command = if let Some(package_name) = package_name {
        CommandSpec::new(NIX_ENV_COMMAND).args(["-u", package_name])
    } else {
        CommandSpec::new(NIX_ENV_COMMAND).arg("-u")
    };

    nix_darwin_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
}

fn nix_darwin_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::NixDarwin, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_nix_darwin_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;

    let candidate = line.strip_prefix("darwin-rebuild").unwrap_or(line).trim();
    let token = candidate.split_whitespace().next()?;
    if token.is_empty() || !token.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }

    Some(token.to_string())
}

fn parse_nix_darwin_installed(output: &str) -> Vec<InstalledPackage> {
    let mut packages = Vec::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }

        let (name, version) = split_nix_name_version(line);
        if name.is_empty() {
            continue;
        }

        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::NixDarwin,
                name,
            },
            installed_version: version,
            pinned: false,
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    packages
}

fn parse_nix_darwin_outdated(output: &str) -> Vec<OutdatedPackage> {
    let mut outdated = Vec::new();

    for line in output.lines().map(str::trim) {
        if !line.starts_with("upgrading '") {
            continue;
        }

        let quoted = quoted_segments(line);
        if quoted.len() < 2 {
            continue;
        }

        let (old_name, old_version) = split_nix_name_version(quoted[0]);
        let (new_name, new_version) = split_nix_name_version(quoted[1]);
        let package_name = if !new_name.is_empty() {
            new_name
        } else {
            old_name
        };

        let candidate_version = new_version.unwrap_or_else(|| quoted[1].to_string());

        outdated.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::NixDarwin,
                name: package_name,
            },
            installed_version: old_version,
            candidate_version,
            pinned: false,
            restart_required: false,
        });
    }

    outdated
}

fn parse_nix_darwin_search(output: &str, query: &SearchQuery) -> Vec<CachedSearchResult> {
    let mut results = Vec::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty() {
            continue;
        }

        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.is_empty() {
            continue;
        }

        let attr_path = columns[0];
        let package_name = attr_path
            .rsplit('.')
            .next()
            .unwrap_or(attr_path)
            .to_string();
        if package_name.is_empty() {
            continue;
        }

        let version = columns.get(1).map(|value| split_nix_name_version(value).1);

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::NixDarwin,
                    name: package_name,
                },
                version: version.unwrap_or(None),
                summary: Some(attr_path.to_string()),
            },
            source_manager: ManagerId::NixDarwin,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    results
}

fn split_nix_name_version(identifier: &str) -> (String, Option<String>) {
    for (index, _) in identifier.rmatch_indices('-') {
        let (name, version_candidate) = identifier.split_at(index);
        let version_candidate = version_candidate.trim_start_matches('-').trim();
        if !name.is_empty()
            && !version_candidate.is_empty()
            && version_candidate.starts_with(|ch: char| ch.is_ascii_digit())
        {
            return (name.to_string(), Some(version_candidate.to_string()));
        }
    }

    (identifier.to_string(), None)
}

fn quoted_segments(line: &str) -> Vec<&str> {
    let mut values = Vec::new();
    let mut in_quote = false;
    let mut start = 0usize;

    for (idx, ch) in line.char_indices() {
        if ch == '\'' {
            if in_quote {
                values.push(&line[start..idx]);
                in_quote = false;
            } else {
                in_quote = true;
                start = idx + ch.len_utf8();
            }
        }
    }

    values
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter, SearchRequest,
    };
    use crate::adapters::nix_darwin::{
        NixDarwinAdapter, NixDarwinDetectOutput, NixDarwinSource, nix_darwin_detect_request,
        nix_darwin_install_request, nix_darwin_list_installed_request,
        nix_darwin_list_outdated_request, nix_darwin_search_request, nix_darwin_uninstall_request,
        nix_darwin_upgrade_request, parse_nix_darwin_installed, parse_nix_darwin_outdated,
        parse_nix_darwin_search, parse_nix_darwin_version,
    };
    use crate::models::{ManagerAction, ManagerId, SearchQuery, TaskType};

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/nix_darwin/version.txt");
    const INSTALLED_FIXTURE: &str = include_str!("../../tests/fixtures/nix_darwin/installed.txt");
    const OUTDATED_FIXTURE: &str =
        include_str!("../../tests/fixtures/nix_darwin/outdated_dry_run.txt");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/nix_darwin/search.txt");

    #[test]
    fn parses_nix_darwin_version_fixture() {
        let version = parse_nix_darwin_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("25.05.20250219.abcdef"));
    }

    #[test]
    fn parses_nix_installed_fixture() {
        let installed = parse_nix_darwin_installed(INSTALLED_FIXTURE);
        assert_eq!(installed.len(), 2);
        assert_eq!(installed[0].package.name, "hello");
        assert_eq!(installed[0].installed_version.as_deref(), Some("2.12.1"));
    }

    #[test]
    fn parses_nix_outdated_dry_run_fixture() {
        let outdated = parse_nix_darwin_outdated(OUTDATED_FIXTURE);
        assert_eq!(outdated.len(), 1);
        assert_eq!(outdated[0].package.name, "hello");
        assert_eq!(outdated[0].installed_version.as_deref(), Some("2.12.1"));
        assert_eq!(outdated[0].candidate_version, "2.12.2");
    }

    #[test]
    fn parses_nix_search_fixture() {
        let query = SearchQuery {
            text: "hello".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let results = parse_nix_darwin_search(SEARCH_FIXTURE, &query);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.package.name, "hello");
        assert_eq!(results[0].result.version.as_deref(), Some("2.12.2"));
    }

    #[test]
    fn request_shapes_match_expected_commands() {
        let detect = nix_darwin_detect_request(None);
        assert_eq!(detect.manager, ManagerId::NixDarwin);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.action, ManagerAction::Detect);
        assert_eq!(detect.command.args, vec!["--version"]);

        let list_installed = nix_darwin_list_installed_request(None);
        assert_eq!(list_installed.task_type, TaskType::Refresh);
        assert_eq!(list_installed.command.args, vec!["-q", "--installed"]);

        let list_outdated = nix_darwin_list_outdated_request(None);
        assert_eq!(list_outdated.action, ManagerAction::ListOutdated);
        assert_eq!(list_outdated.command.args, vec!["-u", "--dry-run"]);

        let query = SearchQuery {
            text: "ripgrep".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let search = nix_darwin_search_request(None, &query);
        assert_eq!(search.task_type, TaskType::Search);
        assert_eq!(search.command.args, vec!["-qaP", "ripgrep"]);

        let install = nix_darwin_install_request(None, "hello");
        assert_eq!(install.task_type, TaskType::Install);
        assert_eq!(install.command.args, vec!["-iA", "nixpkgs.hello"]);

        let uninstall = nix_darwin_uninstall_request(None, "hello");
        assert_eq!(uninstall.task_type, TaskType::Uninstall);
        assert_eq!(uninstall.command.args, vec!["-e", "hello"]);

        let upgrade_one = nix_darwin_upgrade_request(None, Some("hello"));
        assert_eq!(upgrade_one.command.args, vec!["-u", "hello"]);

        let upgrade_all = nix_darwin_upgrade_request(None, None);
        assert_eq!(upgrade_all.command.args, vec!["-u"]);
    }

    #[test]
    fn adapter_paths_for_detect_list_outdated_search_work() {
        let source = FixtureSource {
            detect_result: Ok(NixDarwinDetectOutput {
                executable_path: Some(PathBuf::from("/run/current-system/sw/bin/darwin-rebuild")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok(INSTALLED_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
            search_result: Ok(SEARCH_FIXTURE.to_string()),
        };

        let adapter = NixDarwinAdapter::new(source);
        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let AdapterResponse::Detection(info) = detect else {
            panic!("expected detection response");
        };
        assert!(info.installed);
        assert_eq!(info.version.as_deref(), Some("25.05.20250219.abcdef"));

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
        assert_eq!(packages.len(), 1);

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
        assert_eq!(results.len(), 2);
    }

    struct FixtureSource {
        detect_result: AdapterResult<NixDarwinDetectOutput>,
        list_installed_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
        search_result: AdapterResult<String>,
    }

    impl NixDarwinSource for FixtureSource {
        fn detect(&self) -> AdapterResult<NixDarwinDetectOutput> {
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

        fn install(&self, _package_name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn uninstall(&self, _package_name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade(&self, _package_name: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
