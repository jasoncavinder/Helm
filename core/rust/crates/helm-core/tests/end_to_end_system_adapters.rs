use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use helm_core::adapters::colima::{ColimaAdapter, ColimaDetectOutput, ColimaSource};
use helm_core::adapters::docker_desktop::{
    DockerDesktopAdapter, DockerDesktopDetectOutput, DockerDesktopSource,
};
use helm_core::adapters::firmware_updates::{
    FirmwareUpdatesAdapter, FirmwareUpdatesDetectOutput, FirmwareUpdatesSource,
};
use helm_core::adapters::nix_darwin::{NixDarwinAdapter, NixDarwinDetectOutput, NixDarwinSource};
use helm_core::adapters::parallels_desktop::{
    ParallelsDesktopAdapter, ParallelsDesktopDetectOutput, ParallelsDesktopSource,
};
use helm_core::adapters::podman::{PodmanAdapter, PodmanDetectOutput, PodmanSource};
use helm_core::adapters::rosetta2::{Rosetta2Adapter, Rosetta2DetectOutput, Rosetta2Source};
use helm_core::adapters::setapp::{SetappAdapter, SetappDetectOutput, SetappSource};
use helm_core::adapters::sparkle::{SparkleAdapter, SparkleDetectOutput, SparkleSource};
use helm_core::adapters::xcode_command_line_tools::{
    XcodeCommandLineToolsAdapter, XcodeCommandLineToolsDetectOutput, XcodeCommandLineToolsSource,
};
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, RefreshRequest, UpgradeRequest,
};
use helm_core::models::{CoreErrorKind, ManagerId, PackageRef};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const COLIMA_OUTDATED_FIXTURE: &str = include_str!("fixtures/colima/outdated_brew.json");
const COLIMA_HOMEBREW_INFO_FIXTURE: &str = r#"{
  "formulae": [
    {
      "name": "colima",
      "installed": [
        { "version": "0.8.0" }
      ]
    }
  ],
  "casks": []
}"#;
const DOCKER_DESKTOP_OUTDATED_FIXTURE: &str =
    include_str!("fixtures/docker_desktop/outdated_brew.json");
const DOCKER_DESKTOP_HOMEBREW_INFO_FIXTURE: &str = r#"{
  "formulae": [],
  "casks": [
    {
      "name": "docker-desktop",
      "installed": [
        { "version": "4.39.0" }
      ]
    }
  ]
}"#;
const PODMAN_OUTDATED_FIXTURE: &str = include_str!("fixtures/podman/outdated_brew.json");
const PODMAN_HOMEBREW_INFO_FIXTURE: &str = r#"{
  "formulae": [
    {
      "name": "podman",
      "installed": [
        { "version": "5.4.0" }
      ]
    }
  ],
  "casks": []
}"#;
const XCODE_CLT_INFO_FIXTURE: &str =
    include_str!("fixtures/xcode_command_line_tools/pkgutil_info.txt");
const XCODE_CLT_OUTDATED_FIXTURE: &str =
    include_str!("fixtures/xcode_command_line_tools/list_available.txt");
const FIRMWARE_HISTORY_FIXTURE: &str = include_str!("fixtures/firmware_updates/history.txt");
const FIRMWARE_LIST_FIXTURE: &str = include_str!("fixtures/firmware_updates/list.txt");
const ROSETTA_INFO_FIXTURE: &str = include_str!("fixtures/rosetta2/pkgutil_info.txt");
const NIX_DARWIN_VERSION_FIXTURE: &str = include_str!("fixtures/nix_darwin/version.txt");

fn build_runtime(adapter: Arc<dyn ManagerAdapter>) -> AdapterRuntime {
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

struct StaticColimaSource;

impl ColimaSource for StaticColimaSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<ColimaDetectOutput> {
        Ok(ColimaDetectOutput {
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/colima")),
            version_output: "colima version 0.8.0".to_string(),
        })
    }

    fn homebrew_info(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(COLIMA_HOMEBREW_INFO_FIXTURE.to_string())
    }

    fn list_outdated(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(COLIMA_OUTDATED_FIXTURE.to_string())
    }
}

struct StaticDockerDesktopSource;

impl DockerDesktopSource for StaticDockerDesktopSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<DockerDesktopDetectOutput> {
        Ok(DockerDesktopDetectOutput {
            executable_path: Some(PathBuf::from("/Applications/Docker.app")),
            version_output: "4.39.0".to_string(),
        })
    }

    fn homebrew_info(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(DOCKER_DESKTOP_HOMEBREW_INFO_FIXTURE.to_string())
    }

    fn list_outdated(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(DOCKER_DESKTOP_OUTDATED_FIXTURE.to_string())
    }
}

