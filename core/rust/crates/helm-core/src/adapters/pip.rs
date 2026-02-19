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

const PIP_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const PIP_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Pip,
    display_name: "pip",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: PIP_CAPABILITIES,
};

const PYTHON_COMMAND: &str = "python3";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PipDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait PipSource: Send + Sync {
    fn detect(&self) -> AdapterResult<PipDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn install(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall(&self, name: &str) -> AdapterResult<String>;
    fn upgrade(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct PipAdapter<S: PipSource> {
    source: S,
}

impl<S: PipSource> PipAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: PipSource> ManagerAdapter for PipAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &PIP_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_pip_version(&output.version_output);
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
                let packages = parse_pip_list(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_pip_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                // pip does not provide a stable broad remote search API in modern versions.
                // For local-first behavior, derive matches from installed packages.
                let raw = self.source.list_installed()?;
                let results = parse_pip_local_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::Pip,
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
                    ManagerId::Pip,
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
                    manager: ManagerId::Pip,
                    name: "__self__".to_string(),
                });

                let target_name = if package.name == "__self__" {
                    None
                } else {
                    crate::adapters::validate_package_identifier(
                        ManagerId::Pip,
                        ManagerAction::Upgrade,
                        package.name.as_str(),
                    )?;
                    Some(package.name.as_str())
                };
                let _ = self.source.upgrade(target_name)?;
                if let Some(name) = target_name {
                    ensure_pip_no_longer_outdated(&self.source, name)?;
                }

                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Pip),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "pip adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn pip_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    pip_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(PYTHON_COMMAND).args(["-m", "pip", "--version"]),
        DETECT_TIMEOUT,
    )
}

pub fn pip_list_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    pip_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(PYTHON_COMMAND).args([
            "-m",
            "pip",
            "list",
            "--format=json",
            "--disable-pip-version-check",
        ]),
        LIST_TIMEOUT,
    )
}

pub fn pip_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    pip_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(PYTHON_COMMAND).args([
            "-m",
            "pip",
            "list",
            "--outdated",
            "--format=json",
            "--disable-pip-version-check",
        ]),
        LIST_TIMEOUT,
    )
}

pub fn pip_search_request(task_id: Option<TaskId>, _query: &SearchQuery) -> ProcessSpawnRequest {
    // Search is implemented via local installed package filtering.
    // Command shape remains deterministic for task attribution.
    pip_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(PYTHON_COMMAND).args([
            "-m",
            "pip",
            "list",
            "--format=json",
            "--disable-pip-version-check",
        ]),
        SEARCH_TIMEOUT,
    )
}

pub fn pip_install_request(
    task_id: Option<TaskId>,
    name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let package = match version {
        Some(version) if !version.trim().is_empty() => format!("{name}=={}", version.trim()),
        _ => name.to_string(),
    };

    pip_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(PYTHON_COMMAND)
            .args(["-m", "pip", "install", "--disable-pip-version-check"])
            .arg(package),
        MUTATION_TIMEOUT,
    )
}

pub fn pip_uninstall_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    pip_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(PYTHON_COMMAND).args([
            "-m",
            "pip",
            "uninstall",
            "-y",
            "--disable-pip-version-check",
            name,
        ]),
        MUTATION_TIMEOUT,
    )
}

pub fn pip_upgrade_request(task_id: Option<TaskId>, name: Option<&str>) -> ProcessSpawnRequest {
    let command = if let Some(name) = name {
        CommandSpec::new(PYTHON_COMMAND)
            .args([
                "-m",
                "pip",
                "install",
                "--upgrade",
                "--disable-pip-version-check",
            ])
            .arg(name)
    } else {
        CommandSpec::new(PYTHON_COMMAND).args([
            "-m",
            "pip",
            "install",
            "--upgrade",
            "--disable-pip-version-check",
            "pip",
        ])
    };

    pip_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
}

fn pip_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Pip, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

#[derive(Debug, Deserialize)]
struct PipListEntry {
    name: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct PipOutdatedEntry {
    name: String,
    version: String,
    latest_version: String,
}

fn parse_pip_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    // Example: pip 24.3.1 from /opt/homebrew/lib/python3.12/site-packages/pip (python 3.12)
    let rest = line.strip_prefix("pip ")?;
    let version = rest.split_whitespace().next()?.trim();
    if version.is_empty() {
        return None;
    }
    Some(version.to_string())
}

