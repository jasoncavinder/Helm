use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, PackageRuntimeState, SearchQuery,
    TaskId, TaskType,
};
use crate::versioning::{PackageCoordinate, VersionSelector};

const MISE_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const MISE_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Mise,
    display_name: "mise",
    category: ManagerCategory::ToolRuntime,
    authority: ManagerAuthority::Authoritative,
    capabilities: MISE_CAPABILITIES,
};

const MISE_COMMAND: &str = "mise";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(180);
const REGISTRY_TIMEOUT: Duration = Duration::from_secs(120);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const INSTALL_IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(25 * 60);
const UNINSTALL_IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const UPGRADE_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiseDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MiseInstallSource {
    OfficialDownload,
    ExistingBinaryPath(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MiseUninstallMode {
    ManagerOnlyKeepConfig,
    FullCleanupKeepConfig,
    FullCleanupRemoveConfig,
}

pub trait MiseSource: Send + Sync {
    fn detect(&self) -> AdapterResult<MiseDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn list_remote_packages(&self) -> AdapterResult<Vec<MiseRemotePackage>>;
    fn install_self(&self, source: MiseInstallSource) -> AdapterResult<String>;
    fn install_tool(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall_tool(&self, name: &str, version: Option<&str>) -> AdapterResult<String>;
    fn self_uninstall(&self, mode: MiseUninstallMode) -> AdapterResult<String>;
    fn upgrade_tool(&self, name: &str) -> AdapterResult<String>;
}

pub struct MiseAdapter<S: MiseSource> {
    source: S,
}

impl<S: MiseSource> MiseAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }

    fn load_installed_packages(&self) -> AdapterResult<Vec<InstalledPackage>> {
        let raw = self.source.list_installed()?;
        parse_mise_installed(&raw)
    }

    fn resolve_installed_target(
        &self,
        raw_package_name: &str,
        action: ManagerAction,
    ) -> AdapterResult<ResolvedMiseInstalledTarget> {
        let (tool_name, requested_version) = parse_package_uninstall_target(raw_package_name)?;
        crate::adapters::validate_package_identifier(ManagerId::Mise, action, tool_name.as_str())?;

        let mut matches = self
            .load_installed_packages()?
            .into_iter()
            .filter(|package| package.package.name == tool_name)
            .collect::<Vec<_>>();

        if matches.is_empty() {
            return Err(CoreError {
                manager: Some(ManagerId::Mise),
                task: Some(TaskType::Uninstall),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!("mise tool '{}' is not installed", tool_name),
            });
        }

        if let Some(requested_version) = requested_version {
            let package = matches
                .into_iter()
                .find(|package| {
                    package.installed_version.as_deref() == Some(requested_version.as_str())
                })
                .ok_or(CoreError {
                    manager: Some(ManagerId::Mise),
                    task: Some(TaskType::Uninstall),
                    action: Some(action),
                    kind: CoreErrorKind::InvalidInput,
                    message: format!(
                        "mise tool '{}' does not have installed version '{}'",
                        tool_name, requested_version
                    ),
                })?;
            return Ok(ResolvedMiseInstalledTarget {
                tool_name: package.package.name,
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
            return Ok(ResolvedMiseInstalledTarget {
                tool_name: package.package.name.clone(),
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
                manager: Some(ManagerId::Mise),
                task: Some(TaskType::Uninstall),
                action: Some(action),
                kind: CoreErrorKind::InvalidInput,
                message: format!(
                    "mise tool '{}' has multiple active versions ({}); specify '{}@<version>'",
                    tool_name, versions, tool_name
                ),
            });
        }
        if matches.len() == 1 {
            let package = matches.pop().expect("single match must exist");
            return Ok(ResolvedMiseInstalledTarget {
                tool_name: package.package.name,
                version: package.installed_version.unwrap_or_default(),
                runtime_state: package.runtime_state,
            });
        }

        matches.sort_by(|lhs, rhs| {
            lhs.installed_version
                .as_deref()
                .unwrap_or("")
                .cmp(rhs.installed_version.as_deref().unwrap_or(""))
                .reverse()
        });
        let versions = matches
            .iter()
            .filter_map(|package| package.installed_version.as_deref())
            .map(str::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        Err(CoreError {
            manager: Some(ManagerId::Mise),
            task: Some(TaskType::Uninstall),
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "mise tool '{}' has multiple installed versions ({}); specify '{}@<version>'",
                tool_name, versions, tool_name
            ),
        })
    }
}

impl<S: MiseSource> ManagerAdapter for MiseAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &MISE_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_mise_version(&output.version_output);
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
                let packages = parse_mise_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let mut packages = parse_mise_outdated(&raw)?;
                if let Ok(installed_raw) = self.source.list_installed()
                    && let Ok(installed) = parse_mise_installed(&installed_raw)
                {
                    hydrate_mise_outdated_runtime_state(&mut packages, &installed);
                }
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let packages = self.source.list_remote_packages()?;
                let results = filter_mise_remote_packages(&packages, &search_request.query);
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                if install_request.package.name == "__self__" {
                    let source = parse_install_source(install_request.version.as_deref())?;
                    let _ = self.source.install_self(source)?;
                    return Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                        package: install_request.package,
                        action: ManagerAction::Install,
                        before_version: None,
                        after_version: None,
                    }));
                }

                let (tool_name, requested_version) = parse_package_install_target(
                    install_request.package.name.as_str(),
                    install_request.version.as_deref(),
                )?;
                crate::adapters::validate_package_identifier(
                    ManagerId::Mise,
                    ManagerAction::Install,
                    tool_name.as_str(),
                )?;
                let _ = self
                    .source
                    .install_tool(tool_name.as_str(), requested_version.as_deref())?;

                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: PackageRef {
                        manager: ManagerId::Mise,
                        name: tool_name,
                    },
                    action: ManagerAction::Install,
                    before_version: None,
                    after_version: requested_version,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                if uninstall_request
                    .package
                    .name
                    .trim_start()
                    .starts_with("__self__")
                {
                    let uninstall_spec =
                        parse_uninstall_mode(uninstall_request.package.name.as_str())?;
                    let _ = self.source.self_uninstall(uninstall_spec.mode)?;
                    if uninstall_spec.remove_shell_setup {
                        match crate::post_install_setup::remove_helm_managed_post_install_setup(
                            ManagerId::Mise,
                        ) {
                            Ok(result) => {
                                crate::execution::record_task_log_note(result.summary().as_str());
                                if !result.malformed_files.is_empty() {
                                    crate::execution::record_task_log_note(
                                        format!(
                                            "helm-managed mise setup markers were malformed in {} shell startup file(s); left unchanged",
                                            result.malformed_files.len()
                                        )
                                        .as_str(),
                                    );
                                }
                            }
                            Err(error) => {
                                crate::execution::record_task_log_note(
                                    format!(
                                        "failed to remove Helm-managed mise shell setup block(s): {error}"
                                    )
                                    .as_str(),
                                );
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
                    .uninstall_tool(target.tool_name.as_str(), Some(target.version.as_str()))?;
                if target.runtime_state.is_active {
                    let note = if target.runtime_state.is_default {
                        format!(
                            "removed installed mise version '{}' for '{}' but active global mise configuration may still reference this tool",
                            target.version, target.tool_name
                        )
                    } else if target.runtime_state.has_override {
                        format!(
                            "removed installed mise version '{}' for '{}' but a local or environment-specific mise configuration may still reference this tool",
                            target.version, target.tool_name
                        )
                    } else {
                        format!(
                            "removed active mise version '{}' for '{}'",
                            target.version, target.tool_name
                        )
                    };
                    crate::execution::record_task_log_note(note.as_str());
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: PackageRef {
                        manager: ManagerId::Mise,
                        name: target.tool_name,
                    },
                    action: ManagerAction::Uninstall,
                    before_version: Some(target.version),
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Mise,
                    name: "__all__".to_string(),
                });
                let _ = self.source.upgrade_tool(&package.name)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Mise),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "mise adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn mise_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    mise_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(MISE_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn mise_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    mise_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(MISE_COMMAND).args(["ls", "--json"]),
        LIST_TIMEOUT,
    )
}

