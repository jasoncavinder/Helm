use std::path::PathBuf;

use helm_core::adapters::asdf::AsdfDetectOutput;
use helm_core::adapters::homebrew::HomebrewDetectOutput;
use helm_core::adapters::npm::NpmDetectOutput;
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AdapterResult, AsdfAdapter, AsdfSource, HomebrewAdapter,
    HomebrewSource, InstallRequest, ManagerAdapter, NpmAdapter, NpmSource, PinRequest,
    UninstallRequest, UnpinRequest, UpgradeRequest,
};
use helm_core::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, PackageRef};

fn package(manager: ManagerId, name: &str) -> PackageRef {
    PackageRef {
        manager,
        name: name.to_string(),
    }
}

fn process_failure(manager: ManagerId, action: ManagerAction, message: &str) -> CoreError {
    CoreError {
        manager: Some(manager),
        task: None,
        action: Some(action),
        kind: CoreErrorKind::ProcessFailure,
        message: message.to_string(),
    }
}

struct AsdfLifecycleSource;

impl AsdfSource for AsdfLifecycleSource {
    fn detect(&self) -> AdapterResult<AsdfDetectOutput> {
        Ok(AsdfDetectOutput {
            executable_path: Some(PathBuf::from("/Users/dev/.asdf/bin/asdf")),
            version_output: "v0.16.0".to_string(),
        })
    }

    fn list_current(&self) -> AdapterResult<String> {
        Ok("nodejs 20.12.2\n".to_string())
    }

    fn list_plugins(&self) -> AdapterResult<String> {
        Ok(String::new())
    }

    fn list_all_plugins(&self) -> AdapterResult<String> {
        Ok(String::new())
    }

    fn latest_version(&self, _plugin: &str) -> AdapterResult<String> {
        Ok(String::new())
    }

    fn install(&self, _plugin: &str, _version: Option<&str>) -> AdapterResult<String> {
        Ok("installed".to_string())
    }

    fn uninstall(&self, _plugin: &str, _version: &str) -> AdapterResult<String> {
        Ok("uninstalled".to_string())
    }

    fn upgrade(&self, _plugin: Option<&str>) -> AdapterResult<String> {
        Ok("upgraded".to_string())
    }
}

struct NpmLifecycleSource;

impl NpmSource for NpmLifecycleSource {
    fn detect(&self) -> AdapterResult<NpmDetectOutput> {
        Ok(NpmDetectOutput {
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/npm")),
            version_output: "10.9.0".to_string(),
        })
    }

    fn list_installed_global(&self) -> AdapterResult<String> {
        Ok("{\"dependencies\":{}}".to_string())
    }

    fn list_outdated_global(&self) -> AdapterResult<String> {
        Ok("{}".to_string())
    }

    fn search(&self, _query: &str) -> AdapterResult<String> {
        Ok("[]".to_string())
    }

    fn install_global(&self, _name: &str, _version: Option<&str>) -> AdapterResult<String> {
        Ok("installed".to_string())
    }

    fn uninstall_global(&self, _name: &str) -> AdapterResult<String> {
        Ok("removed".to_string())
    }

    fn upgrade_global(&self, _name: Option<&str>) -> AdapterResult<String> {
        Ok("updated".to_string())
    }
}

struct HomebrewIdempotentSource;

impl HomebrewSource for HomebrewIdempotentSource {
    fn detect(&self) -> AdapterResult<HomebrewDetectOutput> {
        Ok(HomebrewDetectOutput {
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
            version_output: "Homebrew 4.4.0".to_string(),
        })
    }

    fn list_installed_formulae(&self) -> AdapterResult<String> {
        Ok(String::new())
    }

    fn list_outdated_formulae(&self) -> AdapterResult<String> {
        Ok(String::new())
    }

    fn search_local_formulae(&self, _query: &str) -> AdapterResult<String> {
        Ok(String::new())
    }

    fn install_formula(&self, _name: &str) -> AdapterResult<String> {
        Err(process_failure(
            ManagerId::HomebrewFormula,
            ManagerAction::Install,
            "formula is already installed",
        ))
    }

    fn uninstall_formula(&self, _name: &str) -> AdapterResult<String> {
        Err(process_failure(
            ManagerId::HomebrewFormula,
            ManagerAction::Uninstall,
            "Error: No such keg: formula",
        ))
    }

    fn upgrade_formula(&self, _name: Option<&str>) -> AdapterResult<String> {
        Ok("upgraded".to_string())
    }

    fn cleanup_formula(&self, _name: &str) -> AdapterResult<String> {
        Ok("cleaned".to_string())
    }

    fn pin_formula(&self, _name: &str) -> AdapterResult<String> {
        Ok("pinned".to_string())
    }

    fn unpin_formula(&self, _name: &str) -> AdapterResult<String> {
        Ok("unpinned".to_string())
    }
}

