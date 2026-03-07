use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter, RustupAddComponentRequest,
    RustupAddTargetRequest, RustupRemoveComponentRequest, RustupRemoveTargetRequest,
    RustupSetDefaultToolchainRequest, RustupSetOverrideRequest, RustupSetProfileRequest,
    RustupUnsetOverrideRequest,
};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, PackageRuntimeState, SearchQuery,
    TaskId, TaskType,
};
use serde::Serialize;

const RUSTUP_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::Search,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const RUSTUP_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Rustup,
    display_name: "rustup",
    category: ManagerCategory::ToolRuntime,
    authority: ManagerAuthority::Authoritative,
    capabilities: RUSTUP_CAPABILITIES,
};

const RUSTUP_COMMAND: &str = "rustup";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const TOOLCHAIN_VERSION_TIMEOUT: Duration = Duration::from_secs(15);
const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const INSTALL_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(25 * 60);
const UNINSTALL_IDLE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RustupDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RustupInstallSource {
    OfficialDownload,
    ExistingBinaryPath(PathBuf),
}

pub trait RustupSource: Send + Sync {
    fn detect(&self) -> AdapterResult<RustupDetectOutput>;
    fn show(&self) -> AdapterResult<String>;
    fn toolchain_list(&self) -> AdapterResult<String>;
    fn component_list(&self, toolchain: &str) -> AdapterResult<String>;
    fn target_list(&self, toolchain: &str) -> AdapterResult<String>;
    fn override_list(&self) -> AdapterResult<String>;
    fn show_profile(&self) -> AdapterResult<String>;
    fn toolchain_version(&self, toolchain: &str) -> AdapterResult<String>;
    fn check(&self) -> AdapterResult<String>;
    fn install_self(&self, source: RustupInstallSource) -> AdapterResult<String>;
    fn install_toolchain(&self, toolchain: &str) -> AdapterResult<String>;
    fn uninstall_toolchain(&self, toolchain: &str) -> AdapterResult<String>;
    fn update_toolchain(&self, toolchain: &str) -> AdapterResult<String>;
    fn add_component(&self, toolchain: &str, component: &str) -> AdapterResult<String>;
    fn remove_component(&self, toolchain: &str, component: &str) -> AdapterResult<String>;
    fn add_target(&self, toolchain: &str, target: &str) -> AdapterResult<String>;
    fn remove_target(&self, toolchain: &str, target: &str) -> AdapterResult<String>;
    fn set_default_toolchain(&self, toolchain: &str) -> AdapterResult<String>;
    fn set_override(&self, toolchain: &str, path: &std::path::Path) -> AdapterResult<String>;
    fn unset_override(&self, path: &std::path::Path) -> AdapterResult<String>;
    fn set_profile(&self, profile: &str) -> AdapterResult<String>;
    fn self_uninstall(&self) -> AdapterResult<String>;
    fn self_update(&self) -> AdapterResult<String>;
}

pub struct RustupAdapter<S: RustupSource> {
    source: S,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RustupToolchainDetailEntry {
    pub name: String,
    pub installed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RustupToolchainDetail {
    pub toolchain: String,
    pub current_profile: Option<String>,
    pub override_paths: Vec<String>,
    pub components: Vec<RustupToolchainDetailEntry>,
    pub targets: Vec<RustupToolchainDetailEntry>,
}

impl<S: RustupSource> RustupAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: RustupSource> ManagerAdapter for RustupAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &RUSTUP_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_rustup_version(&output.version_output);
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
            AdapterRequest::Search(search_request) => {
                let default_host = resolve_rustup_search_default_host(&self.source);
                let results =
                    build_rustup_search_results(default_host.as_deref(), &search_request.query);
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::ListInstalled(_) => {
                let raw = self.source.toolchain_list()?;
                let mut packages = parse_toolchain_list(&raw)?;
                hydrate_toolchain_versions(&self.source, &mut packages);
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.check()?;
                let mut packages = parse_rustup_check(&raw)?;
                if !packages.is_empty()
                    && let Ok(installed_raw) = self.source.toolchain_list()
                {
                    apply_rustup_runtime_state_from_installed_list(&installed_raw, &mut packages);
                }
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Install(install_request) => {
                if install_request.package.name == "__self__" {
                    let install_source = parse_install_source(install_request.version.as_deref())?;
                    let _ = self.source.install_self(install_source)?;
                } else {
                    reject_toolchain_install_version(install_request.version.as_deref())?;
                    let _ = self
                        .source
                        .install_toolchain(install_request.package.name.as_str())?;
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: install_request.package,
                    action: ManagerAction::Install,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                if looks_like_self_uninstall_target(uninstall_request.package.name.as_str()) {
                    let (uninstall_target, remove_shell_setup) =
                        parse_self_uninstall_target(uninstall_request.package.name.as_str())?;
                    if uninstall_target != "__self__" {
                        return Err(CoreError {
                            manager: Some(ManagerId::Rustup),
                            task: Some(TaskType::Uninstall),
                            action: Some(ManagerAction::Uninstall),
                            kind: CoreErrorKind::InvalidInput,
                            message: format!(
                                "unsupported rustup uninstall target: {uninstall_target}"
                            ),
                        });
                    }
                    let _ = self.source.self_uninstall()?;
                    if remove_shell_setup {
                        match crate::post_install_setup::remove_helm_managed_post_install_setup(
                            ManagerId::Rustup,
                        ) {
                            Ok(result) => {
                                crate::execution::record_task_log_note(result.summary().as_str());
                                if !result.malformed_files.is_empty() {
                                    crate::execution::record_task_log_note(
                                        format!(
                                            "helm-managed rustup setup markers were malformed in {} shell startup file(s); left unchanged",
                                            result.malformed_files.len()
                                        )
                                        .as_str(),
                                    );
                                }
                            }
                            Err(error) => {
                                crate::execution::record_task_log_note(
                                    format!(
                                        "failed to remove Helm-managed rustup shell setup block(s): {error}"
                                    )
                                    .as_str(),
                                );
                            }
                        }
                    }
                } else {
                    let _ = self
                        .source
                        .uninstall_toolchain(uninstall_request.package.name.as_str())?;
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                });
                if package.name == "__self__" {
                    let _ = self.source.self_update()?;
                } else {
                    let _ = self.source.update_toolchain(&package.name)?;
                }
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::RustupAddComponent(RustupAddComponentRequest {
                toolchain,
                component,
            }) => {
                validate_rustup_toolchain_identifier(&toolchain, TaskType::Configure)?;
                validate_rustup_detail_identifier(
                    component.as_str(),
                    "component",
                    TaskType::Configure,
                )?;
                let _ = self
                    .source
                    .add_component(toolchain.as_str(), component.as_str())?;
                sync_package_state_after_configuration(&self.source)
            }
            AdapterRequest::RustupRemoveComponent(RustupRemoveComponentRequest {
                toolchain,
                component,
            }) => {
                validate_rustup_toolchain_identifier(&toolchain, TaskType::Configure)?;
                validate_rustup_detail_identifier(
                    component.as_str(),
                    "component",
                    TaskType::Configure,
                )?;
                let _ = self
                    .source
                    .remove_component(toolchain.as_str(), component.as_str())?;
                sync_package_state_after_configuration(&self.source)
            }
            AdapterRequest::RustupAddTarget(RustupAddTargetRequest { toolchain, target }) => {
                validate_rustup_toolchain_identifier(&toolchain, TaskType::Configure)?;
                validate_rustup_detail_identifier(target.as_str(), "target", TaskType::Configure)?;
                let _ = self
                    .source
                    .add_target(toolchain.as_str(), target.as_str())?;
                sync_package_state_after_configuration(&self.source)
            }
            AdapterRequest::RustupRemoveTarget(RustupRemoveTargetRequest { toolchain, target }) => {
                validate_rustup_toolchain_identifier(&toolchain, TaskType::Configure)?;
                validate_rustup_detail_identifier(target.as_str(), "target", TaskType::Configure)?;
                let _ = self
                    .source
                    .remove_target(toolchain.as_str(), target.as_str())?;
                sync_package_state_after_configuration(&self.source)
            }
            AdapterRequest::RustupSetDefaultToolchain(RustupSetDefaultToolchainRequest {
                toolchain,
            }) => {
                validate_rustup_toolchain_identifier(&toolchain, TaskType::Configure)?;
                let _ = self.source.set_default_toolchain(toolchain.as_str())?;
                sync_package_state_after_configuration(&self.source)
            }
            AdapterRequest::RustupSetOverride(RustupSetOverrideRequest { toolchain, path }) => {
                validate_rustup_toolchain_identifier(&toolchain, TaskType::Configure)?;
                validate_rustup_override_path(path.as_path(), TaskType::Configure)?;
                let _ = self
                    .source
                    .set_override(toolchain.as_str(), path.as_path())?;
                sync_package_state_after_configuration(&self.source)
            }
            AdapterRequest::RustupUnsetOverride(RustupUnsetOverrideRequest { toolchain, path }) => {
                validate_rustup_toolchain_identifier(&toolchain, TaskType::Configure)?;
                validate_rustup_override_path(path.as_path(), TaskType::Configure)?;
                let _ = self.source.unset_override(path.as_path())?;
                sync_package_state_after_configuration(&self.source)
            }
            AdapterRequest::RustupSetProfile(RustupSetProfileRequest { profile }) => {
                validate_rustup_profile(profile.as_str())?;
                let _ = self.source.set_profile(profile.as_str())?;
                sync_package_state_after_configuration(&self.source)
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Rustup),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "rustup adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn rustup_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(RUSTUP_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn rustup_show_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Search,
        ManagerAction::Search,
        CommandSpec::new(RUSTUP_COMMAND).arg("show"),
        SEARCH_TIMEOUT,
    )
}

pub fn rustup_toolchain_list_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(RUSTUP_COMMAND).args(["toolchain", "list"]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_component_list_request(
    task_id: Option<TaskId>,
    toolchain: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(RUSTUP_COMMAND).args(["component", "list", "--toolchain", toolchain]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_target_list_request(task_id: Option<TaskId>, toolchain: &str) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(RUSTUP_COMMAND).args(["target", "list", "--toolchain", toolchain]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_override_list_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(RUSTUP_COMMAND).args(["override", "list"]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_show_profile_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(RUSTUP_COMMAND).args(["show", "profile"]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_toolchain_version_request(
    task_id: Option<TaskId>,
    toolchain: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(RUSTUP_COMMAND).args(["run", toolchain, "rustc", "--version"]),
        TOOLCHAIN_VERSION_TIMEOUT,
    )
}

pub fn rustup_check_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(RUSTUP_COMMAND).arg("check"),
        LIST_TIMEOUT,
    )
}

pub fn rustup_toolchain_install_request(
    task_id: Option<TaskId>,
    toolchain: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(RUSTUP_COMMAND).args(["toolchain", "install", toolchain]),
        INSTALL_TIMEOUT,
    )
    .idle_timeout(INSTALL_IDLE_TIMEOUT)
}

pub fn rustup_self_uninstall_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(RUSTUP_COMMAND).args(["--verbose", "self", "uninstall", "-y"]),
        UNINSTALL_TIMEOUT,
    )
    .idle_timeout(UNINSTALL_IDLE_TIMEOUT)
}

pub fn rustup_toolchain_uninstall_request(
    task_id: Option<TaskId>,
    toolchain: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(RUSTUP_COMMAND).args(["toolchain", "uninstall", toolchain]),
        UNINSTALL_TIMEOUT,
    )
    .idle_timeout(UNINSTALL_IDLE_TIMEOUT)
}

pub fn rustup_init_install_request(
    task_id: Option<TaskId>,
    rustup_init_program: impl Into<PathBuf>,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new(rustup_init_program).args(["-y", "--no-modify-path"]),
        INSTALL_TIMEOUT,
    )
    .idle_timeout(INSTALL_IDLE_TIMEOUT)
}

pub fn rustup_download_install_script_request(
    task_id: Option<TaskId>,
    output_script: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new("curl").args([
            "--proto",
            "=https",
            "--tlsv1.2",
            "-sSf",
            "https://sh.rustup.rs",
            "-o",
            output_script,
        ]),
        INSTALL_TIMEOUT,
    )
}

