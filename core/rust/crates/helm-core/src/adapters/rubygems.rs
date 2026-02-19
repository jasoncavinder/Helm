use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const RUBYGEMS_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Search,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const RUBYGEMS_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::RubyGems,
    display_name: "RubyGems",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: RUBYGEMS_CAPABILITIES,
};

const RUBYGEMS_COMMAND: &str = "gem";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RubyGemsDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait RubyGemsSource: Send + Sync {
    fn detect(&self) -> AdapterResult<RubyGemsDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall(&self, name: &str) -> AdapterResult<String>;
    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct RubyGemsAdapter<S: RubyGemsSource> {
    source: S,
}

impl<S: RubyGemsSource> RubyGemsAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: RubyGemsSource> ManagerAdapter for RubyGemsAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &RUBYGEMS_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_rubygems_version(&output.version_output);
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
                let packages = parse_rubygems_list_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_rubygems_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search(search_request.query.text.as_str())?;
                let results = parse_rubygems_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::RubyGems,
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
                    ManagerId::RubyGems,
                    ManagerAction::Uninstall,
                    uninstall_request.package.name.as_str(),
                )?;
                let _ = self
                    .source
                    .uninstall(uninstall_request.package.name.as_str())?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::RubyGems,
                    name: "__all__".to_string(),
                });
                let target_name = if package.name == "__all__" {
                    None
                } else {
                    crate::adapters::validate_package_identifier(
                        ManagerId::RubyGems,
                        ManagerAction::Upgrade,
                        package.name.as_str(),
                    )?;
                    Some(package.name.as_str())
                };
                let _ = self.source.upgrade(target_name)?;
                if let Some(name) = target_name {
                    ensure_gem_no_longer_outdated(&self.source, name)?;
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::RubyGems),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "rubygems adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn rubygems_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rubygems_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(RUBYGEMS_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn rubygems_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rubygems_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(RUBYGEMS_COMMAND).args(["list", "--local"]),
        LIST_TIMEOUT,
    )
}

pub fn rubygems_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rubygems_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(RUBYGEMS_COMMAND).arg("outdated"),
        LIST_TIMEOUT,
    )
}

pub fn rubygems_search_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    rubygems_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(RUBYGEMS_COMMAND)
            .arg("search")
            .arg(query.text.clone())
            .args(["--remote", "--details"]),
        SEARCH_TIMEOUT,
    )
}

pub fn rubygems_install_request(
    task_id: Option<TaskId>,
    name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let mut command = CommandSpec::new(RUBYGEMS_COMMAND).args(["install", name]);
    if let Some(version) = version.map(str::trim).filter(|value| !value.is_empty()) {
        command = command.args(["--version", version]);
    }

    rubygems_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        command,
        MUTATION_TIMEOUT,
    )
}

pub fn rubygems_uninstall_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    rubygems_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(RUBYGEMS_COMMAND).args(["uninstall", name, "-a", "-x"]),
        MUTATION_TIMEOUT,
    )
}

pub fn rubygems_upgrade_request(
    task_id: Option<TaskId>,
    name: Option<&str>,
) -> ProcessSpawnRequest {
    let command = if let Some(name) = name {
        CommandSpec::new(RUBYGEMS_COMMAND).args(["update", name])
    } else {
        CommandSpec::new(RUBYGEMS_COMMAND).arg("update")
    };

    rubygems_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
}

fn rubygems_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::RubyGems, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn ensure_gem_no_longer_outdated<S: RubyGemsSource>(
    source: &S,
    gem_name: &str,
) -> AdapterResult<()> {
    let raw = source.list_outdated()?;
    let outdated = parse_rubygems_outdated(&raw)?;
    if outdated
        .iter()
        .any(|item| item.package.name == gem_name)
    {
        return Err(CoreError {
            manager: Some(ManagerId::RubyGems),
            task: Some(TaskType::Upgrade),
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::ProcessFailure,
            message: format!(
                "gem update reported success but '{gem_name}' remains outdated"
            ),
        });
    }
    Ok(())
}

