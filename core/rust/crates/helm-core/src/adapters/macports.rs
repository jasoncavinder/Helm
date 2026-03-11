use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, PackageRuntimeState, SearchQuery,
    TaskId, TaskType,
};

const MACPORTS_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const MACPORTS_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::MacPorts,
    display_name: "MacPorts",
    category: ManagerCategory::SystemOs,
    authority: ManagerAuthority::Guarded,
    capabilities: MACPORTS_CAPABILITIES,
};

const PORT_COMMAND: &str = "port";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(180);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(1800);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacPortsDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MacPortsRequestedTarget {
    base_name: String,
    display_name: String,
    requested_version: Option<String>,
    variants: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedMacPortsInstalledTarget {
    base_name: String,
    display_name: String,
    version: String,
    variants: Vec<String>,
    runtime_state: PackageRuntimeState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedMacPortsEntry {
    base_name: String,
    display_name: String,
    version: String,
    variants: Vec<String>,
    runtime_state: PackageRuntimeState,
}

pub trait MacPortsSource: Send + Sync {
    fn detect(&self) -> AdapterResult<MacPortsDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn search(&self, query: &str) -> AdapterResult<String>;
    fn install(
        &self,
        port_name: &str,
        version: Option<&str>,
        variants: &[String],
    ) -> AdapterResult<String>;
    fn uninstall(
        &self,
        port_name: &str,
        version: Option<&str>,
        variants: &[String],
    ) -> AdapterResult<String>;
    fn upgrade(
        &self,
        port_name: Option<&str>,
        version: Option<&str>,
        variants: &[String],
    ) -> AdapterResult<String>;
}

pub struct MacPortsAdapter<S: MacPortsSource> {
    source: S,
}

impl<S: MacPortsSource> MacPortsAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    fn load_installed_packages(&self) -> AdapterResult<Vec<InstalledPackage>> {
        let raw = self.source.list_installed()?;
        parse_macports_installed(&raw)
    }

    fn resolve_installed_target(
        &self,
        raw_package_name: &str,
        explicit_version: Option<&str>,
        action: ManagerAction,
    ) -> AdapterResult<ResolvedMacPortsInstalledTarget> {
        let target = parse_macports_requested_target(raw_package_name, explicit_version, action)?;
        let installed = self.load_installed_packages()?;
        let requested_family = target.display_name.as_str();
        let requested_base = target.base_name.as_str();

        let exact_family_matches = installed
            .iter()
            .filter(|package| package.package.name == requested_family)
            .cloned()
            .collect::<Vec<_>>();
        let matches = if !exact_family_matches.is_empty() {
            exact_family_matches
        } else {
            installed
                .into_iter()
                .filter(|package| {
                    macports_base_name(package.package.name.as_str()) == requested_base
                })
                .collect::<Vec<_>>()
        };

        if matches.is_empty() {
            return Err(CoreError {
                manager: Some(ManagerId::MacPorts),
                task: Some(task_type_for_action(action)),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!("MacPorts port '{}' is not installed", target.display_name),
            });
        }

        if let Some(requested_version) = target.requested_version.as_deref() {
            let matching_versions = matches
                .iter()
                .filter(|package| package.installed_version.as_deref() == Some(requested_version))
                .cloned()
                .collect::<Vec<_>>();
            if matching_versions.len() == 1 {
                let package = matching_versions
                    .into_iter()
                    .next()
                    .expect("single match exists");
                let (base_name, variants) = parse_macports_display_name(
                    package.package.name.as_str(),
                )
                .ok_or(CoreError {
                    manager: Some(ManagerId::MacPorts),
                    task: Some(task_type_for_action(action)),
                    action: Some(action),
                    kind: CoreErrorKind::ParseFailure,
                    message: format!(
                        "failed to parse stored MacPorts package name '{}'",
                        package.package.name
                    ),
                })?;
                return Ok(ResolvedMacPortsInstalledTarget {
                    base_name,
                    display_name: package.package.name,
                    version: package.installed_version.unwrap_or_default(),
                    variants,
                    runtime_state: package.runtime_state,
                });
            }
            if matching_versions.len() > 1 {
                let families = matching_versions
                    .iter()
                    .map(|package| package.package.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(CoreError {
                    manager: Some(ManagerId::MacPorts),
                    task: Some(task_type_for_action(action)),
                    action: Some(action),
                    kind: CoreErrorKind::InvalidInput,
                    message: format!(
                        "MacPorts port '{}' has multiple installed variant selections for version '{}' ({}); specify the exact family name",
                        requested_base, requested_version, families
                    ),
                });
            }
            return Err(CoreError {
                manager: Some(ManagerId::MacPorts),
                task: Some(task_type_for_action(action)),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!(
                    "MacPorts port '{}' does not have installed version '{}'",
                    target.display_name, requested_version
                ),
            });
        }

        let active = matches
            .iter()
            .filter(|package| package.runtime_state.is_active)
            .cloned()
            .collect::<Vec<_>>();
        if active.len() == 1 {
            let package = active
                .into_iter()
                .next()
                .expect("single active match exists");
            let (base_name, variants) = parse_macports_display_name(package.package.name.as_str())
                .ok_or(CoreError {
                    manager: Some(ManagerId::MacPorts),
                    task: Some(task_type_for_action(action)),
                    action: Some(action),
                    kind: CoreErrorKind::ParseFailure,
                    message: format!(
                        "failed to parse stored MacPorts package name '{}'",
                        package.package.name
                    ),
                })?;
            return Ok(ResolvedMacPortsInstalledTarget {
                base_name,
                display_name: package.package.name,
                version: package.installed_version.unwrap_or_default(),
                variants,
                runtime_state: package.runtime_state,
            });
        }

        if matches.len() == 1 {
            let package = matches.into_iter().next().expect("single match exists");
            let (base_name, variants) = parse_macports_display_name(package.package.name.as_str())
                .ok_or(CoreError {
                    manager: Some(ManagerId::MacPorts),
                    task: Some(task_type_for_action(action)),
                    action: Some(action),
                    kind: CoreErrorKind::ParseFailure,
                    message: format!(
                        "failed to parse stored MacPorts package name '{}'",
                        package.package.name
                    ),
                })?;
            return Ok(ResolvedMacPortsInstalledTarget {
                base_name,
                display_name: package.package.name,
                version: package.installed_version.unwrap_or_default(),
                variants,
                runtime_state: package.runtime_state,
            });
        }

        let mut variants_by_family = BTreeMap::<String, Vec<String>>::new();
        for package in &matches {
            if let Some(version) = package.installed_version.as_deref() {
                variants_by_family
                    .entry(package.package.name.clone())
                    .or_default()
                    .push(version.to_string());
            }
        }
        let details = variants_by_family
            .into_iter()
            .map(|(family, mut versions)| {
                versions.sort_by(|lhs, rhs| versionish_cmp(lhs, rhs).reverse());
                format!("{} [{}]", family, versions.join(", "))
            })
            .collect::<Vec<_>>()
            .join("; ");

        Err(CoreError {
            manager: Some(ManagerId::MacPorts),
            task: Some(task_type_for_action(action)),
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "MacPorts port '{}' has multiple installed versions or variant selections ({}); specify the exact package family and version",
                requested_base, details
            ),
        })
    }

    fn load_outdated_packages(&self) -> AdapterResult<Vec<OutdatedPackage>> {
        let raw = self.source.list_outdated()?;
        let mut packages = parse_macports_outdated(&raw);
        if let Ok(installed) = self.load_installed_packages() {
            hydrate_macports_outdated_runtime_state(&mut packages, installed.as_slice());
        }
        Ok(packages)
    }
}

impl<S: MacPortsSource> ManagerAdapter for MacPortsAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &MACPORTS_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_macports_version(&output.version_output);
                let installed = version.is_some();
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: output.executable_path,
                    version,
                }))
            }
            AdapterRequest::Refresh(_) => {
                let output = self.source.detect()?;
                let version = parse_macports_version(&output.version_output);
                if version.is_none() {
                    return Ok(AdapterResponse::SnapshotSync {
                        installed: Some(Vec::new()),
                        outdated: Some(Vec::new()),
                    });
                }

                let installed = self.load_installed_packages()?;
                let outdated = self.load_outdated_packages()?;
                Ok(AdapterResponse::SnapshotSync {
                    installed: Some(installed),
                    outdated: Some(outdated),
                })
            }
            AdapterRequest::ListInstalled(_) => {
                let packages = self.load_installed_packages()?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let packages = self.load_outdated_packages()?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search(search_request.query.text.as_str())?;
                let results = parse_macports_search(&raw, &search_request.query);
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                let target = parse_macports_requested_target(
                    install_request
                        .target_name
                        .as_deref()
                        .unwrap_or(install_request.package.name.as_str()),
                    install_request.version.as_deref(),
                    ManagerAction::Install,
                )?;
                let before_version = self
                    .resolve_installed_target(
                        target.display_name.as_str(),
                        target.requested_version.as_deref(),
                        ManagerAction::Install,
                    )
                    .ok()
                    .map(|resolved| resolved.version);
                let _ = self.source.install(
                    target.base_name.as_str(),
                    target.requested_version.as_deref(),
                    target.variants.as_slice(),
                )?;
                let after_version = self
                    .resolve_installed_target(
                        target.display_name.as_str(),
                        target.requested_version.as_deref(),
                        ManagerAction::Install,
                    )
                    .ok()
                    .map(|resolved| resolved.version);
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: PackageRef {
                        manager: ManagerId::MacPorts,
                        name: target.display_name,
                    },
                    package_identifier: None,
                    action: ManagerAction::Install,
                    before_version,
                    after_version,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                let target = self.resolve_installed_target(
                    uninstall_request
                        .target_name
                        .as_deref()
                        .unwrap_or(uninstall_request.package.name.as_str()),
                    uninstall_request.version.as_deref(),
                    ManagerAction::Uninstall,
                )?;
                let _ = self.source.uninstall(
                    target.base_name.as_str(),
                    Some(target.version.as_str()),
                    target.variants.as_slice(),
                )?;
                if target.runtime_state.is_active {
                    crate::execution::record_task_log_note(
                        format!(
                            "removed active MacPorts port '{}' @{}",
                            target.display_name, target.version
                        )
                        .as_str(),
                    );
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: PackageRef {
                        manager: ManagerId::MacPorts,
                        name: target.display_name,
                    },
                    package_identifier: None,
                    action: ManagerAction::Uninstall,
                    before_version: Some(target.version),
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::MacPorts,
                    name: "__all__".to_string(),
                });
                if package.name == "__all__" {
                    let _ = self.source.upgrade(None, None, &[])?;
                    return Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                        package,
                        package_identifier: None,
                        action: ManagerAction::Upgrade,
                        before_version: None,
                        after_version: None,
                    }));
                }

                let target = self.resolve_installed_target(
                    upgrade_request
                        .target_name
                        .as_deref()
                        .unwrap_or(package.name.as_str()),
                    upgrade_request.version.as_deref(),
                    ManagerAction::Upgrade,
                )?;
                let candidate_version = self.load_outdated_packages().ok().and_then(|packages| {
                    packages
                        .into_iter()
                        .find(|candidate| {
                            candidate.package.name == target.display_name
                                && candidate.installed_version.as_deref()
                                    == Some(target.version.as_str())
                        })
                        .map(|candidate| candidate.candidate_version)
                });
                let _ = self.source.upgrade(
                    Some(target.base_name.as_str()),
                    Some(target.version.as_str()),
                    target.variants.as_slice(),
                )?;
                ensure_macports_target_no_longer_outdated(
                    self.load_outdated_packages()?.as_slice(),
                    &target,
                )?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: PackageRef {
                        manager: ManagerId::MacPorts,
                        name: target.display_name,
                    },
                    package_identifier: None,
                    action: ManagerAction::Upgrade,
                    before_version: Some(target.version),
                    after_version: candidate_version,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::MacPorts),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "macports adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn macports_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(PORT_COMMAND).arg("version"),
        DETECT_TIMEOUT,
    )
}