pub fn rustup_run_downloaded_install_script_request(
    task_id: Option<TaskId>,
    script_path: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        CommandSpec::new("sh").args([script_path, "-y", "--no-modify-path"]),
        INSTALL_TIMEOUT,
    )
    .idle_timeout(INSTALL_IDLE_TIMEOUT)
}

pub fn rustup_self_update_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(RUSTUP_COMMAND).args(["self", "update"]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_toolchain_update_request(
    task_id: Option<TaskId>,
    toolchain: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(RUSTUP_COMMAND).args(["update", toolchain]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_add_component_request(
    task_id: Option<TaskId>,
    toolchain: &str,
    component: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Configure,
        ManagerAction::Configure,
        CommandSpec::new(RUSTUP_COMMAND).args([
            "component",
            "add",
            component,
            "--toolchain",
            toolchain,
        ]),
        LIST_TIMEOUT,
    )
    .idle_timeout(INSTALL_IDLE_TIMEOUT)
}

pub fn rustup_remove_component_request(
    task_id: Option<TaskId>,
    toolchain: &str,
    component: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Configure,
        ManagerAction::Configure,
        CommandSpec::new(RUSTUP_COMMAND).args([
            "component",
            "remove",
            component,
            "--toolchain",
            toolchain,
        ]),
        LIST_TIMEOUT,
    )
    .idle_timeout(UNINSTALL_IDLE_TIMEOUT)
}

pub fn rustup_add_target_request(
    task_id: Option<TaskId>,
    toolchain: &str,
    target: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Configure,
        ManagerAction::Configure,
        CommandSpec::new(RUSTUP_COMMAND).args(["target", "add", target, "--toolchain", toolchain]),
        LIST_TIMEOUT,
    )
    .idle_timeout(INSTALL_IDLE_TIMEOUT)
}

pub fn rustup_remove_target_request(
    task_id: Option<TaskId>,
    toolchain: &str,
    target: &str,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Configure,
        ManagerAction::Configure,
        CommandSpec::new(RUSTUP_COMMAND).args([
            "target",
            "remove",
            target,
            "--toolchain",
            toolchain,
        ]),
        LIST_TIMEOUT,
    )
    .idle_timeout(UNINSTALL_IDLE_TIMEOUT)
}

pub fn rustup_set_default_request(task_id: Option<TaskId>, toolchain: &str) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Configure,
        ManagerAction::Configure,
        CommandSpec::new(RUSTUP_COMMAND).args(["default", toolchain]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_set_override_request(
    task_id: Option<TaskId>,
    toolchain: &str,
    path: &std::path::Path,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Configure,
        ManagerAction::Configure,
        CommandSpec::new(RUSTUP_COMMAND).args([
            "override",
            "set",
            "--path",
            &path.to_string_lossy(),
            toolchain,
        ]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_unset_override_request(
    task_id: Option<TaskId>,
    path: &std::path::Path,
) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Configure,
        ManagerAction::Configure,
        CommandSpec::new(RUSTUP_COMMAND).args([
            "override",
            "unset",
            "--path",
            &path.to_string_lossy(),
        ]),
        LIST_TIMEOUT,
    )
}

pub fn rustup_set_profile_request(task_id: Option<TaskId>, profile: &str) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Configure,
        ManagerAction::Configure,
        CommandSpec::new(RUSTUP_COMMAND).args(["set", "profile", profile]),
        LIST_TIMEOUT,
    )
}

fn rustup_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Rustup, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_install_source(version: Option<&str>) -> AdapterResult<RustupInstallSource> {
    let Some(version) = version.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(RustupInstallSource::OfficialDownload);
    };
    if version.eq_ignore_ascii_case("officialDownload") {
        return Ok(RustupInstallSource::OfficialDownload);
    }
    if let Some(path) = version.strip_prefix("existingBinaryPath:") {
        let path = path.trim();
        if path.is_empty() {
            return Err(CoreError {
                manager: Some(ManagerId::Rustup),
                task: Some(TaskType::Install),
                action: Some(ManagerAction::Install),
                kind: CoreErrorKind::InvalidInput,
                message: "rustup existingBinaryPath install source requires a non-empty path"
                    .to_string(),
            });
        }
        return Ok(RustupInstallSource::ExistingBinaryPath(PathBuf::from(path)));
    }
    Err(CoreError {
        manager: Some(ManagerId::Rustup),
        task: Some(TaskType::Install),
        action: Some(ManagerAction::Install),
        kind: CoreErrorKind::InvalidInput,
        message: format!("unsupported rustup install source: {version}"),
    })
}

fn parse_self_uninstall_target(raw: &str) -> AdapterResult<(&str, bool)> {
    let (base, remove_shell_setup) =
        crate::manager_lifecycle::strip_shell_setup_cleanup_suffix(raw);
    if base == "__self__" {
        return Ok((base, remove_shell_setup));
    }
    Err(CoreError {
        manager: Some(ManagerId::Rustup),
        task: Some(TaskType::Uninstall),
        action: Some(ManagerAction::Uninstall),
        kind: CoreErrorKind::InvalidInput,
        message: format!("unsupported rustup uninstall target: {raw}"),
    })
}

fn looks_like_self_uninstall_target(raw: &str) -> bool {
    let trimmed = raw.trim();
    trimmed == "__self__" || trimmed.starts_with("__self__:")
}

fn reject_toolchain_install_version(version: Option<&str>) -> AdapterResult<()> {
    let Some(version) = version.map(str::trim) else {
        return Ok(());
    };
    if version.is_empty() {
        return Ok(());
    }

    Err(CoreError {
        manager: Some(ManagerId::Rustup),
        task: Some(TaskType::Install),
        action: Some(ManagerAction::Install),
        kind: CoreErrorKind::InvalidInput,
        message: "rustup toolchain install does not support --version; include the full toolchain selector in the package name".to_string(),
    })
}

fn validate_rustup_toolchain_identifier(raw: &str, task_type: TaskType) -> AdapterResult<()> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError {
            manager: Some(ManagerId::Rustup),
            task: Some(task_type),
            action: Some(ManagerAction::Configure),
            kind: CoreErrorKind::InvalidInput,
            message: "rustup toolchain selector must not be empty".to_string(),
        });
    }
    Ok(())
}

