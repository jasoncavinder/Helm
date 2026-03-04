use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const MISE_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
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
    fn install_self(&self, source: MiseInstallSource) -> AdapterResult<String>;
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
                let packages = parse_mise_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Install(install_request) => {
                let source = parse_install_source(install_request.version.as_deref())?;
                let _ = self.source.install_self(source)?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: install_request.package,
                    action: ManagerAction::Install,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                let uninstall_spec = parse_uninstall_mode(uninstall_request.package.name.as_str())?;
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
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: None,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MiseUninstallSpec {
    mode: MiseUninstallMode,
    remove_shell_setup: bool,
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
struct MiseInstalledEntry {
    version: String,
    installed: bool,
}

fn parse_mise_installed(json: &str) -> AdapterResult<Vec<InstalledPackage>> {
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
        })
        .collect();

    packages.sort_by(|a, b| a.package.name.cmp(&b.package.name));

    Ok(packages)
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::adapters::manager::{
        AdapterRequest, AdapterResponse, AdapterResult, DetectRequest, ListInstalledRequest,
        ListOutdatedRequest, ManagerAdapter,
    };
    use crate::models::{CoreErrorKind, ManagerAction, ManagerId, TaskId, TaskType};

    use super::{
        MiseAdapter, MiseDetectOutput, MiseSource, MiseUninstallMode, mise_detect_request,
        mise_download_install_script_request, mise_implode_request, mise_list_installed_request,
        mise_list_outdated_request, mise_run_downloaded_install_script_request,
        mise_upgrade_request, parse_install_source, parse_mise_installed, parse_mise_outdated,
        parse_mise_version, parse_uninstall_mode,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/mise/version.txt");
    const INSTALLED_FIXTURE: &str = include_str!("../../tests/fixtures/mise/ls_json.txt");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/mise/outdated_json.txt");

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
        let packages = parse_mise_installed(INSTALLED_FIXTURE).unwrap();
        assert_eq!(packages.len(), 4); // node, python 3.12.3, python 3.11.9, go
        assert_eq!(packages[0].package.name, "go");
        assert_eq!(packages[0].installed_version.as_deref(), Some("1.22.4"));
        assert_eq!(packages[1].package.name, "node");
        assert_eq!(packages[1].installed_version.as_deref(), Some("22.5.1"));
        // python entries sorted by version
        assert_eq!(packages[2].package.name, "python");
        assert_eq!(packages[2].installed_version.as_deref(), Some("3.11.9"));
        assert_eq!(packages[3].package.name, "python");
        assert_eq!(packages[3].installed_version.as_deref(), Some("3.12.3"));
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
    }

    #[test]
    fn adapter_executes_uninstall_request() {
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

        fn install_self(&self, _source: super::MiseInstallSource) -> AdapterResult<String> {
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
