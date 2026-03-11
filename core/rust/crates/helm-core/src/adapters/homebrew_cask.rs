use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::adapters::homebrew::parse_homebrew_version;
use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const HOMEBREW_CASK_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const HOMEBREW_CASK_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::HomebrewCask,
    display_name: "Homebrew (casks)",
    category: ManagerCategory::GuiApp,
    authority: ManagerAuthority::Standard,
    capabilities: HOMEBREW_CASK_CAPABILITIES,
};

const BREW_COMMAND: &str = "brew";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(180);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
const LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(4 * 60 * 60);
const LIFECYCLE_IDLE_TIMEOUT: Duration = Duration::from_secs(45 * 60);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomebrewCaskDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait HomebrewCaskSource: Send + Sync {
    fn detect(&self) -> AdapterResult<HomebrewCaskDetectOutput>;
    fn list_installed_casks(&self) -> AdapterResult<String>;
    fn list_outdated_casks(&self) -> AdapterResult<String>;
    fn search_casks(&self, query: &SearchQuery) -> AdapterResult<String>;
    fn install_cask(&self, name: &str) -> AdapterResult<String>;
    fn uninstall_cask(&self, name: &str) -> AdapterResult<String>;
    fn upgrade_cask(&self, name: Option<&str>) -> AdapterResult<String>;
}

pub struct HomebrewCaskAdapter<S: HomebrewCaskSource> {
    source: S,
}

impl<S: HomebrewCaskSource> HomebrewCaskAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: HomebrewCaskSource> ManagerAdapter for HomebrewCaskAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &HOMEBREW_CASK_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_homebrew_version(&output.version_output);
                let installed = version.is_some();
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: output.executable_path,
                    version,
                }))
            }
            AdapterRequest::Refresh(_) => {
                let output = self.source.detect()?;
                let version = parse_homebrew_version(&output.version_output);
                if version.is_none() {
                    return Ok(AdapterResponse::SnapshotSync {
                        installed: Some(Vec::new()),
                        outdated: Some(Vec::new()),
                    });
                }

                let installed =
                    parse_homebrew_cask_installed(&self.source.list_installed_casks()?)?;
                let outdated = parse_homebrew_cask_outdated(&self.source.list_outdated_casks()?)?;
                Ok(AdapterResponse::SnapshotSync {
                    installed: Some(installed),
                    outdated: Some(outdated),
                })
            }
            AdapterRequest::ListInstalled(_) => {
                let raw = self.source.list_installed_casks()?;
                let packages = parse_homebrew_cask_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated_casks()?;
                let packages = parse_homebrew_cask_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search_casks(&search_request.query)?;
                let results = parse_homebrew_cask_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                validate_homebrew_cask_target(
                    install_request.package.name.as_str(),
                    install_request.version.as_deref(),
                    ManagerAction::Install,
                )?;
                let before_version = resolve_homebrew_cask_version(
                    &self.source,
                    install_request.package.name.as_str(),
                )?;
                if let Err(error) = self.source.install_cask(&install_request.package.name)
                    && !is_homebrew_cask_already_installed_error(&error)
                {
                    return Err(error);
                }
                let after_version = resolve_homebrew_cask_version(
                    &self.source,
                    install_request.package.name.as_str(),
                )?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: install_request.package,
                    package_identifier: None,
                    action: ManagerAction::Install,
                    before_version,
                    after_version,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                validate_homebrew_cask_target(
                    uninstall_request.package.name.as_str(),
                    uninstall_request.version.as_deref(),
                    ManagerAction::Uninstall,
                )?;
                let before_version = resolve_homebrew_cask_version(
                    &self.source,
                    uninstall_request.package.name.as_str(),
                )?;
                if let Err(error) = self.source.uninstall_cask(&uninstall_request.package.name)
                    && !is_homebrew_cask_already_absent_error(&error)
                {
                    return Err(error);
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    package_identifier: None,
                    action: ManagerAction::Uninstall,
                    before_version,
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let requested_package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::HomebrewCask,
                    name: "__all__".to_string(),
                });
                validate_homebrew_cask_upgrade_target(
                    requested_package.name.as_str(),
                    upgrade_request.version.as_deref(),
                )?;
                let target_name = if requested_package.name == "__all__" {
                    None
                } else {
                    Some(requested_package.name.as_str())
                };
                let targeted_outdated = if let Some(target_name) = target_name {
                    find_outdated_homebrew_cask(&self.source, target_name)?
                } else {
                    None
                };
                let _ = self.source.upgrade_cask(target_name)?;
                if let Some(target_name) = target_name {
                    ensure_cask_no_longer_outdated(&self.source, target_name)?;
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: requested_package,
                    package_identifier: None,
                    action: ManagerAction::Upgrade,
                    before_version: targeted_outdated
                        .as_ref()
                        .and_then(|item| item.installed_version.clone()),
                    after_version: targeted_outdated.map(|item| item.candidate_version),
                }))
            }
            AdapterRequest::ConfigurePackageDetail(_) => unreachable!(
                "unsupported package detail request should have been rejected by ensure_request_supported"
            ),
            AdapterRequest::Pin(_) | AdapterRequest::Unpin(_) => Err(CoreError {
                manager: Some(ManagerId::HomebrewCask),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "homebrew cask does not support native pinning".to_string(),
            }),
        }
    }
}