fn validate_rustup_detail_identifier(
    raw: &str,
    label: &str,
    task_type: TaskType,
) -> AdapterResult<()> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError {
            manager: Some(ManagerId::Rustup),
            task: Some(task_type),
            action: Some(ManagerAction::Configure),
            kind: CoreErrorKind::InvalidInput,
            message: format!("rustup {label} identifier must not be empty"),
        });
    }
    Ok(())
}

fn validate_rustup_override_path(path: &std::path::Path, task_type: TaskType) -> AdapterResult<()> {
    if !path.is_absolute() {
        return Err(CoreError {
            manager: Some(ManagerId::Rustup),
            task: Some(task_type),
            action: Some(ManagerAction::Configure),
            kind: CoreErrorKind::InvalidInput,
            message: format!(
                "rustup override path must be absolute: '{}'",
                path.display()
            ),
        });
    }
    Ok(())
}

fn validate_rustup_profile(raw: &str) -> AdapterResult<()> {
    let normalized = raw.trim().to_ascii_lowercase();
    if matches!(normalized.as_str(), "minimal" | "default" | "complete") {
        return Ok(());
    }
    Err(CoreError {
        manager: Some(ManagerId::Rustup),
        task: Some(TaskType::Configure),
        action: Some(ManagerAction::Configure),
        kind: CoreErrorKind::InvalidInput,
        message: format!(
            "unsupported rustup profile '{}'; expected one of: minimal, default, complete",
            raw.trim()
        ),
    })
}

fn parse_rustup_version(output: &str) -> Option<String> {
    // "rustup 1.28.2 (54dd3d00f 2024-04-24)" -> "1.28.2"
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let rest = line.strip_prefix("rustup ")?;
    let version = rest.split_whitespace().next()?;
    if version.is_empty() {
        return None;
    }
    Some(version.to_owned())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RustupToolchainSelector {
    channel: String,
    archive_date: Option<String>,
    host: Option<String>,
}

impl RustupToolchainSelector {
    fn parse(raw: &str) -> Option<Self> {
        let normalized = raw.trim().to_ascii_lowercase();
        if normalized.is_empty() || normalized.contains(char::is_whitespace) {
            return None;
        }

        let parts = normalized.split('-').collect::<Vec<_>>();
        if parts.is_empty() {
            return None;
        }

        let first = *parts.first()?;
        let (mut channel, mut index) =
            if matches!(first, "stable" | "beta" | "nightly") || is_rustup_version_channel(first)
            {
            (first.to_string(), 1usize)
        } else {
            return None;
        };

        if is_rustup_version_channel(first)
            && parts
                .get(index)
                .is_some_and(|candidate| is_rustup_prerelease_segment(candidate))
        {
            channel.push('-');
            channel.push_str(parts[index]);
            index += 1;
        }

        let archive_date = if parts.len() >= index + 3
            && is_rustup_archive_date(parts[index], parts[index + 1], parts[index + 2])
        {
            let value = format!("{}-{}-{}", parts[index], parts[index + 1], parts[index + 2]);
            index += 3;
            Some(value)
        } else {
            None
        };

        let host = if index < parts.len() {
            if looks_like_partial_rustup_archive_date(&parts[index..]) {
                return None;
            }
            let rendered = parts[index..].join("-");
            if !is_valid_rustup_host_segment(rendered.as_str()) {
                return None;
            }
            Some(rendered)
        } else {
            None
        };

        Some(Self {
            channel,
            archive_date,
            host,
        })
    }

    fn canonical_name(&self, default_host: Option<&str>) -> String {
        let effective_host = self.host.as_deref().or(default_host
            .map(str::trim)
            .filter(|value| !value.is_empty()));

        let mut rendered = self.channel.clone();
        if let Some(date) = self.archive_date.as_deref() {
            rendered.push('-');
            rendered.push_str(date);
        }
        if let Some(host) = effective_host {
            rendered.push('-');
            rendered.push_str(host);
        }
        rendered
    }

    fn alias(&self) -> String {
        let mut rendered = self.channel.clone();
        if let Some(date) = self.archive_date.as_deref() {
            rendered.push('-');
            rendered.push_str(date);
        }
        rendered
    }

    fn summary(&self, default_host: Option<&str>) -> String {
        let effective_host = self.host.as_deref().or(default_host
            .map(str::trim)
            .filter(|value| !value.is_empty()));
        let descriptor = match self.channel.as_str() {
            "stable" => "Rust stable channel".to_string(),
            "beta" => "Rust beta channel".to_string(),
            "nightly" => "Rust nightly channel".to_string(),
            _ if self.channel.contains("-beta.") => {
                format!("Rust {} prerelease", self.channel)
            }
            _ if is_rustup_version_channel(self.channel.as_str()) => {
                format!("Rust {} release", self.channel)
            }
            _ => format!("Rust {} toolchain", self.channel),
        };

        let mut summary = format!(
            "{descriptor} toolchain managed by rustup. Includes rustc, cargo, the standard library, and profile-managed components."
        );
        if let Some(date) = self.archive_date.as_deref() {
            summary.push_str(format!(" Snapshot date: {date}.").as_str());
        }
        if let Some(host) = effective_host {
            summary.push_str(format!(" Host target: {host}.").as_str());
        }
        summary
    }

    fn candidate_version(&self) -> Option<String> {
        self.channel
            .chars()
            .next()
            .filter(|value| value.is_ascii_digit())
            .map(|_| self.channel.clone())
    }

    fn host(&self) -> Option<&str> {
        self.host.as_deref()
    }
}

fn is_rustup_version_channel(value: &str) -> bool {
    let pieces = value.split('.').collect::<Vec<_>>();
    if !(pieces.len() == 2 || pieces.len() == 3) {
        return false;
    }
    pieces
        .iter()
        .all(|piece| !piece.is_empty() && piece.chars().all(|ch| ch.is_ascii_digit()))
}

fn is_rustup_prerelease_segment(value: &str) -> bool {
    value == "beta"
        || value.strip_prefix("beta.").is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|ch| ch.is_ascii_digit())
        })
}

fn is_rustup_archive_date(year: &str, month: &str, day: &str) -> bool {
    year.len() == 4
        && month.len() == 2
        && day.len() == 2
        && year.chars().all(|ch| ch.is_ascii_digit())
        && month.chars().all(|ch| ch.is_ascii_digit())
        && day.chars().all(|ch| ch.is_ascii_digit())
}

fn looks_like_partial_rustup_archive_date(segments: &[&str]) -> bool {
    matches!(segments, [year] if year.len() == 4 && year.chars().all(|ch| ch.is_ascii_digit()))
        || matches!(segments, [year, month]
            if year.len() == 4
                && month.len() == 2
                && year.chars().all(|ch| ch.is_ascii_digit())
                && month.chars().all(|ch| ch.is_ascii_digit()))
}

fn is_valid_rustup_host_segment(value: &str) -> bool {
    let normalized = value.trim();
    if normalized.is_empty() {
        return false;
    }
    if normalized.contains(char::is_whitespace) {
        return false;
    }
    if normalized.contains('-') {
        return normalized.split('-').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.'))
        });
    }
    matches!(normalized, "msvc" | "gnu" | "musl")
}

