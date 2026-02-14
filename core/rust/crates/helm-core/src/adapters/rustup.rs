use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor, ManagerId,
    OutdatedPackage, PackageRef, TaskId, TaskType,
};

const RUSTUP_READ_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Uninstall,
    Capability::Upgrade,
];

const RUSTUP_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Rustup,
    display_name: "rustup",
    category: ManagerCategory::ToolRuntime,
    authority: ManagerAuthority::Authoritative,
    capabilities: RUSTUP_READ_CAPABILITIES,
};

const RUSTUP_COMMAND: &str = "rustup";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);

const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RustupDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait RustupSource: Send + Sync {
    fn detect(&self) -> AdapterResult<RustupDetectOutput>;
    fn toolchain_list(&self) -> AdapterResult<String>;
    fn check(&self) -> AdapterResult<String>;
    fn update_toolchain(&self, toolchain: &str) -> AdapterResult<String>;
    fn self_uninstall(&self) -> AdapterResult<String>;
    fn self_update(&self) -> AdapterResult<String>;
}

pub struct RustupAdapter<S: RustupSource> {
    source: S,
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
            AdapterRequest::ListInstalled(_) => {
                let raw = self.source.toolchain_list()?;
                let packages = parse_toolchain_list(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.check()?;
                let packages = parse_rustup_check(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                let _ = self.source.self_uninstall()?;
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

pub fn rustup_toolchain_list_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(RUSTUP_COMMAND).args(["toolchain", "list"]),
        LIST_TIMEOUT,
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

pub fn rustup_self_uninstall_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    rustup_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(RUSTUP_COMMAND).args(["self", "uninstall", "-y"]),
        UNINSTALL_TIMEOUT,
    )
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

fn parse_toolchain_list(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
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

        packages.push(InstalledPackage {
            package: PackageRef {
                manager: ManagerId::Rustup,
                name: name.to_owned(),
            },
            installed_version: None,
            pinned: false,
        });
    }

    Ok(packages)
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
        });
    }

    Ok(packages)
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
        RustupAdapter, RustupDetectOutput, RustupSource, parse_rustup_check, parse_rustup_version,
        parse_toolchain_list, rustup_check_request, rustup_detect_request,
        rustup_self_update_request, rustup_toolchain_list_request, rustup_toolchain_update_request,
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
        assert_eq!(packages[1].package.name, "nightly-x86_64-apple-darwin");
        assert_eq!(packages[2].package.name, "1.75.0-x86_64-apple-darwin");
    }

    #[test]
    fn parses_empty_toolchain_list() {
        let packages = parse_toolchain_list("").unwrap();
        assert!(packages.is_empty());
    }

    #[test]
    fn parses_rustup_check_from_fixture() {
        let packages = parse_rustup_check(CHECK_FIXTURE).unwrap();
        assert_eq!(packages.len(), 1); // Only the "Update available" line
        assert_eq!(packages[0].package.name, "stable-x86_64-apple-darwin");
        assert_eq!(packages[0].installed_version.as_deref(), Some("1.82.0"));
        assert_eq!(packages[0].candidate_version, "1.93.0");
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
    fn adapter_rejects_unsupported_install_action() {
        let source = FixtureSource::default();
        let adapter = RustupAdapter::new(source);

        let error = adapter
            .execute(AdapterRequest::Install(crate::adapters::InstallRequest {
                package: crate::models::PackageRef {
                    manager: ManagerId::Rustup,
                    name: "stable".to_string(),
                },
                version: None,
            }))
            .unwrap_err();
        assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
    }

    #[test]
    fn adapter_executes_uninstall_request() {
        let source = FixtureSource::default();
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
    }

    impl RustupSource for FixtureSource {
        fn detect(&self) -> AdapterResult<RustupDetectOutput> {
            self.detect_calls.fetch_add(1, Ordering::SeqCst);
            Ok(RustupDetectOutput {
                executable_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                version_output: VERSION_FIXTURE.to_string(),
            })
        }

        fn toolchain_list(&self) -> AdapterResult<String> {
            Ok(TOOLCHAIN_LIST_FIXTURE.to_string())
        }

        fn check(&self) -> AdapterResult<String> {
            Ok(CHECK_FIXTURE.to_string())
        }

        fn self_uninstall(&self) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn update_toolchain(&self, _toolchain: &str) -> AdapterResult<String> {
            Ok(String::new())
        }

        fn self_update(&self) -> AdapterResult<String> {
            Ok(String::new())
        }
    }
}