pub fn macports_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(PORT_COMMAND).arg("installed"),
        LIST_TIMEOUT,
    )
}

pub fn macports_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(PORT_COMMAND).arg("outdated"),
        LIST_TIMEOUT,
    )
}

pub fn macports_search_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(PORT_COMMAND).args(["search", query.text.as_str()]),
        SEARCH_TIMEOUT,
    )
}

pub fn macports_install_request(
    task_id: Option<TaskId>,
    port_name: &str,
    version: Option<&str>,
    variants: &[String],
) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        macports_command("install", Some(port_name), version, variants),
        MUTATION_TIMEOUT,
    )
    .requires_elevation(true)
}

pub fn macports_uninstall_request(
    task_id: Option<TaskId>,
    port_name: &str,
    version: Option<&str>,
    variants: &[String],
) -> ProcessSpawnRequest {
    macports_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        macports_command("uninstall", Some(port_name), version, variants),
        MUTATION_TIMEOUT,
    )
    .requires_elevation(true)
}

pub fn macports_upgrade_request(
    task_id: Option<TaskId>,
    port_name: Option<&str>,
    version: Option<&str>,
    variants: &[String],
) -> ProcessSpawnRequest {
    let command = if let Some(port_name) = port_name {
        macports_command("upgrade", Some(port_name), version, variants)
    } else {
        CommandSpec::new(PORT_COMMAND).args(["upgrade", "outdated"])
    };

    macports_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        MUTATION_TIMEOUT,
    )
    .requires_elevation(true)
}

