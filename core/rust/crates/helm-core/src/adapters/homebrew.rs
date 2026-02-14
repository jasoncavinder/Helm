use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const HOMEBREW_READ_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
    Capability::Pin,
    Capability::Unpin,
];

const HOMEBREW_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::HomebrewFormula,
    display_name: "Homebrew (formulae)",
    category: ManagerCategory::SystemOs,
    authority: ManagerAuthority::Guarded,
    capabilities: HOMEBREW_READ_CAPABILITIES,
};

const HOMEBREW_COMMAND: &str = "brew";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomebrewDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);

pub trait HomebrewSource: Send + Sync {
    fn detect(&self) -> AdapterResult<HomebrewDetectOutput>;

    fn list_installed_formulae(&self) -> AdapterResult<String>;

    fn list_outdated_formulae(&self) -> AdapterResult<String>;

    fn search_local_formulae(&self, query: &str) -> AdapterResult<String>;

    fn install_formula(&self, name: &str) -> AdapterResult<String>;

    fn uninstall_formula(&self, name: &str) -> AdapterResult<String>;

    fn upgrade_formula(&self, name: Option<&str>) -> AdapterResult<String>;

    fn pin_formula(&self, name: &str) -> AdapterResult<String>;

    fn unpin_formula(&self, name: &str) -> AdapterResult<String>;
}

pub struct HomebrewAdapter<S: HomebrewSource> {
    source: S,
}

impl<S: HomebrewSource> HomebrewAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: HomebrewSource> ManagerAdapter for HomebrewAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &HOMEBREW_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let detect_output = self.source.detect()?;
                Ok(AdapterResponse::Detection(parse_detection_output(
                    detect_output,
                )))
            }
            AdapterRequest::Refresh(_) => {
                let _ = self.source.detect()?;
                Ok(AdapterResponse::Refreshed)
            }
            AdapterRequest::ListInstalled(_) => {
                let raw = self.source.list_installed_formulae()?;
                let packages = parse_installed_formulae(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated_formulae()?;
                let packages = parse_outdated_formulae(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self
                    .source
                    .search_local_formulae(search_request.query.text.as_str())?;
                let results = parse_search_formulae(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                if let Err(error) = self.source.install_formula(&install_request.package.name) {
                    let lower = error.message.to_ascii_lowercase();
                    let already_installed = error.kind == CoreErrorKind::ProcessFailure
                        && (lower.contains("already installed")
                            || lower.contains("is already installed"));
                    if !already_installed {
                        return Err(error);
                    }
                }
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
                    .uninstall_formula(&uninstall_request.package.name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "__all__".to_string(),
                });
                let _ = self.source.upgrade_formula(Some(package.name.as_str()))?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Pin(pin_request) => {
                let _ = self.source.pin_formula(&pin_request.package.name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: pin_request.package,
                    action: ManagerAction::Pin,
                    before_version: None,
                    after_version: pin_request.version,
                }))
            }
            AdapterRequest::Unpin(unpin_request) => {
                let _ = self.source.unpin_formula(&unpin_request.package.name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: unpin_request.package,
                    action: ManagerAction::Unpin,
                    before_version: None,
                    after_version: None,
                }))
            }
        }
    }
}

pub fn homebrew_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(HOMEBREW_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn homebrew_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(HOMEBREW_COMMAND).args(["list", "--formula", "--versions"]),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(HOMEBREW_COMMAND).args(["outdated", "--formula", "--verbose"]),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_search_local_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(HOMEBREW_COMMAND)
            .args(["search", "--formula"])
            .arg(query.text.clone()),
        SEARCH_TIMEOUT,
    )
}

pub fn homebrew_install_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(HOMEBREW_COMMAND).args(["install", name]),
        INSTALL_TIMEOUT,
    )
}

pub fn homebrew_uninstall_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(HOMEBREW_COMMAND).args(["uninstall", name]),
        INSTALL_TIMEOUT,
    )
}