fn ensure_cask_no_longer_outdated<S: HomebrewCaskSource>(
    source: &S,
    cask_name: &str,
) -> AdapterResult<()> {
    let raw = source.list_outdated_casks()?;
    let outdated = parse_homebrew_cask_outdated(&raw)?;
    if outdated.iter().any(|item| item.package.name == cask_name) {
        return Err(CoreError {
            manager: Some(ManagerId::HomebrewCask),
            task: Some(TaskType::Upgrade),
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::ProcessFailure,
            message: format!(
                "homebrew cask upgrade reported success but '{cask_name}' remains outdated"
            ),
        });
    }
    Ok(())
}

fn resolve_homebrew_cask_version<S: HomebrewCaskSource>(
    source: &S,
    cask_name: &str,
) -> AdapterResult<Option<String>> {
    let installed = parse_homebrew_cask_installed(&source.list_installed_casks()?)?;
    Ok(installed
        .into_iter()
        .find(|item| item.package.name == cask_name)
        .and_then(|item| item.installed_version))
}

fn find_outdated_homebrew_cask<S: HomebrewCaskSource>(
    source: &S,
    cask_name: &str,
) -> AdapterResult<Option<OutdatedPackage>> {
    let outdated = parse_homebrew_cask_outdated(&source.list_outdated_casks()?)?;
    Ok(outdated
        .into_iter()
        .find(|item| item.package.name == cask_name))
}

pub fn homebrew_cask_catalog_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::CatalogSync,
        ManagerAction::Search,
        CommandSpec::new(BREW_COMMAND).arg("casks"),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_cask_search_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    if query.text.trim().is_empty() {
        return homebrew_cask_catalog_request(task_id);
    }

    homebrew_cask_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(BREW_COMMAND)
            .args(["search", "--cask", "--desc"])
            .arg(query.text.clone()),
        SEARCH_TIMEOUT,
    )
}

pub fn homebrew_cask_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(BREW_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn homebrew_cask_config_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(BREW_COMMAND).arg("config"),
        DETECT_TIMEOUT,
    )
}

pub fn homebrew_cask_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(BREW_COMMAND).args(["info", "--cask", "--json=v2", "--installed"]),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_cask_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(BREW_COMMAND).args(["outdated", "--cask", "--json=v2"]),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_cask_install_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(BREW_COMMAND).args(["install", "--cask", name]),
        LIFECYCLE_TIMEOUT,
    )
    .idle_timeout(LIFECYCLE_IDLE_TIMEOUT)
}

