use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const MAS_READ_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
];

const MAS_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Mas,
    display_name: "mas",
    category: ManagerCategory::GuiApp,
    authority: ManagerAuthority::Standard,
    capabilities: MAS_READ_CAPABILITIES,
};

const MAS_COMMAND: &str = "mas";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);

pub trait MasSource: Send + Sync {
    fn detect(&self) -> AdapterResult<String>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
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
                let raw = self.source.detect()?;
                let version = parse_mas_version(&raw);
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
                let packages = parse_mas_list(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_mas_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
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
    // `mas version` outputs just the version number, e.g. "1.8.7"
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    if line.is_empty() {
        return None;
    }
    // Take first whitespace-delimited token in case of extra info
    let version = line.split_whitespace().next()?;
    if version.is_empty() {
        return None;
    }
    Some(version.to_owned())
}

fn parse_mas_list(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        // Format: "497799835  Xcode               (16.2)"
        // App ID is the first token, version is in parens at end
        let Some((app_id, rest)) = split_app_id(line) else {
            continue;
        };

        let version = extract_parenthesized_version(rest);

        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Mas,
                name: app_id.to_owned(),
            },
            installed_version: version,
            pinned: false,
        });
    }

    Ok(packages)
}

fn parse_mas_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        // Format: "497799835  Xcode               (16.1 -> 16.2)"
        let Some((app_id, rest)) = split_app_id(line) else {
            continue;
        };

        let (installed, candidate) = extract_outdated_versions(rest);
        let Some(candidate) = candidate else {
            continue;
        };

        packages.push(OutdatedPackage {
            package: PackageRef {
                manager: ManagerId::Mas,
                name: app_id.to_owned(),
            },
            installed_version: installed,
            candidate_version: candidate,
            pinned: false,
        });
    }

    Ok(packages)
}

/// Split an app ID (numeric) from the rest of the line.
fn split_app_id(line: &str) -> Option<(&str, &str)> {
    let mut chars = line.char_indices();
    // Find end of numeric prefix
    let end = loop {
        match chars.next() {
            Some((i, ch)) if ch.is_ascii_digit() => continue,
            Some((i, _)) => break i,
            None => return None, // entire line is digits, no rest
        }
    };
    let app_id = &line[..end];
    if app_id.is_empty() {
        return None;
    }
    let rest = line[end..].trim_start();
    Some((app_id, rest))
}

/// Extract a version from parenthesized suffix, e.g., "(16.2)" -> Some("16.2")
fn extract_parenthesized_version(text: &str) -> Option<String> {
    let open = text.rfind('(')?;
    let close = text.rfind(')')?;
    if close <= open + 1 {
        return None;
    }
    let inner = text[open + 1..close].trim();
    if inner.is_empty() {
        return None;
    }
    Some(inner.to_owned())
}

/// Extract installed and candidate versions from "(16.1 -> 16.2)" format.
fn extract_outdated_versions(text: &str) -> (Option<String>, Option<String>) {
    let Some(inner) = extract_parenthesized_version(text) else {
        return (None, None);
    };

    if let Some((old, new)) = inner.split_once("->") {
        let old = old.trim();
        let new = new.trim();
        (
            if old.is_empty() {
                None
            } else {
                Some(old.to_owned())
            },
            if new.is_empty() {
                None
            } else {
                Some(new.to_owned())
            },
        )
    } else {
        // No arrow, treat as just candidate version
        (None, Some(inner))
    }
}

fn _parse_error(message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Mas),
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
        MasAdapter, MasSource, mas_detect_request, mas_list_installed_request,
        mas_list_outdated_request, parse_mas_list, parse_mas_outdated, parse_mas_version,
    };

    const VERSION_FIXTURE: &str = include_str!("../../tests/fixtures/mas/version.txt");
    const LIST_FIXTURE: &str = include_str!("../../tests/fixtures/mas/list.txt");
    const OUTDATED_FIXTURE: &str = include_str!("../../tests/fixtures/mas/outdated.txt");
    const LIST_EMPTY_FIXTURE: &str = include_str!("../../tests/fixtures/mas/list_empty.txt");

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

        assert_eq!(packages[0].package.name, "497799835");
        assert_eq!(packages[0].installed_version.as_deref(), Some("16.2"));
        assert_eq!(packages[0].package.manager, ManagerId::Mas);

        assert_eq!(packages[1].package.name, "409183694");
        assert_eq!(packages[1].installed_version.as_deref(), Some("14.3"));

        assert_eq!(packages[3].package.name, "1295203466");
        assert_eq!(packages[3].installed_version.as_deref(), Some("10.9.5"));
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

        assert_eq!(packages[0].package.name, "497799835");
        assert_eq!(packages[0].installed_version.as_deref(), Some("16.1"));
        assert_eq!(packages[0].candidate_version, "16.2");

        assert_eq!(packages[1].package.name, "409183694");
        assert_eq!(packages[1].installed_version.as_deref(), Some("14.2"));
        assert_eq!(packages[1].candidate_version, "14.3");
    }

    #[test]
    fn parses_empty_outdated() {
        let packages = parse_mas_outdated("").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn adapter_executes_supported_read_only_requests() {
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

        assert!(matches!(detect, AdapterResponse::Detection(_)));
        assert!(matches!(installed, AdapterResponse::InstalledPackages(_)));
        assert!(matches!(outdated, AdapterResponse::OutdatedPackages(_)));
    }

    #[test]
    fn adapter_rejects_unsupported_action() {
        let source = FixtureSource::default();
        let adapter = MasAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Mas,
                    name: "497799835".to_string(),
                },
                version: None,
            }))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
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
    fn list_command_specs_use_structured_args() {
        let installed = mas_list_installed_request(None);
        assert_eq!(installed.command.args, vec!["list".to_string()]);
        assert_eq!(installed.action, ManagerAction::ListInstalled);
        assert_eq!(installed.task_type, TaskType::Refresh);

        let outdated = mas_list_outdated_request(None);
        assert_eq!(outdated.command.args, vec!["outdated".to_string()]);
        assert_eq!(outdated.action, ManagerAction::ListOutdated);
        assert_eq!(outdated.task_type, TaskType::Refresh);
    }

    #[derive(Default, Clone)]
    struct FixtureSource {
        detect_calls: Arc<AtomicUsize>,
    }

    impl MasSource for FixtureSource {
        fn detect(&self) -> AdapterResult<String> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(VERSION_FIXTURE.to_string())
        }

        fn list_installed(&self) -> AdapterResult<String> {
            Ok(LIST_FIXTURE.to_string())
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            Ok(OUTDATED_FIXTURE.to_string())
        }
    }
}