fn macports_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::MacPorts, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn macports_command(
    action: &str,
    port_name: Option<&str>,
    version: Option<&str>,
    variants: &[String],
) -> CommandSpec {
    let mut command = CommandSpec::new(PORT_COMMAND).arg(action);
    if let Some(port_name) = port_name {
        command = command.arg(port_name);
    }
    if let Some(version) = version {
        command = command.arg(format!("@{version}"));
    }
    for variant in variants {
        command = command.arg(format!("+{variant}"));
    }
    command
}

fn parse_macports_version(output: &str) -> Option<String> {
    output.lines().map(str::trim).find_map(|line| {
        line.strip_prefix("Version:")
            .map(str::trim)
            .or_else(|| {
                line.split_whitespace()
                    .find(|part| part.starts_with(|ch: char| ch.is_ascii_digit()))
            })
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn ensure_macports_target_no_longer_outdated(
    outdated: &[OutdatedPackage],
    target: &ResolvedMacPortsInstalledTarget,
) -> AdapterResult<()> {
    if outdated.iter().any(|candidate| {
        candidate.package.name == target.display_name
            && candidate.installed_version.as_deref() == Some(target.version.as_str())
    }) {
        return Err(CoreError {
            manager: Some(ManagerId::MacPorts),
            task: Some(TaskType::Upgrade),
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::ProcessFailure,
            message: format!(
                "MacPorts upgrade reported success but '{} @{}' remains outdated",
                target.display_name, target.version
            ),
        });
    }

    Ok(())
}

fn parse_macports_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut installed = Vec::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty()
            || line.starts_with("The following")
            || line.starts_with("No ports are installed")
            || line.starts_with("None of the specified ports")
        {
            continue;
        }

        let Some(entry) = parse_macports_installed_entry(line) else {
            continue;
        };
        installed.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::MacPorts,
                name: entry.display_name,
            },
            package_identifier: None,
            installed_version: Some(entry.version),
            pinned: false,
            runtime_state: entry.runtime_state,
        });
    }

    installed.sort_by(compare_macports_installed_packages);
    Ok(installed)
}

