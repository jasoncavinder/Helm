use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, PackageRuntimeState, SearchQuery,
    TaskId, TaskType,
};
use crate::versioning::PackageCoordinate;

const ASDF_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const ASDF_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Asdf,
    display_name: "asdf",
    category: ManagerCategory::ToolRuntime,
    authority: ManagerAuthority::Authoritative,
    capabilities: ASDF_CAPABILITIES,
};

const ASDF_COMMAND: &str = "asdf";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(180);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(180);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const INSTALL_IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const MANAGER_UPDATE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

fn task_type_for_action(action: ManagerAction) -> TaskType {
    match action {
        ManagerAction::Install => TaskType::Install,
        ManagerAction::Uninstall => TaskType::Uninstall,
        ManagerAction::Upgrade => TaskType::Upgrade,
        _ => TaskType::Refresh,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsdfDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AsdfInstallSource {
    OfficialDownload,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AsdfCurrentSelection {
    plugin: String,
    version: String,
    runtime_state: PackageRuntimeState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AsdfMutationTarget {
    plugin: String,
    requested_version: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedAsdfInstalledTarget {
    plugin: String,
    version: String,
    runtime_state: PackageRuntimeState,
}

pub trait AsdfSource: Send + Sync {
    fn detect(&self) -> AdapterResult<AsdfDetectOutput>;
    fn list_current(&self) -> AdapterResult<String>;
    fn list_plugins(&self) -> AdapterResult<String>;
    fn list_installed_versions(&self, plugin: &str) -> AdapterResult<String>;
    fn search_plugins(&self, query: &SearchQuery) -> AdapterResult<String>;
    fn latest_version(&self, plugin: &str) -> AdapterResult<String>;
    fn add_plugin(&self, plugin: &str) -> AdapterResult<String>;
    fn install_plugin(&self, plugin: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall_plugin(&self, plugin: &str, version: &str) -> AdapterResult<String>;
    fn set_home_version(&self, plugin: &str, version: &str) -> AdapterResult<String>;
    fn install_self(&self, source: AsdfInstallSource) -> AdapterResult<String>;
    fn self_uninstall(&self) -> AdapterResult<String>;
    fn self_update(&self) -> AdapterResult<String>;
}

pub struct AsdfAdapter<S: AsdfSource> {
    source: S,
}

impl<S: AsdfSource> AsdfAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    fn load_installed_packages(&self) -> AdapterResult<Vec<InstalledPackage>> {
        let current_raw = self.source.list_current()?;
        let current = parse_asdf_current(&current_raw, current_home_dir().as_deref());
        let current_by_plugin = current_selection_map(&current);
        let plugins_raw = self.source.list_plugins()?;
        let plugins = parse_asdf_plugins(&plugins_raw);
        let mut packages = Vec::new();

        for plugin in plugins {
            let versions_raw = self.source.list_installed_versions(plugin.as_str())?;
            let versions = parse_asdf_installed_versions(&versions_raw);
            for version in versions {
                let runtime_state = current_by_plugin
                    .get(&plugin)
                    .and_then(|versions| versions.get(&version))
                    .cloned()
                    .unwrap_or_default();
                packages.push(InstalledPackage {
                    package: PackageRef {
                        manager: ManagerId::Asdf,
                        name: plugin.clone(),
                    },
                    installed_version: Some(version),
                    pinned: false,
                    runtime_state,
                });
            }
        }

        packages.sort_by(|lhs, rhs| {
            let name_order = lhs.package.name.cmp(&rhs.package.name);
            if name_order != Ordering::Equal {
                return name_order;
            }
            let lhs_version = lhs.installed_version.as_deref().unwrap_or("");
            let rhs_version = rhs.installed_version.as_deref().unwrap_or("");
            versionish_cmp(lhs_version, rhs_version)
        });
        Ok(packages)
    }

    fn resolve_installed_target(
        &self,
        raw_package_name: &str,
        action: ManagerAction,
    ) -> AdapterResult<ResolvedAsdfInstalledTarget> {
        let target = parse_asdf_mutation_target(raw_package_name, action, None)?;
        let installed = self.load_installed_packages()?;
        let mut matches = installed
            .into_iter()
            .filter(|package| package.package.name == target.plugin)
            .collect::<Vec<_>>();

        if matches.is_empty() {
            return Err(CoreError {
                manager: Some(ManagerId::Asdf),
                task: Some(task_type_for_action(action)),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!("asdf tool '{}' is not installed", target.plugin),
            });
        }

        if let Some(requested_version) = target.requested_version {
            let package = matches
                .into_iter()
                .find(|package| {
                    package.installed_version.as_deref() == Some(requested_version.as_str())
                })
                .ok_or(CoreError {
                    manager: Some(ManagerId::Asdf),
                    task: Some(task_type_for_action(action)),
                    action: Some(action),
                    kind: CoreErrorKind::InvalidInput,
                    message: format!(
                        "asdf tool '{}' does not have installed version '{}'",
                        target.plugin, requested_version
                    ),
                })?;
            return Ok(ResolvedAsdfInstalledTarget {
                plugin: package.package.name,
                version: package.installed_version.unwrap_or_default(),
                runtime_state: package.runtime_state,
            });
        }

        let active = matches
            .iter()
            .filter(|package| package.runtime_state.is_active)
            .collect::<Vec<_>>();
        if active.len() == 1 {
            let package = active[0];
            return Ok(ResolvedAsdfInstalledTarget {
                plugin: package.package.name.clone(),
                version: package.installed_version.clone().unwrap_or_default(),
                runtime_state: package.runtime_state.clone(),
            });
        }
        if active.len() > 1 {
            let versions = active
                .iter()
                .filter_map(|package| package.installed_version.as_deref())
                .map(str::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(CoreError {
                manager: Some(ManagerId::Asdf),
                task: Some(task_type_for_action(action)),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!(
                    "asdf tool '{}' has multiple active versions ({}); specify '{}@<version>'",
                    target.plugin, versions, target.plugin
                ),
            });
        }
        if matches.len() == 1 {
            let package = matches.pop().expect("single match must exist");
            return Ok(ResolvedAsdfInstalledTarget {
                plugin: package.package.name,
                version: package.installed_version.unwrap_or_default(),
                runtime_state: package.runtime_state,
            });
        }

        matches.sort_by(|lhs, rhs| {
            versionish_cmp(
                lhs.installed_version.as_deref().unwrap_or(""),
                rhs.installed_version.as_deref().unwrap_or(""),
            )
            .reverse()
        });
        let versions = matches
            .iter()
            .filter_map(|package| package.installed_version.as_deref())
            .map(str::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        Err(CoreError {
            manager: Some(ManagerId::Asdf),
            task: Some(task_type_for_action(action)),
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "asdf tool '{}' has multiple installed versions ({}); specify '{}@<version>'",
                target.plugin, versions, target.plugin
            ),
        })
    }

    fn upgrade_single_plugin(
        &self,
        raw_package_name: &str,
    ) -> AdapterResult<crate::adapters::MutationResult> {
        let target = self.resolve_installed_target(raw_package_name, ManagerAction::Upgrade)?;
        let latest_raw = self.source.latest_version(target.plugin.as_str())?;
        let latest_version = parse_asdf_latest_version(&latest_raw).ok_or(CoreError {
            manager: Some(ManagerId::Asdf),
            task: Some(TaskType::Upgrade),
            action: Some(ManagerAction::Upgrade),
            kind: CoreErrorKind::ParseFailure,
            message: format!(
                "failed to parse latest asdf version for '{}' from '{}': unsupported output",
                target.plugin,
                latest_raw.trim()
            ),
        })?;

        if latest_version != target.version {
            let _ = self
                .source
                .install_plugin(target.plugin.as_str(), Some(latest_version.as_str()))?;
            if target.runtime_state.is_active
                && target.runtime_state.is_default
                && !target.runtime_state.has_override
            {
                let _ = self
                    .source
                    .set_home_version(target.plugin.as_str(), latest_version.as_str())?;
            } else if target.runtime_state.is_active && target.runtime_state.has_override {
                let message = format!(
                    "installed latest asdf version '{}' for '{}' but preserved local/environment override selection on '{}'",
                    latest_version, target.plugin, target.version
                );
                crate::execution::record_task_log_note(message.as_str());
            } else {
                let message = format!(
                    "installed latest asdf version '{}' for '{}' without changing active selection",
                    latest_version, target.plugin
                );
                crate::execution::record_task_log_note(message.as_str());
            }
        }

        Ok(crate::adapters::MutationResult {
            package: PackageRef {
                manager: ManagerId::Asdf,
                name: target.plugin,
            },
            action: ManagerAction::Upgrade,
            before_version: Some(target.version),
            after_version: Some(latest_version),
        })
    }

    fn upgrade_all_plugins(&self) -> AdapterResult<crate::adapters::MutationResult> {
        let installed = self.load_installed_packages()?;
        let mut by_plugin = HashMap::<String, Vec<InstalledPackage>>::new();
        for package in installed {
            by_plugin
                .entry(package.package.name.clone())
                .or_default()
                .push(package);
        }

        for (plugin, packages) in by_plugin {
            let Some(representative) = representative_installed_package(&packages) else {
                continue;
            };
            let Some(installed_version) = representative.installed_version.as_deref() else {
                continue;
            };
            let latest_raw = match self.source.latest_version(plugin.as_str()) {
                Ok(output) => output,
                Err(error) => {
                    let message = format!(
                        "skipped asdf latest-version probe for '{}' during upgrade-all: {}",
                        plugin, error.message
                    );
                    crate::execution::record_task_log_note(message.as_str());
                    continue;
                }
            };
            let Some(latest_version) = parse_asdf_latest_version(&latest_raw) else {
                let message = format!(
                    "skipped asdf latest-version probe for '{}' during upgrade-all: unsupported output '{}'",
                    plugin,
                    latest_raw.trim()
                );
                crate::execution::record_task_log_note(message.as_str());
                continue;
            };
            if latest_version == installed_version {
                continue;
            }

            let _ = self
                .source
                .install_plugin(plugin.as_str(), Some(latest_version.as_str()))?;
            if representative.runtime_state.is_active
                && representative.runtime_state.is_default
                && !representative.runtime_state.has_override
            {
                let _ = self
                    .source
                    .set_home_version(plugin.as_str(), latest_version.as_str())?;
            }
        }

        Ok(crate::adapters::MutationResult {
            package: PackageRef {
                manager: ManagerId::Asdf,
                name: "__all__".to_string(),
            },
            action: ManagerAction::Upgrade,
            before_version: None,
            after_version: None,
        })
    }
}

impl<S: AsdfSource> ManagerAdapter for AsdfAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &ASDF_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_asdf_version(&output.version_output);
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
                let _ = self.source.list_plugins()?;
                Ok(AdapterResponse::Refreshed)
            }
            AdapterRequest::ListInstalled(_) => {
                let packages = self.load_installed_packages()?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let installed = self.load_installed_packages()?;
                let mut by_plugin = HashMap::<String, Vec<InstalledPackage>>::new();
                for package in installed {
                    by_plugin
                        .entry(package.package.name.clone())
                        .or_default()
                        .push(package);
                }

                let mut outdated = Vec::new();
                for (plugin, packages) in by_plugin {
                    let Some(representative) = representative_installed_package(&packages) else {
                        continue;
                    };
                    let Some(installed_version) = representative.installed_version.clone() else {
                        continue;
                    };
                    let latest_raw = match self.source.latest_version(plugin.as_str()) {
                        Ok(output) => output,
                        Err(error) => {
                            let message = format!(
                                "skipped asdf latest-version probe for '{}': {}",
                                plugin, error.message
                            );
                            crate::execution::record_task_log_note(message.as_str());
                            continue;
                        }
                    };
                    let Some(latest_version) = parse_asdf_latest_version(&latest_raw) else {
                        let message = format!(
                            "skipped asdf latest-version probe for '{}': unsupported output '{}'",
                            plugin,
                            latest_raw.trim()
                        );
                        crate::execution::record_task_log_note(message.as_str());
                        continue;
                    };
                    if latest_version == installed_version {
                        continue;
                    }

                    outdated.push(OutdatedPackage {
                        package: PackageRef {
                            manager: ManagerId::Asdf,
                            name: plugin,
                        },
                        installed_version: Some(installed_version),
                        candidate_version: latest_version,
                        pinned: false,
                        restart_required: false,
                        runtime_state: representative.runtime_state.clone(),
                    });
                }

                outdated.sort_by(|lhs, rhs| lhs.package.name.cmp(&rhs.package.name));
                Ok(AdapterResponse::OutdatedPackages(outdated))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.search_plugins(&search_request.query)?;
                let results = parse_asdf_search(&raw, &search_request.query);
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                if install_request.package.name == "__self__" {
                    let install_source = parse_install_source(install_request.version.as_deref())?;
                    let _ = self.source.install_self(install_source)?;
                    return Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                        package: install_request.package,
                        action: ManagerAction::Install,
                        before_version: None,
                        after_version: None,
                    }));
                }

                let target = parse_asdf_mutation_target(
                    install_request.package.name.as_str(),
                    ManagerAction::Install,
                    install_request.version.as_deref(),
                )?;
                let plugins = parse_asdf_plugins(&self.source.list_plugins()?);
                if !plugins.iter().any(|plugin| plugin == &target.plugin) {
                    let _ = self.source.add_plugin(target.plugin.as_str())?;
                    let message = format!("bootstrapped missing asdf plugin '{}'", target.plugin);
                    crate::execution::record_task_log_note(message.as_str());
                }

                let before_versions = self
                    .source
                    .list_installed_versions(target.plugin.as_str())
                    .ok()
                    .map(|output| parse_asdf_installed_versions(&output))
                    .unwrap_or_default();
                let latest_hint = if target.requested_version.is_none() {
                    self.source
                        .latest_version(target.plugin.as_str())
                        .ok()
                        .and_then(|output| parse_asdf_latest_version(&output))
                } else {
                    None
                };
                let _ = self
                    .source
                    .install_plugin(target.plugin.as_str(), target.requested_version.as_deref())?;
                let after_versions = parse_asdf_installed_versions(
                    &self
                        .source
                        .list_installed_versions(target.plugin.as_str())?,
                );
                let after_version = resolve_installed_after_version(
                    target.requested_version.as_deref(),
                    latest_hint.as_deref(),
                    &before_versions,
                    &after_versions,
                );

                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: PackageRef {
                        manager: ManagerId::Asdf,
                        name: target.plugin,
                    },
                    action: ManagerAction::Install,
                    before_version: None,
                    after_version,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                let (uninstall_target, remove_shell_setup) =
                    parse_self_uninstall_target(uninstall_request.package.name.as_str())?;
                if uninstall_target == "__self__" {
                    let _ = self.source.self_uninstall()?;
                    if remove_shell_setup {
                        match crate::post_install_setup::remove_helm_managed_post_install_setup(
                            ManagerId::Asdf,
                        ) {
                            Ok(result) => {
                                crate::execution::record_task_log_note(result.summary().as_str());
                                if !result.malformed_files.is_empty() {
                                    let message = format!(
                                        "helm-managed asdf setup markers were malformed in {} shell startup file(s); left unchanged",
                                        result.malformed_files.len()
                                    );
                                    crate::execution::record_task_log_note(message.as_str());
                                }
                            }
                            Err(error) => {
                                let message = format!(
                                    "failed to remove Helm-managed asdf shell setup block(s): {error}"
                                );
                                crate::execution::record_task_log_note(message.as_str());
                            }
                        }
                    }
                    return Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                        package: uninstall_request.package,
                        action: ManagerAction::Uninstall,
                        before_version: None,
                        after_version: None,
                    }));
                }

                let target = self.resolve_installed_target(
                    uninstall_request.package.name.as_str(),
                    ManagerAction::Uninstall,
                )?;
                let _ = self
                    .source
                    .uninstall_plugin(target.plugin.as_str(), target.version.as_str())?;
                if target.runtime_state.is_active {
                    let message = format!(
                        "removed asdf version '{}' for '{}' without changing configured selection source; update your .tool-versions or environment if needed",
                        target.version, target.plugin
                    );
                    crate::execution::record_task_log_note(message.as_str());
                }

                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: PackageRef {
                        manager: ManagerId::Asdf,
                        name: target.plugin,
                    },
                    action: ManagerAction::Uninstall,
                    before_version: Some(target.version),
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Asdf,
                    name: "__all__".to_string(),
                });

                if package.name == "__self__" {
                    let _ = self.source.self_update()?;
                    return Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                        package,
                        action: ManagerAction::Upgrade,
                        before_version: None,
                        after_version: None,
                    }));
                }

                let result = if package.name == "__all__" {
                    self.upgrade_all_plugins()?
                } else {
                    self.upgrade_single_plugin(package.name.as_str())?
                };
                Ok(AdapterResponse::Mutation(result))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Asdf),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "asdf adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn asdf_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(ASDF_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn asdf_list_current_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(ASDF_COMMAND).arg("current"),
        LIST_TIMEOUT,
    )
}

