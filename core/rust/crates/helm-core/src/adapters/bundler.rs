use std::path::PathBuf;
use std::time::Duration;

use crate::adapters::manager::{AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter};
use crate::execution::{CommandSpec, ProcessSpawnRequest};
use crate::models::{
    ActionSafety, CachedSearchResult, Capability, CoreError, CoreErrorKind, DetectionInfo,
    InstalledPackage, ManagerAction, ManagerAuthority, ManagerCategory, ManagerDescriptor,
    ManagerId, OutdatedPackage, PackageCandidate, PackageRef, SearchQuery, TaskId, TaskType,
};

const BUNDLER_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Search,
    Capability::Install,
    Capability::Uninstall,
    Capability::Upgrade,
];

const BUNDLER_DESCRIPTOR: ManagerDescriptor = ManagerDescriptor {
    id: ManagerId::Bundler,
    display_name: "bundler",
    category: ManagerCategory::Language,
    authority: ManagerAuthority::Standard,
    capabilities: BUNDLER_CAPABILITIES,
};

const BUNDLER_COMMAND: &str = "bundle";
const GEM_COMMAND: &str = "gem";
const BUNDLER_PACKAGE_NAME: &str = "bundler";
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const MUTATION_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BundlerDetectOutput {
    pub executable_path: Option<PathBuf>,
    pub version_output: String,
}

pub trait BundlerSource: Send + Sync {
    fn detect(&self) -> AdapterResult<BundlerDetectOutput>;
    fn list_installed(&self) -> AdapterResult<String>;
    fn list_outdated(&self) -> AdapterResult<String>;
    fn install(&self, version: Option<&str>) -> AdapterResult<String>;
    fn uninstall(&self) -> AdapterResult<String>;
    fn upgrade(&self) -> AdapterResult<String>;
}

pub struct BundlerAdapter<S: BundlerSource> {
    source: S,
}

impl<S: BundlerSource> BundlerAdapter<S> {
    pub fn new(source: S) -> Self {
        Self { source }
    }
}

impl<S: BundlerSource> ManagerAdapter for BundlerAdapter<S> {
    fn descriptor(&self) -> &ManagerDescriptor {
        &BUNDLER_DESCRIPTOR
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        crate::adapters::ensure_request_supported(self.descriptor(), &request)?;

        match request {
            AdapterRequest::Detect(_) => {
                let output = self.source.detect()?;
                let version = parse_bundler_version(&output.version_output);
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
                let packages = parse_bundler_list_installed(&raw)?;
                Ok(AdapterResponse::InstalledPackages(packages))
            }
            AdapterRequest::ListOutdated(_) => {
                let raw = self.source.list_outdated()?;
                let packages = parse_bundler_outdated(&raw)?;
                Ok(AdapterResponse::OutdatedPackages(packages))
            }
            AdapterRequest::Search(search_request) => {
                let raw = self.source.list_installed()?;
                let results = parse_bundler_search(&raw, &search_request.query)?;
                Ok(AdapterResponse::SearchResults(results))
            }
            AdapterRequest::Install(install_request) => {
                validate_bundler_package_name(
                    ManagerAction::Install,
                    install_request.package.name.as_str(),
                )?;
                let _ = self.source.install(install_request.version.as_deref())?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: install_request.package,
                    action: ManagerAction::Install,
                    before_version: None,
                    after_version: install_request.version,
                }))
            }
            AdapterRequest::Uninstall(uninstall_request) => {
                validate_bundler_package_name(
                    ManagerAction::Uninstall,
                    uninstall_request.package.name.as_str(),
                )?;
                let _ = self.source.uninstall()?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package: uninstall_request.package,
                    action: ManagerAction::Uninstall,
                    before_version: None,
                    after_version: None,
                }))
            }
            AdapterRequest::Upgrade(upgrade_request) => {
                let package = upgrade_request.package.unwrap_or(PackageRef {
                    manager: ManagerId::Bundler,
                    name: BUNDLER_PACKAGE_NAME.to_string(),
                });
                validate_bundler_package_name(ManagerAction::Upgrade, package.name.as_str())?;
                let _ = self.source.upgrade()?;
                Ok(AdapterResponse::Mutation(crate::adapters::MutationResult {
                    package,
                    action: ManagerAction::Upgrade,
                    before_version: None,
                    after_version: None,
                }))
            }
            _ => Err(CoreError {
                manager: Some(ManagerId::Bundler),
                task: None,
                action: Some(request.action()),
                kind: CoreErrorKind::UnsupportedCapability,
                message: "bundler adapter action not implemented in this milestone".to_string(),
            }),
        }
    }
}

