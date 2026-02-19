use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const POETRY_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Search,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const POETRY_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Poetry,
    display_name: "poetry (self/plugins)",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: POETRY_CAPABILITIES,
};

const POETRY_COMMAND: &str = "poetry";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoetryDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait PoetrySource: Send + Sync {
    fn detect(&self) -> AdapterResult<PoetryDetectOutput>;
    fn list_plugins(&self) -> AdapterResult<String>;
    fn list_outdated_plugins(&self) -> AdapterResult<String>;
    fn install_plugin(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall_plugin(&self, name: &str) -> AdapterResult<String>;
    fn upgrade_plugins(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct PoetryAdapter<S: PoetrySource> {
    source: S,
}

impl<S: PoetrySource> PoetryAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: PoetrySource> ManagerAdapter for PoetryAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &POETRY_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_poetry_version(&output.version_output);
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
                let raw = self.source.list_plugins()?;
                let packages = parse_poetry_plugins_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated_plugins()?;
                let packages = parse_poetry_plugins_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.list_plugins()?;
                let results = parse_poetry_plugins_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::Poetry,
                    ManagerAction::Install,
                    install_request.package.name.as_str(),
                )?;
                let _ = self.source.install_plugin(
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
                    ManagerId::Poetry,
                    ManagerAction::Uninstall,
                    uninstall_request.package.name.as_str(),
                )?;
                let _ = self
                    .source
                    .uninstall_plugin(uninstall_request.package.name.as_str())?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Poetry,
                    name: "__all__".to_string(),
                });
                let target_name = if package.name == "__all__" {
                    None
                } else {
                    crate::adapters::validate_package_identifier(
                        ManagerId::Poetry,
                        ManagerAction::Upgrade,
                        package.name.as_str(),
                    )?;
                    Some(package.name.as_str())
                };
                let _ = self.source.upgrade_plugins(target_name)?;
                if let Some(name) = target_name {
                    ensure_poetry_plugin_no_longer_outdated(&self.source, name)?;
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Poetry),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "poetry adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn poetry_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    poetry_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(POETRY_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn poetry_list_plugins_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    poetry_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(POETRY_COMMAND).args(["self", "show", "plugins", "--no-ansi"]),
        LIST_TIMEOUT,
    )
}

pub fn poetry_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    poetry_list_plugins_request(task_id)
}

pub fn poetry_list_outdated_plugins_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    poetry_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(POETRY_COMMAND).args([
            "self",
            "show",
            "plugins",
            "--outdated",
            "--no-ansi",
        ]),
        LIST_TIMEOUT,
    )
}

pub fn poetry_install_plugin_request(
    task_id: Option<TaskId>,
    name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let spec = match version {
        Some(version) if !version.trim().is_empty() => format!("{name}@{}", version.trim()),
        _ => name.to_string(),
    };

    poetry_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(POETRY_COMMAND)
            .args(["self", "add"])
            .arg(spec),
        MUTATION_TIMEOUT,
    )
}

pub fn poetry_uninstall_plugin_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    poetry_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(POETRY_COMMAND).args(["self", "remove", name]),
        MUTATION_TIMEOUT,
    )
}

pub fn poetry_upgrade_plugins_request(
    task_id: Option<TaskId>,
    name: Option<&str>,
) -> ProcessSpawnRequest {
    let command = if let Some(name) = name {
        CommandSpec::new(POETRY_COMMAND).args(["self", "update", name])
    } else {
        CommandSpec::new(POETRY_COMMAND).args(["self", "update"])
    };

    poetry_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
}

fn poetry_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Poetry, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_poetry_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;

    if let Some(start) = line.find("version") {
        let candidate = line[start + "version".len()..]
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim();
        if !candidate.is_empty() && candidate.starts_with(|c: char| c.is_ascii_digit()) {
            return Some(candidate.to_string());
        }
    }

    let version = line.split_whitespace().next()?.trim();
    if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(version.to_string())
}

fn parse_plugin_line(line: &str) -> Option<(String, Option<String>)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('-') {
        return None;
    }

    let body = trimmed.trim_start_matches('-').trim();
    let (name, rest) = body.split_once(' ')?;
    if name.is_empty() {
        return None;
    }

    let version = rest
        .trim()
        .strip_prefix('(')
        .and_then(|v| v.split(')').next())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);

    Some((name.to_string(), version))
}