pub fn asdf_list_plugins_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(ASDF_COMMAND).args(["plugin", "list"]),
        LIST_TIMEOUT,
    )
}

pub fn asdf_list_installed_versions_request(
    task_id: Option<TaskId>,
    plugin: &str,
) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(ASDF_COMMAND).args(["list", plugin]),
        LIST_TIMEOUT,
    )
}

pub fn asdf_search_plugins_request(
    task_id: Option<TaskId>,
    query: &SearchQuery,
) -> ProcessSpawnRequest {
    let normalized_query = query.text.trim();
    let mut command = CommandSpec::new(ASDF_COMMAND).args(["plugin", "list", "all"]);
    if !normalized_query.is_empty() {
        command = command.arg(normalized_query);
    }

    asdf_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        command,
        SEARCH_TIMEOUT,
    )
}

pub fn asdf_latest_request(task_id: Option<TaskId>, plugin: &str) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(ASDF_COMMAND).args(["latest", plugin]),
        LIST_TIMEOUT,
    )
}

pub fn asdf_add_plugin_request(task_id: Option<TaskId>, plugin: &str) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(ASDF_COMMAND).args(["plugin", "add", plugin]),
        MUTATION_TIMEOUT,
    )
}

pub fn asdf_install_request(
    task_id: Option<TaskId>,
    plugin: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let target = version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("latest");

    asdf_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(ASDF_COMMAND).args(["install", plugin, target]),
        MUTATION_TIMEOUT,
    )
}

