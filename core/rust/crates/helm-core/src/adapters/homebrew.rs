use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::manager_lifecycle::{
    HomebrewUninstallCleanupMode, parse_homebrew_manager_uninstall_package_name,
};
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
const HOMEBREW_CLEANUP_MARKER: &str = "@@helm.cleanup";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomebrewDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

const LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(4 * 60 * 60);
const LIFECYCLE_IDLE_TIMEOUT: Duration = Duration::from_secs(45 * 60);
const PIN_TIMEOUT: Duration = Duration::from_secs(300);

pub trait HomebrewSource: Send + Sync {
    fn detect(&self) -> AdapterResult<HomebrewDetectOutput>;

    fn list_installed_formulae(&self) -> AdapterResult<String>;

    fn list_outdated_formulae(&self) -> AdapterResult<String>;

    fn search_formulae(&self, query: &SearchQuery) -> AdapterResult<String>;

    fn install_formula(&self, name: &str) -> AdapterResult<String>;

    fn uninstall_formula(&self, name: &str) -> AdapterResult<String>;

    fn upgrade_formula(&self, name: Option<&str>) -> AdapterResult<String>;

    fn cleanup_formula(&self, name: &str) -> AdapterResult<String>;

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
                let detect_output = self.source.detect()?;
                let version = parse_homebrew_version(&detect_output.version_output);
                if version.is_none() {
                    return Ok(AdapterResponse::SnapshotSync {
                        installed: Some(Vec::new()),
                        outdated: Some(Vec::new()),
                    });
                }