struct StaticPodmanSource;

impl PodmanSource for StaticPodmanSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<PodmanDetectOutput> {
        Ok(PodmanDetectOutput {
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/podman")),
            version_output: "podman version 5.4.0".to_string(),
        })
    }

    fn homebrew_info(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(PODMAN_HOMEBREW_INFO_FIXTURE.to_string())
    }

    fn list_outdated(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(PODMAN_OUTDATED_FIXTURE.to_string())
    }
}

struct StatefulXcodeCltSource {
    upgraded: AtomicBool,
}

impl StatefulXcodeCltSource {
    fn new() -> Self {
        Self {
            upgraded: AtomicBool::new(false),
        }
    }
}

impl XcodeCommandLineToolsSource for StatefulXcodeCltSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<XcodeCommandLineToolsDetectOutput> {
        Ok(XcodeCommandLineToolsDetectOutput {
            executable_path: Some(PathBuf::from(
                "/Library/Developer/CommandLineTools/usr/bin/clang",
            )),
            version_output: XCODE_CLT_INFO_FIXTURE.to_string(),
        })
    }

    fn list_outdated(&self) -> helm_core::adapters::AdapterResult<String> {
        if self.upgraded.load(Ordering::SeqCst) {
            Ok("Software Update Tool\n\nFinding available software\nSoftware Update found the following new or updated software:\n* Label: Safari17.4Arm64Auto-17.4\n\tTitle: Safari, Version: 17.4, Size: 200000KiB, Recommended: YES,\n".to_string())
        } else {
            Ok(XCODE_CLT_OUTDATED_FIXTURE.to_string())
        }
    }

    fn upgrade(&self, label: &str) -> helm_core::adapters::AdapterResult<String> {
        assert_eq!(label, "Command Line Tools for Xcode-16.3");
        self.upgraded.store(true, Ordering::SeqCst);
        Ok(String::new())
    }
}

struct StaticFirmwareUpdatesSource;

impl FirmwareUpdatesSource for StaticFirmwareUpdatesSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<FirmwareUpdatesDetectOutput> {
        Ok(FirmwareUpdatesDetectOutput {
            executable_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
            history_output: FIRMWARE_HISTORY_FIXTURE.to_string(),
        })
    }

    fn history(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(FIRMWARE_HISTORY_FIXTURE.to_string())
    }

    fn list_available(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(FIRMWARE_LIST_FIXTURE.to_string())
    }
}

struct StatefulRosetta2Source {
    installed: AtomicBool,
}

impl StatefulRosetta2Source {
    fn new(installed: bool) -> Self {
        Self {
            installed: AtomicBool::new(installed),
        }
    }
}

impl Rosetta2Source for StatefulRosetta2Source {
    fn detect(&self) -> helm_core::adapters::AdapterResult<Rosetta2DetectOutput> {
        let installed = self.installed.load(Ordering::SeqCst);
        Ok(Rosetta2DetectOutput {
            executable_path: installed
                .then(|| PathBuf::from("/Library/Apple/usr/libexec/oah/libRosettaRuntime")),
            version_output: if installed {
                ROSETTA_INFO_FIXTURE.to_string()
            } else {
                String::new()
            },
        })
    }

    fn install(&self) -> helm_core::adapters::AdapterResult<String> {
        self.installed.store(true, Ordering::SeqCst);
        Ok(String::new())
    }
}

struct StaticSetappSource;

impl SetappSource for StaticSetappSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<SetappDetectOutput> {
        Ok(SetappDetectOutput {
            executable_path: Some(PathBuf::from("/Applications/Setapp.app")),
            version_output: "4.7.0".to_string(),
        })
    }
}

struct StaticSparkleSource;

impl SparkleSource for StaticSparkleSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<SparkleDetectOutput> {
        Ok(SparkleDetectOutput {
            executable_path: Some(PathBuf::from("/Applications/Foo.app")),
            version_output: "2.6.4".to_string(),
        })
    }
}

struct StaticParallelsDesktopSource;

impl ParallelsDesktopSource for StaticParallelsDesktopSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<ParallelsDesktopDetectOutput> {
        Ok(ParallelsDesktopDetectOutput {
            executable_path: Some(PathBuf::from("/Applications/Parallels Desktop.app")),
            version_output: "20.2.0".to_string(),
        })
    }
}

