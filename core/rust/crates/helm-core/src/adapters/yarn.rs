use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const YARN_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Search,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const YARN_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Yarn,
    display_name: "yarn (global)",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: YARN_CAPABILITIES,
};

const YARN_COMMAND: &str = "yarn";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YarnDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait YarnSource: Send + Sync {
    fn detect(&self) -> AdapterResult<YarnDetectOutput>;
    fn list_installed_global(&self) -> AdapterResult<String>;
    fn list_outdated_global(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install_global(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall_global(&self, name: &str) -> AdapterResult<String>;
    fn upgrade_global(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct YarnAdapter<S: YarnSource> {
    source: S,
}

impl<S: YarnSource> YarnAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: YarnSource> ManagerAdapter for YarnAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &YARN_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_yarn_version(&output.version_output);
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
                let raw = self.source.list_installed_global()?;
                let packages = parse_yarn_list_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated_global()?;
                let packages = parse_yarn_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search(search_request.query.text.as_str())?;
                let results = parse_yarn_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::Yarn,
                    ManagerAction::Install,
                    install_request.package.name.as_str(),
                )?;
                let _ = self.source.install_global(
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
                    ManagerId::Yarn,
                    ManagerAction::Uninstall,
                    uninstall_request.package.name.as_str(),
                )?;
                let _ = self
                    .source
                    .uninstall_global(uninstall_request.package.name.as_str())?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Yarn,
                    name: "__all__".to_string(),
                });
                let target_name = if package.name == "__all__" {
                    None
                } else {
                    crate::adapters::validate_package_identifier(
                        ManagerId::Yarn,
                        ManagerAction::Upgrade,
                        package.name.as_str(),
                    )?;
                    Some(package.name.as_str())
                };
                let _ = self.source.upgrade_global(target_name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Yarn),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "yarn adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn yarn_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    yarn_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(YARN_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn yarn_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    yarn_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(YARN_COMMAND).args(["global", "list", "--depth=0", "--json"]),
        LIST_TIMEOUT,
    )
}

pub fn yarn_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    yarn_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(YARN_COMMAND).args(["outdated", "--json"]),
        LIST_TIMEOUT,
    )
}

pub fn yarn_search_request(task_id: Option<TaskId>, query: &SearchQuery) -> ProcessSpawnRequest {
    yarn_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(YARN_COMMAND)
            .arg("search")
            .arg(query.text.clone())
            .arg("--json"),
        SEARCH_TIMEOUT,
    )
}

pub fn yarn_install_request(
    task_id: Option<TaskId>,
    name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let spec = match version {
        Some(version) if !version.trim().is_empty() => format!("{name}@{}", version.trim()),
        _ => name.to_string(),
    };

    yarn_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(YARN_COMMAND)
            .args(["global", "add"])
            .arg(spec),
        MUTATION_TIMEOUT,
    )
}

pub fn yarn_uninstall_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    yarn_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(YARN_COMMAND).args(["global", "remove", name]),
        MUTATION_TIMEOUT,
    )
}

pub fn yarn_upgrade_request(task_id: Option<TaskId>, name: Option<&str>) -> ProcessSpawnRequest {
    let command = if let Some(name) = name {
        CommandSpec::new(YARN_COMMAND).args(["global", "upgrade", name])
    } else {
        CommandSpec::new(YARN_COMMAND).args(["global", "upgrade", "--latest"])
    };

    yarn_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
}

fn yarn_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Yarn, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_yarn_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let version = line.split_whitespace().next()?.trim();
    if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(version.to_owned())
}

fn parse_yarn_list_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut parse_attempted = false;
    let mut recognized_shape = false;
    let mut dependencies = BTreeMap::new();
    if let Ok(json) = serde_json::from_str::<Value>(output)
        && let Some(map) = json.get("dependencies").and_then(Value::as_object)
    {
        parse_attempted = true;
        recognized_shape = true;
        for (name, payload) in map {
            let version = if let Some(version) = payload.get("version").and_then(Value::as_str) {
                Some(version.trim().to_string())
            } else if let Some(version) = payload.as_str() {
                Some(version.trim().to_string())
            } else {
                None
            };

            if let Some(version) = version
                && !version.is_empty()
            {
                dependencies.insert(name.clone(), version);
            }
        }
    }

    if dependencies.is_empty() {
        for line in output
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            parse_attempted = true;
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if value.get("type").and_then(Value::as_str) != Some("tree") {
                continue;
            }
            recognized_shape = true;
            let Some(trees) = value
                .get("data")
                .and_then(|data| data.get("trees"))
                .and_then(Value::as_array)
            else {
                continue;
            };

            for tree in trees {
                let Some(name_with_version) = tree.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let Some((name, version)) = name_with_version.rsplit_once('@') else {
                    continue;
                };
                let name = name.trim();
                let version = version.trim();
                if !name.is_empty() && !version.is_empty() {
                    dependencies.insert(name.to_string(), version.to_string());
                }
            }
        }
    }

    if dependencies.is_empty() && parse_attempted && !recognized_shape {
        return Err(parse_error("invalid yarn list payload"));
    }

    Ok(dependencies
        .into_iter()
        .map(|(name, version)| InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Yarn,
                name,
            },
            installed_version: Some(version),
            pinned: false,
        })
        .collect())
}