                let installed = parse_installed_formulae(&self.source.list_installed_formulae()?)?;
                let outdated = parse_outdated_formulae(&self.source.list_outdated_formulae()?)?;
                Ok(AdapterResponse::SnapshotSync {
                    installed: Some(installed),
                    outdated: Some(outdated),
                })
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
                let raw = self.source.search_formulae(&search_request.query)?;
                let results = parse_search_formulae(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                validate_homebrew_formula_target(
                    install_request.package.name.as_str(),
                    install_request.version.as_deref(),
                    ManagerAction::Install,
                )?;
                let before_version = resolve_homebrew_formula_version(
                    &self.source,
                    install_request.package.name.as_str(),
                )?;
                if let Err(error) = self.source.install_formula(&install_request.package.name)
                    && !is_homebrew_already_installed_error(&error)
                {
                    return Err(error);
                }
                let after_version = resolve_homebrew_formula_version(
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
                validate_homebrew_formula_target(
                    uninstall_request.package.name.as_str(),
                    uninstall_request.version.as_deref(),
                    ManagerAction::Uninstall,
                )?;
                let parsed_uninstall = parse_homebrew_manager_uninstall_package_name(
                    uninstall_request.package.name.as_str(),
                );
                let formula_name = parsed_uninstall
                    .as_ref()
                    .map(|spec| spec.formula_name.as_str())
                    .unwrap_or_else(|| uninstall_request.package.name.as_str());
                let before_version = resolve_homebrew_formula_version(&self.source, formula_name)?;
                let uninstall_output = self.source.uninstall_formula(formula_name);
                if let Err(error) = uninstall_output.as_ref()
                    && !is_homebrew_already_absent_uninstall_error(error)
                {
                    return Err(error.clone());
                }
                if let Some(spec) = parsed_uninstall.as_ref()
                    && matches!(spec.cleanup_mode, HomebrewUninstallCleanupMode::FullCleanup)
                {
                    let cleanup_output = perform_manager_full_cleanup(spec.requested_manager)?;
                    if !cleanup_output.trim().is_empty() {
                        crate::execution::record_task_log_note(cleanup_output.as_str());
                    }
                }
                if let Some(spec) = parsed_uninstall.as_ref()
                    && spec.remove_helm_managed_shell_setup
                {
                    match crate::post_install_setup::remove_helm_managed_post_install_setup(
                        spec.requested_manager,
                    ) {
                        Ok(result) => {
                            crate::execution::record_task_log_note(result.summary().as_str());
                            if !result.malformed_files.is_empty() {
                                crate::execution::record_task_log_note(
                                    format!(
                                        "helm-managed {} setup markers were malformed in {} shell startup file(s); left unchanged",
                                        spec.requested_manager.as_str(),
                                        result.malformed_files.len()
                                    )
                                    .as_str(),
                                );
                            }
                        }
                        Err(error) => {
                            crate::execution::record_task_log_note(
                                format!(
                                    "failed to remove Helm-managed {} shell setup block(s): {error}",
                                    spec.requested_manager.as_str()
                                )
                                .as_str(),
                            );
                        }
                    }
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
                    manager: ManagerId::HomebrewFormula,
                    name: "__all__".to_string(),
                });
                validate_homebrew_formula_upgrade_target(
                    requested_package.name.as_str(),
                    upgrade_request.version.as_deref(),
                )?;
                let (target_name, cleanup_after_upgrade) =
                    split_upgrade_target(requested_package.name.as_str());
                let targeted_outdated = if target_name != "__all__" && target_name != "__self__" {
                    find_outdated_homebrew_formula(&self.source, target_name)?
                } else {
                    None
                };
                let _ = self.source.upgrade_formula(Some(target_name))?;
                if target_name != "__all__" && target_name != "__self__" {
                    ensure_formula_no_longer_outdated(&self.source, target_name)?;
                }
                if cleanup_after_upgrade && target_name != "__all__" && target_name != "__self__" {
                    let _ = self.source.cleanup_formula(target_name)?;
                }
                let package = PackageRef {
                    manager: requested_package.manager,
                    name: target_name.to_string(),
                };
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    package_identifier: None,
                    action: ManagerAction::Upgrade,
                    before_version: targeted_outdated
                        .as_ref()
                        .and_then(|item| item.installed_version.clone()),
                    after_version: targeted_outdated.map(|item| item.candidate_version),
                }))
            }
            AdapterRequest::Pin(pin_request) => {
                validate_homebrew_formula_target(
                    pin_request.package.name.as_str(),
                    pin_request.version.as_deref(),
                    ManagerAction::Pin,
                )?;
                let _ = self.source.pin_formula(&pin_request.package.name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: pin_request.package,
                    package_identifier: None,
                    action: ManagerAction::Pin,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Unpin(unpin_request) => {
                crate::adapters::validate_package_identifier(
                    ManagerId::HomebrewFormula,
                    ManagerAction::Unpin,
                    unpin_request.package.name.as_str(),
                )?;
                let _ = self.source.unpin_formula(&unpin_request.package.name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: unpin_request.package,
                    package_identifier: None,
                    action: ManagerAction::Unpin,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::ConfigurePackageDetail(_) => unreachable!(
                "unsupported package detail request should have been rejected by ensure_request_supported"
            ),
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

pub fn homebrew_config_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(HOMEBREW_COMMAND).arg("config"),
        DETECT_TIMEOUT,
    )
}

pub fn homebrew_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(HOMEBREW_COMMAND).args(["info", "--formula", "--json=v2", "--installed"]),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(HOMEBREW_COMMAND).args(["outdated", "--formula", "--json=v2"]),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_catalog_formulae_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::CatalogSync,
        ManagerAction::Search,
        CommandSpec::new(HOMEBREW_COMMAND).arg("formulae"),
        LIST_TIMEOUT,
    )
}

pub fn homebrew_search_formulae_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    if query.text.trim().is_empty() {
        return homebrew_catalog_formulae_request(task_id);
    }

    homebrew_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(HOMEBREW_COMMAND)
            .args(["search", "--formula", "--desc"])
            .arg(query.text.clone()),
        SEARCH_TIMEOUT,
    )
}

pub fn homebrew_search_local_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    homebrew_search_formulae_request(task_id, query)
}

pub fn homebrew_install_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(HOMEBREW_COMMAND).args(["install", name]),
        LIFECYCLE_TIMEOUT,
    )
    .idle_timeout(LIFECYCLE_IDLE_TIMEOUT)
}

pub fn homebrew_uninstall_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(HOMEBREW_COMMAND).args(["uninstall", name]),
        LIFECYCLE_TIMEOUT,
    )
    .idle_timeout(LIFECYCLE_IDLE_TIMEOUT)
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
        LIFECYCLE_TIMEOUT,
    )
    .idle_timeout(LIFECYCLE_IDLE_TIMEOUT)
}

pub fn homebrew_cleanup_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(HOMEBREW_COMMAND).args(["cleanup", name]),
        LIFECYCLE_TIMEOUT,
    )
    .idle_timeout(LIFECYCLE_IDLE_TIMEOUT)
}

fn split_upgrade_target(name: &str) -> (&str, bool) {
    if let Some(stripped) = name.strip_suffix(HOMEBREW_CLEANUP_MARKER) {
        (stripped, true)
    } else {
        (name, false)
    }
}