pub fn asdf_uninstall_request(
    task_id: Option<TaskId>,
    plugin: &str,
    version: &str,
) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(ASDF_COMMAND).args(["uninstall", plugin, version]),
        MUTATION_TIMEOUT,
    )
}

pub fn asdf_set_home_version_request(
    task_id: Option<TaskId>,
    plugin: &str,
    version: &str,
) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(ASDF_COMMAND).args(["set", "-u", plugin, version]),
        MUTATION_TIMEOUT,
    )
}

pub fn asdf_self_update_request(
    task_id: Option<TaskId>,
    install_root: &str,
) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new("git").args(["-C", install_root, "pull", "--ff-only"]),
        MANAGER_UPDATE_TIMEOUT,
    )
}

pub fn asdf_clone_install_request(
    task_id: Option<TaskId>,
    install_root: &str,
) -> ProcessSpawnRequest {
    asdf_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new("git").args([
            "clone",
            "https://github.com/asdf-vm/asdf.git",
            install_root,
        ]),
        INSTALL_TIMEOUT,
    )
    .idle_timeout(INSTALL_IDLE_TIMEOUT)
}

fn asdf_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Asdf, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_install_source(version: Option<&str>) -> AdapterResult<AsdfInstallSource> {
    let Some(version) = version.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(AsdfInstallSource::OfficialDownload);
    };

    if version.eq_ignore_ascii_case("scriptInstaller:officialDownload")
        || version.eq_ignore_ascii_case("officialDownload")
    {
        return Ok(AsdfInstallSource::OfficialDownload);
    }

    Err(CoreError {
        manager: Some(ManagerId::Asdf),
        task: Some(TaskType::Install),
        action: Some(ManagerAction::Install),
        kind: CoreErrorKind::InvalidInput,
        message: format!("unsupported asdf install source: {version}"),
    })
}