pub fn bundler_detect_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    bundler_request(
        task_id,
        TaskType::Detection,
        ManagerAction::Detect,
        CommandSpec::new(BUNDLER_COMMAND).arg("--version"),
        DETECT_TIMEOUT,
    )
}

pub fn bundler_list_installed_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    bundler_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListInstalled,
        CommandSpec::new(GEM_COMMAND).args(["list", "--local", BUNDLER_PACKAGE_NAME]),
        LIST_TIMEOUT,
    )
}

pub fn bundler_list_outdated_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    bundler_request(
        task_id,
        TaskType::Refresh,
        ManagerAction::ListOutdated,
        CommandSpec::new(GEM_COMMAND).args(["outdated", BUNDLER_PACKAGE_NAME]),
        LIST_TIMEOUT,
    )
}

pub fn bundler_install_request(
    task_id: Option<TaskId>,
    version: Option<&str>,
) -> ProcessSpawnRequest {
    let mut command = CommandSpec::new(GEM_COMMAND).args(["install", BUNDLER_PACKAGE_NAME]);
    if let Some(version) = version.map(str::trim).filter(|value| !value.is_empty()) {
        command = command.args(["--version", version]);
    }

    bundler_request(
        task_id,
        TaskType::Install,
        ManagerAction::Install,
        command,
        MUTATION_TIMEOUT,
    )
}

pub fn bundler_uninstall_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    bundler_request(
        task_id,
        TaskType::Uninstall,
        ManagerAction::Uninstall,
        CommandSpec::new(GEM_COMMAND).args(["uninstall", BUNDLER_PACKAGE_NAME, "-a", "-x"]),
        MUTATION_TIMEOUT,
    )
}

pub fn bundler_upgrade_request(task_id: Option<TaskId>) -> ProcessSpawnRequest {
    bundler_request(
        task_id,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        CommandSpec::new(GEM_COMMAND).args(["update", BUNDLER_PACKAGE_NAME]),
        MUTATION_TIMEOUT,
    )
}

fn bundler_request(
    task_id: Option<TaskId>,
    task_type: TaskType,
    action: ManagerAction,
    command: CommandSpec,
    timeout: Duration,
) -> ProcessSpawnRequest {
    let mut request = ProcessSpawnRequest::new(ManagerId::Bundler, task_type, action, command)
        .requires_elevation(false)
        .timeout(timeout);
    if let Some(task_id) = task_id {
        request = request.task_id(task_id);
    }
    request
}

fn parse_bundler_version(output: &str) -> Option<String> {
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line.to_ascii_lowercase().starts_with("bundler version") {
            let version = line
                .trim_start_matches("Bundler version")
                .trim_start_matches("bundler version")
                .trim();
            if !version.is_empty() {
                return Some(version.to_string());
            }
        }
    }
    None
}