#[test]
fn authoritative_asdf_lifecycle_mutations_are_emitted() {
    let adapter = AsdfAdapter::new(AsdfLifecycleSource);

    let install = adapter
        .execute(AdapterRequest::Install(InstallRequest {
            package: package(ManagerId::Asdf, "nodejs"),
            version: Some("20.12.2".to_string()),
        }))
        .expect("authoritative install should succeed");
    match install {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Install);
            assert_eq!(result.package.manager, ManagerId::Asdf);
            assert_eq!(result.package.name, "nodejs");
            assert_eq!(result.after_version.as_deref(), Some("20.12.2"));
        }
        other => panic!("expected install mutation response, got {other:?}"),
    }

    let upgrade = adapter
        .execute(AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(package(ManagerId::Asdf, "nodejs")),
        }))
        .expect("authoritative upgrade should succeed");
    match upgrade {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Upgrade);
            assert_eq!(result.package.name, "nodejs");
        }
        other => panic!("expected upgrade mutation response, got {other:?}"),
    }

    let uninstall = adapter
        .execute(AdapterRequest::Uninstall(UninstallRequest {
            package: package(ManagerId::Asdf, "nodejs"),
        }))
        .expect("authoritative uninstall should succeed");
    match uninstall {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Uninstall);
            assert_eq!(result.package.name, "nodejs");
            assert_eq!(result.before_version.as_deref(), Some("20.12.2"));
            assert_eq!(result.after_version, None);
        }
        other => panic!("expected uninstall mutation response, got {other:?}"),
    }
}

#[test]
fn standard_npm_lifecycle_mutations_are_emitted() {
    let adapter = NpmAdapter::new(NpmLifecycleSource);

    let install = adapter
        .execute(AdapterRequest::Install(InstallRequest {
            package: package(ManagerId::Npm, "eslint"),
            version: Some("9.0.0".to_string()),
        }))
        .expect("standard install should succeed");
    match install {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Install);
            assert_eq!(result.package.manager, ManagerId::Npm);
            assert_eq!(result.package.name, "eslint");
            assert_eq!(result.after_version.as_deref(), Some("9.0.0"));
        }
        other => panic!("expected install mutation response, got {other:?}"),
    }

    let upgrade = adapter
        .execute(AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(package(ManagerId::Npm, "eslint")),
        }))
        .expect("standard upgrade should succeed");
    match upgrade {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Upgrade);
            assert_eq!(result.package.name, "eslint");
        }
        other => panic!("expected upgrade mutation response, got {other:?}"),
    }

    let uninstall = adapter
        .execute(AdapterRequest::Uninstall(UninstallRequest {
            package: package(ManagerId::Npm, "eslint"),
        }))
        .expect("standard uninstall should succeed");
    match uninstall {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Uninstall);
            assert_eq!(result.package.name, "eslint");
            assert_eq!(result.after_version, None);
        }
        other => panic!("expected uninstall mutation response, got {other:?}"),
    }
}

#[test]
fn guarded_homebrew_lifecycle_is_idempotent_for_already_installed_or_absent_formulas() {
    let adapter = HomebrewAdapter::new(HomebrewIdempotentSource);

    let install = adapter
        .execute(AdapterRequest::Install(InstallRequest {
            package: package(ManagerId::HomebrewFormula, "ripgrep"),
            version: None,
        }))
        .expect("guarded install should be idempotent for already-installed formula");
    match install {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Install);
            assert_eq!(result.package.name, "ripgrep");
        }
        other => panic!("expected install mutation response, got {other:?}"),
    }

    let uninstall = adapter
        .execute(AdapterRequest::Uninstall(UninstallRequest {
            package: package(ManagerId::HomebrewFormula, "ripgrep"),
        }))
        .expect("guarded uninstall should be idempotent for already-absent formula");
    match uninstall {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Uninstall);
            assert_eq!(result.package.name, "ripgrep");
        }
        other => panic!("expected uninstall mutation response, got {other:?}"),
    }

    let upgrade = adapter
        .execute(AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(package(ManagerId::HomebrewFormula, "ripgrep")),
        }))
        .expect("guarded upgrade should succeed");
    match upgrade {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Upgrade);
            assert_eq!(result.package.name, "ripgrep");
        }
        other => panic!("expected upgrade mutation response, got {other:?}"),
    }

    let pin = adapter
        .execute(AdapterRequest::Pin(PinRequest {
            package: package(ManagerId::HomebrewFormula, "ripgrep"),
            version: None,
        }))
        .expect("guarded pin should succeed");
    match pin {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Pin);
            assert_eq!(result.package.name, "ripgrep");
        }
        other => panic!("expected pin mutation response, got {other:?}"),
    }

    let unpin = adapter
        .execute(AdapterRequest::Unpin(UnpinRequest {
            package: package(ManagerId::HomebrewFormula, "ripgrep"),
        }))
        .expect("guarded unpin should succeed");
    match unpin {
        AdapterResponse::Mutation(result) => {
            assert_eq!(result.action, ManagerAction::Unpin);
            assert_eq!(result.package.name, "ripgrep");
        }
        other => panic!("expected unpin mutation response, got {other:?}"),
    }
}