fn parse_self_uninstall_target(raw: &str) -> AdapterResult<(&str, bool)> {
    let (base, remove_shell_setup) =
        crate::manager_lifecycle::strip_shell_setup_cleanup_suffix(raw);
    if base == "__self__" {
        return Ok((base, remove_shell_setup));
    }
    Ok((raw, false))
}

fn parse_asdf_mutation_target(
    raw_package_name: &str,
    action: ManagerAction,
    explicit_version: Option<&str>,
) -> AdapterResult<AsdfMutationTarget> {
    let coordinate = PackageCoordinate::parse(raw_package_name).ok_or(CoreError {
        manager: Some(ManagerId::Asdf),
        task: Some(task_type_for_action(action)),
        action: Some(action),
        kind: CoreErrorKind::InvalidInput,
        message: "asdf package name cannot be empty".to_string(),
    })?;
    let inferred_version = coordinate.version_selector.map(|selector| selector.raw);
    if let (Some(explicit_version), Some(inferred_version)) =
        (explicit_version, inferred_version.as_deref())
        && explicit_version != inferred_version
    {
        return Err(CoreError {
            manager: Some(ManagerId::Asdf),
            task: Some(task_type_for_action(action)),
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "conflicting asdf version selectors '{}' and '{}'",
                explicit_version, inferred_version
            ),
        });
    }

    crate::adapters::validate_package_identifier(
        ManagerId::Asdf,
        action,
        coordinate.package_name.as_str(),
    )?;

    Ok(AsdfMutationTarget {
        plugin: coordinate.package_name,
        requested_version: explicit_version
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or(inferred_version),
    })
}

fn parse_asdf_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;

    let token = line.split_whitespace().find(|piece| {
        piece
            .chars()
            .next()
            .is_some_and(|ch| ch == 'v' || ch.is_ascii_digit())
    })?;
    let normalized = token.strip_prefix('v').unwrap_or(token);
    if normalized.is_empty() || !normalized.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }

    Some(normalized.to_string())
}

fn parse_asdf_plugins(output: &str) -> Vec<String> {
    let mut plugins = BTreeSet::new();
    for line in output.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        if !name.is_empty() {
            plugins.insert(name.to_string());
        }
    }
    plugins.into_iter().collect()
}

fn parse_asdf_installed_versions(output: &str) -> Vec<String> {
    let mut versions = BTreeSet::new();
    for line in output.lines().map(str::trim) {
        if line.is_empty()
            || line.starts_with("No versions installed")
            || line.starts_with("No version installed")
            || line.starts_with("not installed")
        {
            continue;
        }
        let normalized = line.trim_start_matches('*').trim();
        if normalized.is_empty() {
            continue;
        }
        let Some(version) = normalized.split_whitespace().next() else {
            continue;
        };
        if !version.is_empty() {
            versions.insert(version.to_string());
        }
    }
    versions.into_iter().collect()
}

fn parse_asdf_current(output: &str, home_dir: Option<&Path>) -> Vec<AsdfCurrentSelection> {
    let mut selections = Vec::new();

    for line in output.lines().map(str::trim) {
        if line.is_empty()
            || line.starts_with("No ")
            || line.starts_with("not installed")
            || line.starts_with("Name")
        {
            continue;
        }

        let tokens = line.split_whitespace().collect::<Vec<_>>();
        if tokens.len() < 2 {
            continue;
        }
        let name = tokens[0].trim();
        let version = tokens[1].trim();
        if name.is_empty()
            || version.is_empty()
            || version.eq_ignore_ascii_case("system")
            || version.starts_with('(')
            || version.eq_ignore_ascii_case("No")
            || version.eq_ignore_ascii_case("not")
        {
            continue;
        }

        let source = tokens.get(2..).unwrap_or(&[]).join(" ");
        selections.push(AsdfCurrentSelection {
            plugin: name.to_string(),
            version: version.to_string(),
            runtime_state: classify_asdf_runtime_state(source.as_str(), home_dir),
        });
    }

    selections
}

fn classify_asdf_runtime_state(source: &str, home_dir: Option<&Path>) -> PackageRuntimeState {
    let trimmed = source.trim();
    let mut runtime_state = PackageRuntimeState {
        is_active: true,
        ..Default::default()
    };
    if trimmed.is_empty() {
        return runtime_state;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("environment variable")
        || lower.starts_with("env:")
        || (lower.contains("asdf_") && lower.contains("_version"))
    {
        runtime_state.has_override = true;
        return runtime_state;
    }

    if trimmed.starts_with('/') {
        let path = Path::new(trimmed);
        if is_home_tool_versions_path(path, home_dir) {
            runtime_state.is_default = true;
            return runtime_state;
        }
        runtime_state.has_override = true;
        return runtime_state;
    }

    runtime_state.has_override = true;
    runtime_state
}

fn is_home_tool_versions_path(path: &Path, home_dir: Option<&Path>) -> bool {
    if path.file_name().and_then(|value| value.to_str()) != Some(".tool-versions") {
        return false;
    }
    if let Some(home_dir) = home_dir
        && path.parent() == Some(home_dir)
    {
        return true;
    }

    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(parent_name) = parent.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if parent_name.is_empty() {
        return false;
    }
    let Some(home_root) = parent.parent() else {
        return false;
    };
    let Some(home_root_name) = home_root.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let is_standard_home_root =
        home_root_name.eq_ignore_ascii_case("Users") || home_root_name.eq_ignore_ascii_case("home");
    is_standard_home_root
        && home_root
            .parent()
            .is_some_and(|value| value == Path::new("/"))
}

fn current_selection_map(
    selections: &[AsdfCurrentSelection],
) -> HashMap<String, HashMap<String, PackageRuntimeState>> {
    let mut by_plugin = HashMap::<String, HashMap<String, PackageRuntimeState>>::new();
    for selection in selections {
        by_plugin
            .entry(selection.plugin.clone())
            .or_default()
            .insert(selection.version.clone(), selection.runtime_state.clone());
    }
    by_plugin
}

fn parse_asdf_latest_version(output: &str) -> Option<String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    let token = line.split_whitespace().find(|piece| {
        piece
            .chars()
            .next()
            .is_some_and(|ch| ch == 'v' || ch.is_ascii_digit())
    })?;
    let normalized = token.strip_prefix('v').unwrap_or(token).trim();
    if normalized.is_empty() || !normalized.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    Some(normalized.to_string())
}