fn validate_homebrew_formula_target(
    package_name: &str,
    version: Option<&str>,
    action: ManagerAction,
) -> AdapterResult<()> {
    crate::adapters::validate_package_identifier(ManagerId::HomebrewFormula, action, package_name)?;
    if version.is_some() {
        return Err(CoreError {
            manager: Some(ManagerId::HomebrewFormula),
            task: None,
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message:
                "homebrew formula actions do not accept a separate version argument; use a versioned formula name like 'python@3.12'"
                    .to_string(),
        });
    }
    Ok(())
}

fn validate_homebrew_formula_upgrade_target(
    package_name: &str,
    version: Option<&str>,
) -> AdapterResult<()> {
    if package_name != "__all__" && package_name != "__self__" {
        let (target_name, _) = split_upgrade_target(package_name);
        validate_homebrew_formula_target(target_name, version, ManagerAction::Upgrade)?;
    } else if version.is_some() {
        return Err(CoreError {
            manager: Some(ManagerId::HomebrewFormula),
            task: None,
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::InvalidInput,
            message: "homebrew upgrade does not accept a separate version argument".to_string(),
        });
    }

    Ok(())
}

fn is_homebrew_already_installed_error(error: &CoreError) -> bool {
    if error.kind != CoreErrorKind::ProcessFailure {
        return false;
    }
    let lower = error.message.to_ascii_lowercase();
    lower.contains("already installed") || lower.contains("is already installed")
}

fn is_homebrew_already_absent_uninstall_error(error: &CoreError) -> bool {
    if error.kind != CoreErrorKind::ProcessFailure {
        return false;
    }
    let lower = error.message.to_ascii_lowercase();
    lower.contains("no such keg")
        || lower.contains("no installed formula")
        || lower.contains("is not installed")
}

fn ensure_formula_no_longer_outdated<S: HomebrewSource>(
    source: &S,
    formula_name: &str,
) -> AdapterResult<()> {
    let raw = source.list_outdated_formulae()?;
    let outdated = parse_outdated_formulae(&raw)?;
    if outdated
        .iter()
        .any(|item| item.package.name == formula_name)
    {
        return Err(CoreError {
            manager: Some(ManagerId::HomebrewFormula),
            task: Some(TaskType::Upgrade),
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::ProcessFailure,
            message: format!(
                "homebrew upgrade reported success but '{formula_name}' remains outdated"
            ),
        });
    }
    Ok(())
}

fn resolve_homebrew_formula_version<S: HomebrewSource>(
    source: &S,
    formula_name: &str,
) -> AdapterResult<Option<String>> {
    let installed = parse_installed_formulae(&source.list_installed_formulae()?)?;
    Ok(installed
        .into_iter()
        .find(|item| item.package.name == formula_name)
        .and_then(|item| item.installed_version))
}

fn find_outdated_homebrew_formula<S: HomebrewSource>(
    source: &S,
    formula_name: &str,
) -> AdapterResult<Option<OutdatedPackage>> {
    let outdated = parse_outdated_formulae(&source.list_outdated_formulae()?)?;
    Ok(outdated
        .into_iter()
        .find(|item| item.package.name == formula_name))
}

pub fn homebrew_pin_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Pin,
        ManagerAction::Pin,
        CommandSpec::new(HOMEBREW_COMMAND).args(["pin", name]),
        PIN_TIMEOUT,
    )
}

pub fn homebrew_unpin_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    homebrew_request(
        task_id,
        TaskType::Unpin,
        ManagerAction::Unpin,
        CommandSpec::new(HOMEBREW_COMMAND).args(["unpin", name]),
        PIN_TIMEOUT,
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
    let installed = parsed_version.is_some();

    DetectionInfo {
        installed,
        executable_path: output.executable_path,
        version: parsed_version,
    }
}

pub fn parse_homebrew_version(output: &str) -> Option<String> {
    let sanitized = strip_ansi_escape_sequences(output);

    for line in sanitized
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.eq_ignore_ascii_case("homebrew") {
            continue;
        }

        if line.to_ascii_lowercase().starts_with("homebrew")
            && let Some(token) = line
                .split_whitespace()
                .find(|token| is_homebrew_version_token(token))
        {
            return Some(normalize_version_token(token));
        }

        if line.to_ascii_lowercase().contains("homebrew")
            && let Some(token) = line
                .split_whitespace()
                .find(|token| is_homebrew_version_token(token))
        {
            return Some(normalize_version_token(token));
        }
    }

    for token in sanitized.split_whitespace() {
        if is_homebrew_version_token(token) {
            let normalized = normalize_version_token(token);
            if normalized.contains('.') {
                return Some(normalized);
            }
        }
    }
    None
}

