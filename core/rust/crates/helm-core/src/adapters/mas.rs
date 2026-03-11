use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const MAS_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const MAS_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Mas,
    display_name: "mas",
    category: ManagerCategory::GuiApp,
    authority: ManagerAuthority::Standard,
    capabilities: MAS_CAPABILITIES,
};

const MAS_COMMAND: &str = "mas";
const ALL_PACKAGES_TARGET: &str = "__all__";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(120);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MasDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait MasSource: Send + Sync {
    fn detect(&self) -> AdapterResult<MasDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install(&self, app_id: &str) -> AdapterResult<String>;
    fn uninstall(&self, app_id: &str) -> AdapterResult<String>;
    fn upgrade(&self, app_id: Option<&str>) -> AdapterResult<String>;
}

pub struct MasAdapter<S: MasSource> {
    source: S,
}

impl<S: MasSource> MasAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: MasSource> ManagerAdapter for MasAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &MAS_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_mas_version(&output.version_output);
                let installed = version.is_some();
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: output.executable_path,
                    version,
                }))
            }
            AdapterRequest::Refresh(_) => {
                let output = self.source.detect()?;
                let version = parse_mas_version(&output.version_output);
                if version.is_none() {
                    return Ok(AdapterResponse::SnapshotSync {
                        installed: Some(Vec::new()),
                        outdated: Some(Vec::new()),
                    });
                }

                let installed = parse_mas_list(&self.source.list_installed()?)?;
                let outdated = parse_mas_outdated(&self.source.list_outdated()?)?;
                Ok(AdapterResponse::SnapshotSync {
                    installed: Some(installed),
                    outdated: Some(outdated),
                })
            }
            AdapterRequest::ListInstalled(_) => {
                let raw = self.source.list_installed()?;
                let packages = parse_mas_list(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_mas_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let query = search_request.query.text.trim();
                if query.is_empty() {
                    return Ok(AdapterResponse::SearchResults(Vec::new()));
                }
                let raw = self.source.search(query)?;
                let results = parse_mas_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                validate_mas_version_absent(
                    ManagerAction::Install,
                    install_request.version.as_deref(),
                )?;
                let target =
                    install_request
                        .target_name
                        .clone()
                        .unwrap_or(resolve_mas_install_target(
                            &self.source,
                            install_request.package.name.as_str(),
                        )?);
                let before_version = find_mas_installed_entry(&self.source, target.as_str())?
                    .and_then(|entry| entry.installed_version.or(entry.candidate_version));
                let _ = self.source.install(target.as_str())?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: install_request.package,
                    package_identifier: Some(target),
                    action: ManagerAction::Install,
                    before_version,
                    after_version: None,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                validate_mas_version_absent(
                    ManagerAction::Uninstall,
                    uninstall_request.version.as_deref(),
                )?;
                let target =
                    uninstall_request
                        .target_name
                        .clone()
                        .unwrap_or(resolve_mas_installed_target(
                            &self.source,
                            uninstall_request.package.name.as_str(),
                            ManagerAction::Uninstall,
                        )?);
                let before_version = find_mas_installed_entry(&self.source, target.as_str())?
                    .and_then(|entry| entry.installed_version.or(entry.candidate_version))
                    .ok_or_else(|| CoreError {
                        manager: Some(ManagerId::Mas),
                        task: Some(TaskType::Uninstall),
                        action: Some(ManagerAction::Uninstall),
                        kind: CoreErrorKind::NotInstalled,
                        message: format!("App Store app '{target}' is not installed"),
                    })?;
                let _ = self.source.uninstall(target.as_str())?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    package_identifier: Some(target),
                    action: ManagerAction::Uninstall,
                    before_version: Some(before_version),
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                validate_mas_version_absent(
                    ManagerAction::Upgrade,
                    upgrade_request.version.as_deref(),
                )?;
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Mas,
                    name: ALL_PACKAGES_TARGET.to_string(),
                });
                let target = if package.name == ALL_PACKAGES_TARGET {
                    None
                } else {
                    Some(upgrade_request.target_name.clone().unwrap_or(
                        resolve_mas_installed_target(
                            &self.source,
                            package.name.as_str(),
                            ManagerAction::Upgrade,
                        )?,
                    ))
                };
                let targeted_outdated = target
                    .as_deref()
                    .map(|app_id| find_mas_outdated_entry(&self.source, app_id))
                    .transpose()?
                    .flatten();
                let _ = self.source.upgrade(target.as_deref())?;
                if let Some(app_id) = target.as_deref() {
                    ensure_mas_no_longer_outdated(&self.source, app_id)?;
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    package_identifier: target,
                    action: ManagerAction::Upgrade,
                    before_version: targeted_outdated
                        .as_ref()
                        .and_then(|entry| entry.installed_version.clone()),
                    after_version: targeted_outdated.and_then(|entry| entry.candidate_version),
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Mas),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "mas adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn mas_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    mas_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(MAS_COMMAND).arg("version"),
        DETECT_TIMEOUT,
    )
}