fn parse_asdf_search(output: &str, query: &SearchQuery) -> Vec<CachedSearchResult> {
    let needle = query.text.trim().to_ascii_lowercase();
    let mut results = Vec::new();

    for name in parse_asdf_plugins(output) {
        if !needle.is_empty() && !name.to_ascii_lowercase().contains(&needle) {
            continue;
        }
        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name,
                },
                version: None,
                summary: Some("asdf plugin".to_string()),
            },
            source_manager: ManagerId::Asdf,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    results.sort_by(|lhs, rhs| lhs.result.package.name.cmp(&rhs.result.package.name));
    results
}

fn representative_installed_package(packages: &[InstalledPackage]) -> Option<InstalledPackage> {
    let mut sorted = packages.to_vec();
    sorted.sort_by(preferred_installed_package_ordering);
    sorted.into_iter().next()
}

fn preferred_installed_package_ordering(
    lhs: &InstalledPackage,
    rhs: &InstalledPackage,
) -> Ordering {
    if lhs.runtime_state.is_active != rhs.runtime_state.is_active {
        return rhs
            .runtime_state
            .is_active
            .cmp(&lhs.runtime_state.is_active);
    }
    if lhs.runtime_state.is_default != rhs.runtime_state.is_default {
        return rhs
            .runtime_state
            .is_default
            .cmp(&lhs.runtime_state.is_default);
    }
    if lhs.runtime_state.has_override != rhs.runtime_state.has_override {
        return lhs
            .runtime_state
            .has_override
            .cmp(&rhs.runtime_state.has_override);
    }
    versionish_cmp(
        lhs.installed_version.as_deref().unwrap_or(""),
        rhs.installed_version.as_deref().unwrap_or(""),
    )
    .reverse()
}

fn resolve_installed_after_version(
    requested_version: Option<&str>,
    latest_hint: Option<&str>,
    before_versions: &[String],
    after_versions: &[String],
) -> Option<String> {
    if let Some(requested_version) = requested_version
        && after_versions
            .iter()
            .any(|version| version == requested_version)
    {
        return Some(requested_version.to_string());
    }
    if let Some(latest_hint) = latest_hint
        && after_versions.iter().any(|version| version == latest_hint)
    {
        return Some(latest_hint.to_string());
    }

    let before_set = before_versions.iter().collect::<BTreeSet<_>>();
    let mut new_versions = after_versions
        .iter()
        .filter(|version| !before_set.contains(version))
        .cloned()
        .collect::<Vec<_>>();
    new_versions.sort_by(|lhs, rhs| versionish_cmp(lhs, rhs).reverse());
    if let Some(version) = new_versions.into_iter().next() {
        return Some(version);
    }

    let mut installed = after_versions.to_vec();
    installed.sort_by(|lhs, rhs| versionish_cmp(lhs, rhs).reverse());
    installed.into_iter().next()
}