pub fn mise_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    mise_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(MISE_COMMAND).args(["outdated", "--json"]),
        LIST_TIMEOUT,
    )
}

pub fn mise_list_remote_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    mise_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(MISE_COMMAND).args(["ls-remote", "--all", "--json"]),
        SEARCH_TIMEOUT,
    )
}

pub fn mise_registry_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    mise_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(MISE_COMMAND).args(["registry", "--json"]),
        REGISTRY_TIMEOUT,
    )
}

pub fn mise_upgrade_request(task_id: Option<TaskId>, name: &str) -> ProcessSpawnRequest {
    let command = if name == "__all__" {
        CommandSpec::new(MISE_COMMAND).arg("upgrade")
    } else {
        CommandSpec::new(MISE_COMMAND).args(["upgrade", name])
    };
    mise_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
        UPGRADE_TIMEOUT,
    )
}

pub fn mise_download_install_script_request(
    task_id: Option<TaskId>,
    output_script: &str,
) -> ProcessSpawnRequest {
    mise_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new("curl").args(["-fsSL", "https://mise.run", "-o", output_script]),
        INSTALL_TIMEOUT,
    )
}

pub fn mise_run_downloaded_install_script_request(
    task_id: Option<TaskId>,
    script_path: &str,
) -> ProcessSpawnRequest {
    mise_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new("sh").arg(script_path),
        INSTALL_TIMEOUT,
    )
    .idle_timeout(INSTALL_IDLE_TIMEOUT)
}

pub fn mise_install_tool_request(
    task_id: Option<TaskId>,
    name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let target = match version.map(str::trim).filter(|value| !value.is_empty()) {
        Some(version) => format!("{name}@{version}"),
        None => name.to_string(),
    };
    mise_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(MISE_COMMAND).args(["use", "--global", target.as_str()]),
        INSTALL_TIMEOUT,
    )
    .idle_timeout(INSTALL_IDLE_TIMEOUT)
}

pub fn mise_implode_request(task_id: Option<TaskId>, remove_config: bool) -> ProcessSpawnRequest {
    let mut command = CommandSpec::new(MISE_COMMAND).args(["implode", "--yes"]);
    if remove_config {
        command = command.arg("--config");
    }
    mise_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        command,
        UNINSTALL_TIMEOUT,
    )
    .idle_timeout(UNINSTALL_IDLE_TIMEOUT)
}

pub fn mise_uninstall_tool_request(
    task_id: Option<TaskId>,
    name: &str,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let target = match version.map(str::trim).filter(|value| !value.is_empty()) {
        Some(version) => format!("{name}@{version}"),
        None => name.to_string(),
    };
    mise_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(MISE_COMMAND).args(["uninstall", "--yes", target.as_str()]),
        UNINSTALL_TIMEOUT,
    )
    .idle_timeout(UNINSTALL_IDLE_TIMEOUT)
}

fn mise_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Mise, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_mise_version(output: &str) -> Option<String> {
    // Old format: "mise 2026.2.6 macos-x64" -> "2026.2.6"
    // New format: "2026.2.6 macos-x64 (2026-02-07)" -> "2026.2.6"
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let candidate = line.strip_prefix("mise ").unwrap_or(line);
    let version = candidate.split_whitespace().next()?;
    if version.is_empty() || !version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(version.to_owned())
}

fn parse_install_source(version: Option<&str>) -> AdapterResult<MiseInstallSource> {
    let Some(version) = version.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(MiseInstallSource::OfficialDownload);
    };
    if version.eq_ignore_ascii_case("scriptInstaller:officialDownload")
        || version.eq_ignore_ascii_case("officialDownload")
    {
        return Ok(MiseInstallSource::OfficialDownload);
    }
    if let Some(path) = version.strip_prefix("scriptInstaller:existingBinaryPath:") {
        let path = path.trim();
        if path.is_empty() {
            return Err(CoreError {
                manager: Some(ManagerId::Mise),
                task: Some(TaskType::Install),
                action: Some(ManagerAction::Install),
                kind: CoreErrorKind::InvalidInput,
                message: "mise existingBinaryPath install source requires a non-empty path"
                    .to_string(),
            });
        }
        return Ok(MiseInstallSource::ExistingBinaryPath(PathBuf::from(path)));
    }

    Err(CoreError {
        manager: Some(ManagerId::Mise),
        task: Some(TaskType::Install),
        action: Some(ManagerAction::Install),
        kind: CoreErrorKind::InvalidInput,
        message: format!("unsupported mise install source: {version}"),
    })
}

fn parse_package_target(
    package_name: &str,
    version: Option<&str>,
    action: ManagerAction,
) -> AdapterResult<(String, Option<String>)> {
    let normalized_package_name = package_name.trim();
    if normalized_package_name.is_empty() {
        return Err(CoreError {
            manager: Some(ManagerId::Mise),
            task: Some(task_type_for_action(action)),
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "mise package {} requires a non-empty package name",
                package_action_label(action)
            ),
        });
    }

    let explicit_version = version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let coordinate = PackageCoordinate::parse(normalized_package_name).ok_or(CoreError {
        manager: Some(ManagerId::Mise),
        task: Some(task_type_for_action(action)),
        action: Some(action),
        kind: CoreErrorKind::InvalidInput,
        message: format!("invalid mise package identifier: {normalized_package_name}"),
    })?;
    let coordinate_version = coordinate
        .version_selector
        .map(|selector| selector.raw.trim().to_string())
        .filter(|value| !value.is_empty());

    if let (Some(coordinate_version), Some(explicit_version)) =
        (coordinate_version.as_ref(), explicit_version.as_ref())
        && coordinate_version != explicit_version
    {
        return Err(CoreError {
            manager: Some(ManagerId::Mise),
            task: Some(task_type_for_action(action)),
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "conflicting mise version selectors '{coordinate_version}' and '{explicit_version}'"
            ),
        });
    }

    let tool_name = coordinate.package_name.trim().to_string();
    let requested_version = explicit_version.or(coordinate_version);
    Ok((tool_name, requested_version))
}