fn strip_ansi_escape_sequences(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            output.push(ch);
            continue;
        }

        if chars.peek() == Some(&'[') {
            let _ = chars.next();
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
    }

    output
}

fn normalize_version_token(token: &str) -> String {
    token
        .trim_matches(|ch: char| ch == ',' || ch == ';' || ch == ')' || ch == '(')
        .to_owned()
}

fn is_homebrew_version_token(token: &str) -> bool {
    let normalized = normalize_version_token(token);
    let mut chars = normalized.chars();
    let starts_with_digit = chars.next().map(|ch| ch.is_ascii_digit()).unwrap_or(false);
    starts_with_digit && normalized.contains('.')
}

fn parse_installed_formulae(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return Ok(Vec::new());
    }

    let payload: HomebrewFormulaInstalledEnvelope =
        serde_json::from_str(trimmed).map_err(|error| {
            parse_error(&format!("invalid Homebrew formula installed JSON: {error}"))
        })?;

    let mut parsed = Vec::new();
    for formula in payload.formulae {
        let Some(name) = normalize_optional_text(Some(formula.name)) else {
            continue;
        };

        let installed_version = formula
            .linked_keg
            .and_then(|value| normalize_optional_text(Some(value)))
            .or_else(|| {
                formula
                    .installed
                    .iter()
                    .filter_map(|installed| installed.version.as_ref())
                    .filter_map(|version| normalize_optional_text(Some(version.clone())))
                    .next_back()
            });

        let Some(installed_version) = installed_version else {
            continue;
        };

        parsed.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name,
            },
            package_identifier: None,
            installed_version: Some(installed_version),
            pinned: formula.pinned,
            runtime_state: Default::default(),
        });
    }

    parsed.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(parsed)
}

fn parse_outdated_formulae(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let trimmed = output.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return Ok(Vec::new());
    }

    let payload: HomebrewFormulaOutdatedEnvelope =
        serde_json::from_str(trimmed).map_err(|error| {
            parse_error(&format!("invalid Homebrew formula outdated JSON: {error}"))
        })?;

    let mut parsed = Vec::new();
    for formula in payload.formulae {
        let Some(name) = normalize_optional_text(Some(formula.name)) else {
            continue;
        };
        let installed_version = formula
            .installed_versions
            .iter()
            .filter_map(|version| normalize_optional_text(Some(version.clone())))
            .next_back();
        let Some(candidate_version) = normalize_optional_text(formula.current_version) else {
            continue;
        };

        if installed_version.as_deref() == Some(candidate_version.as_str()) {
            continue;
        }

        parsed.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name,
            },
            package_identifier: None,
            installed_version,
            candidate_version,
            pinned: formula.pinned,
            restart_required: false,
            runtime_state: Default::default(),
        });
    }

    parsed.sort_by(|a, b| a.package.name.cmp(&b.package.name));
    Ok(parsed)
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
        if line.starts_with("Warning:") {
            continue;
        }

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

        let candidate = if let Some((name, summary)) = parse_search_formulae_desc_line(line) {
            Some((name, summary))
        } else {
            parse_search_formula_catalog_line(line).map(|name| (name, None))
        };

        let Some((name, summary)) = candidate else {
            continue;
        };

        if seen.insert(name.clone()) {
            parsed.push(CachedSearchResult {
                result: PackageCandidate {
                    package: PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name,
                    },
                    package_identifier: None,
                    version: None,
                    summary,
                },
                source_manager: ManagerId::HomebrewFormula,
                originating_query: query.text.clone(),
                cached_at: query.issued_at,
            });
        }
    }

    Ok(parsed)
}

fn parse_search_formulae_desc_line(line: &str) -> Option<(String, Option<String>)> {
    let (name, summary) = line.split_once(':')?;
    let name = name.trim();
    if !is_formula_name_token(name) {
        return None;
    }

    let summary = normalize_homebrew_search_summary(summary);
    Some((name.to_string(), summary))
}