fn parse_yarn_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return Ok(Vec::new());
    }

    let mut parse_attempted = false;
    let mut recognized_shape = false;
    let mut packages = Vec::new();
    if let Ok(json) = serde_json::from_str::<Value>(trimmed)
        && let Some(map) = json.as_object()
    {
        parse_attempted = true;
        recognized_shape = true;
        for (name, payload) in map {
            let installed_version = payload
                .get("current")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);

            let candidate_version = payload
                .get("latest")
                .and_then(Value::as_str)
                .or_else(|| payload.get("wanted").and_then(Value::as_str))
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);

            let Some(candidate_version) = candidate_version else {
                continue;
            };

            packages.push(OutdatedPackage {
                package: PackageRef {
                    manager: ManagerId::Yarn,
                    name: name.clone(),
                },
                installed_version,
                candidate_version,
                pinned: false,
                restart_required: false,
            });
        }
    }

    if packages.is_empty() {
        for line in trimmed
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            parse_attempted = true;
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if value.get("type").and_then(Value::as_str) != Some("table") {
                continue;
            }
            recognized_shape = true;

            let Some(body) = value
                .get("data")
                .and_then(|data| data.get("body"))
                .and_then(Value::as_array)
            else {
                continue;
            };

            for row in body {
                let Some(columns) = row.as_array() else {
                    continue;
                };
                if columns.len() < 4 {
                    continue;
                }
                let Some(name) = columns.first().and_then(Value::as_str) else {
                    continue;
                };
                let installed_version = columns
                    .get(1)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let candidate_version = columns
                    .get(3)
                    .or_else(|| columns.get(2))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string);
                let Some(candidate_version) = candidate_version else {
                    continue;
                };

                packages.push(OutdatedPackage {
                    package: PackageRef {
                        manager: ManagerId::Yarn,
                        name: name.to_string(),
                    },
                    installed_version,
                    candidate_version,
                    pinned: false,
                    restart_required: false,
                });
            }
        }
    }

    if packages.is_empty() && parse_attempted && !recognized_shape {
        return Err(parse_error("invalid yarn outdated payload"));
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

#[derive(Debug, Deserialize)]
struct YarnSearchEntry {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
}

fn parse_yarn_search(output: &str, query: &SearchQuery) -> AdapterResult<Vec<CachedSearchResult>> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let entries: Vec<YarnSearchEntry> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)
            .map_err(|e| parse_error(&format!("invalid yarn search JSON: {e}")))?
    } else {
        let mut parsed = Vec::new();
        for line in trimmed
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let entry: YarnSearchEntry = serde_json::from_str(line)
                .map_err(|e| parse_error(&format!("invalid yarn search JSON line: {e}")))?;
            parsed.push(entry);
        }
        parsed
    };

    let mut results = Vec::new();
    for entry in entries {
        let Some(name) = entry.name.map(|name| name.trim().to_string()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Yarn,
                    name,
                },
                version: entry
                    .version
                    .map(|version| version.trim().to_string())
                    .filter(|version| !version.is_empty()),
                summary: entry
                    .description
                    .map(|description| description.trim().to_string())
                    .filter(|description| !description.is_empty()),
            },
            source_manager: ManagerId::Yarn,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    Ok(results)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Yarn),
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
        YarnAdapter, YarnDetectOutput, YarnSource, parse_yarn_list_installed, parse_yarn_outdated,
        parse_yarn_search, parse_yarn_version, yarn_detect_request, yarn_install_request,
        yarn_list_installed_request, yarn_list_outdated_request, yarn_search_request,
        yarn_uninstall_request, yarn_upgrade_request,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/yarn/version.txt");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/yarn/list_global.json");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/yarn/outdated_global.json");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/yarn/search_array.json");
    const SEARCH_NDJSON_FIXTURE: &str =
        include_str!("../../tests/fixtures/yarn/search_ndjson.jsonl");

    #[test]
    fn parses_yarn_version_from_fixture() {
        let version = parse_yarn_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("10.9.2"));
    }

    #[test]
    fn parses_installed_from_fixture() {
        let packages = parse_yarn_list_installed(LIST_FIXTURE).unwrap();
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].package.name, "npm");
        assert_eq!(packages[0].installed_version.as_deref(), Some("10.9.2"));
        assert_eq!(packages[1].package.name, "typescript");
        assert_eq!(packages[2].package.name, "vercel");
    }

    #[test]
    fn parses_outdated_from_fixture() {
        let outdated = parse_yarn_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(outdated.len(), 2);
        assert_eq!(outdated[0].package.name, "npm-check-updates");
        assert_eq!(outdated[0].installed_version.as_deref(), Some("16.14.11"));
        assert_eq!(outdated[0].candidate_version, "17.1.12");
        assert_eq!(outdated[1].package.name, "typescript");
    }

    #[test]
    fn parses_search_results_from_array_fixture() {
        let query = SearchQuery {
            text: "typescript".to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let results = parse_yarn_search(SEARCH_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.package.name, "typescript");
        assert_eq!(results[0].source_manager, ManagerId::Yarn);
    }

    #[test]
    fn parses_search_results_from_ndjson_fixture() {
        let query = SearchQuery {
            text: "vite".to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let results = parse_yarn_search(SEARCH_NDJSON_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.package.name, "vite");
    }

    #[test]
    fn request_builders_use_expected_commands() {
        let detect = yarn_detect_request(Some(TaskId(11)));
        assert_eq!(detect.manager, ManagerId::Yarn);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.action, ManagerAction::Detect);
        assert_eq!(detect.command.program, PathBuf::from("yarn"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let list = yarn_list_installed_request(None);
        assert_eq!(
            list.command.args,
            vec!["global", "list", "--depth=0", "--json"]
        );

        let outdated = yarn_list_outdated_request(None);
        assert_eq!(outdated.command.args, vec!["outdated", "--json"]);

        let search = yarn_search_request(
            None,
            &SearchQuery {
                text: "ripgrep".to_string(),
                issued_at: std::time::SystemTime::now(),
            },
        );
        assert_eq!(search.command.args, vec!["search", "ripgrep", "--json"]);

        let install = yarn_install_request(None, "typescript", Some("5.7.2"));
        assert_eq!(
            install.command.args,
            vec!["global", "add", "typescript@5.7.2"]
        );

        let uninstall = yarn_uninstall_request(None, "typescript");
        assert_eq!(
            uninstall.command.args,
            vec!["global", "remove", "typescript"]
        );

        let upgrade_one = yarn_upgrade_request(None, Some("typescript"));
        assert_eq!(
            upgrade_one.command.args,
            vec!["global", "upgrade", "typescript"]
        );

        let upgrade_all = yarn_upgrade_request(None, None);
        assert_eq!(
            upgrade_all.command.args,
            vec!["global", "upgrade", "--latest"]
        );
    }

    #[derive(Clone)]
    struct StubYarnSource {
        detect_calls: Arc<AtomicUsize>,
        detect_result: AdapterResult<YarnDetectOutput>,
        list_installed_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
        search_result: AdapterResult<String>,
    }

    impl StubYarnSource {
        fn success() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                detect_result: Ok(YarnDetectOutput {
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/yarn")),
                    version_output: "10.9.2\n".to_string(),
                }),
                list_installed_result: Ok(LIST_FIXTURE.to_string()),
                list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
                search_result: Ok(SEARCH_FIXTURE.to_string()),
            }
        }
    }

    impl YarnSource for StubYarnSource {
        fn detect(&self) -> AdapterResult<YarnDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            self.detect_result.clone()
        }

        fn list_installed_global(&self) -> AdapterResult<String> {
            self.list_installed_result.clone()
        }

        fn list_outdated_global(&self) -> AdapterResult<String> {
            self.list_outdated_result.clone()
        }

        fn search(&self, _query: &str) -> AdapterResult<String> {
            self.search_result.clone()
        }

        fn install_global(&self, _name: &str, _version: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn uninstall_global(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade_global(&self, _name: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }
    }

    #[test]
    fn execute_detect_returns_detection_response() {
        let source = StubYarnSource::success();
        let calls = source.detect_calls.clone();
        let adapter = YarnAdapter::new(source);

        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .expect("detect should succeed");

        match response {
            AdapterResponse::Detection(detection) => {
                assert!(detection.installed);
                assert_eq!(detection.version.as_deref(), Some("10.9.2"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn execute_list_installed_uses_parser() {
        let adapter = YarnAdapter::new(StubYarnSource::success());

        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .expect("list installed should succeed");

        match response {
            AdapterResponse::InstalledPackages(packages) => {
                assert_eq!(packages.len(), 3);
                assert_eq!(packages[0].package.manager, ManagerId::Yarn);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn execute_search_returns_cached_results() {
        let adapter = YarnAdapter::new(StubYarnSource::success());

        let response = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "typescript".to_string(),
                    issued_at: std::time::SystemTime::now(),
                },
            }))
            .expect("search should succeed");

        match response {
            AdapterResponse::SearchResults(results) => {
                assert_eq!(results.len(), 2);
                assert_eq!(results[0].source_manager, ManagerId::Yarn);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_requires_capability_and_returns_mutation() {
        let adapter = YarnAdapter::new(StubYarnSource::success());

        let response = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Yarn,
                    name: "typescript".to_string(),
                },
                version: Some("5.7.2".to_string()),
            }))
            .expect("install should succeed");

        match response {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Install);
                assert_eq!(mutation.package.manager, ManagerId::Yarn);
                assert_eq!(mutation.after_version.as_deref(), Some("5.7.2"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_rejects_option_like_package_name() {
        let adapter = YarnAdapter::new(StubYarnSource::success());

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Yarn,
                    name: "--registry=http://malicious".to_string(),
                },
                version: None,
            }))
            .expect_err("expected invalid input");
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn parse_errors_are_structured() {
        let error = parse_yarn_list_installed("{not json").expect_err("expected parse failure");
        assert_eq!(error.manager, Some(ManagerId::Yarn));
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
    }
}