pub fn homebrew_upgrade_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    let command = if name == "__self__" {
        CommandSpec::new(HOMEBREW_COMMAND).arg("update")
    } else if name == "__all__" {
        CommandSpec::new(HOMEBREW_COMMAND).arg("upgrade")
    } else {
        CommandSpec::new(HOMEBREW_COMMAND).args(["upgrade", name])
    };
    homebrew_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        INSTALL_TIMEOUT,
    )
}

pub fn homebrew_pin_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Pin,
        ManagerAction::Pin,
        CommandSpec::new(HOMEBREW_COMMAND).args(["pin", name]),
        INSTALL_TIMEOUT,
    )
}

pub fn homebrew_unpin_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Unpin,
        ManagerAction::Unpin,
        CommandSpec::new(HOMEBREW_COMMAND).args(["unpin", name]),
        INSTALL_TIMEOUT,
    )
}

fn homebrew_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request =
        ProcessSpawnRequest::new(ManagerId::HomebrewFormula, task_type, action, command)
            .requires_elevation(false)
            .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_detection_output(output: HomebrewDetectOutput) -> DetectionInfo {
    let parsed_version = parse_homebrew_version(&output.version_output);
    let has_executable = output.executable_path.is_some();
    let installed = has_executable || parsed_version.is_some();

    DetectionInfo {
        installed,
        executable_path: output.executable_path,
        version: parsed_version,
    }
}

fn parse_homebrew_version(output: &str) -> Option<String> {
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(tail) = line.strip_prefix("Homebrew ") {
            let token = tail.split_whitespace().next().unwrap_or(tail);
            if !token.is_empty() {
                return Some(token.to_owned());
            }
        }
    }
    None
}

fn parse_installed_formulae(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut parsed = Vec::new();
    let mut malformed_lines = 0usize;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match parse_installed_line(line) {
            Some((name, version)) => parsed.push(InstalledPackage {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name,
                },
                installed_version: version,
                pinned: false,
            }),
            None => malformed_lines += 1,
        }
    }

    if parsed.is_empty() && malformed_lines > 0 {
        return Err(parse_error(
            "unable to parse any installed Homebrew formulae lines",
        ));
    }

    Ok(parsed)
}

fn parse_installed_line(line: &str) -> Option<(String, Option<String>)> {
    let mut segments = line.split_whitespace();
    let name = segments.next()?.trim();
    if name.is_empty() {
        return None;
    }

    // `brew list --formula --versions` may print multiple installed versions;
    // normalize to the latest token as the active version.
    let version = segments.last().map(str::to_owned);
    Some((name.to_owned(), version))
}

fn parse_outdated_formulae(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut parsed = Vec::new();
    let mut malformed_lines = 0usize;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        match parse_outdated_line(line) {
            Some((name, installed_version, candidate_version)) => parsed.push(OutdatedPackage {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name,
                },
                installed_version,
                candidate_version,
                pinned: false,
                restart_required: false,
            }),
            None => malformed_lines += 1,
        }
    }

    if parsed.is_empty() && malformed_lines > 0 {
        return Err(parse_error(
            "unable to parse any outdated Homebrew formulae lines",
        ));
    }

    Ok(parsed)
}

fn parse_outdated_line(line: &str) -> Option<(String, Option<String>, String)> {
    // Common formats:
    // - "name (installed_version) < candidate_version"
    // - "name (installed_version) != candidate_version"
    // - "name installed_version -> candidate_version"
    let separator = if line.contains(" < ") {
        " < "
    } else if line.contains(" != ") {
        " != "
    } else if line.contains(" -> ") {
        " -> "
    } else if line.contains(" → ") {
        " → "
    } else {
        return None;
    };

    let (left, candidate_version) = line.split_once(separator)?;
    let candidate_version = candidate_version.trim();
    if candidate_version.is_empty() {
        return None;
    }

    let (name, installed_version) = if let Some(paren_start) = left.find(" (") {
        let name = left[..paren_start].trim();
        let version_part = left[paren_start + 2..].trim_end_matches(')').trim();
        (name, Some(version_part.to_owned()))
    } else if let Some((name_part, version_part)) = left.rsplit_once(' ') {
        let name = name_part.trim();
        let version = version_part.trim();
        if !name.is_empty()
            && !version.is_empty()
            && version
                .chars()
                .next()
                .map(|ch| ch.is_ascii_digit())
                .unwrap_or(false)
        {
            (name, Some(version.to_owned()))
        } else {
            (left.trim(), None)
        }
    } else {
        (left.trim(), None)
    };

    if name.is_empty() {
        return None;
    }

    let candidate_token = candidate_version
        .split_whitespace()
        .next()
        .unwrap_or(candidate_version);
    if candidate_token.is_empty() {
        return None;
    }

    Some((
        name.to_owned(),
        installed_version,
        candidate_token.to_owned(),
    ))
}