fn parse_poetry_plugins_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut packages = Vec::new();

    for line in output.lines() {
        if let Some((name, version)) = parse_plugin_line(line) {
            packages.push(InstalledPackage {
                package: PackageRef {
                    manager: ManagerId::Poetry,
                    name,
                },
                installed_version: version,
                pinned: false,
            });
        }
    }

    if packages.is_empty() && !output.trim().is_empty() {
        return Err(parse_error("invalid poetry plugins output"));
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn ensure_poetry_plugin_no_longer_outdated<S: PoetrySource>(
    source: &S,
    plugin_name: &str,
) -> AdapterResult<()> {
    let raw = source.list_outdated_plugins()?;
    let outdated = parse_poetry_plugins_outdated(&raw)?;
    if outdated
        .iter()
        .any(|item| item.package.name == plugin_name)
    {
        return Err(CoreError {
            manager: Some(ManagerId::Poetry),
            task: Some(TaskType::Upgrade),
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::ProcessFailure,
            message: format!(
                "poetry plugin upgrade reported success but '{plugin_name}' remains outdated"
            ),
        });
    }
    Ok(())
}

fn parse_poetry_plugins_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if !line.starts_with('-') {
            continue;
        }
        let body = line.trim_start_matches('-').trim();
        let (name, rest) = match body.split_once(' ') {
            Some(parts) => parts,
            None => continue,
        };
        if name.is_empty() {
            continue;
        }

        let Some(payload) = rest
            .trim()
            .strip_prefix('(')
            .and_then(|v| v.split(')').next())
        else {
            continue;
        };
        let (installed, latest) = if let Some((left, right)) = payload.split_once("->") {
            (left.trim(), right.trim())
        } else {
            continue;
        };
        if latest.is_empty() {
            continue;
        }

        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Poetry,
                name: name.to_string(),
            },
            installed_version: if installed.is_empty() {
                None
            } else {
                Some(installed.to_string())
            },
            candidate_version: latest.to_string(),
            pinned: false,
            restart_required: false,
        });
    }

    if packages.is_empty() && !output.trim().is_empty() {
        return Err(parse_error("invalid poetry outdated plugins output"));
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_poetry_plugins_search(
    output: &str,
    query: &SearchQuery,
) -> AdapterResult<Vec<CachedSearchResult>> {
    let normalized_query = query.text.to_lowercase();
    let plugins = parse_poetry_plugins_installed(output)?;

    let mut results = Vec::new();
    for plugin in plugins {
        if !plugin
            .package
            .name
            .to_lowercase()
            .contains(&normalized_query)
        {
            continue;
        }

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Poetry,
                    name: plugin.package.name,
                },
                version: plugin.installed_version,
                summary: Some("Installed Poetry plugin".to_string()),
            },
            source_manager: ManagerId::Poetry,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    Ok(results)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Poetry),
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
        ManagerAdapter,
    };
    use crate::models::{
        CoreErrorKind, ManagerAction, ManagerId, PackageRef, SearchQuery, TaskId, TaskType,
    };

    use super::{
        PoetryAdapter, PoetryDetectOutput, PoetrySource, parse_poetry_plugins_installed,
        parse_poetry_plugins_outdated, parse_poetry_plugins_search, parse_poetry_version,
        poetry_detect_request, poetry_install_plugin_request, poetry_list_installed_request,
        poetry_list_outdated_plugins_request, poetry_uninstall_plugin_request,
        poetry_upgrade_plugins_request,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/poetry/version.txt");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/poetry/self_show_plugins.txt");
    const OUTDATED_FIXTURE: &str =
        include_str!("../../tests/fixtures/poetry/self_show_plugins_outdated.txt");

    #[test]
    fn parses_poetry_version_from_fixture() {
        let version = parse_poetry_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("2.1.2"));
    }

    #[test]
    fn parses_installed_from_fixture() {
        let packages = parse_poetry_plugins_installed(LIST_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "poetry-plugin-bundle");
        assert_eq!(packages[1].package.name, "poetry-plugin-export");
    }

    #[test]
    fn parses_outdated_from_fixture() {
        let outdated = parse_poetry_plugins_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(outdated.len(), 1);
        assert_eq!(outdated[0].package.name, "poetry-plugin-export");
        assert_eq!(outdated[0].installed_version.as_deref(), Some("1.8.0"));
        assert_eq!(outdated[0].candidate_version, "1.9.0");
    }

    #[test]
    fn parses_search_results_from_fixture() {
        let query = SearchQuery {
            text: "export".to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let results = parse_poetry_plugins_search(LIST_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "poetry-plugin-export");
        assert_eq!(results[0].source_manager, ManagerId::Poetry);
    }

    #[test]
    fn request_builders_use_expected_commands() {
        let detect = poetry_detect_request(Some(TaskId(11)));
        assert_eq!(detect.manager, ManagerId::Poetry);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.action, ManagerAction::Detect);
        assert_eq!(detect.command.program, PathBuf::from("poetry"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let list = poetry_list_installed_request(None);
        assert_eq!(
            list.command.args,
            vec!["self", "show", "plugins", "--no-ansi"]
        );

        let outdated = poetry_list_outdated_plugins_request(None);
        assert_eq!(
            outdated.command.args,
            vec!["self", "show", "plugins", "--outdated", "--no-ansi"]
        );

        let install = poetry_install_plugin_request(None, "poetry-plugin-export", Some("1.9.0"));
        assert_eq!(
            install.command.args,
            vec!["self", "add", "poetry-plugin-export@1.9.0"]
        );

        let uninstall = poetry_uninstall_plugin_request(None, "poetry-plugin-export");
        assert_eq!(
            uninstall.command.args,
            vec!["self", "remove", "poetry-plugin-export"]
        );

        let upgrade_one = poetry_upgrade_plugins_request(None, Some("poetry-plugin-export"));
        assert_eq!(
            upgrade_one.command.args,
            vec!["self", "update", "poetry-plugin-export"]
        );

        let upgrade_all = poetry_upgrade_plugins_request(None, None);
        assert_eq!(upgrade_all.command.args, vec!["self", "update"]);
    }

    #[derive(Clone)]
    struct StubPoetrySource {
        detect_calls: Arc<AtomicUsize>,
        detect_result: AdapterResult<PoetryDetectOutput>,
        list_result: AdapterResult<String>,
        outdated_result: AdapterResult<String>,
    }

    impl StubPoetrySource {
        fn success() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                detect_result: Ok(PoetryDetectOutput {
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/poetry")),
                    version_output: "Poetry (version 2.1.2)\n".to_string(),
                }),
                list_result: Ok(LIST_FIXTURE.to_string()),
                outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
            }
        }
    }

    impl PoetrySource for StubPoetrySource {
        fn detect(&self) -> AdapterResult<PoetryDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            self.detect_result.clone()
        }

        fn list_plugins(&self) -> AdapterResult<String> {
            self.list_result.clone()
        }

        fn list_outdated_plugins(&self) -> AdapterResult<String> {
            self.outdated_result.clone()
        }

        fn install_plugin(&self, _name: &str, _version: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn uninstall_plugin(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade_plugins(&self, _name: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }
    }

    #[test]
    fn execute_detect_returns_detection_response() {
        let source = StubPoetrySource::success();
        let calls = source.detect_calls.clone();
        let adapter = PoetryAdapter::new(source);

        let response = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .expect("detect should succeed");

        match response {
            AdapterResponse::Detection(detection) => {
                assert!(detection.installed);
                assert_eq!(detection.version.as_deref(), Some("2.1.2"));
            }
            other => panic!("unexpected response: {other:?}"),
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn execute_list_installed_uses_parser() {
        let adapter = PoetryAdapter::new(StubPoetrySource::success());

        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .expect("list installed should succeed");

        match response {
            AdapterResponse::InstalledPackages(packages) => {
                assert_eq!(packages.len(), 2);
                assert_eq!(packages[0].package.manager, ManagerId::Poetry);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_rejects_option_like_package_name() {
        let adapter = PoetryAdapter::new(StubPoetrySource::success());

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Poetry,
                    name: "--source=http://malicious".to_string(),
                },
                version: None,
            }))
            .expect_err("expected invalid input");
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }
}