fn parse_package_install_target(
    package_name: &str,
    version: Option<&str>,
) -> AdapterResult<(String, Option<String>)> {
    parse_package_target(package_name, version, ManagerAction::Install)
}

fn parse_package_uninstall_target(package_name: &str) -> AdapterResult<(String, Option<String>)> {
    parse_package_target(package_name, None, ManagerAction::Uninstall)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MiseUninstallSpec {
    mode: MiseUninstallMode,
    remove_shell_setup: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedMiseInstalledTarget {
    tool_name: String,
    version: String,
    runtime_state: PackageRuntimeState,
}

fn parse_uninstall_mode(package_name: &str) -> AdapterResult<MiseUninstallSpec> {
    let (base_name, remove_shell_setup) =
        crate::manager_lifecycle::strip_shell_setup_cleanup_suffix(package_name);
    let mode = match base_name.trim() {
        "__self__" => MiseUninstallMode::ManagerOnlyKeepConfig,
        "__self__:fullCleanup:keepConfig" => MiseUninstallMode::FullCleanupKeepConfig,
        "__self__:fullCleanup:removeConfig" => MiseUninstallMode::FullCleanupRemoveConfig,
        other => Err(CoreError {
            manager: Some(ManagerId::Mise),
            task: Some(TaskType::Uninstall),
            action: Some(ManagerAction::Uninstall),
            kind: CoreErrorKind::InvalidInput,
            message: format!("unsupported mise uninstall mode: {other}"),
        })?,
    };
    Ok(MiseUninstallSpec {
        mode,
        remove_shell_setup,
    })
}

#[derive(Debug, Deserialize)]
struct MiseInstalledSource {
    #[serde(default, rename = "type")]
    source_type: Option<String>,
    #[serde(default)]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MiseInstalledEntry {
    version: String,
    installed: bool,
    #[serde(default)]
    source: Option<MiseInstalledSource>,
    #[serde(default)]
    active: bool,
}

fn parse_mise_installed(json: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let home_dir = std::env::var_os("HOME").map(PathBuf::from);
    parse_mise_installed_with_home(json, home_dir.as_deref())
}

fn parse_mise_installed_with_home(
    json: &str,
    home_dir: Option<&Path>,
) -> AdapterResult<Vec<InstalledPackage>> {
    let tools: HashMap<String, Vec<MiseInstalledEntry>> = serde_json::from_str(json)
        .map_err(|e| parse_error(&format!("invalid mise ls JSON: {e}")))?;

    let mut packages = Vec::new();
    for (tool_name, entries) in &tools {
        for entry in entries {
            if !entry.installed {
                continue;
            }
            packages.push(InstalledPackage {
                package: PackageRef {
                    manager: ManagerId::Mise,
                    name: tool_name.clone(),
                },
                installed_version: Some(entry.version.clone()),
                pinned: false,
                runtime_state: classify_mise_runtime_state(
                    entry.active,
                    entry.source.as_ref(),
                    home_dir,
                ),
            });
        }
    }

    // Sort for deterministic output
    packages.sort_by(|a, b| {
        a.package
            .name
            .cmp(&b.package.name)
            .then_with(|| a.installed_version.cmp(&b.installed_version))
    });

    Ok(packages)
}

fn classify_mise_runtime_state(
    active: bool,
    source: Option<&MiseInstalledSource>,
    home_dir: Option<&Path>,
) -> PackageRuntimeState {
    if !active {
        return PackageRuntimeState::default();
    }

    let mut runtime_state = PackageRuntimeState {
        is_active: true,
        ..Default::default()
    };

    let Some(source) = source else {
        return runtime_state;
    };
    let Some(path) = source
        .path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return runtime_state;
    };

    if mise_source_path_is_default(path, source.source_type.as_deref(), home_dir) {
        runtime_state.is_default = true;
        return runtime_state;
    }

    runtime_state.has_override = true;
    runtime_state
}

fn mise_source_path_is_default(
    path: &str,
    source_type: Option<&str>,
    home_dir: Option<&Path>,
) -> bool {
    let path_obj = Path::new(path);
    if let Some(home_dir) = home_dir
        && (path_obj == home_dir.join(".config/mise/config.toml")
            || path_obj == home_dir.join(".tool-versions"))
    {
        return true;
    }

    let normalized_source_type = source_type
        .map(str::trim)
        .unwrap_or_default()
        .trim_start_matches('.');
    let normalized_path = path.replace('\\', "/");
    normalized_source_type.eq_ignore_ascii_case("mise.toml")
        && normalized_path.ends_with("/.config/mise/config.toml")
}

#[derive(Debug, Deserialize)]
struct MiseOutdatedEntry {
    current: String,
    latest: String,
}

fn parse_mise_outdated(json: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let tools: HashMap<String, MiseOutdatedEntry> = serde_json::from_str(json)
        .map_err(|e| parse_error(&format!("invalid mise outdated JSON: {e}")))?;

    let mut packages: Vec<OutdatedPackage> = tools
        .into_iter()
        .map(|(tool_name, entry)| OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Mise,
                name: tool_name,
            },
            installed_version: Some(entry.current),
            candidate_version: entry.latest,
            pinned: false,
            restart_required: false,
            runtime_state: Default::default(),
        })
        .collect();

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));

    Ok(packages)
}

fn hydrate_mise_outdated_runtime_state(
    outdated: &mut [OutdatedPackage],
    installed: &[InstalledPackage],
) {
    let mut runtime_state_by_key = HashMap::<(String, String), PackageRuntimeState>::new();
    for package in installed {
        let Some(installed_version) = package.installed_version.as_deref() else {
            continue;
        };
        runtime_state_by_key.insert(
            (
                package.package.name.to_ascii_lowercase(),
                installed_version.to_string(),
            ),
            package.runtime_state.clone(),
        );
    }

    for package in outdated {
        let Some(installed_version) = package.installed_version.as_deref() else {
            continue;
        };
        let key = (
            package.package.name.to_ascii_lowercase(),
            installed_version.to_string(),
        );
        if let Some(runtime_state) = runtime_state_by_key.get(&key) {
            package.runtime_state = runtime_state.clone();
        }
    }
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
        ManagerAction::Pin => TaskType::Pin,
        ManagerAction::Unpin => TaskType::Unpin,
        ManagerAction::Configure => TaskType::Refresh,
    }
}

