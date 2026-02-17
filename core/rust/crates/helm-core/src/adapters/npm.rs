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

const NPM_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Search,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const NPM_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Npm,
    display_name: "npm (global)",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: NPM_CAPABILITIES,
};

const NPM_COMMAND: &str = "npm";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NpmDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait NpmSource: Send + Sync {
    fn detect(&self) -> AdapterResult<NpmDetectOutput>;
    fn list_installed_global(&self) -> AdapterResult<String>;
    fn list_outdated_global(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install_global(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall_global(&self, name: &str) -> AdapterResult<String>;
    fn upgrade_global(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct NpmAdapter<S: NpmSource> {
    source: S,
}

impl<S: NpmSource> NpmAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: NpmSource> ManagerAdapter for NpmAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &NPM_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_npm_version(&output.version_output);
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
                let packages = parse_npm_list_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated_global()?;
                let packages = parse_npm_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search(search_request.query.text.as_str())?;
                let results = parse_npm_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
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
                    manager: ManagerId::Npm,
                    name: "__all__".to_string(),
                });
                let target_name = if package.name == "__all__" {
                    None
                } else {
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
                manager: Some(ManagerId::Npm),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "npm adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn npm_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    npm_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(NPM_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn npm_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    npm_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(NPM_COMMAND).args(["ls", "-g", "--depth=0", "--json"]),
        LIST_TIMEOUT,
    )
}

pub fn npm_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    npm_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(NPM_COMMAND).args(["outdated", "-g", "--json"]),
        LIST_TIMEOUT,
    )
}

pub fn npm_search_request(task_id: Option<TaskId>, query: &SearchQuery) -> ProcessSpawnRequest {
    npm_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(NPM_COMMAND)
            .args(["search", "--json", "--searchlimit=20"])
            .arg(query.text.clone()),
        SEARCH_TIMEOUT,
    )
}

pub fn npm_install_request(
    task_id: Option<TaskId>,
    name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let spec = match version {
        Some(version) if !version.trim().is_empty() => format!("{name}@{}", version.trim()),
        _ => name.to_string(),
    };

    npm_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(NPM_COMMAND)
            .args(["install", "-g"])
            .arg(spec),
        MUTATION_TIMEOUT,
    )
}

pub fn npm_uninstall_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    npm_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(NPM_COMMAND).args(["uninstall", "-g", name]),
        MUTATION_TIMEOUT,
    )
}

pub fn npm_upgrade_request(task_id: Option<TaskId>, name: Option<&str>) -> ProcessSpawnRequest {
    let command = if let Some(name) = name {
        CommandSpec::new(NPM_COMMAND).args(["update", "-g", name])
    } else {
        CommandSpec::new(NPM_COMMAND).args(["update", "-g"])
    };

    npm_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
}

fn npm_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Npm, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_npm_version(output: &str) -> Option<String> {
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

fn parse_npm_list_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let json: Value = serde_json::from_str(output)
        .map_err(|e| parse_error(&format!("invalid npm ls JSON: {e}")))?;

    let mut dependencies = BTreeMap::new();
    if let Some(map) = json.get("dependencies").and_then(Value::as_object) {
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

    Ok(dependencies
        .into_iter()
        .map(|(name, version)| InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Npm,
                name,
            },
            installed_version: Some(version),
            pinned: false,
        })
        .collect())
}

fn parse_npm_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return Ok(Vec::new());
    }

    let json: Value = serde_json::from_str(trimmed)
        .map_err(|e| parse_error(&format!("invalid npm outdated JSON: {e}")))?;

    let mut packages = Vec::new();
    let Some(map) = json.as_object() else {
        return Ok(packages);
    };

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
                manager: ManagerId::Npm,
                name: name.clone(),
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

#[derive(Debug, Deserialize)]
struct NpmSearchEntry {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
}