pub fn mas_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    mas_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(MAS_COMMAND).arg("list"),
        LIST_TIMEOUT,
    )
}

pub fn mas_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    mas_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(MAS_COMMAND).arg("outdated"),
        LIST_TIMEOUT,
    )
}

pub fn mas_search_request(task_id: Option<TaskId>, query: &SearchQuery) -> ProcessSpawnRequest {
    mas_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(MAS_COMMAND).args(["search", query.text.as_str()]),
        SEARCH_TIMEOUT,
    )
}

pub fn mas_install_request(task_id: Option<TaskId>, app_id: &str) -> ProcessSpawnRequest {
    mas_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(MAS_COMMAND).args(["install", app_id]),
        MUTATION_TIMEOUT,
    )
}

pub fn mas_get_request(task_id: Option<TaskId>, app_id: &str) -> ProcessSpawnRequest {
    mas_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(MAS_COMMAND).args(["get", app_id]),
        MUTATION_TIMEOUT,
    )
}

pub fn mas_uninstall_request(task_id: Option<TaskId>, app_id: &str) -> ProcessSpawnRequest {
    mas_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(MAS_COMMAND).args(["uninstall", app_id]),
        MUTATION_TIMEOUT,
    )
}

pub fn mas_upgrade_request(task_id: Option<TaskId>, app_id: Option<&str>) -> ProcessSpawnRequest {
    let command = if let Some(app_id) = app_id {
        CommandSpec::new(MAS_COMMAND).args(["upgrade", app_id])
    } else {
        CommandSpec::new(MAS_COMMAND).arg("upgrade")
    };
    mas_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
}

fn mas_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Mas, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_mas_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let version = line.split_whitespace().next()?;
    (!version.is_empty()).then(|| version.to_owned())
}

fn parse_mas_list(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    parse_mas_entries(output)?
        .into_iter()
        .map(installed_package_from_entry)
        .collect()
}

fn parse_mas_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut packages = Vec::new();

    for entry in parse_mas_entries(output)? {
        let Some(candidate_version) = entry.candidate_version.clone() else {
            continue;
        };
        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Mas,
                name: entry.name,
            },
            package_identifier: Some(entry.app_id),
            installed_version: entry.installed_version,
            candidate_version,
            pinned: false,
            restart_required: false,
            runtime_state: Default::default(),
        });
    }

    Ok(packages)
}

fn parse_mas_search(output: &str, query: &SearchQuery) -> AdapterResult<Vec<CachedSearchResult>> {
    let normalized_query = query.text.trim();
    if normalized_query.is_empty() {
        return Ok(Vec::new());
    }

    Ok(parse_mas_entries(output)?
        .into_iter()
        .map(|entry| CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Mas,
                    name: entry.name,
                },
                package_identifier: Some(entry.app_id),
                version: entry.candidate_version.or(entry.installed_version),
                summary: None,
            },
            source_manager: ManagerId::Mas,
            originating_query: normalized_query.to_string(),
            cached_at: SystemTime::now(),
        })
        .collect())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MasEntry {
    app_id: String,
    name: String,
    installed_version: Option<String>,
    candidate_version: Option<String>,
}