fn package_action_label(action: ManagerAction) -> &'static str {
    match action {
        ManagerAction::Install => "install",
        ManagerAction::Uninstall => "uninstall",
        ManagerAction::Upgrade => "upgrade",
        _ => "mutation",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiseRemotePackage {
    pub name: String,
    pub latest_version: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MiseRemoteEntry {
    tool: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MiseRemoteVersionCandidate {
    version: String,
    created_at: Option<String>,
    stable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiseRegistryPackage {
    pub name: String,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MiseRegistryEntry {
    #[serde(default)]
    short: Option<Value>,
    #[serde(default)]
    aliases: Vec<Value>,
    #[serde(default)]
    description: Option<String>,
}

pub(crate) fn parse_mise_remote_catalog(json: &str) -> AdapterResult<Vec<MiseRemotePackage>> {
    let parsed = parse_mise_remote_entries(json)?;
    let mut by_name_and_variant: BTreeMap<(String, String), Option<MiseRemoteVersionCandidate>> =
        BTreeMap::new();

    for entry in parsed {
        let name = entry.tool.trim();
        if name.is_empty() {
            continue;
        }

        let candidate = entry
            .version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|version| MiseRemoteVersionCandidate {
                version: version.to_string(),
                created_at: normalize_optional_text(entry.created_at.clone()),
                stable: looks_like_stable_mise_version(version),
            });

        let variant_key = candidate
            .as_ref()
            .and_then(|value| mise_variant_qualifier_key(value.version.as_str()))
            .unwrap_or_default();
        let current = by_name_and_variant
            .entry((name.to_string(), variant_key))
            .or_insert(None);
        if let Some(next) = candidate {
            let replace = match current.as_ref() {
                Some(existing) => should_replace_mise_remote_version(existing, &next),
                None => true,
            };
            if replace {
                *current = Some(next);
            }
        }
    }

    Ok(by_name_and_variant
        .into_iter()
        .map(|((name, _), selected)| MiseRemotePackage {
            name,
            latest_version: selected.map(|version| version.version),
            summary: None,
        })
        .collect())
}

fn parse_mise_remote_entries(json: &str) -> AdapterResult<Vec<MiseRemoteEntry>> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    match serde_json::from_str::<Vec<MiseRemoteEntry>>(json) {
        Ok(entries) => Ok(entries),
        Err(primary_error) => {
            let first_array = json.find('[');
            let last_array = json.rfind(']');
            if let (Some(start), Some(end)) = (first_array, last_array)
                && start <= end
            {
                let payload = &json[start..=end];
                return serde_json::from_str::<Vec<MiseRemoteEntry>>(payload).map_err(|error| {
                    parse_error(&format!(
                        "invalid mise ls-remote JSON: {primary_error}; fallback parse failed: {error}"
                    ))
                });
            }
            Err(parse_error(&format!(
                "invalid mise ls-remote JSON: {primary_error}"
            )))
        }
    }
}

pub(crate) fn parse_mise_registry_catalog(json: &str) -> AdapterResult<Vec<MiseRegistryPackage>> {
    let parsed = parse_mise_registry_entries(json)?;
    let mut by_name: BTreeMap<String, Option<String>> = BTreeMap::new();

    for entry in parsed {
        let summary = normalize_optional_text(entry.description);
        let mut names = Vec::new();
        if let Some(short) = entry.short.as_ref().and_then(parse_registry_name_value) {
            names.push(short);
        }
        for alias in entry.aliases {
            if let Some(alias_name) = parse_registry_name_value(&alias) {
                names.push(alias_name);
            }
        }

        for name in names {
            let current = by_name.entry(name).or_insert(None);
            if current.is_none() && summary.is_some() {
                *current = summary.clone();
            }
        }
    }

    Ok(by_name
        .into_iter()
        .map(|(name, summary)| MiseRegistryPackage { name, summary })
        .collect())
}

fn parse_mise_registry_entries(json: &str) -> AdapterResult<Vec<MiseRegistryEntry>> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    match serde_json::from_str::<Vec<MiseRegistryEntry>>(json) {
        Ok(entries) => Ok(entries),
        Err(primary_error) => {
            let first_array = json.find('[');
            let last_array = json.rfind(']');
            if let (Some(start), Some(end)) = (first_array, last_array)
                && start <= end
            {
                let payload = &json[start..=end];
                return serde_json::from_str::<Vec<MiseRegistryEntry>>(payload).map_err(|error| {
                    parse_error(&format!(
                        "invalid mise registry JSON: {primary_error}; fallback parse failed: {error}"
                    ))
                });
            }
            Err(parse_error(&format!(
                "invalid mise registry JSON: {primary_error}"
            )))
        }
    }
}

fn parse_registry_name_value(value: &Value) -> Option<String> {
    let Value::String(raw) = value else {
        return None;
    };
    let normalized = raw.trim();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized.to_string())
    }
}

fn should_replace_mise_remote_version(
    current: &MiseRemoteVersionCandidate,
    candidate: &MiseRemoteVersionCandidate,
) -> bool {
    if current.stable != candidate.stable {
        return candidate.stable;
    }

    match candidate.created_at.cmp(&current.created_at) {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => candidate.version > current.version,
    }
}

fn looks_like_stable_mise_version(version: &str) -> bool {
    version
        .strip_prefix('v')
        .unwrap_or(version)
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
}

fn mise_variant_qualifier_key(version: &str) -> Option<String> {
    let normalized_version = version.trim();
    if normalized_version.is_empty() {
        return None;
    }

    let selector = VersionSelector::parse(normalized_version);
    if selector.first_release_atom == Some(0) {
        return None;
    }
    let qualifier = selector
        .qualifier_atoms()
        .join("-")
        .trim()
        .to_ascii_lowercase();
    if qualifier.is_empty() {
        None
    } else {
        Some(qualifier)
    }
}

fn mise_package_identity_key(name: &str, version: Option<&str>) -> String {
    let normalized_name = name.trim().to_ascii_lowercase();
    if normalized_name.is_empty() {
        return String::new();
    }
    let qualifier = version.and_then(mise_variant_qualifier_key);
    if let Some(qualifier) = qualifier {
        format!("{normalized_name}@{qualifier}")
    } else {
        normalized_name
    }
}

fn mise_query_identity_key(query: &str) -> String {
    let trimmed_query = query.trim();
    if trimmed_query.is_empty() {
        return String::new();
    }

    let parsed = PackageCoordinate::parse(trimmed_query);
    let Some(parsed) = parsed else {
        return trimmed_query.to_ascii_lowercase();
    };

    let normalized_name = parsed.package_name.trim().to_ascii_lowercase();
    if normalized_name.is_empty() {
        return String::new();
    }

    let qualifier = parsed.version_selector.and_then(|selector| {
        if selector.first_release_atom == Some(0) {
            return None;
        }
        let qualifier = selector
            .qualifier_atoms()
            .join("-")
            .trim()
            .to_ascii_lowercase();
        if qualifier.is_empty() {
            None
        } else {
            Some(qualifier)
        }
    });

    if let Some(qualifier) = qualifier {
        format!("{normalized_name}@{qualifier}")
    } else {
        normalized_name
    }
}