fn parse_search_formula_catalog_line(line: &str) -> Option<String> {
    let token = line.trim();
    if !is_formula_name_token(token) {
        return None;
    }
    Some(token.to_string())
}

fn normalize_homebrew_search_summary(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("[no description]")
        || trimmed.eq_ignore_ascii_case("no description")
    {
        return None;
    }
    Some(trimmed.to_string())
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Deserialize)]
struct HomebrewFormulaInstalledEnvelope {
    #[serde(default)]
    formulae: Vec<HomebrewFormulaInstalledEntry>,
}

#[derive(Debug, Deserialize)]
struct HomebrewFormulaInstalledEntry {
    name: String,
    #[serde(default)]
    linked_keg: Option<String>,
    #[serde(default)]
    installed: Vec<HomebrewFormulaInstalledVersion>,
    #[serde(default)]
    pinned: bool,
}

#[derive(Debug, Deserialize)]
struct HomebrewFormulaInstalledVersion {
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct HomebrewFormulaOutdatedEnvelope {
    #[serde(default)]
    formulae: Vec<HomebrewFormulaOutdatedEntry>,
}

#[derive(Debug, Deserialize)]
struct HomebrewFormulaOutdatedEntry {
    name: String,
    #[serde(default)]
    installed_versions: Vec<String>,
    #[serde(default)]
    current_version: Option<String>,
    #[serde(default)]
    pinned: bool,
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

fn perform_manager_full_cleanup(manager: ManagerId) -> AdapterResult<String> {
    match manager {
        ManagerId::Rustup => cleanup_rustup_artifacts(),
        ManagerId::Mise => cleanup_mise_artifacts(),
        _ => Ok(format!(
            "Helm full-cleanup: no additional manager-owned artifacts are currently defined for '{}'.",
            manager.as_str()
        )),
    }
}

fn cleanup_rustup_artifacts() -> AdapterResult<String> {
    let mut lines: Vec<String> = Vec::new();
    let rustup_home = resolve_rustup_home();
    let cargo_home = resolve_cargo_home();
    let cargo_bin = cargo_home.join("bin");

    let removed_rustup_home = remove_directory_if_exists(rustup_home.as_path())?;
    if removed_rustup_home {
        lines.push(format!(
            "Helm full-cleanup: removed rustup home directory '{}'.",
            rustup_home.display()
        ));
    } else {
        lines.push(format!(
            "Helm full-cleanup: rustup home directory '{}' was not present.",
            rustup_home.display()
        ));
    }

    let mut removed_proxy_count = 0usize;
    for binary in [
        "rustup",
        "cargo",
        "rustc",
        "rustdoc",
        "rustfmt",
        "clippy-driver",
        "rust-gdb",
        "rust-gdbgui",
        "rust-lldb",
    ] {
        let path = cargo_bin.join(binary);
        if remove_file_if_exists(path.as_path())? {
            removed_proxy_count += 1;
        }
    }
    lines.push(format!(
        "Helm full-cleanup: removed {} rustup proxy binaries under '{}'.",
        removed_proxy_count,
        cargo_bin.display()
    ));

    Ok(lines.join("\n"))
}

fn cleanup_mise_artifacts() -> AdapterResult<String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"));
    let state_dir = home.join(".local/share/mise");
    let cache_dir = home.join(".cache/mise");

    let removed_state = remove_directory_if_exists(state_dir.as_path())?;
    let removed_cache = remove_directory_if_exists(cache_dir.as_path())?;