pub fn homebrew_cask_uninstall_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_cask_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(BREW_COMMAND).args(["uninstall", "--cask", name]),
        LIFECYCLE_TIMEOUT,
    )
    .idle_timeout(LIFECYCLE_IDLE_TIMEOUT)
}

pub fn homebrew_cask_upgrade_request(
    task_id: Option<TaskId>,
    name: Option<&str>,
) -> ProcessSpawnRequest {
    let command = if let Some(name) = name {
        CommandSpec::new(BREW_COMMAND).args(["upgrade", "--cask", name])
    } else {
        CommandSpec::new(BREW_COMMAND).args(["upgrade", "--cask"])
    };
    homebrew_cask_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        LIFECYCLE_TIMEOUT,
    )
    .idle_timeout(LIFECYCLE_IDLE_TIMEOUT)
}

fn homebrew_cask_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::HomebrewCask, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_homebrew_cask_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let payload: Value = serde_json::from_str(trimmed)
        .map_err(|error| parse_error(&format!("invalid homebrew cask installed JSON: {error}")))?;

    let casks = payload
        .get("casks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut packages = Vec::new();
    for cask in casks {
        let Some(name) = cask_identifier(&cask) else {
            continue;
        };

        let installed_version =
            cask.get("installed")
                .and_then(Value::as_array)
                .and_then(|versions| {
                    versions
                        .iter()
                        .rev()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .find(|value| !value.is_empty())
                        .map(str::to_string)
                });

        let Some(installed_version) = installed_version else {
            continue;
        };

        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewCask,
                name,
            },
            package_identifier: None,
            installed_version: Some(installed_version),
            pinned: false,
            runtime_state: Default::default(),
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_homebrew_cask_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return Ok(Vec::new());
    }

    let payload: Value = serde_json::from_str(trimmed)
        .map_err(|error| parse_error(&format!("invalid homebrew cask outdated JSON: {error}")))?;

    let casks = payload
        .get("casks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut packages = Vec::new();
    for cask in casks {
        let Some(name) = cask_identifier(&cask) else {
            continue;
        };

        let candidate_version = cask
            .get("current_version")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let Some(candidate_version) = candidate_version else {
            continue;
        };

        let installed_version = cask
            .get("installed_versions")
            .and_then(Value::as_array)
            .and_then(|versions| {
                versions
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::trim)
                    .find(|value| !value.is_empty())
                    .map(str::to_string)
            });

        if installed_version.as_deref() == Some(candidate_version.as_str()) {
            continue;
        }

        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewCask,
                name,
            },
            package_identifier: None,
            installed_version,
            candidate_version,
            pinned: false,
            restart_required: false,
            runtime_state: Default::default(),
        });
    }

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(packages)
}

fn parse_homebrew_cask_search(
    output: &str,
    query: &SearchQuery,
) -> AdapterResult<Vec<CachedSearchResult>> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut in_cask_section = query.text.trim().is_empty();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.starts_with("Warning:") {
            continue;
        }

        if let Some(next) = parse_search_section_header(line) {
            in_cask_section = next == SearchSection::Casks;
            continue;
        }

        if !in_cask_section || is_no_results_diagnostic(line) {
            continue;
        }

        let candidate = if let Some((name, summary)) = parse_homebrew_cask_desc_line(line) {
            Some((name, summary))
        } else {
            parse_homebrew_cask_catalog_line(line).map(|name| (name, None))
        };

        let Some((name, summary)) = candidate else {
            continue;
        };

        if seen.insert(name.clone()) {
            results.push(CachedSearchResult {
                result: PackageCandidate {
                    package: PackageRef {
                        manager: ManagerId::HomebrewCask,
                        name,
                    },
                    package_identifier: None,
                    version: None,
                    summary,
                },
                source_manager: ManagerId::HomebrewCask,
                originating_query: query.text.clone(),
                cached_at: query.issued_at,
            });
        }
    }

    Ok(results)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SearchSection {
    Unspecified,
    Formulae,
    Casks,
}