fn parse_npm_search(output: &str, query: &SearchQuery) -> AdapterResult<Vec<CachedSearchResult>> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let entries: Vec<NpmSearchEntry> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed)
            .map_err(|e| parse_error(&format!("invalid npm search JSON: {e}")))?
    } else {
        let mut parsed = Vec::new();
        for line in trimmed
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let entry: NpmSearchEntry = serde_json::from_str(line)
                .map_err(|e| parse_error(&format!("invalid npm search JSON line: {e}")))?;
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
                    manager: ManagerId::Npm,
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
            source_manager: ManagerId::Npm,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    Ok(results)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Npm),
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
        NpmAdapter, NpmDetectOutput, NpmSource, npm_detect_request, npm_install_request,
        npm_list_installed_request, npm_list_outdated_request, npm_search_request,
        npm_uninstall_request, npm_upgrade_request, parse_npm_list_installed, parse_npm_outdated,
        parse_npm_search, parse_npm_version,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/npm/version.txt");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/npm/list_global.json");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/npm/outdated_global.json");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/npm/search_array.json");
    const SEARCH_NDJSON_FIXTURE: &str =
        include_str!("../../tests/fixtures/npm/search_ndjson.jsonl");

    #[test]
    fn parses_npm_version_from_fixture() {
        let version = parse_npm_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("10.9.2"));
    }

    #[test]
    fn parses_installed_from_fixture() {
        let packages = parse_npm_list_installed(LIST_FIXTURE).unwrap();
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].package.name, "npm");
        assert_eq!(packages[0].installed_version.as_deref(), Some("10.9.2"));
        assert_eq!(packages[1].package.name, "typescript");
        assert_eq!(packages[2].package.name, "vercel");
    }

    #[test]
    fn parses_outdated_from_fixture() {
        let outdated = parse_npm_outdated(OUTDATED_FIXTURE).unwrap();
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
        let results = parse_npm_search(SEARCH_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.package.name, "typescript");
        assert_eq!(results[0].source_manager, ManagerId::Npm);
    }

    #[test]
    fn parses_search_results_from_ndjson_fixture() {
        let query = SearchQuery {
            text: "vite".to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let results = parse_npm_search(SEARCH_NDJSON_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.package.name, "vite");
    }

    #[test]
    fn request_builders_use_expected_commands() {
        let detect = npm_detect_request(Some(TaskId(11)));
        assert_eq!(detect.manager, ManagerId::Npm);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.action, ManagerAction::Detect);
        assert_eq!(detect.command.program, PathBuf::from("npm"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let list = npm_list_installed_request(None);
        assert_eq!(list.command.args, vec!["ls", "-g", "--depth=0", "--json"]);

        let outdated = npm_list_outdated_request(None);
        assert_eq!(outdated.command.args, vec!["outdated", "-g", "--json"]);

        let search = npm_search_request(
            None,
            &SearchQuery {
                text: "ripgrep".to_string(),
                issued_at: std::time::SystemTime::now(),
            },
        );
        assert_eq!(
            search.command.args,
            vec!["search", "--json", "--searchlimit=20", "ripgrep"]
        );

        let install = npm_install_request(None, "typescript", Some("5.7.2"));
        assert_eq!(
            install.command.args,
            vec!["install", "-g", "typescript@5.7.2"]
        );

        let uninstall = npm_uninstall_request(None, "typescript");
        assert_eq!(
            uninstall.command.args,
            vec!["uninstall", "-g", "typescript"]
        );

        let upgrade_one = npm_upgrade_request(None, Some("typescript"));
        assert_eq!(upgrade_one.command.args, vec!["update", "-g", "typescript"]);

        let upgrade_all = npm_upgrade_request(None, None);
        assert_eq!(upgrade_all.command.args, vec!["update", "-g"]);
    }

    #[derive(Clone)]
    struct StubNpmSource {
        detect_calls: Arc<AtomicUsize>,
        detect_result: AdapterResult<NpmDetectOutput>,
        list_installed_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
        search_result: AdapterResult<String>,
    }

    impl StubNpmSource {
        fn success() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                detect_result: Ok(NpmDetectOutput {
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/npm")),
                    version_output: "10.9.2\n".to_string(),
                }),
                list_installed_result: Ok(LIST_FIXTURE.to_string()),
                list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
                search_result: Ok(SEARCH_FIXTURE.to_string()),
            }
        }
    }

    impl NpmSource for StubNpmSource {
        fn detect(&self) -> AdapterResult<NpmDetectOutput> {
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
        let source = StubNpmSource::success();
        let calls = source.detect_calls.clone();
        let adapter = NpmAdapter::new(source);

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
        let adapter = NpmAdapter::new(StubNpmSource::success());

        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .expect("list installed should succeed");

        match response {
            AdapterResponse::InstalledPackages(packages) => {
                assert_eq!(packages.len(), 3);
                assert_eq!(packages[0].package.manager, ManagerId::Npm);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn execute_search_returns_cached_results() {
        let adapter = NpmAdapter::new(StubNpmSource::success());

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
                assert_eq!(results[0].source_manager, ManagerId::Npm);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_requires_capability_and_returns_mutation() {
        let adapter = NpmAdapter::new(StubNpmSource::success());

        let response = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Npm,
                    name: "typescript".to_string(),
                },
                version: Some("5.7.2".to_string()),
            }))
            .expect("install should succeed");

        match response {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Install);
                assert_eq!(mutation.package.manager, ManagerId::Npm);
                assert_eq!(mutation.after_version.as_deref(), Some("5.7.2"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn parse_errors_are_structured() {
        let error = parse_npm_list_installed("{not json").expect_err("expected parse failure");
        assert_eq!(error.manager, Some(ManagerId::Npm));
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
    }
}