struct StaticNixDarwinSource;

impl NixDarwinSource for StaticNixDarwinSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<NixDarwinDetectOutput> {
        Ok(NixDarwinDetectOutput {
            executable_path: Some(PathBuf::from("/run/current-system/sw/bin/darwin-rebuild")),
            version_output: NIX_DARWIN_VERSION_FIXTURE.to_string(),
        })
    }

    fn list_installed(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }

    fn list_outdated(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }

    fn search(&self, _query: &str) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }

    fn install(&self, _package_name: &str) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }

    fn uninstall(&self, _package_name: &str) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }

    fn upgrade(&self, _package_name: Option<&str>) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }
}

#[tokio::test]
async fn colima_detect_list_and_outdated_through_orchestration() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(ColimaAdapter::new(StaticColimaSource));
    let runtime = build_runtime(adapter);

    let detect_task = runtime
        .submit(ManagerId::Colima, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("0.8.0"));
        }
        other => panic!("expected colima detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Colima,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let installed_snapshot = runtime
        .wait_for_terminal(installed_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match installed_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].package.name, "colima");
        }
        other => panic!("expected colima installed packages, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Colima,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();
    let outdated_snapshot = runtime
        .wait_for_terminal(outdated_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match outdated_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].candidate_version, "0.8.1");
        }
        other => panic!("expected colima outdated packages, got {other:?}"),
    }
}

#[tokio::test]
async fn docker_desktop_detect_list_and_outdated_through_orchestration() {
    let adapter: Arc<dyn ManagerAdapter> =
        Arc::new(DockerDesktopAdapter::new(StaticDockerDesktopSource));
    let runtime = build_runtime(adapter);

    let detect_task = runtime
        .submit(
            ManagerId::DockerDesktop,
            AdapterRequest::Detect(DetectRequest),
        )
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("4.39.0"));
        }
        other => panic!("expected docker desktop detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::DockerDesktop,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let installed_snapshot = runtime
        .wait_for_terminal(installed_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match installed_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].package.name, "Docker Desktop");
        }
        other => panic!("expected docker desktop installed packages, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::DockerDesktop,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();
    let outdated_snapshot = runtime
        .wait_for_terminal(outdated_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match outdated_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].candidate_version, "4.40.0");
        }
        other => panic!("expected docker desktop outdated packages, got {other:?}"),
    }
}

#[tokio::test]
async fn podman_detect_list_and_outdated_through_orchestration() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(PodmanAdapter::new(StaticPodmanSource));
    let runtime = build_runtime(adapter);

    let detect_task = runtime
        .submit(ManagerId::Podman, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("5.4.0"));
        }
        other => panic!("expected podman detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Podman,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let installed_snapshot = runtime
        .wait_for_terminal(installed_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match installed_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].package.name, "podman");
        }
        other => panic!("expected podman installed packages, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Podman,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();
    let outdated_snapshot = runtime
        .wait_for_terminal(outdated_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match outdated_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].candidate_version, "5.5.0");
        }
        other => panic!("expected podman outdated packages, got {other:?}"),
    }
}

#[tokio::test]
async fn xcode_command_line_tools_detect_list_and_upgrade_through_orchestration() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(XcodeCommandLineToolsAdapter::new(
        StatefulXcodeCltSource::new(),
    ));
    let runtime = build_runtime(adapter);

    let detect_task = runtime
        .submit(
            ManagerId::XcodeCommandLineTools,
            AdapterRequest::Detect(DetectRequest),
        )
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("16.3.0.0.1.1742423573"));
        }
        other => panic!("expected xcode clt detection response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::XcodeCommandLineTools,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();
    let outdated_snapshot = runtime
        .wait_for_terminal(outdated_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match outdated_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(
                packages[0].package.name,
                "Command Line Tools for Xcode-16.3"
            );
            assert_eq!(packages[0].candidate_version, "16.3");
        }
        other => panic!("expected xcode clt outdated packages, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::XcodeCommandLineTools,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::XcodeCommandLineTools,
                    name: "Command Line Tools for Xcode-16.3".to_string(),
                }),
                target_name: None,
                version: None,
            }),
        )
        .await
        .unwrap();
    let upgrade_snapshot = runtime
        .wait_for_terminal(upgrade_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match upgrade_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "Command Line Tools for Xcode-16.3");
            assert_eq!(mutation.after_version.as_deref(), Some("16.3"));
        }
        other => panic!("expected xcode clt upgrade mutation, got {other:?}"),
    }
}