fn parse_rubygems_version(output: &str) -> Option<String> {
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

fn parse_rubygems_list_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("***") {
            continue;
        }
        let Some((name, versions_raw)) = line.split_once(" (") else {
            continue;
        };
        let versions = versions_raw.trim_end_matches(')').trim();
        let installed_version = versions
            .split(',')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::RubyGems,
                name: name.trim().to_string(),
            },
            installed_version,
            pinned: false,
        });
    }

    if packages.is_empty() && !output.trim().is_empty() {
        return Err(parse_error("invalid rubygems list output"));
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_rubygems_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((name, payload_raw)) = line.split_once(" (") else {
            continue;
        };
        let payload = payload_raw.trim_end_matches(')').trim();

        let (installed, candidate) = if let Some((current, latest)) = payload.split_once('<') {
            (current.trim(), latest.trim())
        } else if let Some((current, latest)) = payload.split_once(',') {
            (current.trim(), latest.trim())
        } else {
            continue;
        };

        if candidate.is_empty() {
            continue;
        }

        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::RubyGems,
                name: name.trim().to_string(),
            },
            installed_version: if installed.is_empty() {
                None
            } else {
                Some(installed.to_string())
            },
            candidate_version: candidate.to_string(),
            pinned: false,
            restart_required: false,
        });
    }

    if packages.is_empty() && !output.trim().is_empty() {
        return Err(parse_error("invalid rubygems outdated output"));
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_rubygems_search(
    output: &str,
    query: &SearchQuery,
) -> AdapterResult<Vec<CachedSearchResult>> {
    let mut results = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("***") || line.starts_with('"') {
            continue;
        }
        let Some((name, versions_raw)) = line.split_once(" (") else {
            continue;
        };
        let version = versions_raw
            .trim_end_matches(')')
            .split(',')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let normalized_name = name.trim().to_string();
        if normalized_name.is_empty() {
            continue;
        }

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::RubyGems,
                    name: normalized_name,
                },
                version,
                summary: None,
            },
            source_manager: ManagerId::RubyGems,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    if results.is_empty() && !output.trim().is_empty() {
        return Err(parse_error("invalid rubygems search output"));
    }

    Ok(results)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::RubyGems),
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
        ManagerAdapter, SearchRequest,
    };
    use crate::models::{
        CoreErrorKind, ManagerAction, ManagerId, PackageRef, SearchQuery, TaskId, TaskType,
    };

    use super::{
        RubyGemsAdapter, RubyGemsDetectOutput, RubyGemsSource, parse_rubygems_list_installed,
        parse_rubygems_outdated, parse_rubygems_search, parse_rubygems_version,
        rubygems_detect_request, rubygems_install_request, rubygems_list_installed_request,
        rubygems_list_outdated_request, rubygems_search_request, rubygems_uninstall_request,
        rubygems_upgrade_request,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/rubygems/version.txt");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/rubygems/list_local.txt");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/rubygems/outdated.txt");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/rubygems/search.txt");

    #[test]
    fn parses_rubygems_version_from_fixture() {
        let version = parse_rubygems_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("3.5.22"));
    }

    #[test]
    fn parses_installed_from_fixture() {
        let packages = parse_rubygems_list_installed(LIST_FIXTURE).unwrap();
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].package.name, "bundler");
        assert_eq!(packages[1].package.name, "rake");
        assert_eq!(packages[2].package.name, "rubocop");
    }

    #[test]
    fn parses_outdated_from_fixture() {
        let outdated = parse_rubygems_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(outdated.len(), 2);
        assert_eq!(outdated[0].package.name, "rake");
        assert_eq!(outdated[0].candidate_version, "13.2.1");
        assert_eq!(outdated[1].package.name, "rubocop");
    }

    #[test]
    fn parses_search_results_from_fixture() {
        let query = SearchQuery {
            text: "rake".to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let results = parse_rubygems_search(SEARCH_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.package.name, "rake");
        assert_eq!(results[0].source_manager, ManagerId::RubyGems);
    }

    #[test]
    fn request_builders_use_expected_commands() {
        let detect = rubygems_detect_request(Some(TaskId(11)));
        assert_eq!(detect.manager, ManagerId::RubyGems);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.action, ManagerAction::Detect);
        assert_eq!(detect.command.program, PathBuf::from("gem"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let list = rubygems_list_installed_request(None);
        assert_eq!(list.command.args, vec!["list", "--local"]);

        let outdated = rubygems_list_outdated_request(None);
        assert_eq!(outdated.command.args, vec!["outdated"]);

        let search = rubygems_search_request(
            None,
            &SearchQuery {
                text: "rake".to_string(),
                issued_at: std::time::SystemTime::now(),
            },
        );
        assert_eq!(
            search.command.args,
            vec!["search", "rake", "--remote", "--details"]
        );

        let install = rubygems_install_request(None, "rubocop", Some("1.72.0"));
        assert_eq!(
            install.command.args,
            vec!["install", "rubocop", "--version", "1.72.0"]
        );

        let uninstall = rubygems_uninstall_request(None, "rubocop");
        assert_eq!(
            uninstall.command.args,
            vec!["uninstall", "rubocop", "-a", "-x"]
        );

        let upgrade_one = rubygems_upgrade_request(None, Some("rubocop"));
        assert_eq!(upgrade_one.command.args, vec!["update", "rubocop"]);

        let upgrade_all = rubygems_upgrade_request(None, None);
        assert_eq!(upgrade_all.command.args, vec!["update"]);
    }

    #[derive(Clone)]
    struct StubRubyGemsSource {
        detect_calls: Arc<AtomicUsize>,
        detect_result: AdapterResult<RubyGemsDetectOutput>,
        list_installed_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
        search_result: AdapterResult<String>,
    }

    impl StubRubyGemsSource {
        fn success() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                detect_result: Ok(RubyGemsDetectOutput {
                    executable_path: Some(PathBuf::from("/usr/bin/gem")),
                    version_output: "3.5.22\n".to_string(),
                }),
                list_installed_result: Ok(LIST_FIXTURE.to_string()),
                list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
                search_result: Ok(SEARCH_FIXTURE.to_string()),
            }
        }
    }

    impl RubyGemsSource for StubRubyGemsSource {
        fn detect(&self) -> AdapterResult<RubyGemsDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
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
        let source = StubRubyGemsSource::success();
        let calls = source.detect_calls.clone();
        let adapter = RubyGemsAdapter::new(source);

        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .expect("detect should succeed");

        match response {
            AdapterResponse::Detection(detection) => {
                assert!(detection.installed);
                assert_eq!(detection.version.as_deref(), Some("3.5.22"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn execute_list_installed_uses_parser() {
        let adapter = RubyGemsAdapter::new(StubRubyGemsSource::success());

        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .expect("list installed should succeed");

        match response {
            AdapterResponse::InstalledPackages(packages) => {
                assert_eq!(packages.len(), 3);
                assert_eq!(packages[0].package.manager, ManagerId::RubyGems);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn execute_search_returns_cached_results() {
        let adapter = RubyGemsAdapter::new(StubRubyGemsSource::success());

        let response = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "rake".to_string(),
                    issued_at: std::time::SystemTime::now(),
                },
            }))
            .expect("search should succeed");

        match response {
            AdapterResponse::SearchResults(results) => {
                assert_eq!(results.len(), 2);
                assert_eq!(results[0].source_manager, ManagerId::RubyGems);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_requires_capability_and_returns_mutation() {
        let adapter = RubyGemsAdapter::new(StubRubyGemsSource::success());

        let response = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::RubyGems,
                    name: "rubocop".to_string(),
                },
                version: Some("1.72.0".to_string()),
            }))
            .expect("install should succeed");

        match response {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Install);
                assert_eq!(mutation.package.manager, ManagerId::RubyGems);
                assert_eq!(mutation.after_version.as_deref(), Some("1.72.0"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_rejects_option_like_package_name() {
        let adapter = RubyGemsAdapter::new(StubRubyGemsSource::success());

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::RubyGems,
                    name: "--source=http://malicious".to_string(),
                },
                version: None,
            }))
            .expect_err("expected invalid input");
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn parse_errors_are_structured() {
        let error = parse_rubygems_list_installed("{not text").expect_err("expected parse failure");
        assert_eq!(error.manager, Some(ManagerId::RubyGems));
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
    }

    #[test]
    fn upgrade_single_gem_succeeds_when_no_longer_outdated() {
        let mut source = StubRubyGemsSource::success();
        source.list_outdated_result = Ok(String::new());
        let adapter = RubyGemsAdapter::new(source);

        let response = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::RubyGems,
                    name: "rake".to_string(),
                }),
            }))
            .expect("upgrade should succeed when gem is no longer outdated");

        match response {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Upgrade);
                assert_eq!(mutation.package.name, "rake");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn upgrade_single_gem_fails_when_still_outdated() {
        let source = StubRubyGemsSource::success();
        let adapter = RubyGemsAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::RubyGems,
                    name: "rake".to_string(),
                }),
            }))
            .expect_err("upgrade should fail when gem remains outdated");

        assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
        assert!(error.message.contains("remains outdated"));
    }

    #[test]
    fn upgrade_all_gems_skips_post_validation() {
        let source = StubRubyGemsSource::success();
        let adapter = RubyGemsAdapter::new(source);

        let response = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: None,
            }))
            .expect("upgrade all should succeed without post-validation");

        match response {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Upgrade);
                assert_eq!(mutation.package.name, "__all__");
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }
}