fn parse_pip_list(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let entries: Vec<PipListEntry> = serde_json::from_str(output)
        .map_err(|e| parse_error(&format!("invalid pip list JSON: {e}")))?;

    let mut packages: Vec<InstalledPackage> = entries
        .into_iter()
        .filter_map(|entry| {
            let name = entry.name.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let version = entry.version.trim().to_string();
            Some(InstalledPackage {
                package: PackageRef {
                    manager: ManagerId::Pip,
                    name,
                },
                installed_version: if version.is_empty() {
                    None
                } else {
                    Some(version)
                },
                pinned: false,
            })
        })
        .collect();

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn ensure_pip_no_longer_outdated<S: PipSource>(
    source: &S,
    package_name: &str,
) -> AdapterResult<()> {
    let raw = source.list_outdated()?;
    let outdated = parse_pip_outdated(&raw)?;
    if outdated
        .iter()
        .any(|item| item.package.name == package_name)
    {
        return Err(CoreError {
            manager: Some(ManagerId::Pip),
            task: Some(TaskType::Upgrade),
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::ProcessFailure,
            message: format!(
                "pip install --upgrade reported success but '{package_name}' remains outdated"
            ),
        });
    }
    Ok(())
}

fn parse_pip_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let entries: Vec<PipOutdatedEntry> = serde_json::from_str(output)
        .map_err(|e| parse_error(&format!("invalid pip outdated JSON: {e}")))?;

    let mut packages: Vec<OutdatedPackage> = entries
        .into_iter()
        .filter_map(|entry| {
            let name = entry.name.trim().to_string();
            let installed = entry.version.trim().to_string();
            let latest = entry.latest_version.trim().to_string();
            if name.is_empty() || latest.is_empty() {
                return None;
            }
            Some(OutdatedPackage {
                package: PackageRef {
                    manager: ManagerId::Pip,
                    name,
                },
                installed_version: if installed.is_empty() {
                    None
                } else {
                    Some(installed)
                },
                candidate_version: latest,
                pinned: false,
                restart_required: false,
            })
        })
        .collect();

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_pip_local_search(
    output: &str,
    query: &SearchQuery,
) -> AdapterResult<Vec<CachedSearchResult>> {
    let entries: Vec<PipListEntry> = serde_json::from_str(output)
        .map_err(|e| parse_error(&format!("invalid pip list JSON: {e}")))?;

    let needle = query.text.to_ascii_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }

    let mut results: Vec<CachedSearchResult> = entries
        .into_iter()
        .filter_map(|entry| {
            let name = entry.name.trim().to_string();
            if name.is_empty() || !name.to_ascii_lowercase().contains(&needle) {
                return None;
            }
            let version = entry.version.trim().to_string();

            Some(CachedSearchResult {
                result: PackageCandidate {
                    package: PackageRef {
                        manager: ManagerId::Pip,
                        name,
                    },
                    version: if version.is_empty() {
                        None
                    } else {
                        Some(version)
                    },
                    summary: None,
                },
                source_manager: ManagerId::Pip,
                originating_query: query.text.clone(),
                cached_at: query.issued_at,
            })
        })
        .collect();

    results.sort_by(|a, b| a.result.package.name.cmp(&b.result.package.name));
    Ok(results)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Pip),
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
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, PackageRef, SearchQuery, TaskId};

    use super::{
        PipAdapter, PipDetectOutput, PipSource, parse_pip_list, parse_pip_local_search,
        parse_pip_outdated, parse_pip_version, pip_detect_request, pip_install_request,
        pip_list_outdated_request, pip_list_request, pip_search_request, pip_uninstall_request,
        pip_upgrade_request,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/pip/version.txt");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/pip/list.json");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/pip/outdated.json");

    #[test]
    fn parses_pip_version_from_fixture() {
        assert_eq!(
            parse_pip_version(VERSION_FIXTURE).as_deref(),
            Some("24.3.1")
        );
    }

    #[test]
    fn parses_installed_list_from_fixture() {
        let packages = parse_pip_list(LIST_FIXTURE).unwrap();
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].package.name, "black");
        assert_eq!(packages[1].package.name, "pip");
        assert_eq!(packages[2].package.name, "requests");
    }

    #[test]
    fn parses_outdated_list_from_fixture() {
        let packages = parse_pip_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "black");
        assert_eq!(packages[0].installed_version.as_deref(), Some("24.8.0"));
        assert_eq!(packages[0].candidate_version, "24.10.0");
    }

    #[test]
    fn parses_local_search_from_installed_payload() {
        let query = SearchQuery {
            text: "pi".to_string(),
            issued_at: std::time::SystemTime::now(),
        };
        let results = parse_pip_local_search(LIST_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "pip");
    }

    #[test]
    fn request_builders_use_structured_python_args() {
        let detect = pip_detect_request(Some(TaskId(33)));
        assert_eq!(detect.manager, ManagerId::Pip);
        assert_eq!(detect.command.program, PathBuf::from("python3"));
        assert_eq!(detect.command.args, vec!["-m", "pip", "--version"]);

        let list = pip_list_request(None);
        assert_eq!(
            list.command.args,
            vec![
                "-m",
                "pip",
                "list",
                "--format=json",
                "--disable-pip-version-check"
            ]
        );

        let outdated = pip_list_outdated_request(None);
        assert_eq!(
            outdated.command.args,
            vec![
                "-m",
                "pip",
                "list",
                "--outdated",
                "--format=json",
                "--disable-pip-version-check"
            ]
        );

        let search = pip_search_request(
            None,
            &SearchQuery {
                text: "black".to_string(),
                issued_at: std::time::SystemTime::now(),
            },
        );
        assert_eq!(
            search.command.args,
            vec![
                "-m",
                "pip",
                "list",
                "--format=json",
                "--disable-pip-version-check"
            ]
        );

        let install = pip_install_request(None, "black", Some("24.10.0"));
        assert_eq!(
            install.command.args,
            vec![
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "black==24.10.0"
            ]
        );

        let uninstall = pip_uninstall_request(None, "black");
        assert_eq!(
            uninstall.command.args,
            vec![
                "-m",
                "pip",
                "uninstall",
                "-y",
                "--disable-pip-version-check",
                "black"
            ]
        );

        let upgrade_one = pip_upgrade_request(None, Some("black"));
        assert_eq!(
            upgrade_one.command.args,
            vec![
                "-m",
                "pip",
                "install",
                "--upgrade",
                "--disable-pip-version-check",
                "black"
            ]
        );

        let upgrade_self = pip_upgrade_request(None, None);
        assert_eq!(
            upgrade_self.command.args,
            vec![
                "-m",
                "pip",
                "install",
                "--upgrade",
                "--disable-pip-version-check",
                "pip"
            ]
        );
    }

    #[derive(Clone)]
    struct StubPipSource {
        detect_calls: Arc<AtomicUsize>,
        detect_result: AdapterResult<PipDetectOutput>,
        list_result: AdapterResult<String>,
        outdated_result: AdapterResult<String>,
    }

    impl StubPipSource {
        fn success() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                detect_result: Ok(PipDetectOutput {
                    executable_path: Some(PathBuf::from("/usr/bin/python3")),
                    version_output: VERSION_FIXTURE.to_string(),
                }),
                list_result: Ok(LIST_FIXTURE.to_string()),
                outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
            }
        }
    }

    impl PipSource for StubPipSource {
        fn detect(&self) -> AdapterResult<PipDetectOutput> {
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
    fn execute_supported_requests() {
        let adapter = PipAdapter::new(StubPipSource::success());

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
        let adapter = PipAdapter::new(StubPipSource::success());

        match adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pip,
                    name: "black".to_string(),
                },
                version: Some("24.10.0".to_string()),
            }))
            .unwrap()
        {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.action, ManagerAction::Install);
                assert_eq!(mutation.package.manager, ManagerId::Pip);
            }
            other => panic!("unexpected response: {other:?}"),
        }
    }

    #[test]
    fn install_rejects_option_like_package_name() {
        let adapter = PipAdapter::new(StubPipSource::success());

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Pip,
                    name: "--index-url=https://invalid".to_string(),
                },
                version: None,
            }))
            .expect_err("expected invalid input");
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn invalid_json_produces_parse_failure() {
        let error = parse_pip_list("{not-json").expect_err("expected parse failure");
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
        assert_eq!(error.manager, Some(ManagerId::Pip));
    }
}