    Ok(format!(
        "Helm full-cleanup: state_dir={} ({}) cache_dir={} ({})",
        state_dir.display(),
        if removed_state {
            "removed"
        } else {
            "not_present"
        },
        cache_dir.display(),
        if removed_cache {
            "removed"
        } else {
            "not_present"
        }
    ))
}

fn remove_directory_if_exists(path: &std::path::Path) -> AdapterResult<bool> {
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(path).map_err(|error| CoreError {
        manager: Some(ManagerId::HomebrewFormula),
        task: Some(TaskType::Uninstall),
        action: Some(ManagerAction::Uninstall),
        kind: CoreErrorKind::ProcessFailure,
        message: format!(
            "homebrew full-cleanup failed to remove directory '{}': {error}",
            path.display()
        ),
    })?;
    Ok(true)
}

fn remove_file_if_exists(path: &std::path::Path) -> AdapterResult<bool> {
    if !path.exists() {
        return Ok(false);
    }
    fs::remove_file(path).map_err(|error| CoreError {
        manager: Some(ManagerId::HomebrewFormula),
        task: Some(TaskType::Uninstall),
        action: Some(ManagerAction::Uninstall),
        kind: CoreErrorKind::ProcessFailure,
        message: format!(
            "homebrew full-cleanup failed to remove file '{}': {error}",
            path.display()
        ),
    })?;
    Ok(true)
}

fn resolve_cargo_home() -> PathBuf {
    if let Some(raw) = std::env::var_os("CARGO_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(raw);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".cargo"))
        .unwrap_or_else(|| PathBuf::from("~/.cargo"))
}

fn resolve_rustup_home() -> PathBuf {
    if let Some(raw) = std::env::var_os("RUSTUP_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(raw);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".rustup"))
        .unwrap_or_else(|| PathBuf::from("~/.rustup"))
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
        HomebrewAdapter, HomebrewDetectOutput, HomebrewSource, homebrew_catalog_formulae_request,
        homebrew_cleanup_request, homebrew_detect_request, homebrew_install_request,
        homebrew_list_installed_request, homebrew_list_outdated_request, homebrew_pin_request,
        homebrew_search_formulae_request, homebrew_search_local_request,
        homebrew_uninstall_request, homebrew_unpin_request, homebrew_upgrade_request,
        parse_homebrew_version, parse_installed_formulae, parse_outdated_formulae,
        parse_search_formulae,
    };

    const INSTALLED_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew/installed.json");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew/outdated.json");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/homebrew/search_local.txt");
    const SEARCH_DESC_FIXTURE: &str = "==> Formulae\nripgrep: Recursively search directories for a regex pattern\nripgrep-all: Search all the things\n==> Casks\nripper: should be ignored\n";

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
    fn parses_homebrew_version_with_ansi_sequences() {
        let version = parse_homebrew_version(
            "\u{1b}[1;32mHomebrew 4.4.31\u{1b}[0m\nHomebrew/homebrew-core (git revision)\n",
        );
        assert_eq!(version.as_deref(), Some("4.4.31"));
    }

    #[test]
    fn parses_homebrew_version_from_config_output() {
        let version = parse_homebrew_version(
            "HOMEBREW_VERSION: 5.0.14-52-g807be07\nORIGIN: https://github.com/Homebrew/brew\n",
        );
        assert_eq!(version.as_deref(), Some("5.0.14-52-g807be07"));
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
        assert_eq!(parsed[0].package.name, "node");
        assert_eq!(parsed[0].installed_version.as_deref(), Some("22.5.1"));
        assert_eq!(parsed[1].package.name, "openssl@3");
        assert_eq!(parsed[1].installed_version.as_deref(), Some("3.3.1"));
        assert!(parsed[1].pinned);
        assert_eq!(parsed[2].package.name, "python@3.12");
        assert_eq!(parsed[2].installed_version.as_deref(), Some("3.12.3"));
    }

    #[test]
    fn parses_outdated_formulae_fixture() {
        let parsed = parse_outdated_formulae(OUTDATED_FIXTURE).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].package.name, "git");
        assert_eq!(parsed[0].installed_version.as_deref(), Some("2.44.0"));
        assert_eq!(parsed[0].candidate_version, "2.45.1");
        assert_eq!(parsed[1].package.name, "libzip");
        assert!(parsed[1].pinned);
    }

    #[test]
    fn skips_outdated_entries_when_current_version_matches_installed_version() {
        let parsed = parse_outdated_formulae(OUTDATED_FIXTURE).unwrap();
        assert!(!parsed.iter().any(|package| package.package.name == "bat"));
    }

    #[test]
    fn parses_outdated_formulae_marks_pinned_entries() {
        let parsed = parse_outdated_formulae(OUTDATED_FIXTURE).unwrap();
        let libzip = parsed
            .iter()
            .find(|package| package.package.name == "libzip")
            .expect("expected libzip outdated entry");
        assert_eq!(libzip.installed_version.as_deref(), Some("1.11.4"));
        assert_eq!(libzip.candidate_version, "1.11.4_1");
        assert!(libzip.pinned);
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
    fn parses_search_formulae_with_descriptions() {
        let query = SearchQuery {
            text: "rip".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let parsed = parse_search_formulae(SEARCH_DESC_FIXTURE, &query).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].result.package.name, "ripgrep");
        assert_eq!(
            parsed[0].result.summary.as_deref(),
            Some("Recursively search directories for a regex pattern")
        );
        assert_eq!(parsed[1].result.package.name, "ripgrep-all");
        assert_eq!(
            parsed[1].result.summary.as_deref(),
            Some("Search all the things")
        );
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
                target_name: None,
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
                target_name: None,
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
                    target_name: None,
                    version: None,
                },
            ))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_treats_already_absent_as_success_for_uninstall() {
        let source =
            FixtureSource::with_uninstall_error("Error: No such keg: /opt/homebrew/Cellar/mas");
        let adapter = HomebrewAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Uninstall(
                crate::adapters::UninstallRequest {
                    package: crate::models::PackageRef {
                        manager: crate::models::ManagerId::HomebrewFormula,
                        name: "mas".to_string(),
                    },
                    target_name: None,
                    version: None,
                },
            ))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_surfaces_uninstall_errors_that_are_not_already_absent() {
        let source =
            FixtureSource::with_uninstall_error("Error: uninstall failed due to lock timeout");
        let adapter = HomebrewAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Uninstall(
                crate::adapters::UninstallRequest {
                    package: crate::models::PackageRef {
                        manager: crate::models::ManagerId::HomebrewFormula,
                        name: "mas".to_string(),
                    },
                    target_name: None,
                    version: None,
                },
            ))
            .expect_err("non-idempotent uninstall error should be returned");
        assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
        assert!(error.message.contains("lock timeout"));
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
                target_name: None,
                version: None,
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_executes_pin_request() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Pin(crate::adapters::PinRequest {
                package: crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: "git".to_string(),
                },
                version: Some("2.45.1".to_string()),
            }))
            .expect_err("separate pin version should be rejected for Homebrew formulae");
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
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
                "info".to_string(),
                "--formula".to_string(),
                "--json=v2".to_string(),
                "--installed".to_string()
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
                "--json=v2".to_string()
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
                "--desc".to_string(),
                "rip grep".to_string()
            ]
        );
    }

    #[test]
    fn search_command_plan_uses_catalog_sync_for_empty_query() {
        let query = SearchQuery {
            text: "".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let request = homebrew_search_formulae_request(Some(TaskId(10)), &query);
        assert_eq!(request.task_type, TaskType::CatalogSync);
        assert_eq!(request.action, ManagerAction::Search);
        assert_eq!(request.command.args, vec!["formulae".to_string()]);
    }

    #[test]
    fn catalog_request_is_structured_for_formulae_listing() {
        let request = homebrew_catalog_formulae_request(None);
        assert_eq!(request.task_type, TaskType::CatalogSync);
        assert_eq!(request.action, ManagerAction::Search);
        assert_eq!(request.command.args, vec!["formulae".to_string()]);
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

        let cleanup = homebrew_cleanup_request(None, "sevenzip");
        assert_eq!(
            cleanup.command.args,
            vec!["cleanup".to_string(), "sevenzip".to_string()]
        );
        assert_eq!(cleanup.task_type, TaskType::Upgrade);
        assert_eq!(cleanup.action, ManagerAction::Upgrade);
    }

    #[test]
    fn lifecycle_command_plans_include_extended_hard_and_idle_timeouts() {
        let install = homebrew_install_request(None, "rustup");
        assert_eq!(install.timeout, Some(super::LIFECYCLE_TIMEOUT));
        assert_eq!(install.idle_timeout, Some(super::LIFECYCLE_IDLE_TIMEOUT));

        let uninstall = homebrew_uninstall_request(None, "rustup");
        assert_eq!(uninstall.timeout, Some(super::LIFECYCLE_TIMEOUT));
        assert_eq!(uninstall.idle_timeout, Some(super::LIFECYCLE_IDLE_TIMEOUT));

        let upgrade = homebrew_upgrade_request(None, "rustup");
        assert_eq!(upgrade.timeout, Some(super::LIFECYCLE_TIMEOUT));
        assert_eq!(upgrade.idle_timeout, Some(super::LIFECYCLE_IDLE_TIMEOUT));

        let cleanup = homebrew_cleanup_request(None, "rustup");
        assert_eq!(cleanup.timeout, Some(super::LIFECYCLE_TIMEOUT));
        assert_eq!(cleanup.idle_timeout, Some(super::LIFECYCLE_IDLE_TIMEOUT));
    }

    #[test]
    fn pin_command_plans_keep_short_timeout_and_no_idle_timeout() {
        let pin = homebrew_pin_request(None, "git");
        assert_eq!(pin.timeout, Some(super::PIN_TIMEOUT));
        assert_eq!(pin.idle_timeout, None);

        let unpin = homebrew_unpin_request(None, "git");
        assert_eq!(unpin.timeout, Some(super::PIN_TIMEOUT));
        assert_eq!(unpin.idle_timeout, None);
    }

    #[test]
    fn adapter_upgrade_supports_cleanup_marker_target() {
        let source = FixtureSource::default();
        let adapter = HomebrewAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: Some(crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: format!("sevenzip{}", super::HOMEBREW_CLEANUP_MARKER),
                }),
                target_name: None,
                version: None,
            }))
            .unwrap();
        match result {
            AdapterResponse::Mutation(mutation) => {
                assert_eq!(mutation.package.name, "sevenzip");
            }
            _ => panic!("expected mutation response"),
        }
    }

    #[test]
    fn split_upgrade_target_plain_name() {
        let (name, cleanup) = super::split_upgrade_target("sevenzip");
        assert_eq!(name, "sevenzip");
        assert!(!cleanup);
    }

    #[test]
    fn split_upgrade_target_with_cleanup_marker() {
        let input = format!("sevenzip{}", super::HOMEBREW_CLEANUP_MARKER);
        let (name, cleanup) = super::split_upgrade_target(&input);
        assert_eq!(name, "sevenzip");
        assert!(cleanup);
    }

    #[test]
    fn split_upgrade_target_empty_string() {
        let (name, cleanup) = super::split_upgrade_target("");
        assert_eq!(name, "");
        assert!(!cleanup);
    }

    #[test]
    fn split_upgrade_target_marker_only() {
        let (name, cleanup) = super::split_upgrade_target(super::HOMEBREW_CLEANUP_MARKER);
        assert_eq!(name, "");
        assert!(cleanup);
    }

    #[test]
    fn adapter_upgrade_fails_when_formula_still_outdated_after_upgrade() {
        let source = FixtureSource::with_outdated_output(
            r#"{"formulae":[{"name":"gdu","installed_versions":["1.0.0"],"current_version":"1.0.1","pinned":false}]}"#,
        );
        let adapter = HomebrewAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: Some(crate::models::PackageRef {
                    manager: crate::models::ManagerId::HomebrewFormula,
                    name: "gdu".to_string(),
                }),
                target_name: None,
                version: None,
            }))
            .unwrap_err();

        assert_eq!(error.kind, CoreErrorKind::ProcessFailure);
        assert!(error.message.contains("remains outdated"));
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

    #[derive(Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
        install_error: Option<String>,
        uninstall_error: Option<String>,
        outdated_output: String,
    }

    impl FixtureSource {
        fn with_install_error(message: &str) -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                install_error: Some(message.to_string()),
                uninstall_error: None,
                outdated_output: OUTDATED_FIXTURE.to_string(),
            }
        }

        fn with_uninstall_error(message: &str) -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                install_error: None,
                uninstall_error: Some(message.to_string()),
                outdated_output: OUTDATED_FIXTURE.to_string(),
            }
        }

        fn with_outdated_output(output: &str) -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                install_error: None,
                uninstall_error: None,
                outdated_output: output.to_string(),
            }
        }
    }

    impl Default for FixtureSource {
        fn default() -> Self {
            Self {
                detect_calls: Arc::new(AtomicUsize::new(0)),
                install_error: None,
                uninstall_error: None,
                outdated_output: OUTDATED_FIXTURE.to_string(),
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
            Ok(self.outdated_output.clone())
        }

        fn search_formulae(&self, query: &SearchQuery) -> AdapterResult<String> {
            if query.text.trim().is_empty() {
                Ok("ripgrep\nripgrep-all\nripsecret\n".to_string())
            } else {
                Ok(SEARCH_FIXTURE.to_string())
            }
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
            if let Some(message) = &self.uninstall_error {
                return Err(CoreError {
                    manager: Some(crate::models::ManagerId::HomebrewFormula),
                    task: Some(TaskType::Uninstall),
                    action: Some(ManagerAction::Uninstall),
                    kind: CoreErrorKind::ProcessFailure,
                    message: message.clone(),
                });
            }
            Ok(String::new())
        }

        fn upgrade_formula(&self, _name: Option<&str>) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn cleanup_formula(&self, _name: &str) -> AdapterResult<String> {
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