fn parse_mas_entries(output: &str) -> AdapterResult<Vec<MasEntry>> {
    let mut entries = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((app_id, rest)) = split_app_id(line) else {
            continue;
        };
        let name = extract_app_name(rest).unwrap_or_else(|| app_id.to_owned());
        let (installed_version, candidate_version) = extract_versions(rest);
        entries.push(MasEntry {
            app_id: app_id.to_owned(),
            name,
            installed_version,
            candidate_version,
        });
    }

    Ok(entries)
}

fn installed_package_from_entry(entry: MasEntry) -> AdapterResult<InstalledPackage> {
    Ok(InstalledPackage {
        package: PackageRef {
            manager: ManagerId::Mas,
            name: entry.name,
        },
        package_identifier: Some(entry.app_id),
        installed_version: entry.installed_version.or(entry.candidate_version),
        pinned: false,
        runtime_state: Default::default(),
    })
}

fn looks_like_mas_identifier(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && !trimmed.starts_with('-')
        && !trimmed.chars().any(char::is_whitespace)
        && (trimmed.chars().all(|char| char.is_ascii_digit()) || trimmed.contains('.'))
}

fn validate_mas_version_absent(action: ManagerAction, version: Option<&str>) -> AdapterResult<()> {
    if version.is_none_or(|value| value.trim().is_empty()) {
        return Ok(());
    }

    Err(CoreError {
        manager: Some(ManagerId::Mas),
        task: None,
        action: Some(action),
        kind: CoreErrorKind::InvalidInput,
        message: "mas does not support explicit package version selection".to_string(),
    })
}

fn resolve_mas_install_target<S: MasSource>(
    source: &S,
    package_name: &str,
) -> AdapterResult<String> {
    if looks_like_mas_identifier(package_name) {
        return Ok(package_name.trim().to_string());
    }

    let raw = source.search(package_name)?;
    let entries = parse_mas_entries(&raw)?;
    resolve_unique_mas_entry(entries, package_name, ManagerAction::Install)
}

fn resolve_mas_installed_target<S: MasSource>(
    source: &S,
    package_name: &str,
    action: ManagerAction,
) -> AdapterResult<String> {
    if looks_like_mas_identifier(package_name) {
        return Ok(package_name.trim().to_string());
    }

    let raw = source.list_installed()?;
    let entries = parse_mas_entries(&raw)?;
    resolve_unique_mas_entry(entries, package_name, action)
}

fn resolve_unique_mas_entry(
    entries: Vec<MasEntry>,
    package_name: &str,
    action: ManagerAction,
) -> AdapterResult<String> {
    let normalized_target = package_name.trim().to_ascii_lowercase();
    let matches = entries
        .into_iter()
        .filter(|entry| entry.name.trim().to_ascii_lowercase() == normalized_target)
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [entry] => Ok(entry.app_id.clone()),
        [] => Err(CoreError {
            manager: Some(ManagerId::Mas),
            task: None,
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "no App Store app matched '{package_name}'; use an App Store ID if needed"
            ),
        }),
        _ => {
            let ids = matches
                .iter()
                .map(|entry| entry.app_id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Err(CoreError {
                manager: Some(ManagerId::Mas),
                task: None,
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!(
                    "multiple App Store apps matched '{package_name}'; use an exact App Store ID ({ids})"
                ),
            })
        }
    }
}

fn find_mas_installed_entry<S: MasSource>(
    source: &S,
    app_id_or_name: &str,
) -> AdapterResult<Option<MasEntry>> {
    let raw = source.list_installed()?;
    let entries = parse_mas_entries(&raw)?;
    let normalized = app_id_or_name.trim().to_ascii_lowercase();
    Ok(entries.into_iter().find(|entry| {
        entry.app_id == normalized || entry.name.trim().to_ascii_lowercase() == normalized
    }))
}

fn find_mas_outdated_entry<S: MasSource>(
    source: &S,
    app_id_or_name: &str,
) -> AdapterResult<Option<MasEntry>> {
    let raw = source.list_outdated()?;
    let entries = parse_mas_entries(&raw)?;
    let normalized = app_id_or_name.trim().to_ascii_lowercase();
    Ok(entries.into_iter().find(|entry| {
        entry.app_id == normalized || entry.name.trim().to_ascii_lowercase() == normalized
    }))
}