fn parse_search_section_header(line: &str) -> Option<SearchSection> {
    if !line.starts_with("==>") {
        return None;
    }
    let lowered = line.to_ascii_lowercase();
    if lowered.contains("formula") {
        return Some(SearchSection::Formulae);
    }
    if lowered.contains("cask") {
        return Some(SearchSection::Casks);
    }
    Some(SearchSection::Unspecified)
}

fn is_no_results_diagnostic(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.starts_with("no formulae or casks found for")
        || lowered.starts_with("no formula or cask found for")
}

fn is_homebrew_cask_name_token(token: &str) -> bool {
    if token.is_empty() || token.starts_with("==>") {
        return false;
    }
    token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '+' | '-' | '_' | '.' | '/'))
}

fn parse_homebrew_cask_desc_line(line: &str) -> Option<(String, Option<String>)> {
    let (name, raw_summary) = line.split_once(':')?;
    let name = name.trim();
    if !is_homebrew_cask_name_token(name) {
        return None;
    }

    let mut summary = raw_summary.trim();
    if summary.starts_with('(')
        && let Some(end) = summary.find(')')
    {
        summary = summary[end + 1..].trim();
    }
    let summary = normalize_search_summary(summary);
    Some((name.to_string(), summary))
}

fn parse_homebrew_cask_catalog_line(line: &str) -> Option<String> {
    let token = line.trim();
    if !is_homebrew_cask_name_token(token) {
        return None;
    }
    Some(token.to_string())
}