fn parse_rustup_default_host(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("Default host:"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn infer_default_host_from_installed_toolchains(output: &str) -> Option<String> {
    parse_toolchain_list(output)
        .ok()?
        .into_iter()
        .find_map(|package| {
            RustupToolchainSelector::parse(package.package.name.as_str())
                .and_then(|selector| selector.host().map(str::to_string))
        })
}

fn fallback_rustup_default_host() -> Option<String> {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("x86_64", "macos") => Some("x86_64-apple-darwin".to_string()),
        ("aarch64", "macos") => Some("aarch64-apple-darwin".to_string()),
        ("x86_64", "linux") => Some("x86_64-unknown-linux-gnu".to_string()),
        ("aarch64", "linux") => Some("aarch64-unknown-linux-gnu".to_string()),
        ("x86_64", "windows") => Some("x86_64-pc-windows-msvc".to_string()),
        ("aarch64", "windows") => Some("aarch64-pc-windows-msvc".to_string()),
        _ => None,
    }
}

fn resolve_rustup_search_default_host<S: RustupSource>(source: &S) -> Option<String> {
    if let Ok(output) = source.show()
        && let Some(host) = parse_rustup_default_host(&output)
    {
        return Some(host);
    }

    if let Ok(output) = source.toolchain_list()
        && let Some(host) = infer_default_host_from_installed_toolchains(&output)
    {
        return Some(host);
    }

    fallback_rustup_default_host()
}

fn sync_package_state_after_configuration<S: RustupSource>(
    source: &S,
) -> AdapterResult<AdapterResponse> {
    let installed = match source.toolchain_list() {
        Ok(raw) => {
            let mut packages = parse_toolchain_list(&raw)?;
            hydrate_toolchain_versions(source, &mut packages);
            Some(packages)
        }
        Err(error) => {
            tracing::warn!(
                manager = ?ManagerId::Rustup,
                error = %error.message,
                "rustup package-state sync skipped installed snapshot refresh after configuration"
            );
            None
        }
    };

    let outdated = match source.check() {
        Ok(raw) => {
            let mut packages = parse_rustup_check(&raw)?;
            if !packages.is_empty()
                && let Some(installed_packages) = installed.as_ref()
            {
                apply_rustup_runtime_state_from_installed_packages(
                    installed_packages,
                    &mut packages,
                );
            }
            Some(packages)
        }
        Err(error) => {
            tracing::warn!(
                manager = ?ManagerId::Rustup,
                error = %error.message,
                "rustup package-state sync skipped outdated snapshot refresh after configuration"
            );
            None
        }
    };

    Ok(AdapterResponse::SnapshotSync {
        installed,
        outdated,
    })
}

pub fn rustup_toolchain_detail<S: RustupSource>(
    source: &S,
    toolchain: &str,
) -> AdapterResult<RustupToolchainDetail> {
    let normalized_toolchain = toolchain.trim();
    if normalized_toolchain.is_empty() {
        return Err(CoreError {
            manager: Some(ManagerId::Rustup),
            task: Some(TaskType::Refresh),
            action: Some(ManagerAction::ListInstalled),
            kind: CoreErrorKind::InvalidInput,
            message: "rustup toolchain detail requires a non-empty toolchain selector".to_string(),
        });
    }

    let host = RustupToolchainSelector::parse(normalized_toolchain)
        .and_then(|selector| selector.host().map(str::to_string));

    let components_output = source.component_list(normalized_toolchain)?;
    let targets_output = source.target_list(normalized_toolchain)?;
    let override_output = source.override_list()?;
    let profile_output = source.show_profile()?;

    Ok(RustupToolchainDetail {
        toolchain: normalized_toolchain.to_string(),
        current_profile: parse_rustup_profile_output(&profile_output),
        override_paths: parse_rustup_override_list(&override_output, normalized_toolchain),
        components: parse_rustup_component_list(&components_output, host.as_deref()),
        targets: parse_rustup_target_list(&targets_output),
    })
}

fn parse_toolchain_version_output(output: &str) -> Option<String> {
    // "rustc 1.93.1 (01f6ddf75 2026-02-11)" -> "1.93.1"
    let line = output
        .lines()
        .map(str::trim)
        .find(|value| !value.is_empty())?;
    let rest = line.strip_prefix("rustc ")?;
    let version = rest.split_whitespace().next()?;
    if version.is_empty() {
        return None;
    }
    Some(version.to_owned())
}

fn derive_toolchain_version_from_name(name: &str) -> Option<String> {
    let selector = name.split('-').next()?;
    selector
        .chars()
        .next()
        .filter(|character| character.is_ascii_digit())?;
    Some(selector.to_owned())
}

fn parse_rustup_component_list(
    output: &str,
    toolchain_host: Option<&str>,
) -> Vec<RustupToolchainDetailEntry> {
    let mut entries = output
        .lines()
        .filter_map(|line| parse_rustup_detail_entry(line, toolchain_host, true))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

fn parse_rustup_target_list(output: &str) -> Vec<RustupToolchainDetailEntry> {
    let mut entries = output
        .lines()
        .filter_map(|line| parse_rustup_detail_entry(line, None, false))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    entries
}

fn parse_rustup_profile_output(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .and_then(|line| {
            let normalized = line.to_ascii_lowercase();
            if matches!(normalized.as_str(), "minimal" | "default" | "complete") {
                Some(normalized)
            } else {
                None
            }
        })
}

fn parse_rustup_override_list(output: &str, toolchain: &str) -> Vec<String> {
    let normalized_toolchain = toolchain.trim().to_ascii_lowercase();
    if normalized_toolchain.is_empty() {
        return Vec::new();
    }

    let mut paths = output
        .lines()
        .filter_map(parse_rustup_override_entry)
        .filter(|(_, candidate_toolchain)| {
            candidate_toolchain.eq_ignore_ascii_case(normalized_toolchain.as_str())
        })
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn parse_rustup_override_entry(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("no overrides") {
        return None;
    }

    if let Some((path, toolchain)) =
        split_override_columns(trimmed, '\t').or_else(|| split_override_columns(trimmed, ' '))
    {
        return Some((path, toolchain));
    }

    let (path, toolchain) = trimmed.rsplit_once(char::is_whitespace)?;
    let normalized_path = path.trim();
    let normalized_toolchain = toolchain.trim();
    if normalized_path.is_empty() || normalized_toolchain.is_empty() {
        return None;
    }
    Some((
        normalized_path.to_string(),
        normalized_toolchain.to_string(),
    ))
}

fn split_override_columns(line: &str, delimiter: char) -> Option<(String, String)> {
    let segments = if delimiter == '\t' {
        line.split('\t')
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
    } else {
        line.split("  ")
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
    };
    if segments.len() < 2 {
        return None;
    }

    let toolchain = segments.last()?.trim();
    let path = segments[..segments.len() - 1].join(" ");
    if path.is_empty() || toolchain.is_empty() {
        return None;
    }
    Some((path, toolchain.to_string()))
}

fn parse_rustup_detail_entry(
    line: &str,
    toolchain_host: Option<&str>,
    normalize_component_name: bool,
) -> Option<RustupToolchainDetailEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (name, annotations) = match trimmed.split_once(" (") {
        Some((name, annotations)) => (name.trim(), annotations.trim_end_matches(')').trim()),
        None => (trimmed, ""),
    };
    if name.is_empty() {
        return None;
    }

    if normalize_component_name && name.starts_with("rust-std-") {
        return None;
    }

    let normalized_name = if normalize_component_name {
        normalize_rustup_component_name(name, toolchain_host)
    } else {
        name.to_string()
    };
    if normalized_name.is_empty() {
        return None;
    }

    Some(RustupToolchainDetailEntry {
        name: normalized_name,
        installed: rustup_entry_is_installed(annotations),
    })
}

fn normalize_rustup_component_name(name: &str, toolchain_host: Option<&str>) -> String {
    let trimmed = name.trim();
    let Some(host) = toolchain_host
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return trimmed.to_string();
    };
    let suffix = format!("-{host}");
    trimmed
        .strip_suffix(suffix.as_str())
        .unwrap_or(trimmed)
        .to_string()
}

fn rustup_entry_is_installed(annotations: &str) -> bool {
    annotations.split(',').map(str::trim).any(|token| {
        let normalized = token.to_ascii_lowercase();
        normalized.contains("installed")
            || normalized.contains("default")
            || normalized.contains("active")
    })
}

fn parse_toolchain_list(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.eq_ignore_ascii_case("no installed toolchains")
            || line
                .to_ascii_lowercase()
                .starts_with("no installed toolchains")
        {
            continue;
        }

        // Each line: "stable-x86_64-apple-darwin (active, default)" or "nightly-x86_64-apple-darwin"
        // Name is everything before " (" or end of line
        let name = if let Some(paren_start) = line.find(" (") {
            &line[..paren_start]
        } else {
            line
        };

        if name.is_empty() {
            continue;
        }

        let runtime_state = parse_rustup_runtime_state(line);
        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Rustup,
                name: name.to_owned(),
            },
            installed_version: derive_toolchain_version_from_name(name),
            pinned: false,
            runtime_state,
        });
    }

    Ok(packages)
}

fn parse_rustup_runtime_state(line: &str) -> PackageRuntimeState {
    let annotations = line
        .split_once(" (")
        .and_then(|(_, rest)| rest.strip_suffix(')'))
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut state = PackageRuntimeState::default();
    let Some(annotations) = annotations else {
        return state;
    };

    for token in annotations
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let normalized = token.to_ascii_lowercase();
        if normalized.contains("active") {
            state.is_active = true;
        }
        if normalized.contains("default") {
            state.is_default = true;
        }
        if normalized.contains("override") {
            state.has_override = true;
        }
    }

    state
}