#[tokio::test]
async fn firmware_updates_detect_and_list_outdated_through_orchestration() {
    let adapter: Arc<dyn ManagerAdapter> =
        Arc::new(FirmwareUpdatesAdapter::new(StaticFirmwareUpdatesSource));
    let runtime = build_runtime(adapter);

    let detect_task = runtime
        .submit(
            ManagerId::FirmwareUpdates,
            AdapterRequest::Detect(DetectRequest),
        )
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("2069.0.0.0.1"));
        }
        other => panic!("expected firmware updates detection response, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::FirmwareUpdates,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();
    let outdated_snapshot = runtime
        .wait_for_terminal(outdated_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match outdated_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 1);
            assert_eq!(packages[0].package.name, "EFI Firmware Update");
            assert!(packages[0].restart_required);
        }
        other => panic!("expected firmware updates outdated packages, got {other:?}"),
    }
}

#[tokio::test]
async fn rosetta2_detect_and_install_through_orchestration() {
    let adapter: Arc<dyn ManagerAdapter> =
        Arc::new(Rosetta2Adapter::new(StatefulRosetta2Source::new(false)));
    let runtime = build_runtime(adapter);

    let detect_task = runtime
        .submit(ManagerId::Rosetta2, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(!info.installed);
        }
        other => panic!("expected rosetta2 detection response, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::Rosetta2,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Rosetta2,
                    name: "rosetta2".to_string(),
                },
                target_name: None,
                version: None,
            }),
        )
        .await
        .unwrap();
    let install_snapshot = runtime
        .wait_for_terminal(install_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();

    if matches!(std::env::consts::ARCH, "aarch64" | "arm64") {
        match install_snapshot.terminal_state {
            Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
                assert_eq!(mutation.package.name, "rosetta2");
                assert_eq!(
                    mutation.after_version.as_deref(),
                    Some("1.0.0.0.1.1700000000")
                );
            }
            other => panic!("expected rosetta2 install mutation, got {other:?}"),
        }
    } else {
        match install_snapshot.terminal_state {
            Some(AdapterTaskTerminalState::Failed(error)) => {
                assert_eq!(error.kind, CoreErrorKind::UnsupportedCapability);
            }
            other => panic!("expected rosetta2 unsupported error, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn detection_only_gui_apps_detect_through_orchestration() {
    let setapp_runtime = build_runtime(Arc::new(SetappAdapter::new(StaticSetappSource)));
    let sparkle_runtime = build_runtime(Arc::new(SparkleAdapter::new(StaticSparkleSource)));
    let parallels_runtime = build_runtime(Arc::new(ParallelsDesktopAdapter::new(
        StaticParallelsDesktopSource,
    )));

    for (manager, runtime, expected_version) in [
        (ManagerId::Setapp, &setapp_runtime, "4.7.0"),
        (ManagerId::Sparkle, &sparkle_runtime, "2.6.4"),
        (ManagerId::ParallelsDesktop, &parallels_runtime, "20.2.0"),
    ] {
        let task = runtime
            .submit(manager, AdapterRequest::Detect(DetectRequest))
            .await
            .unwrap();
        let snapshot = runtime
            .wait_for_terminal(task, Some(Duration::from_secs(5)))
            .await
            .unwrap();
        match snapshot.terminal_state {
            Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
                assert!(info.installed);
                assert_eq!(info.version.as_deref(), Some(expected_version));
            }
            other => panic!("expected detection response, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn nix_darwin_detect_and_refresh_through_orchestration() {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(NixDarwinAdapter::new(StaticNixDarwinSource));
    let runtime = build_runtime(adapter);

    let detect_task = runtime
        .submit(ManagerId::NixDarwin, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("25.05.20250219.abcdef"));
        }
        other => panic!("expected nix-darwin detection response, got {other:?}"),
    }

    let refresh_task = runtime
        .submit(
            ManagerId::NixDarwin,
            AdapterRequest::Refresh(RefreshRequest),
        )
        .await
        .unwrap();
    let refresh_snapshot = runtime
        .wait_for_terminal(refresh_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match refresh_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::SnapshotSync {
            installed,
            outdated,
        })) => {
            assert!(
                installed
                    .as_ref()
                    .is_some_and(|packages| packages.is_empty())
            );
            assert!(
                outdated
                    .as_ref()
                    .is_some_and(|packages| packages.is_empty())
            );
        }
        other => panic!("expected nix-darwin refresh snapshot, got {other:?}"),
    }
}