fn parse_macports_outdated(output: &str) -> Vec<OutdatedPackage> {
    let mut outdated = Vec::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty()
            || line.starts_with("The following")
            || line.starts_with("No installed ports are outdated")
        {
            continue;
        }

        let Some((left, right)) = line.split_once('<') else {
            continue;
        };

        let Some(installed_entry) = parse_macports_entry(left.trim()) else {
            continue;
        };
        let Some(candidate) = parse_macports_version_and_variants(right.trim()) else {
            continue;
        };

        outdated.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::MacPorts,
                name: installed_entry.display_name,
            },
            package_identifier: None,
            installed_version: Some(installed_entry.version),
            candidate_version: candidate.0,
            pinned: false,
            restart_required: false,
            runtime_state: installed_entry.runtime_state,
        });
    }

    outdated.sort_by(compare_macports_outdated_packages);
    outdated
}

fn hydrate_macports_outdated_runtime_state(
    outdated: &mut [OutdatedPackage],
    installed: &[InstalledPackage],
) {
    let mut runtime_state_by_key = HashMap::<(String, String), PackageRuntimeState>::new();
    for package in installed {
        let Some(installed_version) = package.installed_version.as_deref() else {
            continue;
        };
        runtime_state_by_key.insert(
            (package.package.name.clone(), installed_version.to_string()),
            package.runtime_state.clone(),
        );
    }

    for package in outdated {
        let Some(installed_version) = package.installed_version.as_deref() else {
            continue;
        };
        if let Some(runtime_state) =
            runtime_state_by_key.get(&(package.package.name.clone(), installed_version.to_string()))
        {
            package.runtime_state = runtime_state.clone();
        }
    }
}

fn parse_macports_search(output: &str, query: &SearchQuery) -> Vec<CachedSearchResult> {
    let mut results: Vec<CachedSearchResult> = Vec::new();
    let mut pending_summary: Option<usize> = None;

    for raw_line in output.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Found ") || trimmed.starts_with("No match") {
            pending_summary = None;
            continue;
        }

        if raw_line.starts_with(' ') || raw_line.starts_with('\t') {
            if let Some(index) = pending_summary {
                let summary = trimmed.trim_matches('-').trim();
                if !summary.is_empty() {
                    results[index].result.summary = Some(summary.to_string());
                }
            }
            continue;
        }

        let Some(entry) = parse_macports_entry(trimmed) else {
            pending_summary = None;
            continue;
        };

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::MacPorts,
                    name: entry.display_name,
                },
                package_identifier: None,
                version: Some(entry.version),
                summary: None,
            },
            source_manager: ManagerId::MacPorts,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
        pending_summary = Some(results.len() - 1);
    }

    results
}

fn parse_macports_installed_entry(line: &str) -> Option<ParsedMacPortsEntry> {
    let mut entry = parse_macports_entry(line)?;
    if line.contains("(active)") {
        entry.runtime_state.is_active = true;
    }
    Some(entry)
}

fn parse_macports_entry(line: &str) -> Option<ParsedMacPortsEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (base_name, version, variants) = if let Some((name, rest)) = trimmed.split_once('@') {
        let (version, variants) = parse_macports_version_and_variants(rest.trim())?;
        (name.trim().to_string(), version, variants)
    } else {
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.len() < 2 {
            return None;
        }
        let (version, variants) = tokens
            .iter()
            .skip(1)
            .find_map(|token| parse_macports_version_and_variants(token))?;
        (tokens[0].trim().to_string(), version, variants)
    };

    let display_name = macports_display_name(base_name.as_str(), variants.as_slice());
    Some(ParsedMacPortsEntry {
        base_name,
        display_name,
        version,
        variants,
        runtime_state: PackageRuntimeState::default(),
    })
}