fn filter_mise_remote_packages(
    packages: &[MiseRemotePackage],
    query: &SearchQuery,
) -> Vec<CachedSearchResult> {
    let needle = query.text.trim().to_ascii_lowercase();
    let normalized_query_identity = mise_query_identity_key(query.text.as_str());
    let mut results = Vec::new();

    for package in packages {
        if package.name.is_empty() {
            continue;
        }
        if !needle.is_empty() {
            let name_matches = package.name.to_ascii_lowercase().contains(needle.as_str());
            let identity_key =
                mise_package_identity_key(package.name.as_str(), package.latest_version.as_deref());
            let identity_matches = !normalized_query_identity.is_empty()
                && identity_key.contains(normalized_query_identity.as_str());
            if !name_matches && !identity_matches {
                continue;
            }
        }
        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Mise,
                    name: package.name.clone(),
                },
                version: package.latest_version.clone(),
                summary: package.summary.clone(),
            },
            source_manager: ManagerId::Mise,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        });
    }

    results.sort_by(|left, right| {
        left.result
            .package
            .name
            .to_ascii_lowercase()
            .cmp(&right.result.package.name.to_ascii_lowercase())
            .then_with(|| left.result.version.cmp(&right.result.version))
    });
    results
}

fn parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Mise),
        task: None,
        action: None,
        kind: CoreErrorKind::ParseFailure,
        message: message.to_string(),
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::UNIX_EPOCH;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter, SearchRequest,
    };
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, SearchQuery, TaskId, TaskType};

    use super::{
        MiseAdapter, MiseDetectOutput, MiseSource, MiseUninstallMode, mise_detect_request,
        mise_download_install_script_request, mise_implode_request, mise_install_tool_request,
        mise_list_installed_request, mise_list_outdated_request, mise_list_remote_request,
        mise_registry_request, mise_run_downloaded_install_script_request,
        mise_uninstall_tool_request, mise_upgrade_request, parse_install_source,
        parse_mise_installed, parse_mise_installed_with_home, parse_mise_outdated,
        parse_mise_registry_catalog, parse_mise_remote_catalog, parse_mise_version,
        parse_package_install_target, parse_package_uninstall_target, parse_uninstall_mode,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/mise/version.txt");
    const INSTALLED_FIXTURE: &str = include_str!("../../tests/fixtures/mise/ls_json.txt");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/mise/outdated_json.txt");
    const REMOTE_FIXTURE: &str = include_str!("../../tests/fixtures/mise/ls_remote_all_json.txt");
    const REGISTRY_FIXTURE: &str = r#"
[
  {"short":"python","description":"python language","aliases":["python3"]},
  {"short":"java","description":"jdk java"},
  {"short":"jq","description":"JSON processor"}
]
"#;

    #[test]
    fn parses_mise_version_from_standard_banner() {
        let version = parse_mise_version("mise 2026.2.6 macos-x64\n");
        assert_eq!(version.as_deref(), Some("2026.2.6"));
    }

    #[test]
    fn parses_mise_version_from_new_format() {
        let version = parse_mise_version("2026.2.6 macos-x64 (2026-02-07)\n");
        assert_eq!(version.as_deref(), Some("2026.2.6"));
    }

    #[test]
    fn parses_mise_version_from_fixture() {
        let version = parse_mise_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("2026.2.6"));
    }

    #[test]
    fn version_parse_returns_none_for_empty_input() {
        assert!(parse_mise_version("").is_none());
        assert!(parse_mise_version("   \n  ").is_none());
    }

    #[test]
    fn version_parse_returns_none_for_unrecognized_format() {
        assert!(parse_mise_version("not-mise output").is_none());
    }

    #[test]
    fn parses_installed_from_fixture() {
        let packages =
            parse_mise_installed_with_home(INSTALLED_FIXTURE, Some(Path::new("/Users/dev")))
                .unwrap();
        assert_eq!(packages.len(), 4); // node, python 3.12.3, python 3.11.9, go
        assert_eq!(packages[0].package.name, "go");
        assert_eq!(packages[0].installed_version.as_deref(), Some("1.22.4"));
        assert!(packages[0].runtime_state.is_active);
        assert!(!packages[0].runtime_state.is_default);
        assert!(packages[0].runtime_state.has_override);
        assert_eq!(packages[1].package.name, "node");
        assert_eq!(packages[1].installed_version.as_deref(), Some("22.5.1"));
        assert!(packages[1].runtime_state.is_active);
        assert!(!packages[1].runtime_state.is_default);
        assert!(packages[1].runtime_state.has_override);
        // python entries sorted by version
        assert_eq!(packages[2].package.name, "python");
        assert_eq!(packages[2].installed_version.as_deref(), Some("3.11.9"));
        assert!(packages[2].runtime_state.is_empty());
        assert_eq!(packages[3].package.name, "python");
        assert_eq!(packages[3].installed_version.as_deref(), Some("3.12.3"));
        assert!(packages[3].runtime_state.is_active);
        assert!(packages[3].runtime_state.is_default);
        assert!(!packages[3].runtime_state.has_override);
    }

    #[test]
    fn parses_empty_installed_json() {
        let packages = parse_mise_installed("{}").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn returns_parse_error_for_invalid_installed_json() {
        let error = parse_mise_installed("not json").unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
        assert_eq!(error.manager, Some(ManagerId::Mise));
    }

    #[test]
    fn parses_outdated_from_fixture() {
        let packages = parse_mise_outdated(OUTDATED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].package.name, "node");
        assert_eq!(packages[0].installed_version.as_deref(), Some("22.5.1"));
        assert_eq!(packages[0].candidate_version, "22.12.0");
        assert_eq!(packages[1].package.name, "python");
        assert_eq!(packages[1].installed_version.as_deref(), Some("3.12.3"));
        assert_eq!(packages[1].candidate_version, "3.12.8");
    }

    #[test]
    fn outdated_runtime_state_is_hydrated_from_installed_snapshot() {
        let mut outdated = parse_mise_outdated(OUTDATED_FIXTURE).expect("outdated should parse");
        let installed =
            parse_mise_installed_with_home(INSTALLED_FIXTURE, Some(Path::new("/Users/dev")))
                .expect("installed should parse");

        super::hydrate_mise_outdated_runtime_state(&mut outdated, installed.as_slice());

        assert!(outdated[0].runtime_state.is_active);
        assert!(outdated[0].runtime_state.has_override);
        assert!(!outdated[0].runtime_state.is_default);
        assert!(outdated[1].runtime_state.is_active);
        assert!(outdated[1].runtime_state.is_default);
        assert!(!outdated[1].runtime_state.has_override);
    }

    #[test]
    fn parses_empty_outdated_json() {
        let packages = parse_mise_outdated("{}").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn returns_parse_error_for_invalid_outdated_json() {
        let error = parse_mise_outdated("not json").unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::ParseFailure);
        assert_eq!(error.manager, Some(ManagerId::Mise));
    }

    #[test]
    fn parses_remote_catalog_from_fixture() {
        let packages = parse_mise_remote_catalog(REMOTE_FIXTURE).unwrap();
        assert_eq!(packages.len(), 6);

        let node_versions = packages
            .iter()
            .filter(|package| package.name == "node")
            .map(|package| package.latest_version.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(node_versions, vec!["22.10.0".to_string()]);
        assert!(
            packages.iter().all(|package| package.summary.is_none()),
            "remote catalog should not inject summaries on its own"
        );

        let python_versions = packages
            .iter()
            .filter(|package| package.name == "python")
            .map(|package| package.latest_version.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(python_versions, vec!["3.13.0".to_string()]);

        let rust_versions = packages
            .iter()
            .filter(|package| package.name == "rust")
            .map(|package| package.latest_version.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(rust_versions.len(), 2);
        assert!(rust_versions.contains(&"1.83.0".to_string()));
        assert!(rust_versions.contains(&"beta".to_string()));

        let zig_versions = packages
            .iter()
            .filter(|package| package.name == "zig")
            .map(|package| package.latest_version.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(zig_versions.len(), 2);
        assert!(zig_versions.contains(&"0.14.1".to_string()));
        assert!(zig_versions.contains(&"master".to_string()));
    }

    #[test]
    fn parses_remote_catalog_with_prefixed_warning_lines() {
        let raw = format!("mise WARN cache issue\n{REMOTE_FIXTURE}");
        let packages = parse_mise_remote_catalog(raw.as_str()).unwrap();
        assert!(!packages.is_empty());
        let zig_versions = packages
            .iter()
            .filter(|package| package.name == "zig")
            .map(|package| package.latest_version.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert!(zig_versions.contains(&"0.14.1".to_string()));
        assert!(zig_versions.contains(&"master".to_string()));
    }

    #[test]
    fn parses_empty_remote_catalog_as_empty_list() {
        let packages = parse_mise_remote_catalog("   ").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn parses_registry_catalog_from_fixture() {
        let packages = parse_mise_registry_catalog(REGISTRY_FIXTURE).unwrap();
        let by_name = packages
            .iter()
            .map(|package| (package.name.as_str(), package.summary.as_deref()))
            .collect::<HashMap<_, _>>();

        assert_eq!(by_name.get("python"), Some(&Some("python language")));
        assert_eq!(by_name.get("python3"), Some(&Some("python language")));
        assert_eq!(by_name.get("java"), Some(&Some("jdk java")));
        assert_eq!(by_name.get("jq"), Some(&Some("JSON processor")));
    }

    #[test]
    fn parses_registry_catalog_with_prefixed_warning_lines() {
        let raw = format!("mise WARN stale lockfile\n{REGISTRY_FIXTURE}");
        let packages = parse_mise_registry_catalog(raw.as_str()).unwrap();
        assert!(!packages.is_empty());
        assert!(packages.iter().any(|package| package.name == "python"));
    }

    #[test]
    fn parses_empty_registry_catalog_as_empty_list() {
        let packages = parse_mise_registry_catalog(" ").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn parses_remote_catalog_with_distinct_qualifier_variants() {
        let raw = r#"
[
  {"tool":"python","version":"3.13.2","created_at":"2026-01-10T00:00:00Z"},
  {"tool":"python","version":"mambaforge-24.11.0-1","created_at":"2026-01-11T00:00:00Z"},
  {"tool":"python","version":"mambaforge-24.12.0-1","created_at":"2026-02-11T00:00:00Z"},
  {"tool":"java","version":"8.92.0.21","created_at":"2026-01-09T00:00:00Z"},
  {"tool":"java","version":"zulu-jre-javafx-8.92.0.21","created_at":"2026-01-12T00:00:00Z"}
]
"#;
        let packages = parse_mise_remote_catalog(raw).unwrap();

        let python_versions = packages
            .iter()
            .filter(|package| package.name == "python")
            .map(|package| package.latest_version.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(python_versions.len(), 2);
        assert!(python_versions.contains(&"3.13.2".to_string()));
        assert!(python_versions.contains(&"mambaforge-24.12.0-1".to_string()));

        let java_versions = packages
            .iter()
            .filter(|package| package.name == "java")
            .map(|package| package.latest_version.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(java_versions.len(), 2);
        assert!(java_versions.contains(&"8.92.0.21".to_string()));
        assert!(java_versions.contains(&"zulu-jre-javafx-8.92.0.21".to_string()));
    }

    #[test]
    fn adapter_executes_supported_read_only_requests() {
        let source = FixtureSource::default();
        let adapter = MiseAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let installed = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();
        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        assert!(matches!(detect, AdapterResponse::Detection(_)));
        assert!(matches!(installed, AdapterResponse::InstalledPackages(_)));
        assert!(matches!(outdated, AdapterResponse::OutdatedPackages(_)));
    }

    #[test]
    fn adapter_executes_search_request() {
        let source = FixtureSource::default();
        let adapter = MiseAdapter::new(source);

        let response = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "zi".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .expect("search should succeed");

        let AdapterResponse::SearchResults(results) = response else {
            panic!("expected search results response");
        };
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|result| result.result.package.name == "zig")
        );
        let versions = results
            .iter()
            .map(|result| result.result.version.clone().unwrap_or_default())
            .collect::<Vec<_>>();
        assert!(versions.contains(&"0.14.1".to_string()));
        assert!(versions.contains(&"master".to_string()));
    }

    #[test]
    fn filter_remote_packages_matches_qualifier_queries() {
        let packages = vec![
            super::MiseRemotePackage {
                name: "python".to_string(),
                latest_version: Some("3.13.2".to_string()),
                summary: None,
            },
            super::MiseRemotePackage {
                name: "python".to_string(),
                latest_version: Some("mambaforge-24.12.0-1".to_string()),
                summary: None,
            },
        ];

        let results = super::filter_mise_remote_packages(
            packages.as_slice(),
            &SearchQuery {
                text: "python@mambaforge-24.11.0-1".to_string(),
                issued_at: UNIX_EPOCH,
            },
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "python");
        assert_eq!(
            results[0].result.version.as_deref(),
            Some("mambaforge-24.12.0-1")
        );
    }

    #[test]
    fn parses_remote_catalog_with_digit_suffix_and_release_less_qualifiers() {
        let raw = r#"
[
  {"tool":"python","version":"3.13.2","created_at":"2026-01-10T00:00:00Z"},
  {"tool":"python","version":"anaconda3-2024.10-1","created_at":"2026-01-11T00:00:00Z"},
  {"tool":"python","version":"mambaforge","created_at":"2026-01-12T00:00:00Z"}
]
"#;
        let packages = parse_mise_remote_catalog(raw).unwrap();
        let python_versions = packages
            .iter()
            .filter(|package| package.name == "python")
            .map(|package| package.latest_version.clone().unwrap_or_default())
            .collect::<Vec<_>>();

        assert_eq!(python_versions.len(), 3);
        assert!(python_versions.contains(&"3.13.2".to_string()));
        assert!(python_versions.contains(&"anaconda3-2024.10-1".to_string()));
        assert!(python_versions.contains(&"mambaforge".to_string()));
    }

    #[test]
    fn filter_remote_packages_matches_qualifier_with_digit_suffix() {
        let packages = vec![
            super::MiseRemotePackage {
                name: "python".to_string(),
                latest_version: Some("3.13.2".to_string()),
                summary: None,
            },
            super::MiseRemotePackage {
                name: "python".to_string(),
                latest_version: Some("anaconda3-2024.10-1".to_string()),
                summary: None,
            },
            super::MiseRemotePackage {
                name: "python".to_string(),
                latest_version: Some("mambaforge".to_string()),
                summary: None,
            },
        ];

        let anaconda_results = super::filter_mise_remote_packages(
            packages.as_slice(),
            &SearchQuery {
                text: "python@anaconda3-2024.2.0".to_string(),
                issued_at: UNIX_EPOCH,
            },
        );
        assert_eq!(anaconda_results.len(), 1);
        assert_eq!(
            anaconda_results[0].result.version.as_deref(),
            Some("anaconda3-2024.10-1")
        );

        let mambaforge_results = super::filter_mise_remote_packages(
            packages.as_slice(),
            &SearchQuery {
                text: "python@mambaforge".to_string(),
                issued_at: UNIX_EPOCH,
            },
        );
        assert_eq!(mambaforge_results.len(), 1);
        assert_eq!(
            mambaforge_results[0].result.version.as_deref(),
            Some("mambaforge")
        );
    }

    #[test]
    fn adapter_executes_install_request() {
        let source = FixtureSource::default();
        let adapter = MiseAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Mise,
                    name: "__self__".to_string(),
                },
                version: Some("scriptInstaller:officialDownload".to_string()),
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
        assert_eq!(
            adapter.source.install_self_calls.load(Ordering::SeqCst),
            1,
            "manager self install should use install_self"
        );
        assert_eq!(
            adapter.source.install_tool_calls.load(Ordering::SeqCst),
            0,
            "manager self install must not use install_tool"
        );
    }

    #[test]
    fn adapter_executes_tool_install_request_from_coordinate_name() {
        let source = FixtureSource::default();
        let adapter = MiseAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Mise,
                    name: "java@zulu-jre-javafx".to_string(),
                },
                version: None,
            }))
            .unwrap();
        let AdapterResponse::Mutation(mutation) = result else {
            panic!("expected mutation response");
        };
        assert_eq!(mutation.package.name, "java");
        assert_eq!(
            mutation.after_version.as_deref(),
            Some("zulu-jre-javafx"),
            "coordinate qualifier should be used as install selector"
        );
        assert_eq!(
            adapter.source.install_self_calls.load(Ordering::SeqCst),
            0,
            "tool install must not use install_self"
        );
        assert_eq!(
            adapter.source.install_tool_calls.load(Ordering::SeqCst),
            1,
            "tool install should use install_tool"
        );
        assert_eq!(
            *adapter.source.last_install_tool_name.lock().unwrap(),
            Some("java".to_string())
        );
        assert_eq!(
            *adapter.source.last_install_tool_version.lock().unwrap(),
            Some("zulu-jre-javafx".to_string())
        );
    }

    #[test]
    fn adapter_executes_self_uninstall_request() {
        let source = FixtureSource::default();
        let adapter = MiseAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Uninstall(
                crate::adapters::UninstallRequest {
                    package: crate::models::PackageRef {
                        manager: ManagerId::Mise,
                        name: "__self__".to_string(),
                    },
                },
            ))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_executes_tool_uninstall_request_from_exact_version() {
        let source = FixtureSource::default();
        let adapter = MiseAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Uninstall(
                crate::adapters::UninstallRequest {
                    package: crate::models::PackageRef {
                        manager: ManagerId::Mise,
                        name: "python@3.12.3".to_string(),
                    },
                },
            ))
            .expect("tool uninstall should succeed");

        let AdapterResponse::Mutation(mutation) = result else {
            panic!("expected mutation response");
        };
        assert_eq!(mutation.package.name, "python");
        assert_eq!(mutation.before_version.as_deref(), Some("3.12.3"));
        assert_eq!(
            adapter.source.uninstall_tool_calls.load(Ordering::SeqCst),
            1,
            "tool uninstall should use uninstall_tool"
        );
        assert_eq!(
            *adapter.source.last_uninstall_tool_name.lock().unwrap(),
            Some("python".to_string())
        );
        assert_eq!(
            *adapter.source.last_uninstall_tool_version.lock().unwrap(),
            Some("3.12.3".to_string())
        );
    }

    #[test]
    fn adapter_executes_upgrade_request() {
        let source = FixtureSource::default();
        let adapter = MiseAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: Some(crate::models::PackageRef {
                    manager: ManagerId::Mise,
                    name: "node".to_string(),
                }),
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn detect_command_spec_uses_structured_args() {
        let request = mise_detect_request(Some(TaskId(42)));
        assert_eq!(request.manager, ManagerId::Mise);
        assert_eq!(request.task_id, Some(TaskId(42)));
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program, PathBuf::from("mise"));
        assert_eq!(request.command.args, vec!["--version".to_string()]);
        assert!(request.timeout.is_some());
    }

    #[test]
    fn list_command_specs_use_structured_args() {
        let installed = mise_list_installed_request(None);
        assert_eq!(
            installed.command.args,
            vec!["ls".to_string(), "--json".to_string()]
        );
        assert_eq!(installed.action, ManagerAction::ListInstalled);
        assert_eq!(installed.task_type, TaskType::Refresh);

        let outdated = mise_list_outdated_request(None);
        assert_eq!(
            outdated.command.args,
            vec!["outdated".to_string(), "--json".to_string()]
        );
        assert_eq!(outdated.action, ManagerAction::ListOutdated);
        assert_eq!(outdated.task_type, TaskType::Refresh);

        let remote = mise_list_remote_request(None);
        assert_eq!(
            remote.command.args,
            vec![
                "ls-remote".to_string(),
                "--all".to_string(),
                "--json".to_string()
            ]
        );
        assert_eq!(remote.action, ManagerAction::Search);
        assert_eq!(remote.task_type, TaskType::Search);

        let registry = mise_registry_request(None);
        assert_eq!(
            registry.command.args,
            vec!["registry".to_string(), "--json".to_string()]
        );
        assert_eq!(registry.action, ManagerAction::Search);
        assert_eq!(registry.task_type, TaskType::Search);
    }

    #[test]
    fn upgrade_command_spec_uses_structured_args() {
        let package_upgrade = mise_upgrade_request(Some(TaskId(99)), "node");
        assert_eq!(package_upgrade.task_id, Some(TaskId(99)));
        assert_eq!(
            package_upgrade.command.args,
            vec!["upgrade".to_string(), "node".to_string()]
        );
        assert_eq!(package_upgrade.task_type, TaskType::Upgrade);
        assert_eq!(package_upgrade.action, ManagerAction::Upgrade);

        let all_upgrade = mise_upgrade_request(None, "__all__");
        assert_eq!(all_upgrade.command.args, vec!["upgrade".to_string()]);
        assert_eq!(all_upgrade.task_type, TaskType::Upgrade);
        assert_eq!(all_upgrade.action, ManagerAction::Upgrade);
    }

    #[test]
    fn install_command_specs_use_structured_args() {
        let download = mise_download_install_script_request(Some(TaskId(77)), "/tmp/mise.sh");
        assert_eq!(download.task_type, TaskType::Install);
        assert_eq!(download.action, ManagerAction::Install);
        assert_eq!(
            download.command.args,
            vec![
                "-fsSL".to_string(),
                "https://mise.run".to_string(),
                "-o".to_string(),
                "/tmp/mise.sh".to_string()
            ]
        );

        let run = mise_run_downloaded_install_script_request(None, "/tmp/mise.sh");
        assert_eq!(run.task_type, TaskType::Install);
        assert_eq!(run.action, ManagerAction::Install);
        assert_eq!(run.command.args, vec!["/tmp/mise.sh".to_string()]);

        let install_without_version = mise_install_tool_request(None, "java", None);
        assert_eq!(
            install_without_version.command.args,
            vec![
                "use".to_string(),
                "--global".to_string(),
                "java".to_string()
            ]
        );
        assert_eq!(install_without_version.task_type, TaskType::Install);
        assert_eq!(install_without_version.action, ManagerAction::Install);

        let install_with_version = mise_install_tool_request(None, "java", Some("zulu-jre-javafx"));
        assert_eq!(
            install_with_version.command.args,
            vec![
                "use".to_string(),
                "--global".to_string(),
                "java@zulu-jre-javafx".to_string()
            ]
        );
        assert_eq!(install_with_version.task_type, TaskType::Install);
        assert_eq!(install_with_version.action, ManagerAction::Install);
    }

    #[test]
    fn uninstall_command_spec_uses_structured_args() {
        let keep = mise_implode_request(Some(TaskId(88)), false);
        assert_eq!(keep.task_type, TaskType::Uninstall);
        assert_eq!(keep.action, ManagerAction::Uninstall);
        assert_eq!(
            keep.command.args,
            vec!["implode".to_string(), "--yes".to_string()]
        );

        let remove_config = mise_implode_request(None, true);
        assert_eq!(
            remove_config.command.args,
            vec![
                "implode".to_string(),
                "--yes".to_string(),
                "--config".to_string()
            ]
        );

        let uninstall_tool = mise_uninstall_tool_request(None, "python", Some("3.12.3"));
        assert_eq!(
            uninstall_tool.command.args,
            vec![
                "uninstall".to_string(),
                "--yes".to_string(),
                "python@3.12.3".to_string()
            ]
        );
    }

    #[test]
    fn parse_install_source_defaults_to_official_download() {
        assert_eq!(
            parse_install_source(None).expect("default source should parse"),
            super::MiseInstallSource::OfficialDownload
        );
        assert_eq!(
            parse_install_source(Some("scriptInstaller:officialDownload"))
                .expect("source should parse"),
            super::MiseInstallSource::OfficialDownload
        );
    }

    #[test]
    fn parse_package_install_target_supports_coordinate_input() {
        let (name, version) = parse_package_install_target("java@zulu-jre-javafx", None)
            .expect("target should parse");
        assert_eq!(name, "java");
        assert_eq!(version.as_deref(), Some("zulu-jre-javafx"));
    }

    #[test]
    fn parse_package_install_target_rejects_conflicting_version_selectors() {
        let error = parse_package_install_target("java@zulu-jre-javafx", Some("corretto"))
            .expect_err("conflicting selectors should fail");
        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
        assert!(
            error.message.contains("conflicting mise version selectors"),
            "error should mention conflicting selectors"
        );
    }

    #[test]
    fn parse_package_uninstall_target_supports_exact_version_input() {
        let (name, version) =
            parse_package_uninstall_target("python@3.12.3").expect("target should parse");
        assert_eq!(name, "python");
        assert_eq!(version.as_deref(), Some("3.12.3"));
    }

    #[test]
    fn parse_uninstall_mode_supports_known_modes() {
        assert_eq!(
            parse_uninstall_mode("__self__")
                .expect("mode should parse")
                .mode,
            MiseUninstallMode::ManagerOnlyKeepConfig
        );
        assert_eq!(
            parse_uninstall_mode("__self__:fullCleanup:keepConfig")
                .expect("mode should parse")
                .mode,
            MiseUninstallMode::FullCleanupKeepConfig
        );
        assert_eq!(
            parse_uninstall_mode("__self__:fullCleanup:removeConfig")
                .expect("mode should parse")
                .mode,
            MiseUninstallMode::FullCleanupRemoveConfig
        );
        assert!(
            parse_uninstall_mode("__self__:removeShellSetup")
                .expect("mode should parse")
                .remove_shell_setup
        );
    }

    #[derive(Default, Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
        install_self_calls: Arc<AtomicUsize>,
        install_tool_calls: Arc<AtomicUsize>,
        last_install_tool_name: Arc<Mutex<Option<String>>>,
        last_install_tool_version: Arc<Mutex<Option<String>>>,
        uninstall_tool_calls: Arc<AtomicUsize>,
        last_uninstall_tool_name: Arc<Mutex<Option<String>>>,
        last_uninstall_tool_version: Arc<Mutex<Option<String>>>,
    }

    impl MiseSource for FixtureSource {
        fn detect(&self) -> AdapterResult<MiseDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(MiseDetectOutput {
                executable_path: Some(PathBuf::from("/Users/test/.local/bin/mise")),
                version_output: VERSION_FIXTURE.to_string(),
            })
        }

        fn list_installed(&self) -> AdapterResult<String> {
            Ok(INSTALLED_FIXTURE.to_string())
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            Ok(OUTDATED_FIXTURE.to_string())
        }

        fn list_remote_packages(&self) -> AdapterResult<Vec<super::MiseRemotePackage>> {
            parse_mise_remote_catalog(REMOTE_FIXTURE)
        }

        fn install_self(&self, _source: super::MiseInstallSource) -> AdapterResult<String> {
            self.install_self_calls.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }

        fn install_tool(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
            self.install_tool_calls.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut last_name) = self.last_install_tool_name.lock() {
                *last_name = Some(name.to_string());
            }
            if let Ok(mut last_version) = self.last_install_tool_version.lock() {
                *last_version = version.map(ToOwned::to_owned);
            }
            Ok(String::new())
        }

        fn uninstall_tool(&self, name: &str, version: Option<&str>) -> AdapterResult<String> {
            self.uninstall_tool_calls.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut last_name) = self.last_uninstall_tool_name.lock() {
                *last_name = Some(name.to_string());
            }
            if let Ok(mut last_version) = self.last_uninstall_tool_version.lock() {
                *last_version = version.map(ToOwned::to_owned);
            }
            Ok(String::new())
        }

        fn self_uninstall(&self, _mode: MiseUninstallMode) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn upgrade_tool(&self, _name: &str) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