fn parse_search_formulae(
    output: &str,
    query: &SearchQuery,
) -> AdapterResult<Vec<CachedSearchResult>> {
    let mut parsed = Vec::new();
    let mut seen = HashSet::new();
    let mut section = SearchSection::Unspecified;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some(next) = parse_search_section_header(line) {
            section = next;
            continue;
        }

        if section == SearchSection::Casks {
            continue;
        }

        if is_no_results_diagnostic(line) {
            continue;
        }

        for token in line.split_whitespace() {
            if !is_formula_name_token(token) {
                continue;
            }

            if seen.insert(token.to_string()) {
                parsed.push(CachedSearchResult {
                    result: PackageCandidate {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: token.to_string(),
                        },
                        version: None,
                        summary: None,
                    },
                    source_manager: ManagerId::HomebrewFormula,
                    originating_query: query.text.clone(),
                    cached_at: query.issued_at,
                });
            }
        }
    }

    Ok(parsed)
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

fn is_formula_name_token(token: &str) -> bool {
    if token.is_empty() || token.starts_with("==>") {
        return false;
    }
    token
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '@' | '+' | '-' | '_' | '.' | '/'))
}

fn is_no_results_diagnostic(line: &str) -> bool {
    let lowered = line.to_ascii_lowercase();
    lowered.starts_with("no formulae or casks found for")
        || lowered.starts_with("no formula or cask found for")
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::HomebrewFormula),
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
    use std::time::UNIX_EPOCH;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter, SearchRequest,
    };
    use crate::models::{CoreError, CoreErrorKind, ManagerAction, SearchQuery, TaskId, TaskType};

    use super::{
        HomebrewAdapter, HomebrewDetectOutput, HomebrewSource, homebrew_detect_request,
        homebrew_list_installed_request, homebrew_list_outdated_request, homebrew_pin_request,
        homebrew_search_local_request, homebrew_unpin_request, homebrew_upgrade_request,
        parse_homebrew_version, parse_installed_formulae, parse_outdated_formulae,
        parse_search_formulae,
    };

    const INSTALLED_FIXTURE: &str =
        include_str!("../../tests/fixtures/homebrew/list_installed_versions.txt");
    const OUTDATED_FIXTURE: &str =
        include_str!("../../tests/fixtures/homebrew/list_outdated_verbose.txt");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew/search_local.txt");

    #[test]
    fn parses_homebrew_version_from_standard_banner() {
        let version = parse_homebrew_version("Homebrew 4.2.21\n");
        assert_eq!(version.as_deref(), Some("4.2.21"));
    }

    #[test]
    fn parses_homebrew_version_with_suffix() {
        let version = parse_homebrew_version(
            "Homebrew 5.0.14-46-g17729b5\nHomebrew/homebrew-core (git revision abcdef)\n",
        );
        assert_eq!(version.as_deref(), Some("5.0.14-46-g17729b5"));
    }

    #[test]
    fn detection_marks_not_installed_when_probe_has_no_signals() {
        let detection = super::parse_detection_output(HomebrewDetectOutput {
            executable_path: None,
            version_output: String::new(),
        });
        assert!(!detection.installed);
    }

    #[test]
    fn parses_installed_formulae_fixture() {
        let parsed = parse_installed_formulae(INSTALLED_FIXTURE).unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(parsed[0].package.name, "python@3.12");
        assert_eq!(parsed[0].installed_version.as_deref(), Some("3.12.3"));
        assert_eq!(parsed[2].package.name, "node");
        assert_eq!(parsed[2].installed_version.as_deref(), Some("22.5.1"));
    }

    #[test]
    fn parses_outdated_formulae_fixture() {
        let parsed = parse_outdated_formulae(OUTDATED_FIXTURE).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].package.name, "git");
        assert_eq!(parsed[0].installed_version.as_deref(), Some("2.44.0"));
        assert_eq!(parsed[0].candidate_version, "2.45.1");
    }

    #[test]
    fn parses_outdated_formulae_with_alternate_separators() {
        let parsed = parse_outdated_formulae(
            "foo (1.2.3) != 1.2.4\nbar 2.0.0 -> 2.1.0\nbaz (3.1.0) → 3.2.0",
        )
        .unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].package.name, "foo");
        assert_eq!(parsed[0].candidate_version, "1.2.4");
        assert_eq!(parsed[1].package.name, "bar");
        assert_eq!(parsed[1].installed_version.as_deref(), Some("2.0.0"));
        assert_eq!(parsed[1].candidate_version, "2.1.0");
        assert_eq!(parsed[2].candidate_version, "3.2.0");
    }

    #[test]
    fn returns_parse_error_for_fully_malformed_outdated_output() {
        let error = parse_outdated_formulae("this-is-not-parseable").unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
    }

    #[test]
    fn search_ignores_no_result_diagnostic_line() {
        let query = SearchQuery {
            text: "foo".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let parsed =
            parse_search_formulae("No formulae or casks found for \"foo\".", &query).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn parses_search_formulae_fixture() {
        let query = SearchQuery {
            text: "rip".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let parsed = parse_search_formulae(SEARCH_FIXTURE, &query).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].result.package.name, "ripgrep");
        assert_eq!(parsed[1].result.package.name, "ripgrep-all");
        assert_eq!(parsed[2].result.package.name, "ripsecret");
    }

    #[test]
    fn adapter_executes_supported_read_only_requests() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

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
                    text: "rip".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .unwrap();

        assert!(matches!(detect, AdapterResponse::Detection(_)));
        assert!(matches!(installed, AdapterResponse::InstalledPackages(_)));
        assert!(matches!(outdated, AdapterResponse::OutdatedPackages(_)));
        assert!(matches!(search, AdapterResponse::SearchResults(_)));
    }

    #[test]
    fn adapter_executes_install_request() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: "ripgrep".to_string(),
                },
                version: None,
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_treats_already_installed_as_success_for_install() {
        let source = FixtureSource::with_install_error("Error: mas 1.0.0 is already installed");
        let adapter = HomebrewAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: "mas".to_string(),
                },
                version: None,
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_executes_uninstall_request() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Uninstall(
                crate::adapters::UninstallRequest {
                    package: crate::models::PackageRef {
                        manager: crate::models::ManagerId::HomebrewFormula,
                        name: "ripgrep".to_string(),
                    },
                },
            ))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_executes_upgrade_request() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: Some(crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: "mise".to_string(),
                }),
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_executes_pin_request() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Pin(crate::adapters::PinRequest {
                package: crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: "git".to_string(),
                },
                version: Some("2.45.1".to_string()),
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_executes_unpin_request() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Unpin(crate::adapters::UnpinRequest {
                package: crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: "git".to_string(),
                },
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn detect_command_plan_uses_structured_homebrew_args() {
        let request = homebrew_detect_request(Some(TaskId(11)));
        assert_eq!(request.manager, crate::models::ManagerId::HomebrewFormula);
        assert_eq!(request.task_id, Some(TaskId(11)));
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program, PathBuf::from("brew"));
        assert_eq!(request.command.args, vec!["--version".to_string()]);
        assert!(request.timeout.is_some());
    }

    #[test]
    fn list_command_plans_do_not_build_shell_strings() {
        let installed = homebrew_list_installed_request(None);
        assert_eq!(
            installed.command.args,
            vec![
                "list".to_string(),
                "--formula".to_string(),
                "--versions".to_string()
            ]
        );
        assert_eq!(installed.action, ManagerAction::ListInstalled);
        assert_eq!(installed.task_type, TaskType::Refresh);

        let outdated = homebrew_list_outdated_request(None);
        assert_eq!(
            outdated.command.args,
            vec![
                "outdated".to_string(),
                "--formula".to_string(),
                "--verbose".to_string()
            ]
        );
        assert_eq!(outdated.action, ManagerAction::ListOutdated);
        assert_eq!(outdated.task_type, TaskType::Refresh);
    }

    #[test]
    fn search_command_plan_keeps_query_as_single_argument() {
        let query = SearchQuery {
            text: "rip grep".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let request = homebrew_search_local_request(Some(TaskId(9)), &query);

        assert_eq!(request.task_id, Some(TaskId(9)));
        assert_eq!(request.task_type, TaskType::Search);
        assert_eq!(request.action, ManagerAction::Search);
        assert_eq!(
            request.command.args,
            vec![
                "search".to_string(),
                "--formula".to_string(),
                "rip grep".to_string()
            ]
        );
    }

    #[test]
    fn upgrade_command_plan_is_structured_for_self_and_formula_targets() {
        let self_update = homebrew_upgrade_request(None, "__self__");
        assert_eq!(self_update.command.args, vec!["update".to_string()]);
        assert_eq!(self_update.action, ManagerAction::Upgrade);
        assert_eq!(self_update.task_type, TaskType::Upgrade);

        let formula_upgrade = homebrew_upgrade_request(Some(TaskId(7)), "mise");
        assert_eq!(
            formula_upgrade.command.args,
            vec!["upgrade".to_string(), "mise".to_string()]
        );
        assert_eq!(formula_upgrade.task_id, Some(TaskId(7)));
    }

    #[test]
    fn pin_command_plans_are_structured_for_formula_targets() {
        let pin = homebrew_pin_request(Some(TaskId(12)), "git");
        assert_eq!(pin.task_id, Some(TaskId(12)));
        assert_eq!(pin.task_type, TaskType::Pin);
        assert_eq!(pin.action, ManagerAction::Pin);
        assert_eq!(pin.command.args, vec!["pin".to_string(), "git".to_string()]);

        let unpin = homebrew_unpin_request(None, "git");
        assert_eq!(unpin.task_type, TaskType::Unpin);
        assert_eq!(unpin.action, ManagerAction::Unpin);
        assert_eq!(
            unpin.command.args,
            vec!["unpin".to_string(), "git".to_string()]
        );
    }

    #[derive(Default, Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
        install_error: Option<String>,
    }

    impl FixtureSource {
        fn with_install_error(message: &str) -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                install_error: Some(message.to_string()),
            }
        }
    }

    impl HomebrewSource for FixtureSource {
        fn detect(&self) -> AdapterResult<HomebrewDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(HomebrewDetectOutput {
                executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                version_output: "Homebrew 4.2.21".to_string(),
            })
        }

        fn list_installed_formulae(&self) -> AdapterResult<String> {
            Ok(INSTALLED_FIXTURE.to_string())
        }

        fn list_outdated_formulae(&self) -> AdapterResult<String> {
            Ok(OUTDATED_FIXTURE.to_string())
        }

        fn search_local_formulae(&self, _query: &str) -> AdapterResult<String> {
            Ok(SEARCH_FIXTURE.to_string())
        }

        fn install_formula(&self, _name: &str) -> AdapterResult<String> {
            if let Some(message) = &self.install_error {
                return Err(CoreError {
                    manager: Some(crate::models::ManagerId::HomebrewFormula),
                    task: Some(TaskType::Install),
                    action: Some(ManagerAction::Install),
                    kind: CoreErrorKind::ProcessFailure,
                    message: message.clone(),
                });
            }
            Ok(String::new())
        }

        fn uninstall_formula(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade_formula(&self, _name: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn pin_formula(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn unpin_formula(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