fn parse_macports_requested_target(
    raw_package_name: &str,
    explicit_version: Option<&str>,
    action: ManagerAction,
) -> AdapterResult<MacPortsRequestedTarget> {
    crate::adapters::validate_package_identifier(ManagerId::MacPorts, action, raw_package_name)?;

    let trimmed = raw_package_name.trim();
    if trimmed.is_empty() {
        return Err(CoreError {
            manager: Some(ManagerId::MacPorts),
            task: Some(task_type_for_action(action)),
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: "MacPorts package target cannot be empty".to_string(),
        });
    }

    if let Some((name_with_variants, rest)) = trimmed.split_once('@') {
        let (embedded_version, mut embedded_variants) = parse_macports_version_and_variants(rest)
            .ok_or(CoreError {
            manager: Some(ManagerId::MacPorts),
            task: Some(task_type_for_action(action)),
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "MacPorts package target '{}' has an invalid @version+variant selector",
                raw_package_name
            ),
        })?;
        if let Some(explicit_version) = explicit_version
            && explicit_version != embedded_version
        {
            return Err(CoreError {
                manager: Some(ManagerId::MacPorts),
                task: Some(task_type_for_action(action)),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!(
                    "MacPorts package target '{}' embeds version '{}' but request also specified version '{}'",
                    raw_package_name, embedded_version, explicit_version
                ),
            });
        }
        let (base_name, mut name_variants) = parse_macports_display_name(name_with_variants)
            .ok_or(CoreError {
                manager: Some(ManagerId::MacPorts),
                task: Some(task_type_for_action(action)),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!(
                    "MacPorts package target '{}' has an invalid package family selector",
                    raw_package_name
                ),
            })?;
        name_variants.append(&mut embedded_variants);
        normalize_macports_variants(&mut name_variants);
        let display_name = macports_display_name(base_name.as_str(), name_variants.as_slice());
        return Ok(MacPortsRequestedTarget {
            base_name,
            display_name,
            requested_version: Some(embedded_version),
            variants: name_variants,
        });
    }

    let (base_name, variants) = parse_macports_display_name(trimmed).ok_or(CoreError {
        manager: Some(ManagerId::MacPorts),
        task: Some(task_type_for_action(action)),
        action: Some(action),
        kind: CoreErrorKind::InvalidInput,
        message: format!(
            "MacPorts package target '{}' has an invalid variant selector",
            raw_package_name
        ),
    })?;
    let display_name = macports_display_name(base_name.as_str(), variants.as_slice());
    Ok(MacPortsRequestedTarget {
        base_name,
        display_name,
        requested_version: explicit_version.map(str::to_string),
        variants,
    })
}

fn parse_macports_display_name(raw: &str) -> Option<(String, Vec<String>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut segments = trimmed.split('+');
    let base_name = segments.next()?.trim();
    if base_name.is_empty() {
        return None;
    }
    let mut variants = segments
        .filter_map(|segment| {
            let value = segment.trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        })
        .collect::<Vec<_>>();
    normalize_macports_variants(&mut variants);
    Some((base_name.to_string(), variants))
}

fn parse_macports_version_and_variants(raw: &str) -> Option<(String, Vec<String>)> {
    let token = raw
        .split_whitespace()
        .next()
        .unwrap_or(raw)
        .trim()
        .trim_start_matches('@')
        .trim_end_matches(',');
    if token.is_empty() || !token.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }

    let mut parts = token.split('+');
    let version = parts.next()?.trim().to_string();
    if version.is_empty() {
        return None;
    }
    let mut variants = parts
        .filter_map(|segment| {
            let value = segment.trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        })
        .collect::<Vec<_>>();
    normalize_macports_variants(&mut variants);
    Some((version, variants))
}

fn normalize_macports_variants(variants: &mut Vec<String>) {
    variants.sort();
    variants.dedup();
}

fn macports_display_name(base_name: &str, variants: &[String]) -> String {
    if variants.is_empty() {
        return base_name.to_string();
    }
    format!("{}+{}", base_name, variants.join("+"))
}

fn macports_base_name(display_name: &str) -> &str {
    display_name
        .split_once('+')
        .map_or(display_name, |(base, _)| base)
}

fn compare_macports_installed_packages(lhs: &InstalledPackage, rhs: &InstalledPackage) -> Ordering {
    let name_order = lhs.package.name.cmp(&rhs.package.name);
    if name_order != Ordering::Equal {
        return name_order;
    }
    if lhs.runtime_state.is_active != rhs.runtime_state.is_active {
        return rhs
            .runtime_state
            .is_active
            .cmp(&lhs.runtime_state.is_active);
    }
    versionish_cmp(
        lhs.installed_version.as_deref().unwrap_or(""),
        rhs.installed_version.as_deref().unwrap_or(""),
    )
    .reverse()
}

fn compare_macports_outdated_packages(lhs: &OutdatedPackage, rhs: &OutdatedPackage) -> Ordering {
    let name_order = lhs.package.name.cmp(&rhs.package.name);
    if name_order != Ordering::Equal {
        return name_order;
    }
    if lhs.runtime_state.is_active != rhs.runtime_state.is_active {
        return rhs
            .runtime_state
            .is_active
            .cmp(&lhs.runtime_state.is_active);
    }
    versionish_cmp(
        lhs.installed_version.as_deref().unwrap_or(""),
        rhs.installed_version.as_deref().unwrap_or(""),
    )
    .reverse()
}