fn ensure_mas_no_longer_outdated<S: MasSource>(source: &S, app_id: &str) -> AdapterResult<()> {
    if find_mas_outdated_entry(source, app_id)?.is_some() {
        return Err(CoreError {
            manager: Some(ManagerId::Mas),
            task: Some(TaskType::Upgrade),
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::ProcessFailure,
            message: format!("mas upgrade reported success but '{app_id}' remains outdated"),
        });
    }
    Ok(())
}

fn split_app_id(line: &str) -> Option<(&str, &str)> {
    let mut chars = line.char_indices();
    let end = loop {
        match chars.next() {
            Some((_, ch)) if ch.is_ascii_digit() => continue,
            Some((index, _)) => break index,
            None => return None,
        }
    };
    let app_id = &line[..end];
    if app_id.is_empty() {
        return None;
    }
    let rest = line[end..].trim_start();
    Some((app_id, rest))
}

fn extract_parenthesized_version(text: &str) -> Option<String> {
    let open = text.rfind('(')?;
    let close = text.rfind(')')?;
    if close <= open + 1 {
        return None;
    }
    let inner = text[open + 1..close].trim();
    (!inner.is_empty()).then(|| inner.to_owned())
}

fn extract_versions(text: &str) -> (Option<String>, Option<String>) {
    let Some(inner) = extract_parenthesized_version(text) else {
        return (None, None);
    };

    if let Some((installed, candidate)) = inner.split_once("->") {
        let installed = installed.trim();
        let candidate = candidate.trim();
        (
            (!installed.is_empty()).then(|| installed.to_owned()),
            (!candidate.is_empty()).then(|| candidate.to_owned()),
        )
    } else {
        (Some(inner.clone()), None)
    }
}