fn parse_bundler_list_installed(output: &str) -> AdapterResult<Vec<InstalledPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((name, versions_segment)) = line.split_once('(') else {
            continue;
        };

        let name = name.trim();
        if name != BUNDLER_PACKAGE_NAME {
            continue;
        }

        let versions_segment = versions_segment.trim_end_matches(')').trim();
        if versions_segment.is_empty() {
            continue;
        }

        let version = versions_segment
            .split(',')
            .map(str::trim)
            .find(|candidate| {
                !candidate.is_empty()
                    && !candidate.starts_with("default:")
                    && !candidate.starts_with("ruby")
            })
            .map(str::to_string)
            .or_else(|| {
                versions_segment
                    .split(',')
                    .map(str::trim)
                    .find(|candidate| !candidate.is_empty())
                    .map(str::to_string)
            });

        if let Some(version) = version {
            packages.push(InstalledPackage {
                package: PackageRef {
                    manager: ManagerId::Bundler,
                    name: BUNDLER_PACKAGE_NAME.to_string(),
                },
                installed_version: Some(version),
                pinned: false,
            });
        }
    }

    Ok(packages)
}

fn parse_bundler_outdated(output: &str) -> AdapterResult<Vec<OutdatedPackage>> {
    let mut packages = Vec::new();

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let Some((name, details)) = line.split_once('(') else {
            continue;
        };

        let name = name.trim();
        if name != BUNDLER_PACKAGE_NAME {
            continue;
        }

        let details = details.trim_end_matches(')').trim();
        if details.is_empty() {
            continue;
        }

        let mut installed_version: Option<String> = None;
        let mut candidate_version: Option<String> = None;

        for field in details.split(',').map(str::trim) {
            if let Some(value) = field.strip_prefix("newest ") {
                let value = value.trim();
                if !value.is_empty() {
                    candidate_version = Some(value.to_string());
                }
                continue;
            }

            if let Some(value) = field.strip_prefix("installed ") {
                let value = value.trim();
                if !value.is_empty() {
                    installed_version = Some(value.to_string());
                }
            }
        }

        if let Some(candidate_version) = candidate_version {
            packages.push(OutdatedPackage {
                package: PackageRef {
                    manager: ManagerId::Bundler,
                    name: BUNDLER_PACKAGE_NAME.to_string(),
                },
                installed_version,
                candidate_version,
                pinned: false,
                restart_required: false,
            });
        }
    }

    Ok(packages)
}

fn parse_bundler_search(
    output: &str,
    query: &SearchQuery,
) -> AdapterResult<Vec<CachedSearchResult>> {
    let query_text = query.text.trim().to_ascii_lowercase();
    if query_text.is_empty() {
        return Ok(Vec::new());
    }

    let installed = parse_bundler_list_installed(output)?;

    Ok(installed
        .into_iter()
        .filter(|package| {
            package
                .package
                .name
                .to_ascii_lowercase()
                .contains(query_text.as_str())
        })
        .map(|package| CachedSearchResult {
            result: PackageCandidate {
                package: package.package,
                version: package.installed_version,
                summary: Some("Installed Bundler runtime".to_string()),
            },
            source_manager: ManagerId::Bundler,
            originating_query: query.text.clone(),
            cached_at: query.issued_at,
        })
        .collect())
}