fn versionish_cmp(lhs: &str, rhs: &str) -> Ordering {
    let lhs_chunks = chunk_versionish(lhs);
    let rhs_chunks = chunk_versionish(rhs);
    let max_len = lhs_chunks.len().max(rhs_chunks.len());
    for index in 0..max_len {
        match (lhs_chunks.get(index), rhs_chunks.get(index)) {
            (
                Some(VersionishChunk::Digits(lhs_digits)),
                Some(VersionishChunk::Digits(rhs_digits)),
            ) => {
                let lhs_normalized = lhs_digits.trim_start_matches('0');
                let rhs_normalized = rhs_digits.trim_start_matches('0');
                let lhs_effective = if lhs_normalized.is_empty() {
                    "0"
                } else {
                    lhs_normalized
                };
                let rhs_effective = if rhs_normalized.is_empty() {
                    "0"
                } else {
                    rhs_normalized
                };
                let length_order = lhs_effective.len().cmp(&rhs_effective.len());
                if length_order != Ordering::Equal {
                    return length_order;
                }
                let order = lhs_effective.cmp(rhs_effective);
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(VersionishChunk::Text(lhs_text)), Some(VersionishChunk::Text(rhs_text))) => {
                let order = lhs_text
                    .to_ascii_lowercase()
                    .cmp(&rhs_text.to_ascii_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(VersionishChunk::Digits(_)), Some(VersionishChunk::Text(_))) => {
                return Ordering::Greater;
            }
            (Some(VersionishChunk::Text(_)), Some(VersionishChunk::Digits(_))) => {
                return Ordering::Less;
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => break,
        }
    }
    lhs.to_ascii_lowercase().cmp(&rhs.to_ascii_lowercase())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VersionishChunk {
    Digits(String),
    Text(String),
}

fn chunk_versionish(value: &str) -> Vec<VersionishChunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_is_digit: Option<bool> = None;

    for ch in value.chars() {
        if !ch.is_ascii_alphanumeric() {
            if !current.is_empty() {
                push_versionish_chunk(&mut chunks, &mut current, current_is_digit.take());
            }
            continue;
        }
        let is_digit = ch.is_ascii_digit();
        if let Some(current_is_digit) = current_is_digit
            && current_is_digit != is_digit
        {
            push_versionish_chunk(&mut chunks, &mut current, Some(current_is_digit));
        }
        current.push(ch);
        current_is_digit = Some(is_digit);
    }

    if !current.is_empty() {
        push_versionish_chunk(&mut chunks, &mut current, current_is_digit);
    }

    chunks
}

fn push_versionish_chunk(
    chunks: &mut Vec<VersionishChunk>,
    current: &mut String,
    current_is_digit: Option<bool>,
) {
    let chunk = std::mem::take(current);
    match current_is_digit {
        Some(true) => chunks.push(VersionishChunk::Digits(chunk)),
        _ => chunks.push(VersionishChunk::Text(chunk)),
    }
}

fn current_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::UNIX_EPOCH;

    use crate::adapters::asdf::{
        AsdfAdapter, AsdfDetectOutput, AsdfInstallSource, AsdfSource, asdf_add_plugin_request,
        asdf_clone_install_request, asdf_detect_request, asdf_install_request,
        asdf_list_current_request, asdf_list_installed_versions_request, asdf_list_plugins_request,
        asdf_search_plugins_request, asdf_self_update_request, asdf_set_home_version_request,
        asdf_uninstall_request, parse_asdf_current, parse_asdf_installed_versions,
        parse_asdf_latest_version, parse_asdf_plugins, parse_asdf_search, parse_asdf_version,
        parse_install_source,
    };
    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, InstallRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
    };
    use crate::models::{
        CoreError, CoreErrorKind, ManagerAction, ManagerId, PackageRef, SearchQuery, TaskType,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/asdf/version.txt");
    const CURRENT_FIXTURE: &str = include_str!("../../tests/fixtures/asdf/current.txt");
    const PLUGINS_FIXTURE: &str = include_str!("../../tests/fixtures/asdf/plugin_list_all.txt");

    #[test]
    fn parses_asdf_version_from_fixture() {
        let parsed = parse_asdf_version(VERSION_FIXTURE);
        assert_eq!(parsed.as_deref(), Some("0.16.0"));
    }

    #[test]
    fn parses_asdf_plugin_names() {
        let plugins = parse_asdf_plugins(PLUGINS_FIXTURE);
        assert_eq!(plugins, vec!["nodejs", "python", "ruby"]);
    }

    #[test]
    fn parses_asdf_installed_versions_output() {
        let versions = parse_asdf_installed_versions("  3.12.2\n  3.11.9\n");
        assert_eq!(versions, vec!["3.11.9", "3.12.2"]);
    }

    #[test]
    fn parses_asdf_current_runtime_state() {
        let output = "\
python 3.12.2 /Users/dev/.tool-versions
nodejs 20.12.2 /Users/dev/work/project/.tool-versions
ruby 3.3.0 ASDF_RUBY_VERSION environment variable
";
        let parsed = parse_asdf_current(output, Some(PathBuf::from("/Users/dev").as_path()));
        assert_eq!(parsed.len(), 3);
        assert!(parsed[0].runtime_state.is_active);
        assert!(parsed[0].runtime_state.is_default);
        assert!(!parsed[0].runtime_state.has_override);
        assert!(parsed[1].runtime_state.is_active);
        assert!(!parsed[1].runtime_state.is_default);
        assert!(parsed[1].runtime_state.has_override);
        assert!(parsed[2].runtime_state.is_active);
        assert!(parsed[2].runtime_state.has_override);
    }

    #[test]
    fn parses_latest_version_line() {
        assert_eq!(
            parse_asdf_latest_version("20.12.3\n").as_deref(),
            Some("20.12.3")
        );
        assert_eq!(
            parse_asdf_latest_version("v3.13.0\n").as_deref(),
            Some("3.13.0")
        );
    }

    #[test]
    fn parses_plugin_search_results() {
        let query = SearchQuery {
            text: "py".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let results = parse_asdf_search(PLUGINS_FIXTURE, &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "python");
        assert_eq!(results[0].originating_query, "py");
    }

    #[test]
    fn detect_request_has_expected_shape() {
        let request = asdf_detect_request(None);
        assert_eq!(request.manager, ManagerId::Asdf);
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program.to_str(), Some("asdf"));
        assert_eq!(request.command.args, vec!["--version"]);
        assert!(!request.requires_elevation);
    }

    #[test]
    fn list_requests_have_expected_shapes() {
        let current = asdf_list_current_request(None);
        assert_eq!(current.command.args, vec!["current"]);

        let plugins = asdf_list_plugins_request(None);
        assert_eq!(plugins.command.args, vec!["plugin", "list"]);

        let versions = asdf_list_installed_versions_request(None, "python");
        assert_eq!(versions.command.args, vec!["list", "python"]);
    }

    #[test]
    fn search_and_mutation_requests_have_expected_shapes() {
        let query = SearchQuery {
            text: "node".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let search = asdf_search_plugins_request(None, &query);
        assert_eq!(search.command.args, vec!["plugin", "list", "all", "node"]);

        let add_plugin = asdf_add_plugin_request(None, "python");
        assert_eq!(add_plugin.command.args, vec!["plugin", "add", "python"]);

        let install = asdf_install_request(None, "python", Some("3.12.2"));
        assert_eq!(install.command.args, vec!["install", "python", "3.12.2"]);

        let uninstall = asdf_uninstall_request(None, "python", "3.12.2");
        assert_eq!(
            uninstall.command.args,
            vec!["uninstall", "python", "3.12.2"]
        );

        let set_home = asdf_set_home_version_request(None, "python", "3.13.0");
        assert_eq!(set_home.command.args, vec!["set", "-u", "python", "3.13.0"]);
    }

    #[test]
    fn manager_self_requests_have_expected_shapes() {
        let install = asdf_clone_install_request(None, "/Users/test/.asdf");
        assert_eq!(install.task_type, TaskType::Install);
        assert_eq!(
            install.command.args,
            vec![
                "clone",
                "https://github.com/asdf-vm/asdf.git",
                "/Users/test/.asdf"
            ]
        );

        let update = asdf_self_update_request(None, "/Users/test/.asdf");
        assert_eq!(update.task_type, TaskType::Upgrade);
        assert_eq!(
            update.command.args,
            vec!["-C", "/Users/test/.asdf", "pull", "--ff-only"]
        );
    }

    #[test]
    fn parses_asdf_manager_install_source() {
        assert_eq!(
            parse_install_source(Some("scriptInstaller:officialDownload"))
                .expect("source should parse"),
            AsdfInstallSource::OfficialDownload
        );
        assert_eq!(
            parse_install_source(Some("officialDownload")).expect("source should parse"),
            AsdfInstallSource::OfficialDownload
        );
        assert!(parse_install_source(Some("existingBinaryPath:/tmp/asdf")).is_err());
    }

    #[test]
    fn adapter_list_installed_uses_plugin_inventory_and_runtime_state() {
        let source = FixtureSource::new(
            CURRENT_FIXTURE,
            vec!["nodejs", "python"],
            vec![
                ("nodejs", vec!["20.12.2"]),
                ("python", vec!["3.11.9", "3.12.2"]),
            ],
            vec![("nodejs", "20.12.2\n"), ("python", "3.13.0\n")],
            PLUGINS_FIXTURE,
        );
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();

        let AdapterResponse::InstalledPackages(packages) = response else {
            panic!("expected installed packages response");
        };
        assert_eq!(packages.len(), 3);
        let python_current = packages
            .iter()
            .find(|package| {
                package.package.name == "python"
                    && package.installed_version.as_deref() == Some("3.12.2")
            })
            .expect("current python package should exist");
        assert!(python_current.runtime_state.is_active);
        assert!(python_current.runtime_state.is_default);
        let python_other = packages
            .iter()
            .find(|package| {
                package.package.name == "python"
                    && package.installed_version.as_deref() == Some("3.11.9")
            })
            .expect("secondary python package should exist");
        assert!(python_other.runtime_state.is_empty());
    }

    #[test]
    fn adapter_list_outdated_prefers_active_version() {
        let current = "\
python 3.11.9 /Users/test/.tool-versions
";
        let source = FixtureSource::new(
            current,
            vec!["python"],
            vec![("python", vec!["3.11.9", "3.12.2"])],
            vec![("python", "3.13.0\n")],
            PLUGINS_FIXTURE,
        );
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        let AdapterResponse::OutdatedPackages(packages) = response else {
            panic!("expected outdated packages response");
        };
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].installed_version.as_deref(), Some("3.11.9"));
        assert!(packages[0].runtime_state.is_active);
        assert_eq!(packages[0].candidate_version, "3.13.0");
    }

    #[test]
    fn adapter_install_bootstraps_plugin_and_resolves_after_version() {
        let source = FixtureSource::new(
            "",
            Vec::<&'static str>::new(),
            Vec::<(&'static str, Vec<&'static str>)>::new(),
            vec![("python", "3.12.2\n")],
            PLUGINS_FIXTURE,
        );
        let adapter = AsdfAdapter::new(source.clone());
        let response = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: "python".to_string(),
                },
                version: Some("3.12.2".to_string()),
            }))
            .unwrap();

        let AdapterResponse::Mutation(result) = response else {
            panic!("expected mutation response");
        };
        assert_eq!(result.after_version.as_deref(), Some("3.12.2"));
        assert_eq!(source.added_plugins(), vec!["python".to_string()]);
        assert_eq!(
            source.install_calls(),
            vec![("python".to_string(), Some("3.12.2".to_string()))]
        );
    }

    #[test]
    fn adapter_uninstall_accepts_coordinate_version() {
        let source = FixtureSource::new(
            "",
            vec!["python"],
            vec![("python", vec!["3.11.9", "3.12.2"])],
            vec![("python", "3.13.0\n")],
            PLUGINS_FIXTURE,
        );
        let adapter = AsdfAdapter::new(source.clone());
        let response = adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: "python@3.11.9".to_string(),
                },
            }))
            .unwrap();

        let AdapterResponse::Mutation(result) = response else {
            panic!("expected mutation response");
        };
        assert_eq!(result.before_version.as_deref(), Some("3.11.9"));
        assert_eq!(
            source.uninstall_calls(),
            vec![("python".to_string(), "3.11.9".to_string())]
        );
    }

    #[test]
    fn adapter_uninstall_rejects_ambiguous_multiple_versions_without_current_selection() {
        let source = FixtureSource::new(
            "",
            vec!["python"],
            vec![("python", vec!["3.11.9", "3.12.2"])],
            Vec::<(&'static str, &'static str)>::new(),
            PLUGINS_FIXTURE,
        );
        let adapter = AsdfAdapter::new(source);
        let error = adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: "python".to_string(),
                },
            }))
            .expect_err("ambiguous uninstall should fail");
        assert!(error.message.contains("multiple installed versions"));
    }

    #[test]
    fn adapter_upgrade_global_active_sets_home_version() {
        let source = FixtureSource::new(
            CURRENT_FIXTURE,
            vec!["nodejs", "python"],
            vec![("nodejs", vec!["20.12.2"]), ("python", vec!["3.12.2"])],
            vec![("nodejs", "20.12.2\n"), ("python", "3.13.0\n")],
            PLUGINS_FIXTURE,
        );
        let adapter = AsdfAdapter::new(source.clone());
        let response = adapter
            .execute(AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Asdf,
                    name: "python".to_string(),
                }),
            }))
            .unwrap();

        let AdapterResponse::Mutation(result) = response else {
            panic!("expected mutation response");
        };
        assert_eq!(result.before_version.as_deref(), Some("3.12.2"));
        assert_eq!(result.after_version.as_deref(), Some("3.13.0"));
        assert_eq!(
            source.set_home_calls(),
            vec![("python".to_string(), "3.13.0".to_string())]
        );
    }

    #[test]
    fn adapter_search_filters_plugin_catalog() {
        let source = FixtureSource::new(
            CURRENT_FIXTURE,
            vec!["nodejs", "python"],
            vec![("nodejs", vec!["20.12.2"]), ("python", vec!["3.12.2"])],
            Vec::<(&'static str, &'static str)>::new(),
            PLUGINS_FIXTURE,
        );
        let adapter = AsdfAdapter::new(source);
        let response = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "node".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .unwrap();

        let AdapterResponse::SearchResults(results) = response else {
            panic!("expected search response");
        };
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "nodejs");
    }

    #[test]
    fn adapter_manager_self_actions_accept_self_placeholder() {
        let source = FixtureSource::new(
            CURRENT_FIXTURE,
            vec!["nodejs", "python"],
            vec![("nodejs", vec!["20.12.2"]), ("python", vec!["3.12.2"])],
            Vec::<(&'static str, &'static str)>::new(),
            PLUGINS_FIXTURE,
        );
        let adapter = AsdfAdapter::new(source);

        let install_response = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: "__self__".to_string(),
                },
                version: Some("scriptInstaller:officialDownload".to_string()),
            }))
            .expect("manager install should succeed");
        assert!(matches!(install_response, AdapterResponse::Mutation(_)));

        let uninstall_response = adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: "__self__".to_string(),
                },
            }))
            .expect("manager uninstall should succeed");
        assert!(matches!(uninstall_response, AdapterResponse::Mutation(_)));

        let upgrade_response = adapter
            .execute(AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Asdf,
                    name: "__self__".to_string(),
                }),
            }))
            .expect("manager update should succeed");
        assert!(matches!(upgrade_response, AdapterResponse::Mutation(_)));
    }

    #[derive(Clone)]
    struct FixtureSource {
        detect_result: AdapterResult<AsdfDetectOutput>,
        current_output: String,
        installed_plugins: std::sync::Arc<Mutex<BTreeSet<String>>>,
        installed_versions: std::sync::Arc<Mutex<HashMap<String, BTreeSet<String>>>>,
        latest_by_plugin: HashMap<String, String>,
        search_catalog: String,
        added_plugins_log: std::sync::Arc<Mutex<Vec<String>>>,
        install_log: std::sync::Arc<Mutex<Vec<(String, Option<String>)>>>,
        uninstall_log: std::sync::Arc<Mutex<Vec<(String, String)>>>,
        set_home_log: std::sync::Arc<Mutex<Vec<(String, String)>>>,
    }

    impl FixtureSource {
        fn new(
            current_output: &str,
            installed_plugins: impl IntoIterator<Item = &'static str>,
            installed_versions: impl IntoIterator<Item = (&'static str, Vec<&'static str>)>,
            latest_by_plugin: impl IntoIterator<Item = (&'static str, &'static str)>,
            search_catalog: &str,
        ) -> Self {
            let plugin_set = installed_plugins
                .into_iter()
                .map(str::to_string)
                .collect::<BTreeSet<_>>();
            let version_map = installed_versions
                .into_iter()
                .map(|(plugin, versions)| {
                    (
                        plugin.to_string(),
                        versions
                            .into_iter()
                            .map(str::to_string)
                            .collect::<BTreeSet<_>>(),
                    )
                })
                .collect::<HashMap<_, _>>();
            let latest_map = latest_by_plugin
                .into_iter()
                .map(|(plugin, output)| (plugin.to_string(), output.to_string()))
                .collect::<HashMap<_, _>>();
            Self {
                detect_result: Ok(AsdfDetectOutput {
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/asdf")),
                    version_output: VERSION_FIXTURE.to_string(),
                }),
                current_output: current_output.to_string(),
                installed_plugins: std::sync::Arc::new(Mutex::new(plugin_set)),
                installed_versions: std::sync::Arc::new(Mutex::new(version_map)),
                latest_by_plugin: latest_map,
                search_catalog: search_catalog.to_string(),
                added_plugins_log: std::sync::Arc::new(Mutex::new(Vec::new())),
                install_log: std::sync::Arc::new(Mutex::new(Vec::new())),
                uninstall_log: std::sync::Arc::new(Mutex::new(Vec::new())),
                set_home_log: std::sync::Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn added_plugins(&self) -> Vec<String> {
            self.added_plugins_log.lock().unwrap().clone()
        }

        fn install_calls(&self) -> Vec<(String, Option<String>)> {
            self.install_log.lock().unwrap().clone()
        }

        fn uninstall_calls(&self) -> Vec<(String, String)> {
            self.uninstall_log.lock().unwrap().clone()
        }

        fn set_home_calls(&self) -> Vec<(String, String)> {
            self.set_home_log.lock().unwrap().clone()
        }
    }

    impl AsdfSource for FixtureSource {
        fn detect(&self) -> AdapterResult<AsdfDetectOutput> {
            self.detect_result.clone()
        }

        fn list_current(&self) -> AdapterResult<String> {
            Ok(self.current_output.clone())
        }

        fn list_plugins(&self) -> AdapterResult<String> {
            Ok(self
                .installed_plugins
                .lock()
                .unwrap()
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n"))
        }

        fn list_installed_versions(&self, plugin: &str) -> AdapterResult<String> {
            Ok(self
                .installed_versions
                .lock()
                .unwrap()
                .get(plugin)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect::<Vec<_>>()
                .join("\n"))
        }

        fn search_plugins(&self, _query: &SearchQuery) -> AdapterResult<String> {
            Ok(self.search_catalog.clone())
        }

        fn latest_version(&self, plugin: &str) -> AdapterResult<String> {
            self.latest_by_plugin.get(plugin).cloned().ok_or(CoreError {
                manager: Some(ManagerId::Asdf),
                task: Some(TaskType::Refresh),
                action: Some(ManagerAction::ListOutdated),
                kind: CoreErrorKind::ParseFailure,
                message: format!("missing latest fixture for '{plugin}'"),
            })
        }

        fn add_plugin(&self, plugin: &str) -> AdapterResult<String> {
            self.added_plugins_log
                .lock()
                .unwrap()
                .push(plugin.to_string());
            self.installed_plugins
                .lock()
                .unwrap()
                .insert(plugin.to_string());
            Ok(String::new())
        }

        fn install_plugin(&self, plugin: &str, version: Option<&str>) -> AdapterResult<String> {
            self.install_log
                .lock()
                .unwrap()
                .push((plugin.to_string(), version.map(str::to_string)));
            self.installed_plugins
                .lock()
                .unwrap()
                .insert(plugin.to_string());
            let resolved_version = if let Some(version) = version {
                version.to_string()
            } else {
                parse_asdf_latest_version(
                    self.latest_by_plugin
                        .get(plugin)
                        .map(String::as_str)
                        .unwrap_or(""),
                )
                .unwrap_or_else(|| "latest".to_string())
            };
            self.installed_versions
                .lock()
                .unwrap()
                .entry(plugin.to_string())
                .or_default()
                .insert(resolved_version);
            Ok(String::new())
        }

        fn uninstall_plugin(&self, plugin: &str, version: &str) -> AdapterResult<String> {
            self.uninstall_log
                .lock()
                .unwrap()
                .push((plugin.to_string(), version.to_string()));
            if let Some(versions) = self.installed_versions.lock().unwrap().get_mut(plugin) {
                versions.remove(version);
            }
            Ok(String::new())
        }

        fn set_home_version(&self, plugin: &str, version: &str) -> AdapterResult<String> {
            self.set_home_log
                .lock()
                .unwrap()
                .push((plugin.to_string(), version.to_string()));
            Ok(String::new())
        }

        fn install_self(&self, _source: AsdfInstallSource) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn self_uninstall(&self) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn self_update(&self) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
