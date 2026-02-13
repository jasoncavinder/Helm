use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const MISE_READ_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

const MISE_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Mise,
    display_name: "mise",
    category: ManagerCategory::ToolRuntime,
    authority: ManagerAuthority::Authoritative,
    capabilities: MISE_READ_CAPABILITIES,
};

const MISE_COMMAND: &str = "mise";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);

pub trait MiseSource: Send + Sync {
    fn detect(&self) -> AdapterResult<String>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
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
                let raw = self.source.detect()?;
                let version = parse_mise_version(&raw);
                let installed = version.is_some();
                Ok(AdapterResponse::Detection(DetectionInfo {
                    installed,
                    executable_path: None,
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
    // "mise 2026.2.6 macos-x64" -> "2026.2.6"
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let rest = line.strip_prefix("mise ")?;
    let version = rest.split_whitespace().next()?;
    if version.is_empty() {
        return None;
    }
    Some(version.to_owned())
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
        MiseAdapter, MiseSource, mise_detect_request, mise_list_installed_request,
        mise_list_outdated_request, parse_mise_installed, parse_mise_outdated, parse_mise_version,
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
    fn adapter_rejects_unsupported_action() {
        let source = FixtureSource::default();
        let adapter = MiseAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Mise,
                    name: "node".to_string(),
                },
                version: None,
            }))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
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

    #[derive(Default, Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
    }

    impl MiseSource for FixtureSource {
        fn detect(&self) -> AdapterResult<String> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(VERSION_FIXTURE.to_string())
        }

        fn list_installed(&self) -> AdapterResult<String> {
            Ok(INSTALLED_FIXTURE.to_string())
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            Ok(OUTDATED_FIXTURE.to_string())
        }
    }
}