fn hydrate_toolchain_versions<S: RustupSource>(source: &S, packages: &mut [InstalledPackage]) {
    for package in packages
        .iter_mut()
        .filter(|package| package.installed_version.is_none())
    {
        match source.toolchain_version(package.package.name.as_str()) {
            Ok(output) => {
                if let Some(version) = parse_toolchain_version_output(&output) {
                    package.installed_version = Some(version);
                } else {
                    tracing::warn!(
                        manager = ?ManagerId::Rustup,
                        toolchain = %package.package.name,
                        "rustup toolchain version probe returned unparseable output"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    manager = ?ManagerId::Rustup,
                    toolchain = %package.package.name,
                    error = %error.message,
                    "rustup toolchain version probe failed"
                );
            }
        }
    }
}

fn apply_rustup_runtime_state_from_installed_list(
    installed_output: &str,
    outdated_packages: &mut [OutdatedPackage],
) {
    let installed_packages = parse_toolchain_list(installed_output).unwrap_or_default();
    apply_rustup_runtime_state_from_installed_packages(&installed_packages, outdated_packages);
}

fn apply_rustup_runtime_state_from_installed_packages(
    installed_packages: &[InstalledPackage],
    outdated_packages: &mut [OutdatedPackage],
) {
    let runtime_state_by_name = installed_packages
        .iter()
        .map(|package| (package.package.name.clone(), package.runtime_state.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    for package in outdated_packages {
        if let Some(state) = runtime_state_by_name.get(package.package.name.as_str()) {
            package.runtime_state = state.clone();
        }
    }
}

fn build_rustup_search_results(
    default_host: Option<&str>,
    query: &SearchQuery,
) -> Vec<CachedSearchResult> {
    let needle = query.text.trim().to_ascii_lowercase();
    let mut selectors = vec![
        RustupToolchainSelector {
            channel: "stable".to_string(),
            archive_date: None,
            host: None,
        },
        RustupToolchainSelector {
            channel: "beta".to_string(),
            archive_date: None,
            host: None,
        },
        RustupToolchainSelector {
            channel: "nightly".to_string(),
            archive_date: None,
            host: None,
        },
    ];

    if let Some(selector) = RustupToolchainSelector::parse(query.text.as_str()) {
        selectors.push(selector);
    }

    let mut seen = BTreeSet::new();
    let mut results = Vec::new();
    for selector in selectors {
        let canonical_name = selector.canonical_name(default_host);
        let alias = selector.alias();
        let summary = selector.summary(default_host);
        if !needle.is_empty()
            && !canonical_name
                .to_ascii_lowercase()
                .contains(needle.as_str())
            && !alias.to_ascii_lowercase().contains(needle.as_str())
            && !summary.to_ascii_lowercase().contains(needle.as_str())
        {
            continue;
        }
        if !seen.insert(canonical_name.clone()) {
            continue;
        }

        results.push(CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Rustup,
                    name: canonical_name,
                },
                version: selector.candidate_version(),
                summary: Some(summary),
            },
            source_manager: ManagerId::Rustup,
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

fn parse_rustup_check(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        // Skip rustup self-check line: "rustup - Up to date : ..."
        if line.starts_with("rustup -") || line.starts_with("rustup -") {
            continue;
        }

        // Only process "Update available" lines
        // Format: "stable-x86_64-apple-darwin - Update available : 1.82.0 -> 1.93.0"
        let Some((toolchain_part, update_part)) = line.split_once(" - Update available : ") else {
            continue;
        };

        let toolchain = toolchain_part.trim();
        if toolchain.is_empty() {
            continue;
        }

        // Parse "1.82.0 -> 1.93.0"
        let Some((old_version, new_version)) = update_part.split_once(" -> ") else {
            continue;
        };

        let old_version = old_version.trim();
        let new_version = new_version.trim();

        if new_version.is_empty() {
            continue;
        }

        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Rustup,
                name: toolchain.to_owned(),
            },
            installed_version: if old_version.is_empty() {
                None
            } else {
                Some(old_version.to_owned())
            },
            candidate_version: new_version.to_owned(),
            pinned: false,
            restart_required: false,
            runtime_state: PackageRuntimeState::default(),
        });
    }

    Ok(packages)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::UNIX_EPOCH;

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter, RustupAddComponentRequest, RustupAddTargetRequest,
        RustupRemoveComponentRequest, RustupRemoveTargetRequest, RustupSetDefaultToolchainRequest,
        RustupSetOverrideRequest, RustupSetProfileRequest, RustupUnsetOverrideRequest,
        SearchRequest,
    };
    use crate::models::{ManagerAction, ManagerId, SearchQuery, TaskId, TaskType};

    use super::{
        INSTALL_IDLE_TIMEOUT, INSTALL_TIMEOUT, RustupAdapter, RustupDetectOutput,
        RustupInstallSource, RustupSource, RustupToolchainSelector, UNINSTALL_IDLE_TIMEOUT,
        UNINSTALL_TIMEOUT, build_rustup_search_results,
        infer_default_host_from_installed_toolchains, parse_install_source, parse_rustup_check,
        parse_rustup_component_list, parse_rustup_default_host, parse_rustup_runtime_state,
        parse_rustup_target_list, parse_rustup_version, parse_toolchain_list,
        parse_toolchain_version_output, rustup_check_request, rustup_detect_request,
        rustup_download_install_script_request, rustup_init_install_request,
        rustup_run_downloaded_install_script_request, rustup_self_uninstall_request,
        rustup_self_update_request, rustup_show_request, rustup_toolchain_detail,
        rustup_toolchain_install_request, rustup_toolchain_list_request,
        rustup_toolchain_uninstall_request, rustup_toolchain_update_request,
        rustup_toolchain_version_request,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/rustup/version.txt");
    const TOOLCHAIN_LIST_FIXTURE: &str =
        include_str!("../../tests/fixtures/rustup/toolchain_list.txt");
    const CHECK_FIXTURE: &str = include_str!("../../tests/fixtures/rustup/check.txt");

    #[test]
    fn parses_rustup_version_from_standard_banner() {
        let version = parse_rustup_version("rustup 1.28.2 (54dd3d00f 2024-04-24)\n");
        assert_eq!(version.as_deref(), Some("1.28.2"));
    }

    #[test]
    fn parses_rustup_version_from_fixture() {
        let version = parse_rustup_version(VERSION_FIXTURE);
        assert_eq!(version.as_deref(), Some("1.28.2"));
    }

    #[test]
    fn version_parse_returns_none_for_empty_input() {
        assert!(parse_rustup_version("").is_none());
        assert!(parse_rustup_version("   \n  ").is_none());
    }

    #[test]
    fn version_parse_returns_none_for_unrecognized_format() {
        assert!(parse_rustup_version("cargo 1.82.0").is_none());
    }

    #[test]
    fn parses_toolchain_list_from_fixture() {
        let packages = parse_toolchain_list(TOOLCHAIN_LIST_FIXTURE).unwrap();
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].package.name, "stable-x86_64-apple-darwin");
        assert!(packages[0].installed_version.is_none());
        assert!(packages[0].runtime_state.is_active);
        assert!(packages[0].runtime_state.is_default);
        assert!(!packages[0].runtime_state.has_override);
        assert_eq!(packages[1].package.name, "nightly-x86_64-apple-darwin");
        assert!(packages[1].installed_version.is_none());
        assert!(packages[1].runtime_state.is_empty());
        assert_eq!(packages[2].package.name, "1.75.0-x86_64-apple-darwin");
        assert_eq!(packages[2].installed_version.as_deref(), Some("1.75.0"));
        assert!(packages[2].runtime_state.is_empty());
    }

    #[test]
    fn parses_empty_toolchain_list() {
        let packages = parse_toolchain_list("").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn parses_no_installed_toolchains_message_as_empty_inventory() {
        let packages = parse_toolchain_list("no installed toolchains\n").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn parses_toolchain_version_output() {
        let version = parse_toolchain_version_output("rustc 1.93.1 (01f6ddf75 2026-02-11)\n");
        assert_eq!(version.as_deref(), Some("1.93.1"));
    }

    #[test]
    fn parses_rustup_default_host_from_show_output() {
        let output = "Default host: aarch64-apple-darwin\nrustup home: /Users/test/.rustup\n";
        assert_eq!(
            parse_rustup_default_host(output).as_deref(),
            Some("aarch64-apple-darwin")
        );
    }

    #[test]
    fn infers_rustup_default_host_from_installed_toolchains() {
        assert_eq!(
            infer_default_host_from_installed_toolchains(TOOLCHAIN_LIST_FIXTURE).as_deref(),
            Some("x86_64-apple-darwin")
        );
    }

    #[test]
    fn parses_rustup_runtime_state_override_annotations() {
        let state = parse_rustup_runtime_state(
            "nightly-x86_64-apple-darwin (active, directory override for '/tmp/example')",
        );
        assert!(state.is_active);
        assert!(!state.is_default);
        assert!(state.has_override);
    }

    #[test]
    fn parses_rustup_toolchain_selector_shapes() {
        let selector = RustupToolchainSelector::parse("nightly-2025-01-15-x86_64-apple-darwin")
            .expect("selector should parse");
        assert_eq!(selector.channel, "nightly");
        assert_eq!(selector.archive_date.as_deref(), Some("2025-01-15"));
        assert_eq!(selector.host(), Some("x86_64-apple-darwin"));

        let versioned = RustupToolchainSelector::parse("1.92.0-beta.2")
            .expect("versioned selector should parse");
        assert_eq!(versioned.channel, "1.92.0-beta.2");
        assert_eq!(versioned.archive_date, None);
        assert_eq!(versioned.host(), None);
    }

    #[test]
    fn parses_rustup_component_list_normalizing_host_components() {
        let output = "\
cargo-aarch64-apple-darwin (installed)\n\
clippy-aarch64-apple-darwin (installed)\n\
rust-docs-aarch64-apple-darwin (installed)\n\
rust-src (installed)\n\
rust-std-aarch64-apple-darwin (installed)\n\
llvm-tools-aarch64-apple-darwin\n";
        let components = parse_rustup_component_list(output, Some("aarch64-apple-darwin"));
        assert_eq!(
            components,
            vec![
                super::RustupToolchainDetailEntry {
                    name: "cargo".to_string(),
                    installed: true,
                },
                super::RustupToolchainDetailEntry {
                    name: "clippy".to_string(),
                    installed: true,
                },
                super::RustupToolchainDetailEntry {
                    name: "llvm-tools".to_string(),
                    installed: false,
                },
                super::RustupToolchainDetailEntry {
                    name: "rust-docs".to_string(),
                    installed: true,
                },
                super::RustupToolchainDetailEntry {
                    name: "rust-src".to_string(),
                    installed: true,
                },
            ]
        );
    }

    #[test]
    fn parses_rustup_target_list_installed_markers() {
        let output = "\
aarch64-apple-darwin\n\
x86_64-apple-darwin (installed)\n\
wasm32-unknown-unknown\n";
        let targets = parse_rustup_target_list(output);
        assert_eq!(
            targets,
            vec![
                super::RustupToolchainDetailEntry {
                    name: "aarch64-apple-darwin".to_string(),
                    installed: false,
                },
                super::RustupToolchainDetailEntry {
                    name: "wasm32-unknown-unknown".to_string(),
                    installed: false,
                },
                super::RustupToolchainDetailEntry {
                    name: "x86_64-apple-darwin".to_string(),
                    installed: true,
                },
            ]
        );
    }

    #[test]
    fn rustup_toolchain_detail_collects_components_and_targets() {
        let source = FixtureSource::default();
        let detail = rustup_toolchain_detail(&source, "stable-x86_64-apple-darwin")
            .expect("detail should load");
        assert_eq!(detail.toolchain, "stable-x86_64-apple-darwin");
        assert!(
            detail
                .components
                .iter()
                .any(|entry| entry.name == "cargo" && entry.installed)
        );
        assert!(
            detail
                .components
                .iter()
                .any(|entry| entry.name == "rust-src" && entry.installed)
        );
        assert!(
            detail
                .targets
                .iter()
                .any(|entry| entry.name == "x86_64-apple-darwin" && entry.installed)
        );
    }

    #[test]
    fn rejects_invalid_rustup_toolchain_selector_shapes() {
        assert!(RustupToolchainSelector::parse("stable foo").is_none());
        assert!(RustupToolchainSelector::parse("nightly-2025-01").is_none());
        assert!(RustupToolchainSelector::parse("stable-foo").is_none());
    }

    #[test]
    fn builds_rustup_search_results_for_catalog_sync() {
        let query = SearchQuery {
            text: String::new(),
            issued_at: UNIX_EPOCH,
        };
        let results = build_rustup_search_results(Some("x86_64-apple-darwin"), &query);
        let names = results
            .iter()
            .map(|result| result.result.package.name.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "beta-x86_64-apple-darwin".to_string(),
                "nightly-x86_64-apple-darwin".to_string(),
                "stable-x86_64-apple-darwin".to_string(),
            ]
        );
    }

    #[test]
    fn builds_rustup_search_results_for_versioned_query() {
        let query = SearchQuery {
            text: "1.92.0".to_string(),
            issued_at: UNIX_EPOCH,
        };
        let results = build_rustup_search_results(Some("x86_64-apple-darwin"), &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "1.92.0-x86_64-apple-darwin");
        assert_eq!(results[0].result.version.as_deref(), Some("1.92.0"));
    }

    #[test]
    fn rustup_summary_includes_management_host_and_snapshot_context() {
        let selector = RustupToolchainSelector::parse("nightly-2025-01-15-x86_64-apple-darwin")
            .expect("selector should parse");
        let summary = selector.summary(None);
        assert!(summary.contains("managed by rustup"));
        assert!(summary.contains("Snapshot date: 2025-01-15."));
        assert!(summary.contains("Host target: x86_64-apple-darwin."));
    }

    #[test]
    fn rustup_summary_distinguishes_release_and_prerelease_selectors() {
        let release = RustupToolchainSelector::parse("1.92.0").expect("release should parse");
        let prerelease =
            RustupToolchainSelector::parse("1.92.0-beta.2").expect("prerelease should parse");

        assert!(
            release
                .summary(Some("x86_64-apple-darwin"))
                .contains("1.92.0 release")
        );
        assert!(
            prerelease
                .summary(Some("x86_64-apple-darwin"))
                .contains("1.92.0-beta.2 prerelease")
        );
    }

    #[test]
    fn parses_rustup_check_from_fixture() {
        let packages = parse_rustup_check(CHECK_FIXTURE).unwrap();
        assert_eq!(packages.len(), 1); // Only the "Update available" line
        assert_eq!(packages[0].package.name, "stable-x86_64-apple-darwin");
        assert_eq!(packages[0].installed_version.as_deref(), Some("1.82.0"));
        assert_eq!(packages[0].candidate_version, "1.93.0");
        assert!(packages[0].runtime_state.is_empty());
    }

    #[test]
    fn check_skips_up_to_date_and_rustup_self_lines() {
        let output =
            "nightly-x86_64-apple-darwin - Up to date : 1.86.0\nrustup - Up to date : 1.28.2\n";
        let packages = parse_rustup_check(output).unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn parses_empty_check_output() {
        let packages = parse_rustup_check("").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn adapter_executes_supported_read_only_requests() {
        let source = FixtureSource::default();
        let adapter = RustupAdapter::new(source);

        let detect = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .unwrap();
        let search = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "stable".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .unwrap();
        let installed = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();
        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .unwrap();

        assert!(matches!(detect, AdapterResponse::Detection(_)));
        assert!(matches!(search, AdapterResponse::SearchResults(_)));
        assert!(matches!(installed, AdapterResponse::InstalledPackages(_)));
        assert!(matches!(outdated, AdapterResponse::OutdatedPackages(_)));
    }

    #[test]
    fn adapter_searches_installable_toolchain_candidates() {
        let source = FixtureSource::default();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "1.92.0".to_string(),
                    issued_at: UNIX_EPOCH,
                },
            }))
            .unwrap();

        let AdapterResponse::SearchResults(results) = result else {
            panic!("expected search results response");
        };
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.name, "1.92.0-x86_64-apple-darwin");
        assert_eq!(results[0].result.version.as_deref(), Some("1.92.0"));
    }

    #[test]
    fn adapter_list_installed_hydrates_toolchain_versions() {
        let source = FixtureSource::default();
        let version_probes = source.toolchain_version_probes.clone();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .unwrap();

        let AdapterResponse::InstalledPackages(packages) = result else {
            panic!("expected installed packages response");
        };
        assert_eq!(packages.len(), 3);
        assert_eq!(packages[0].installed_version.as_deref(), Some("1.82.0"));
        assert_eq!(
            packages[1].installed_version.as_deref(),
            Some("1.86.0-nightly")
        );
        assert_eq!(packages[2].installed_version.as_deref(), Some("1.75.0"));
        assert_eq!(
            version_probes.lock().unwrap().clone(),
            vec![
                "stable-x86_64-apple-darwin".to_string(),
                "nightly-x86_64-apple-darwin".to_string()
            ]
        );
    }

    #[test]
    fn adapter_executes_self_install_action() {
        let source = FixtureSource::default();
        let self_install_calls = source.self_install_calls.clone();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                },
                version: None,
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
        assert_eq!(self_install_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn adapter_executes_self_install_action_with_existing_binary_source() {
        let source = FixtureSource::default();
        let self_install_calls = source.self_install_calls.clone();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                },
                version: Some("existingBinaryPath:/tmp/rustup-init".to_string()),
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
        assert_eq!(self_install_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn adapter_executes_toolchain_install_action() {
        let source = FixtureSource::default();
        let toolchain_installs = source.toolchain_installs.clone();
        let self_install_calls = source.self_install_calls.clone();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Rustup,
                    name: "stable-x86_64-apple-darwin".to_string(),
                },
                version: None,
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
        assert_eq!(
            toolchain_installs.lock().unwrap().clone(),
            vec!["stable-x86_64-apple-darwin".to_string()]
        );
        assert_eq!(self_install_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn adapter_rejects_toolchain_install_with_version_override() {
        let source = FixtureSource::default();
        let adapter = RustupAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Rustup,
                    name: "stable-x86_64-apple-darwin".to_string(),
                },
                version: Some("1.93.0".to_string()),
            }))
            .expect_err("toolchain install with --version should fail");
        assert_eq!(error.kind, crate::models::CoreErrorKind::InvalidInput);
    }

    #[test]
    fn adapter_executes_self_uninstall_request() {
        let source = FixtureSource::default();
        let self_uninstall_calls = source.self_uninstall_calls.clone();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Uninstall(
                crate::adapters::UninstallRequest {
                    package: crate::models::PackageRef {
                        manager: ManagerId::Rustup,
                        name: "__self__".to_string(),
                    },
                },
            ))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
        assert_eq!(self_uninstall_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn adapter_executes_toolchain_uninstall_request() {
        let source = FixtureSource::default();
        let toolchain_uninstalls = source.toolchain_uninstalls.clone();
        let self_uninstall_calls = source.self_uninstall_calls.clone();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Uninstall(
                crate::adapters::UninstallRequest {
                    package: crate::models::PackageRef {
                        manager: ManagerId::Rustup,
                        name: "stable-x86_64-apple-darwin".to_string(),
                    },
                },
            ))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
        assert_eq!(
            toolchain_uninstalls.lock().unwrap().clone(),
            vec!["stable-x86_64-apple-darwin".to_string()]
        );
        assert_eq!(self_uninstall_calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn adapter_executes_upgrade_self_update_request() {
        let source = FixtureSource::default();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: Some(crate::models::PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                }),
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_executes_upgrade_for_toolchain_target() {
        let source = FixtureSource::default();
        let adapter = RustupAdapter::new(source);

        let result = adapter
            .execute(AdapterRequest::Upgrade(crate::adapters::UpgradeRequest {
                package: Some(crate::models::PackageRef {
                    manager: ManagerId::Rustup,
                    name: "stable-x86_64-apple-darwin".to_string(),
                }),
            }))
            .unwrap();
        assert!(matches!(result, AdapterResponse::Mutation(_)));
    }

    #[test]
    fn adapter_executes_rustup_component_mutation_requests() {
        let source = FixtureSource::default();
        let detail_mutations = source.detail_mutations.clone();
        let adapter = RustupAdapter::new(source);

        let add_result = adapter
            .execute(AdapterRequest::RustupAddComponent(
                RustupAddComponentRequest {
                    toolchain: "stable-x86_64-apple-darwin".to_string(),
                    component: "clippy".to_string(),
                },
            ))
            .unwrap();
        assert!(matches!(
            add_result,
            AdapterResponse::SnapshotSync {
                installed: _,
                outdated: _
            }
        ));

        let remove_result = adapter
            .execute(AdapterRequest::RustupRemoveComponent(
                RustupRemoveComponentRequest {
                    toolchain: "stable-x86_64-apple-darwin".to_string(),
                    component: "clippy".to_string(),
                },
            ))
            .unwrap();
        assert!(matches!(
            remove_result,
            AdapterResponse::SnapshotSync {
                installed: _,
                outdated: _
            }
        ));

        assert_eq!(
            detail_mutations.lock().unwrap().clone(),
            vec![
                "component-add:stable-x86_64-apple-darwin:clippy".to_string(),
                "component-remove:stable-x86_64-apple-darwin:clippy".to_string(),
            ]
        );
    }

    #[test]
    fn adapter_executes_rustup_target_mutation_requests() {
        let source = FixtureSource::default();
        let detail_mutations = source.detail_mutations.clone();
        let adapter = RustupAdapter::new(source);

        let add_result = adapter
            .execute(AdapterRequest::RustupAddTarget(RustupAddTargetRequest {
                toolchain: "stable-x86_64-apple-darwin".to_string(),
                target: "aarch64-apple-darwin".to_string(),
            }))
            .unwrap();
        assert!(matches!(
            add_result,
            AdapterResponse::SnapshotSync {
                installed: _,
                outdated: _
            }
        ));

        let remove_result = adapter
            .execute(AdapterRequest::RustupRemoveTarget(
                RustupRemoveTargetRequest {
                    toolchain: "stable-x86_64-apple-darwin".to_string(),
                    target: "aarch64-apple-darwin".to_string(),
                },
            ))
            .unwrap();
        assert!(matches!(
            remove_result,
            AdapterResponse::SnapshotSync {
                installed: _,
                outdated: _
            }
        ));

        assert_eq!(
            detail_mutations.lock().unwrap().clone(),
            vec![
                "target-add:stable-x86_64-apple-darwin:aarch64-apple-darwin".to_string(),
                "target-remove:stable-x86_64-apple-darwin:aarch64-apple-darwin".to_string(),
            ]
        );
    }

    #[test]
    fn adapter_executes_rustup_default_override_and_profile_requests() {
        let source = FixtureSource::default();
        let detail_mutations = source.detail_mutations.clone();
        let adapter = RustupAdapter::new(source);

        let default_result = adapter
            .execute(AdapterRequest::RustupSetDefaultToolchain(
                RustupSetDefaultToolchainRequest {
                    toolchain: "stable-x86_64-apple-darwin".to_string(),
                },
            ))
            .unwrap();
        assert!(matches!(
            default_result,
            AdapterResponse::SnapshotSync {
                installed: _,
                outdated: _
            }
        ));

        let override_result = adapter
            .execute(AdapterRequest::RustupSetOverride(
                RustupSetOverrideRequest {
                    toolchain: "stable-x86_64-apple-darwin".to_string(),
                    path: "/tmp/helm-rustup-override".into(),
                },
            ))
            .unwrap();
        assert!(matches!(
            override_result,
            AdapterResponse::SnapshotSync {
                installed: _,
                outdated: _
            }
        ));

        let unset_override_result = adapter
            .execute(AdapterRequest::RustupUnsetOverride(
                RustupUnsetOverrideRequest {
                    toolchain: "stable-x86_64-apple-darwin".to_string(),
                    path: "/tmp/helm-rustup-override".into(),
                },
            ))
            .unwrap();
        assert!(matches!(
            unset_override_result,
            AdapterResponse::SnapshotSync {
                installed: _,
                outdated: _
            }
        ));

        let profile_result = adapter
            .execute(AdapterRequest::RustupSetProfile(RustupSetProfileRequest {
                profile: "minimal".to_string(),
            }))
            .unwrap();
        assert!(matches!(
            profile_result,
            AdapterResponse::SnapshotSync {
                installed: _,
                outdated: _
            }
        ));

        assert_eq!(
            detail_mutations.lock().unwrap().clone(),
            vec![
                "default:stable-x86_64-apple-darwin".to_string(),
                "override:stable-x86_64-apple-darwin:/tmp/helm-rustup-override".to_string(),
                "override-unset:/tmp/helm-rustup-override".to_string(),
                "profile:minimal".to_string(),
            ]
        );
    }

    #[test]
    fn detect_command_spec_uses_structured_args() {
        let request = rustup_detect_request(Some(TaskId(99)));
        assert_eq!(request.manager, ManagerId::Rustup);
        assert_eq!(request.task_id, Some(TaskId(99)));
        assert_eq!(request.task_type, TaskType::Detection);
        assert_eq!(request.action, ManagerAction::Detect);
        assert_eq!(request.command.program, PathBuf::from("rustup"));
        assert_eq!(request.command.args, vec!["--version".to_string()]);
        assert!(request.timeout.is_some());
    }

    #[test]
    fn show_command_spec_uses_structured_args() {
        let request = rustup_show_request(Some(TaskId(3)));
        assert_eq!(request.manager, ManagerId::Rustup);
        assert_eq!(request.task_id, Some(TaskId(3)));
        assert_eq!(request.task_type, TaskType::Search);
        assert_eq!(request.action, ManagerAction::Search);
        assert_eq!(request.command.program, PathBuf::from("rustup"));
        assert_eq!(request.command.args, vec!["show".to_string()]);
    }

    #[test]
    fn list_command_specs_use_structured_args() {
        let toolchain_list = rustup_toolchain_list_request(None);
        assert_eq!(
            toolchain_list.command.args,
            vec!["toolchain".to_string(), "list".to_string()]
        );
        assert_eq!(toolchain_list.action, ManagerAction::ListInstalled);
        assert_eq!(toolchain_list.task_type, TaskType::Refresh);

        let check = rustup_check_request(None);
        assert_eq!(check.command.args, vec!["check".to_string()]);
        assert_eq!(check.action, ManagerAction::ListOutdated);
        assert_eq!(check.task_type, TaskType::Refresh);
    }

    #[test]
    fn toolchain_version_request_uses_structured_args() {
        let request =
            rustup_toolchain_version_request(Some(TaskId(4)), "stable-x86_64-apple-darwin");
        assert_eq!(request.task_id, Some(TaskId(4)));
        assert_eq!(
            request.command.args,
            vec![
                "run".to_string(),
                "stable-x86_64-apple-darwin".to_string(),
                "rustc".to_string(),
                "--version".to_string(),
            ]
        );
        assert_eq!(request.action, ManagerAction::ListInstalled);
        assert_eq!(request.task_type, TaskType::Refresh);
    }

    #[test]
    fn self_update_command_spec_uses_structured_args() {
        let request = rustup_self_update_request(Some(TaskId(5)));
        assert_eq!(request.task_id, Some(TaskId(5)));
        assert_eq!(
            request.command.args,
            vec!["self".to_string(), "update".to_string()]
        );
        assert_eq!(request.action, ManagerAction::Upgrade);
        assert_eq!(request.task_type, TaskType::Upgrade);
    }

    #[test]
    fn self_uninstall_request_sets_hard_and_idle_timeouts() {
        let request = rustup_self_uninstall_request(Some(TaskId(7)));
        assert_eq!(request.task_id, Some(TaskId(7)));
        assert_eq!(
            request.command.args,
            vec![
                "--verbose".to_string(),
                "self".to_string(),
                "uninstall".to_string(),
                "-y".to_string()
            ]
        );
        assert_eq!(request.timeout, Some(UNINSTALL_TIMEOUT));
        assert_eq!(request.idle_timeout, Some(UNINSTALL_IDLE_TIMEOUT));
    }

    #[test]
    fn toolchain_uninstall_request_sets_hard_and_idle_timeouts() {
        let request =
            rustup_toolchain_uninstall_request(Some(TaskId(12)), "stable-x86_64-apple-darwin");
        assert_eq!(request.task_id, Some(TaskId(12)));
        assert_eq!(
            request.command.args,
            vec![
                "toolchain".to_string(),
                "uninstall".to_string(),
                "stable-x86_64-apple-darwin".to_string()
            ]
        );
        assert_eq!(request.timeout, Some(UNINSTALL_TIMEOUT));
        assert_eq!(request.idle_timeout, Some(UNINSTALL_IDLE_TIMEOUT));
    }

    #[test]
    fn rustup_init_install_request_sets_hard_and_idle_timeouts() {
        let request = rustup_init_install_request(Some(TaskId(6)), "rustup-init");
        assert_eq!(request.task_id, Some(TaskId(6)));
        assert_eq!(request.command.program, PathBuf::from("rustup-init"));
        assert_eq!(
            request.command.args,
            vec!["-y".to_string(), "--no-modify-path".to_string(),]
        );
        assert_eq!(request.timeout, Some(INSTALL_TIMEOUT));
        assert_eq!(request.idle_timeout, Some(INSTALL_IDLE_TIMEOUT));
    }

    #[test]
    fn toolchain_install_request_sets_hard_and_idle_timeouts() {
        let request =
            rustup_toolchain_install_request(Some(TaskId(13)), "stable-x86_64-apple-darwin");
        assert_eq!(request.task_id, Some(TaskId(13)));
        assert_eq!(
            request.command.args,
            vec![
                "toolchain".to_string(),
                "install".to_string(),
                "stable-x86_64-apple-darwin".to_string()
            ]
        );
        assert_eq!(request.timeout, Some(INSTALL_TIMEOUT));
        assert_eq!(request.idle_timeout, Some(INSTALL_IDLE_TIMEOUT));
    }

    #[test]
    fn rustup_download_install_script_request_uses_expected_command() {
        let request = rustup_download_install_script_request(Some(TaskId(10)), "/tmp/rustup.sh");
        assert_eq!(request.task_id, Some(TaskId(10)));
        assert_eq!(request.command.program, PathBuf::from("curl"));
        assert_eq!(
            request.command.args,
            vec![
                "--proto".to_string(),
                "=https".to_string(),
                "--tlsv1.2".to_string(),
                "-sSf".to_string(),
                "https://sh.rustup.rs".to_string(),
                "-o".to_string(),
                "/tmp/rustup.sh".to_string(),
            ]
        );
        assert_eq!(request.timeout, Some(INSTALL_TIMEOUT));
    }

    #[test]
    fn rustup_run_downloaded_install_script_request_uses_expected_command() {
        let request =
            rustup_run_downloaded_install_script_request(Some(TaskId(11)), "/tmp/rustup.sh");
        assert_eq!(request.task_id, Some(TaskId(11)));
        assert_eq!(request.command.program, PathBuf::from("sh"));
        assert_eq!(
            request.command.args,
            vec![
                "/tmp/rustup.sh".to_string(),
                "-y".to_string(),
                "--no-modify-path".to_string(),
            ]
        );
        assert_eq!(request.timeout, Some(INSTALL_TIMEOUT));
        assert_eq!(request.idle_timeout, Some(INSTALL_IDLE_TIMEOUT));
    }

    #[test]
    fn parse_install_source_defaults_to_official_download() {
        assert_eq!(
            parse_install_source(None).expect("source should parse"),
            RustupInstallSource::OfficialDownload
        );
        assert_eq!(
            parse_install_source(Some("officialDownload")).expect("source should parse"),
            RustupInstallSource::OfficialDownload
        );
    }

    #[test]
    fn parse_install_source_supports_existing_binary_path() {
        assert_eq!(
            parse_install_source(Some("existingBinaryPath:/tmp/rustup-init"))
                .expect("source should parse"),
            RustupInstallSource::ExistingBinaryPath(PathBuf::from("/tmp/rustup-init"))
        );
    }

    #[test]
    fn parse_install_source_rejects_invalid_values() {
        assert!(parse_install_source(Some("existingBinaryPath:")).is_err());
        assert!(parse_install_source(Some("bad-source")).is_err());
    }

    #[test]
    fn toolchain_update_command_spec_uses_structured_args() {
        let request =
            rustup_toolchain_update_request(Some(TaskId(8)), "stable-x86_64-apple-darwin");
        assert_eq!(request.task_id, Some(TaskId(8)));
        assert_eq!(
            request.command.args,
            vec![
                "update".to_string(),
                "stable-x86_64-apple-darwin".to_string()
            ]
        );
        assert_eq!(request.action, ManagerAction::Upgrade);
        assert_eq!(request.task_type, TaskType::Upgrade);
    }

    #[derive(Default, Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
        self_install_calls: Arc<AtomicUsize>,
        self_uninstall_calls: Arc<AtomicUsize>,
        detail_mutations: Arc<Mutex<Vec<String>>>,
        toolchain_installs: Arc<Mutex<Vec<String>>>,
        toolchain_uninstalls: Arc<Mutex<Vec<String>>>,
        toolchain_updates: Arc<Mutex<Vec<String>>>,
        toolchain_version_probes: Arc<Mutex<Vec<String>>>,
    }

    impl RustupSource for FixtureSource {
        fn detect(&self) -> AdapterResult<RustupDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(RustupDetectOutput {
                executable_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                version_output: VERSION_FIXTURE.to_string(),
            })
        }

        fn show(&self) -> AdapterResult<String> {
            Ok("Default host: x86_64-apple-darwin\n".to_string())
        }

        fn toolchain_list(&self) -> AdapterResult<String> {
            Ok(TOOLCHAIN_LIST_FIXTURE.to_string())
        }

        fn component_list(&self, _toolchain: &str) -> AdapterResult<String> {
            Ok("\
cargo-x86_64-apple-darwin (installed)\n\
clippy-x86_64-apple-darwin (installed)\n\
rust-docs-x86_64-apple-darwin (installed)\n\
rust-src (installed)\n\
rust-std-x86_64-apple-darwin (installed)\n"
                .to_string())
        }

        fn target_list(&self, _toolchain: &str) -> AdapterResult<String> {
            Ok("\
aarch64-apple-darwin\n\
x86_64-apple-darwin (installed)\n"
                .to_string())
        }

        fn override_list(&self) -> AdapterResult<String> {
            Ok("no overrides\n".to_string())
        }

        fn show_profile(&self) -> AdapterResult<String> {
            Ok("default\n".to_string())
        }

        fn toolchain_version(&self, toolchain: &str) -> AdapterResult<String> {
            self.toolchain_version_probes
                .lock()
                .unwrap()
                .push(toolchain.to_string());
            let output = match toolchain {
                "stable-x86_64-apple-darwin" => "rustc 1.82.0 (abc123 2025-01-01)",
                "nightly-x86_64-apple-darwin" => "rustc 1.86.0-nightly (abc1234 2025-01-15)",
                _ => "",
            };
            Ok(output.to_string())
        }

        fn check(&self) -> AdapterResult<String> {
            Ok(CHECK_FIXTURE.to_string())
        }

        fn install_self(&self, _source: RustupInstallSource) -> AdapterResult<String> {
            self.self_install_calls.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }

        fn install_toolchain(&self, toolchain: &str) -> AdapterResult<String> {
            self.toolchain_installs
                .lock()
                .unwrap()
                .push(toolchain.to_string());
            Ok(String::new())
        }

        fn add_component(&self, toolchain: &str, component: &str) -> AdapterResult<String> {
            self.detail_mutations
                .lock()
                .unwrap()
                .push(format!("component-add:{toolchain}:{component}"));
            Ok(String::new())
        }

        fn remove_component(&self, toolchain: &str, component: &str) -> AdapterResult<String> {
            self.detail_mutations
                .lock()
                .unwrap()
                .push(format!("component-remove:{toolchain}:{component}"));
            Ok(String::new())
        }

        fn add_target(&self, toolchain: &str, target: &str) -> AdapterResult<String> {
            self.detail_mutations
                .lock()
                .unwrap()
                .push(format!("target-add:{toolchain}:{target}"));
            Ok(String::new())
        }

        fn remove_target(&self, toolchain: &str, target: &str) -> AdapterResult<String> {
            self.detail_mutations
                .lock()
                .unwrap()
                .push(format!("target-remove:{toolchain}:{target}"));
            Ok(String::new())
        }

        fn set_default_toolchain(&self, toolchain: &str) -> AdapterResult<String> {
            self.detail_mutations
                .lock()
                .unwrap()
                .push(format!("default:{toolchain}"));
            Ok(String::new())
        }

        fn set_override(&self, toolchain: &str, path: &std::path::Path) -> AdapterResult<String> {
            self.detail_mutations.lock().unwrap().push(format!(
                "override:{}:{}",
                toolchain,
                path.display()
            ));
            Ok(String::new())
        }

        fn unset_override(&self, path: &std::path::Path) -> AdapterResult<String> {
            self.detail_mutations
                .lock()
                .unwrap()
                .push(format!("override-unset:{}", path.display()));
            Ok(String::new())
        }

        fn set_profile(&self, profile: &str) -> AdapterResult<String> {
            self.detail_mutations
                .lock()
                .unwrap()
                .push(format!("profile:{profile}"));
            Ok(String::new())
        }

        fn uninstall_toolchain(&self, toolchain: &str) -> AdapterResult<String> {
            self.toolchain_uninstalls
                .lock()
                .unwrap()
                .push(toolchain.to_string());
            Ok(String::new())
        }

        fn self_uninstall(&self) -> AdapterResult<String> {
            self.self_uninstall_calls.fetch_add(1, Ordering::SeqCst);
            Ok(String::new())
        }

        fn update_toolchain(&self, toolchain: &str) -> AdapterResult<String> {
            self.toolchain_updates
                .lock()
                .unwrap()
                .push(toolchain.to_string());
            Ok(String::new())
        }

        fn self_update(&self) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