fn normalize_search_summary(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("[no description]")
        || trimmed.eq_ignore_ascii_case("no description")
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn validate_homebrew_cask_target(
    package_name: &str,
    version: Option<&str>,
    action: ManagerAction,
) -> AdapterResult<()> {
    crate::adapters::validate_package_identifier(ManagerId::HomebrewCask, action, package_name)?;
    if version.is_some() {
        return Err(CoreError {
            manager: Some(ManagerId::HomebrewCask),
            task: None,
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message:
                "homebrew cask actions do not accept a separate version argument; use the cask token itself, e.g. 'iterm2@beta'"
                    .to_string(),
        });
    }
    Ok(())
}

fn validate_homebrew_cask_upgrade_target(
    package_name: &str,
    version: Option<&str>,
) -> AdapterResult<()> {
    if package_name != "__all__" {
        validate_homebrew_cask_target(package_name, version, ManagerAction::Upgrade)?;
    } else if version.is_some() {
        return Err(CoreError {
            manager: Some(ManagerId::HomebrewCask),
            task: None,
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::InvalidInput,
            message: "homebrew cask upgrade does not accept a separate version argument"
                .to_string(),
        });
    }
    Ok(())
}

fn is_homebrew_cask_already_installed_error(error: &CoreError) -> bool {
    if error.kind != CoreErrorKind::ProcessFailure {
        return false;
    }
    let lower = error.message.to_ascii_lowercase();
    lower.contains("already installed")
        || lower.contains("already an app at")
        || lower.contains("it seems there is already an app")
}

fn is_homebrew_cask_already_absent_error(error: &CoreError) -> bool {
    if error.kind != CoreErrorKind::ProcessFailure {
        return false;
    }
    let lower = error.message.to_ascii_lowercase();
    lower.contains("is not installed")
        || lower.contains("not installed")
        || lower.contains("no cask with this name installed")
}

fn cask_identifier(cask: &Value) -> Option<String> {
    if let Some(token) = cask
        .get("token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(token.to_string());
    }

    if let Some(name) = cask
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(name.to_string());
    }

    cask.get("name")
        .and_then(Value::as_array)
        .and_then(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .find(|v| !v.is_empty())
        })
        .map(str::to_string)
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::HomebrewCask),
        task: None,
        action: None,
        kind: CoreErrorKind::ParseFailure,
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::adapters::homebrew_cask::{
        HomebrewCaskAdapter, HomebrewCaskDetectOutput, HomebrewCaskSource,
        homebrew_cask_catalog_request, homebrew_cask_detect_request, homebrew_cask_install_request,
        homebrew_cask_list_installed_request, homebrew_cask_list_outdated_request,
        homebrew_cask_search_request, homebrew_cask_uninstall_request,
        homebrew_cask_upgrade_request, parse_homebrew_cask_installed, parse_homebrew_cask_outdated,
        parse_homebrew_cask_search,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, InstallRequest,
        ListInstalledRequest, ListOutdatedRequest, ManagerAdapter, SearchRequest,
    };
    use crate::models::{ManagerAction, ManagerId, PackageRef, SearchQuery, TaskType};
    use std::time::UNIX_EPOCH;

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew_cask/version.txt");
    const INSTALLED_FIXTURE: &str =
        include_str!("../../tests/fixtures/homebrew_cask/installed.json");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew_cask/outdated.json");
    const SEARCH_FIXTURE: &str = "Warning: Use `--eval-all` to search 1 additional cask in third party taps.\n==> Casks\nfont-ia-writer-mono: (iA Writer Mono) [no description]\niterm2: (iTerm2) Terminal emulator as alternative to Apple's Terminal app\niterm2@beta: (iTerm2) Terminal emulator as alternative to Apple's Terminal app\n";

    #[test]
    fn parses_installed_casks_from_fixture() {
        let packages = parse_homebrew_cask_installed(INSTALLED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "google-chrome");
        assert_eq!(
            packages[0].installed_version.as_deref(),
            Some("133.0.6943.142")
        );
    }

    #[test]
    fn parses_outdated_casks_from_fixture() {
        let packages = parse_homebrew_cask_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "google-chrome");
        assert_eq!(
            packages[0].installed_version.as_deref(),
            Some("133.0.6943.142")
        );
        assert_eq!(packages[0].candidate_version, "134.0.6998.89");
    }

    #[test]
    fn parses_cask_search_results_with_descriptions() {
        let query = SearchQuery {
            text: "iterm".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let results = parse_homebrew_cask_search(SEARCH_FIXTURE, &query).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].result.package.name, "font-ia-writer-mono");
        assert_eq!(results[0].result.summary, None);
        assert_eq!(results[1].result.package.name, "iterm2");
        assert_eq!(
            results[1].result.summary.as_deref(),
            Some("Terminal emulator as alternative to Apple's Terminal app")
        );
        assert_eq!(results[2].result.package.name, "iterm2@beta");
    }

    #[test]
    fn detect_and_list_request_shapes_match_expected_commands() {
        let detect = homebrew_cask_detect_request(None);
        assert_eq!(detect.manager, ManagerId::HomebrewCask);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.action, ManagerAction::Detect);
        assert_eq!(detect.command.program.to_str(), Some("brew"));
        assert_eq!(detect.command.args, vec!["--version"]);

        let installed = homebrew_cask_list_installed_request(None);
        assert_eq!(installed.task_type, TaskType::Refresh);
        assert_eq!(installed.action, ManagerAction::ListInstalled);
        assert_eq!(
            installed.command.args,
            vec!["info", "--cask", "--json=v2", "--installed"]
        );

        let outdated = homebrew_cask_list_outdated_request(None);
        assert_eq!(outdated.task_type, TaskType::Refresh);
        assert_eq!(outdated.action, ManagerAction::ListOutdated);
        assert_eq!(
            outdated.command.args,
            vec!["outdated", "--cask", "--json=v2"]
        );

        let search = homebrew_cask_search_request(
            None,
            &SearchQuery {
                text: "iterm".to_string(),
                issued_at: UNIX_EPOCH,
            },
        );
        assert_eq!(search.task_type, TaskType::Search);
        assert_eq!(search.action, ManagerAction::Search);
        assert_eq!(
            search.command.args,
            vec!["search", "--cask", "--desc", "iterm"]
        );

        let catalog = homebrew_cask_catalog_request(None);
        assert_eq!(catalog.task_type, TaskType::CatalogSync);
        assert_eq!(catalog.command.args, vec!["casks"]);

        let install = homebrew_cask_install_request(None, "iterm2");
        assert_eq!(install.task_type, TaskType::Install);
        assert_eq!(install.command.args, vec!["install", "--cask", "iterm2"]);

        let uninstall = homebrew_cask_uninstall_request(None, "iterm2");
        assert_eq!(uninstall.task_type, TaskType::Uninstall);
        assert_eq!(
            uninstall.command.args,
            vec!["uninstall", "--cask", "iterm2"]
        );

        let upgrade = homebrew_cask_upgrade_request(None, Some("iterm2"));
        assert_eq!(upgrade.task_type, TaskType::Upgrade);
        assert_eq!(upgrade.command.args, vec!["upgrade", "--cask", "iterm2"]);
    }

    #[test]
    fn adapter_executes_supported_read_only_requests() {
        let source = FixtureSource {
            detect_result: Ok(HomebrewCaskDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok(INSTALLED_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
            search_result: Ok(SEARCH_FIXTURE.to_string()),
        };
        let adapter = HomebrewCaskAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let AdapterResponse::Detection(info) = detect else {
            panic!("expected detection response");
        };
        assert!(info.installed);

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
                    text: "iterm".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .unwrap();
        let AdapterResponse::SearchResults(results) = search else {
            panic!("expected search response");
        };
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn adapter_executes_mutating_requests() {
        let source = FixtureSource {
            detect_result: Ok(HomebrewCaskDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok(INSTALLED_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_FIXTURE.to_string()),
            search_result: Ok(SEARCH_FIXTURE.to_string()),
        };
        let adapter = HomebrewCaskAdapter::new(source);
        let install = adapter.execute(AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: ManagerId::HomebrewCask,
                name: "iterm2".to_string(),
            },
            target_name: None,
            version: None,
        }));
        assert!(matches!(install, Ok(AdapterResponse::Mutation(_))));

        let uninstall = adapter.execute(AdapterRequest::Uninstall(
            crate::adapters::UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewCask,
                    name: "iterm2".to_string(),
                },
                target_name: None,
                version: None,
            },
        ));
        assert!(matches!(uninstall, Ok(AdapterResponse::Mutation(_))));

        let upgrade = adapter.execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
            package: Some(PackageRef {
                manager: ManagerId::HomebrewCask,
                name: "raycast".to_string(),
            }),
            target_name: None,
            version: None,
        }));
        assert!(matches!(upgrade, Ok(AdapterResponse::Mutation(_))));
    }

    struct FixtureSource {
        detect_result: AdapterResult<HomebrewCaskDetectOutput>,
        list_installed_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
        search_result: AdapterResult<String>,
    }

    impl HomebrewCaskSource for FixtureSource {
        fn detect(&self) -> AdapterResult<HomebrewCaskDetectOutput> {
            self.detect_result.clone()
        }

        fn list_installed_casks(&self) -> AdapterResult<String> {
            self.list_installed_result.clone()
        }

        fn list_outdated_casks(&self) -> AdapterResult<String> {
            self.list_outdated_result.clone()
        }

        fn search_casks(&self, query: &SearchQuery) -> AdapterResult<String> {
            if query.text.trim().is_empty() {
                Ok("iterm2\niterm2@beta\n".to_string())
            } else {
                self.search_result.clone()
            }
        }

        fn install_cask(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn uninstall_cask(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade_cask(&self, _name: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