fn validate_bundler_package_name(action: ManagerAction, name: &str) -> AdapterResult<()> {
    crate::adapters::validate_package_identifier(ManagerId::Bundler, action, name)?;

    if name.trim() != BUNDLER_PACKAGE_NAME {
        return Err(CoreError {
            manager: Some(ManagerId::Bundler),
            task: None,
            action: Some(action),
            kind: CoreErrorKind::InvalidInput,
            message: "bundler manager only supports the 'bundler' package".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{
        DetectRequest, InstallRequest, ListInstalledRequest, ListOutdatedRequest, RefreshRequest,
        UninstallRequest, UpgradeRequest,
    };

    #[derive(Default)]
    struct FakeBundlerSource {
        detect_output: Option<BundlerDetectOutput>,
        detect_error: Option<CoreError>,
        list_installed_output: Option<String>,
        list_installed_error: Option<CoreError>,
        list_outdated_output: Option<String>,
        list_outdated_error: Option<CoreError>,
        install_error: Option<CoreError>,
        uninstall_error: Option<CoreError>,
        upgrade_error: Option<CoreError>,
    }

    impl BundlerSource for FakeBundlerSource {
        fn detect(&self) -> AdapterResult<BundlerDetectOutput> {
            if let Some(error) = &self.detect_error {
                return Err(error.clone());
            }
            Ok(self.detect_output.clone().unwrap_or(BundlerDetectOutput {
                executable_path: Some(PathBuf::from("/usr/local/bin/bundle")),
                version_output: "Bundler version 2.5.22".to_string(),
            }))
        }

        fn list_installed(&self) -> AdapterResult<String> {
            if let Some(error) = &self.list_installed_error {
                return Err(error.clone());
            }
            Ok(self
                .list_installed_output
                .clone()
                .unwrap_or_else(|| "bundler (2.5.22)".to_string()))
        }

        fn list_outdated(&self) -> AdapterResult<String> {
            if let Some(error) = &self.list_outdated_error {
                return Err(error.clone());
            }
            Ok(self
                .list_outdated_output
                .clone()
                .unwrap_or_else(|| "bundler (newest 2.5.23, installed 2.5.22)".to_string()))
        }

        fn install(&self, _version: Option<&str>) -> AdapterResult<String> {
            if let Some(error) = &self.install_error {
                return Err(error.clone());
            }
            Ok("installed".to_string())
        }

        fn uninstall(&self) -> AdapterResult<String> {
            if let Some(error) = &self.uninstall_error {
                return Err(error.clone());
            }
            Ok("removed".to_string())
        }

        fn upgrade(&self) -> AdapterResult<String> {
            if let Some(error) = &self.upgrade_error {
                return Err(error.clone());
            }
            Ok("upgraded".to_string())
        }
    }

    #[test]
    fn parses_bundler_version_from_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/bundler/version.txt")
            .expect("bundler version fixture");
        assert_eq!(parse_bundler_version(&raw).as_deref(), Some("2.5.22"));
    }

    #[test]
    fn parses_installed_from_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/bundler/list_local.txt")
            .expect("bundler list fixture");
        let packages = parse_bundler_list_installed(&raw).expect("parse installed");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.manager, ManagerId::Bundler);
        assert_eq!(packages[0].package.name, "bundler");
        assert_eq!(packages[0].installed_version.as_deref(), Some("2.5.22"));
    }

    #[test]
    fn parses_outdated_from_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/bundler/outdated.txt")
            .expect("bundler outdated fixture");
        let packages = parse_bundler_outdated(&raw).expect("parse outdated");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].package.manager, ManagerId::Bundler);
        assert_eq!(packages[0].package.name, "bundler");
        assert_eq!(packages[0].installed_version.as_deref(), Some("2.5.22"));
        assert_eq!(packages[0].candidate_version, "2.5.23");
    }

    #[test]
    fn parses_search_results_from_installed_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/bundler/list_local.txt")
            .expect("bundler list fixture");
        let query = SearchQuery {
            text: "bund".to_string(),
            issued_at: std::time::SystemTime::UNIX_EPOCH,
        };

        let results = parse_bundler_search(&raw, &query).expect("parse search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].result.package.manager, ManagerId::Bundler);
        assert_eq!(results[0].result.package.name, "bundler");
    }

    #[test]
    fn execute_supported_requests() {
        let adapter = BundlerAdapter::new(FakeBundlerSource::default());

        let detection = adapter
            .execute(AdapterRequest::Detect(DetectRequest))
            .expect("detect response");
        match detection {
            AdapterResponse::Detection(info) => {
                assert!(info.installed);
                assert_eq!(info.version.as_deref(), Some("2.5.22"));
            }
            _ => panic!("expected detection response"),
        }

        let refreshed = adapter
            .execute(AdapterRequest::Refresh(RefreshRequest))
            .expect("refresh response");
        assert!(matches!(refreshed, AdapterResponse::Refreshed));

        let installed = adapter
            .execute(AdapterRequest::ListInstalled(ListInstalledRequest))
            .expect("list installed response");
        match installed {
            AdapterResponse::InstalledPackages(packages) => {
                assert_eq!(packages.len(), 1);
                assert_eq!(packages[0].package.name, "bundler");
            }
            _ => panic!("expected installed packages"),
        }

        let outdated = adapter
            .execute(AdapterRequest::ListOutdated(ListOutdatedRequest))
            .expect("list outdated response");
        match outdated {
            AdapterResponse::OutdatedPackages(packages) => {
                assert_eq!(packages.len(), 1);
                assert_eq!(packages[0].package.name, "bundler");
            }
            _ => panic!("expected outdated packages"),
        }
    }

    #[test]
    fn install_requires_bundler_package_and_returns_mutation() {
        let adapter = BundlerAdapter::new(FakeBundlerSource::default());

        let package = PackageRef {
            manager: ManagerId::Bundler,
            name: "bundler".to_string(),
        };
        let response = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: package.clone(),
                version: Some("2.5.23".to_string()),
            }))
            .expect("install response");

        match response {
            AdapterResponse::Mutation(result) => {
                assert_eq!(result.package, package);
                assert_eq!(result.action, ManagerAction::Install);
                assert_eq!(result.after_version.as_deref(), Some("2.5.23"));
            }
            _ => panic!("expected mutation"),
        }
    }

    #[test]
    fn install_rejects_non_bundler_package() {
        let adapter = BundlerAdapter::new(FakeBundlerSource::default());

        let error = adapter
            .execute(AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Bundler,
                    name: "rake".to_string(),
                },
                version: None,
            }))
            .expect_err("non-bundler package should be rejected");

        assert_eq!(error.kind, CoreErrorKind::InvalidInput);
    }

    #[test]
    fn upgrade_defaults_to_bundler_package() {
        let adapter = BundlerAdapter::new(FakeBundlerSource::default());

        let response = adapter
            .execute(AdapterRequest::Upgrade(UpgradeRequest { package: None }))
            .expect("upgrade response");

        match response {
            AdapterResponse::Mutation(result) => {
                assert_eq!(result.package.manager, ManagerId::Bundler);
                assert_eq!(result.package.name, "bundler");
                assert_eq!(result.action, ManagerAction::Upgrade);
            }
            _ => panic!("expected mutation"),
        }
    }

    #[test]
    fn uninstall_requires_bundler_package() {
        let adapter = BundlerAdapter::new(FakeBundlerSource::default());

        let response = adapter
            .execute(AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Bundler,
                    name: "bundler".to_string(),
                },
            }))
            .expect("uninstall response");

        match response {
            AdapterResponse::Mutation(result) => {
                assert_eq!(result.package.manager, ManagerId::Bundler);
                assert_eq!(result.package.name, "bundler");
                assert_eq!(result.action, ManagerAction::Uninstall);
            }
            _ => panic!("expected mutation"),
        }
    }

    #[test]
    fn request_builders_use_structured_args() {
        let detect = bundler_detect_request(None);
        assert_eq!(detect.command.program.to_string_lossy(), "bundle");
        assert_eq!(detect.command.args, vec!["--version"]);

        let list = bundler_list_installed_request(None);
        assert_eq!(list.command.program.to_string_lossy(), "gem");
        assert_eq!(list.command.args, vec!["list", "--local", "bundler"]);

        let outdated = bundler_list_outdated_request(None);
        assert_eq!(outdated.command.program.to_string_lossy(), "gem");
        assert_eq!(outdated.command.args, vec!["outdated", "bundler"]);

        let install = bundler_install_request(None, Some("2.5.23"));
        assert_eq!(install.command.program.to_string_lossy(), "gem");
        assert_eq!(
            install.command.args,
            vec!["install", "bundler", "--version", "2.5.23"]
        );

        let uninstall = bundler_uninstall_request(None);
        assert_eq!(uninstall.command.program.to_string_lossy(), "gem");
        assert_eq!(
            uninstall.command.args,
            vec!["uninstall", "bundler", "-a", "-x"]
        );

        let upgrade = bundler_upgrade_request(None);
        assert_eq!(upgrade.command.program.to_string_lossy(), "gem");
        assert_eq!(upgrade.command.args, vec!["update", "bundler"]);
    }
}