fn versionish_cmp(lhs: &str, rhs: &str) -> Ordering {
    let lhs_segments = versionish_segments(lhs);
    let rhs_segments = versionish_segments(rhs);
    for (lhs, rhs) in lhs_segments.iter().zip(rhs_segments.iter()) {
        let ordering = match (lhs.parse::<u64>(), rhs.parse::<u64>()) {
            (Ok(lhs_number), Ok(rhs_number)) => lhs_number.cmp(&rhs_number),
            _ => lhs.cmp(rhs),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    lhs_segments.len().cmp(&rhs_segments.len())
}

fn versionish_segments(version: &str) -> Vec<&str> {
    version
        .split(['.', '_', '-', '+'])
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn task_type_for_action(action: ManagerAction) -> TaskType {
    match action {
        ManagerAction::Detect => TaskType::Detection,
        ManagerAction::Refresh | ManagerAction::ListInstalled | ManagerAction::ListOutdated => {
            TaskType::Refresh
        }
        ManagerAction::Search => TaskType::Search,
        ManagerAction::Install => TaskType::Install,
        ManagerAction::Uninstall => TaskType::Uninstall,
        ManagerAction::Upgrade => TaskType::Upgrade,
        ManagerAction::Configure => TaskType::Configure,
        ManagerAction::Pin => TaskType::Pin,
        ManagerAction::Unpin => TaskType::Unpin,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::UNIX_EPOCH;

    use crate::adapters::macports::{
        MacPortsAdapter, MacPortsDetectOutput, MacPortsSource, macports_detect_request,
        macports_install_request, macports_list_installed_request, macports_list_outdated_request,
        macports_search_request, macports_uninstall_request, macports_upgrade_request,
        parse_macports_installed, parse_macports_outdated, parse_macports_requested_target,
        parse_macports_search, parse_macports_version,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, InstallRequest,
        ListInstalledRequest, ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest,
        UpgradeRequest,
    };
    use crate::models::{ManagerAction, ManagerId, PackageRef, SearchQuery, TaskType};

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/macports/version.txt");
    const INSTALLED_FIXTURE: &str = include_str!("../../tests/fixtures/macports/installed.txt");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/macports/outdated.txt");
    const SEARCH_FIXTURE: &str = include_str!("../../tests/fixtures/macports/search.txt");
    const INSTALLED_VARIANTS_FIXTURE: &str = "The following ports are currently installed:\n  git @2.49.0_0+credential_osxkeychain (active)\n  git @2.48.1_0+credential_osxkeychain\n  python312 @3.12.9_0+optimizations (active)\n";
    const OUTDATED_VARIANTS_FIXTURE: &str = "The following installed ports are outdated:\n  git @2.49.0_0+credential_osxkeychain < 2.50.0_0+credential_osxkeychain\n  python312 @3.12.9_0+optimizations < 3.12.10_0+optimizations\n";

    #[test]
    fn parses_macports_version() {
        assert_eq!(
            parse_macports_version(VERSION_FIXTURE).as_deref(),
            Some("2.8.1")
        );
    }

    #[test]
    fn parses_macports_installed_fixture() {
        let installed = parse_macports_installed(INSTALLED_FIXTURE).unwrap();
        assert_eq!(installed.len(), 2);
        assert_eq!(installed[0].package.name, "git");
        assert_eq!(installed[0].installed_version.as_deref(), Some("2.48.1_0"));
        assert!(installed[0].runtime_state.is_active);
    }

    #[test]
    fn parses_macports_installed_variants_without_collapsing_versions() {
        let installed = parse_macports_installed(INSTALLED_VARIANTS_FIXTURE).unwrap();
        assert_eq!(installed.len(), 3);
        assert_eq!(installed[0].package.name, "git+credential_osxkeychain");
        assert_eq!(installed[0].installed_version.as_deref(), Some("2.49.0_0"));
        assert!(installed[0].runtime_state.is_active);
        assert_eq!(installed[1].package.name, "git+credential_osxkeychain");
        assert_eq!(installed[1].installed_version.as_deref(), Some("2.48.1_0"));
        assert!(!installed[1].runtime_state.is_active);
        assert_eq!(installed[2].package.name, "python312+optimizations");
    }

    #[test]
    fn parses_macports_outdated_fixture() {
        let outdated = parse_macports_outdated(OUTDATED_FIXTURE);
        assert_eq!(outdated.len(), 2);
        assert_eq!(outdated[0].package.name, "git");
        assert_eq!(outdated[0].candidate_version, "2.49.0_0");
    }

    #[test]
    fn parses_macports_outdated_with_variants() {
        let outdated = parse_macports_outdated(OUTDATED_VARIANTS_FIXTURE);
        assert_eq!(outdated.len(), 2);
        assert_eq!(outdated[0].package.name, "git+credential_osxkeychain");
        assert_eq!(outdated[0].installed_version.as_deref(), Some("2.49.0_0"));
        assert_eq!(outdated[0].candidate_version, "2.50.0_0");
    }

    #[test]
    fn parses_macports_search_fixture() {
        let query = SearchQuery {
            text: "ripgrep".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let results = parse_macports_search(SEARCH_FIXTURE, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "ripgrep");
        assert_eq!(results[0].result.version.as_deref(), Some("14.1.1_0"));
    }

    #[test]
    fn parses_macports_requested_target_with_embedded_version_and_variants() {
        let target = parse_macports_requested_target(
            "git+credential_osxkeychain@2.49.0_0",
            None,
            ManagerAction::Uninstall,
        )
        .unwrap();
        assert_eq!(target.base_name, "git");
        assert_eq!(target.display_name, "git+credential_osxkeychain");
        assert_eq!(target.requested_version.as_deref(), Some("2.49.0_0"));
        assert_eq!(target.variants, vec!["credential_osxkeychain".to_string()]);
    }

    #[test]
    fn request_shapes_match_expected_commands() {
        let detect = macports_detect_request(None);
        assert_eq!(detect.manager, ManagerId::MacPorts);
        assert_eq!(detect.task_type, TaskType::Detection);
        assert_eq!(detect.command.args, vec!["version"]);

        let list_installed = macports_list_installed_request(None);
        assert_eq!(list_installed.task_type, TaskType::Refresh);
        assert_eq!(list_installed.command.args, vec!["installed"]);

        let list_outdated = macports_list_outdated_request(None);
        assert_eq!(list_outdated.action, ManagerAction::ListOutdated);
        assert_eq!(list_outdated.command.args, vec!["outdated"]);

        let query = SearchQuery {
            text: "git".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let search = macports_search_request(None, &query);
        assert_eq!(search.task_type, TaskType::Search);
        assert_eq!(search.command.args, vec!["search", "git"]);

        let install = macports_install_request(
            None,
            "git",
            Some("2.49.0_0"),
            &["credential_osxkeychain".to_string()],
        );
        assert!(install.requires_elevation);
        assert_eq!(
            install.command.args,
            vec!["install", "git", "@2.49.0_0", "+credential_osxkeychain"]
        );

        let uninstall = macports_uninstall_request(
            None,
            "git",
            Some("2.49.0_0"),
            &["credential_osxkeychain".to_string()],
        );
        assert!(uninstall.requires_elevation);
        assert_eq!(
            uninstall.command.args,
            vec!["uninstall", "git", "@2.49.0_0", "+credential_osxkeychain"]
        );

        let upgrade_one = macports_upgrade_request(
            None,
            Some("git"),
            Some("2.49.0_0"),
            &["credential_osxkeychain".to_string()],
        );
        assert!(upgrade_one.requires_elevation);
        assert_eq!(
            upgrade_one.command.args,
            vec!["upgrade", "git", "@2.49.0_0", "+credential_osxkeychain"]
        );

        let upgrade_all = macports_upgrade_request(None, None, None, &[]);
        assert_eq!(upgrade_all.command.args, vec!["upgrade", "outdated"]);
    }

    #[test]
    fn adapter_detect_and_listing_paths_work() {
        let source = FixtureSource {
            detect_result: Ok(MacPortsDetectOutput {
                executable_path: Some(PathBuf::from("/opt/local/bin/port")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok(INSTALLED_VARIANTS_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_VARIANTS_FIXTURE.to_string()),
            list_outdated_sequence: Arc::new(std::sync::Mutex::new(Vec::new())),
            search_result: Ok(SEARCH_FIXTURE.to_string()),
            install_result: Ok(String::new()),
            uninstall_result: Ok(String::new()),
            upgrade_result: Ok(String::new()),
        };
        let adapter = MacPortsAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let AdapterResponse::Detection(info) = detect else {
            panic!("expected detection response");
        };
        assert!(info.installed);
        assert_eq!(info.version.as_deref(), Some("2.8.1"));

        let installed = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();
        let AdapterResponse::InstalledPackages(packages) = installed else {
            panic!("expected installed response");
        };
        assert_eq!(packages.len(), 3);
        assert!(packages[0].runtime_state.is_active);

        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();
        let AdapterResponse::OutdatedPackages(packages) = outdated else {
            panic!("expected outdated response");
        };
        assert_eq!(packages.len(), 2);
        assert!(packages[0].runtime_state.is_active);

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
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn adapter_uninstall_rejects_ambiguous_variant_family_without_version() {
        let source = FixtureSource {
            detect_result: Ok(MacPortsDetectOutput {
                executable_path: Some(PathBuf::from("/opt/local/bin/port")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok("The following ports are currently installed:\n  git @2.49.0_0+credential_osxkeychain\n  git @2.49.0_0+diff_highlight\n".to_string()),
            list_outdated_result: Ok(String::new()),
            list_outdated_sequence: Arc::new(std::sync::Mutex::new(Vec::new())),
            search_result: Ok(SEARCH_FIXTURE.to_string()),
            install_result: Ok(String::new()),
            uninstall_result: Ok(String::new()),
            upgrade_result: Ok(String::new()),
        };
        let adapter = MacPortsAdapter::new(source);
        let error = adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::MacPorts,
                    name: "git".to_string(),
                },
                target_name: None,
                version: Some("2.49.0_0".to_string()),
            }))
            .expect_err("ambiguous uninstall should fail");
        assert!(
            error
                .message
                .contains("multiple installed variant selections")
        );
    }

    #[test]
    fn adapter_install_uninstall_and_upgrade_use_exact_targets() {
        let source = FixtureSource {
            detect_result: Ok(MacPortsDetectOutput {
                executable_path: Some(PathBuf::from("/opt/local/bin/port")),
                version_output: VERSION_FIXTURE.to_string(),
            }),
            list_installed_result: Ok(INSTALLED_VARIANTS_FIXTURE.to_string()),
            list_outdated_result: Ok(OUTDATED_VARIANTS_FIXTURE.to_string()),
            list_outdated_sequence: Arc::new(std::sync::Mutex::new(vec![
                Ok(String::new()),
                Ok(OUTDATED_VARIANTS_FIXTURE.to_string()),
            ])),
            search_result: Ok(SEARCH_FIXTURE.to_string()),
            install_result: Ok(String::new()),
            uninstall_result: Ok(String::new()),
            upgrade_result: Ok(String::new()),
        };
        let adapter = MacPortsAdapter::new(source);

        let install = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::MacPorts,
                    name: "git+credential_osxkeychain".to_string(),
                },
                target_name: Some("git+credential_osxkeychain".to_string()),
                version: Some("2.49.0_0".to_string()),
            }))
            .unwrap();
        let AdapterResponse::Mutation(install) = install else {
            panic!("expected install mutation");
        };
        assert_eq!(install.package.name, "git+credential_osxkeychain");
        assert_eq!(install.after_version.as_deref(), Some("2.49.0_0"));

        let uninstall = adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::MacPorts,
                    name: "git+credential_osxkeychain".to_string(),
                },
                target_name: Some("git+credential_osxkeychain".to_string()),
                version: Some("2.49.0_0".to_string()),
            }))
            .unwrap();
        let AdapterResponse::Mutation(uninstall) = uninstall else {
            panic!("expected uninstall mutation");
        };
        assert_eq!(uninstall.package.name, "git+credential_osxkeychain");
        assert_eq!(uninstall.before_version.as_deref(), Some("2.49.0_0"));

        let upgrade = adapter
            .execute(AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::MacPorts,
                    name: "git+credential_osxkeychain".to_string(),
                }),
                target_name: Some("git+credential_osxkeychain".to_string()),
                version: Some("2.49.0_0".to_string()),
            }))
            .unwrap();
        let AdapterResponse::Mutation(upgrade) = upgrade else {
            panic!("expected upgrade mutation");
        };
        assert_eq!(upgrade.before_version.as_deref(), Some("2.49.0_0"));
        assert_eq!(upgrade.after_version.as_deref(), Some("2.50.0_0"));
    }

    struct FixtureSource {
        detect_result: AdapterResult<MacPortsDetectOutput>,
        list_installed_result: AdapterResult<String>,
        list_outdated_result: AdapterResult<String>,
        list_outdated_sequence: Arc<std::sync::Mutex<Vec<AdapterResult<String>>>>,
        search_result: AdapterResult<String>,
        install_result: AdapterResult<String>,
        uninstall_result: AdapterResult<String>,
        upgrade_result: AdapterResult<String>,
    }

    impl MacPortsSource for FixtureSource {
        fn detect(&self) -> AdapterResult<MacPortsDetectOutput> {
            self.detect_result.clone()
        }

        fn list_installed(&self) -> AdapterResult<String> {
            self.list_installed_result.clone()
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            if let Some(next) = self.list_outdated_sequence.lock().unwrap().pop() {
                return next;
            }
            self.list_outdated_result.clone()
        }

        fn search(&self, _query: &str) -> AdapterResult<String> {
            self.search_result.clone()
        }

        fn install(
            &self,
            _port_name: &str,
            _version: Option<&str>,
            _variants: &[String],
        ) -> AdapterResult<String> {
            self.install_result.clone()
        }

        fn uninstall(
            &self,
            _port_name: &str,
            _version: Option<&str>,
            _variants: &[String],
        ) -> AdapterResult<String> {
            self.uninstall_result.clone()
        }

        fn upgrade(
            &self,
            _port_name: Option<&str>,
            _version: Option<&str>,
            _variants: &[String],
        ) -> AdapterResult<String> {
            self.upgrade_result.clone()
        }
    }
}