fn extract_app_name(text: &str) -> Option<String> {
    let base = if let Some(open) = text.rfind('(') {
        text[..open].trim()
    } else {
        text.trim()
    };
    (!base.is_empty()).then(|| base.to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::SystemTime;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, InstallRequest,
        ListInstalledRequest, ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest,
        UpgradeRequest,
    };
    use crate::models::{
        CoreErrorKind, ManagerAction, ManagerId, PackageRef, SearchQuery, TaskId, TaskType,
    };

    use super::{
        MasAdapter, MasDetectOutput, MasSource, mas_detect_request, mas_get_request,
        mas_install_request, mas_search_request, mas_uninstall_request, mas_upgrade_request,
        parse_mas_list, parse_mas_outdated, parse_mas_search, parse_mas_version,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/mas/version.txt");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/mas/list.txt");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/mas/outdated.txt");
    const LIST_EMPTY_FIXTURE: &str = include_str!("../../tests/fixtures/mas/list_empty.txt");
    const SEARCH_FIXTURE: &str = "497799835 Xcode (16.2)\n409183694 Keynote (14.3)\n";
    const SEARCH_AMBIGUOUS_FIXTURE: &str =
        "100000001 Sample App (1.0)\n100000002 Sample App (2.0)\n";

    #[test]
    fn parses_mas_version_from_output() {
        let version = parse_mas_version("1.8.7\n");
        assert_eq!(version.as_deref(), Some("1.8.7"));
    }

    #[test]
    fn parses_mas_version_from_fixture() {
        let version = parse_mas_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("1.8.7"));
    }

    #[test]
    fn version_parse_returns_none_for_empty_input() {
        assert!(parse_mas_version("").is_none());
        assert!(parse_mas_version("   \n  ").is_none());
    }

    #[test]
    fn parses_mas_list_from_fixture() {
        let packages = parse_mas_list(LIST_FIXTURE).unwrap();
        assert_eq!(packages.len(), 4);
        assert_eq!(packages[0].package.name, "Xcode");
        assert_eq!(packages[0].package_identifier.as_deref(), Some("497799835"));
        assert_eq!(packages[0].installed_version.as_deref(), Some("16.2"));
        assert_eq!(packages[0].package.manager, ManagerId::Mas);
        assert_eq!(packages[1].package.name, "Keynote");
        assert_eq!(packages[1].package_identifier.as_deref(), Some("409183694"));
        assert_eq!(packages[1].installed_version.as_deref(), Some("14.3"));
    }

    #[test]
    fn parses_empty_list() {
        let packages = parse_mas_list(LIST_EMPTY_FIXTURE).unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn parses_mas_outdated_from_fixture() {
        let packages = parse_mas_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "Xcode");
        assert_eq!(packages[0].package_identifier.as_deref(), Some("497799835"));
        assert_eq!(packages[0].installed_version.as_deref(), Some("16.1"));
        assert_eq!(packages[0].candidate_version, "16.2");
    }

    #[test]
    fn parses_search_results_with_identifiers() {
        let results = parse_mas_search(
            SEARCH_FIXTURE,
            &SearchQuery {
                text: "xcode".to_string(),
                issued_at: SystemTime::now(),
            },
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].result.package.name, "Xcode");
        assert_eq!(
            results[0].result.package_identifier.as_deref(),
            Some("497799835")
        );
        assert_eq!(results[0].result.version.as_deref(), Some("16.2"));
    }

    #[test]
    fn adapter_executes_supported_requests() {
        let source = FixtureSource::default();
        let adapter = MasAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let installed = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();
        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();
        let search = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "Xcode".to_string(),
                    issued_at: SystemTime::now(),
                },
            }))
            .unwrap();

        assert!(matches!(detect, AdapterResponse::Detection(_)));
        assert!(matches!(installed, AdapterResponse::InstalledPackages(_)));
        assert!(matches!(outdated, AdapterResponse::OutdatedPackages(_)));
        assert!(matches!(search, AdapterResponse::SearchResults(_)));
    }

    #[test]
    fn adapter_install_resolves_display_name_to_app_id() {
        let source = FixtureSource::default();
        let adapter = MasAdapter::new(source.clone());

        let result = adapter.execute(AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: ManagerId::Mas,
                name: "Xcode".to_string(),
            },
            target_name: None,
            version: None,
        }));

        assert!(matches!(result, Ok(AdapterResponse::Mutation(_))));
        assert_eq!(source.installed_target(), Some("497799835".to_string()));
    }

    #[test]
    fn adapter_install_rejects_ambiguous_display_names() {
        let source = FixtureSource {
            search_output: SEARCH_AMBIGUOUS_FIXTURE.to_string(),
            ..Default::default()
        };
        let adapter = MasAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Mas,
                    name: "Sample App".to_string(),
                },
                target_name: None,
                version: None,
            }))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn adapter_uninstall_resolves_display_name_to_app_id() {
        let source = FixtureSource::default();
        let adapter = MasAdapter::new(source.clone());

        let result = adapter.execute(AdapterRequest::Uninstall(UninstallRequest {
            package: PackageRef {
                manager: ManagerId::Mas,
                name: "Xcode".to_string(),
            },
            target_name: None,
            version: None,
        }));

        assert!(matches!(result, Ok(AdapterResponse::Mutation(_))));
        assert_eq!(source.uninstalled_target(), Some("497799835".to_string()));
    }

    #[test]
    fn adapter_upgrade_all_and_targeted_upgrade_are_supported() {
        let source = FixtureSource::default();
        let adapter = MasAdapter::new(source.clone());

        let all = adapter.execute(AdapterRequest::Upgrade(UpgradeRequest {
            package: None,
            target_name: None,
            version: None,
        }));
        assert!(matches!(all, Ok(AdapterResponse::Mutation(_))));
        assert_eq!(source.upgraded_target(), Some("__all__".to_string()));

        let targeted = adapter.execute(AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(PackageRef {
                manager: ManagerId::Mas,
                name: "Xcode".to_string(),
            }),
            target_name: None,
            version: None,
        }));
        assert!(matches!(targeted, Ok(AdapterResponse::Mutation(_))));
        assert_eq!(source.upgraded_target(), Some("497799835".to_string()));
    }

    #[test]
    fn adapter_rejects_explicit_version_selection() {
        let source = FixtureSource::default();
        let adapter = MasAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Mas,
                    name: "497799835".to_string(),
                },
                target_name: None,
                version: Some("16.2".to_string()),
            }))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn detect_command_spec_uses_structured_args() {
        let request = mas_detect_request(Some(TaskId(55)));
        assert_eq!(request.manager, ManagerId::Mas);
        assert_eq!(request.task_id, Some(TaskId(55)));
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program, PathBuf::from("mas"));
        assert_eq!(request.command.args, vec!["version".to_string()]);
        assert!(request.timeout.is_some());
    }

    #[test]
    fn mas_command_specs_use_structured_args() {
        let search = mas_search_request(
            None,
            &SearchQuery {
                text: "Xcode".to_string(),
                issued_at: SystemTime::now(),
            },
        );
        assert_eq!(
            search.command.args,
            vec!["search".to_string(), "Xcode".to_string()]
        );

        let install = mas_install_request(None, "497799835");
        assert_eq!(
            install.command.args,
            vec!["install".to_string(), "497799835".to_string()]
        );

        let get = mas_get_request(None, "497799835");
        assert_eq!(
            get.command.args,
            vec!["get".to_string(), "497799835".to_string()]
        );

        let uninstall = mas_uninstall_request(None, "497799835");
        assert_eq!(
            uninstall.command.args,
            vec!["uninstall".to_string(), "497799835".to_string()]
        );

        let targeted_upgrade = mas_upgrade_request(None, Some("497799835"));
        assert_eq!(
            targeted_upgrade.command.args,
            vec!["upgrade".to_string(), "497799835".to_string()]
        );

        let all_upgrade = mas_upgrade_request(None, None);
        assert_eq!(all_upgrade.command.args, vec!["upgrade".to_string()]);
    }

    #[derive(Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
        install_target: Arc<std::sync::Mutex<Option<String>>>,
        uninstall_target: Arc<std::sync::Mutex<Option<String>>>,
        upgrade_target: Arc<std::sync::Mutex<Option<String>>>,
        search_output: String,
    }

    impl Default for FixtureSource {
        fn default() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                install_target: Arc::new(std::sync::Mutex::new(None)),
                uninstall_target: Arc::new(std::sync::Mutex::new(None)),
                upgrade_target: Arc::new(std::sync::Mutex::new(None)),
                search_output: SEARCH_FIXTURE.to_string(),
            }
        }
    }

    impl FixtureSource {
        fn installed_target(&self) -> Option<String> {
            self.install_target.lock().unwrap().clone()
        }

        fn uninstalled_target(&self) -> Option<String> {
            self.uninstall_target.lock().unwrap().clone()
        }

        fn upgraded_target(&self) -> Option<String> {
            self.upgrade_target.lock().unwrap().clone()
        }
    }

    impl MasSource for FixtureSource {
        fn detect(&self) -> AdapterResult<MasDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(MasDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/mas")),
                version_output: VERSION_FIXTURE.to_string(),
            })
        }

        fn list_installed(&self) -> AdapterResult<String> {
            Ok(LIST_FIXTURE.to_string())
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            if self.upgraded_target() == Some("497799835".to_string()) {
                return Ok("409183694  Keynote              (14.2 -> 14.3)\n".to_string());
            }
            Ok(OUTDATED_FIXTURE.to_string())
        }

        fn search(&self, _query: &str) -> AdapterResult<String> {
            Ok(self.search_output.clone())
        }

        fn install(&self, app_id: &str) -> AdapterResult<String> {
            *self.install_target.lock().unwrap() = Some(app_id.to_string());
            Ok(String::new())
        }

        fn uninstall(&self, app_id: &str) -> AdapterResult<String> {
            *self.uninstall_target.lock().unwrap() = Some(app_id.to_string());
            Ok(String::new())
        }

        fn upgrade(&self, app_id: Option<&str>) -> AdapterResult<String> {
            *self.upgrade_target.lock().unwrap() = Some(app_id.unwrap_or("__all__").to_string());
            Ok(String::new())
        }
    }
}
