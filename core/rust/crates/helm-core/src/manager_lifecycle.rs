use crate::adapters::{AdapterRequest, InstallRequest, UninstallRequest, UpgradeRequest};
use crate::models::{
    InstallProvenance, ManagerId, ManagerInstallInstance, PackageRef, StrategyKind,
};
use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UninstallStrategyResolution {
    pub strategy: StrategyKind,
    pub unknown_override_required: bool,
    pub used_unknown_override: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UninstallStrategyResolutionError {
    AmbiguousProvenance,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateStrategyResolutionError {
    ReadOnly,
    AmbiguousProvenance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManagerUpdateTarget {
    ManagerSelf,
    HomebrewFormula { formula_name: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerUpdatePlan {
    pub target_manager: ManagerId,
    pub target: ManagerUpdateTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagerUpdatePlanError {
    UnsupportedManager,
    ReadOnly,
    AmbiguousProvenance,
    FormulaUnresolved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerUninstallRoutePlan {
    pub target_manager: ManagerId,
    pub request: AdapterRequest,
    pub strategy: StrategyKind,
    pub unknown_override_required: bool,
    pub used_unknown_override: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagerUninstallRouteError {
    UnsupportedManager,
    AmbiguousProvenance,
    FormulaUnresolved,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum RustupInstallSource {
    #[default]
    OfficialDownload,
    ExistingBinaryPath,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ManagerInstallOptions {
    pub rustup_install_source: Option<RustupInstallSource>,
    pub rustup_binary_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerInstallPlan {
    pub target_manager: ManagerId,
    pub request: AdapterRequest,
    pub label_key: &'static str,
    pub label_args: Vec<(&'static str, String)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagerInstallPlanError {
    UnsupportedManager,
    UnsupportedMethod,
    InvalidRustupBinaryPath,
}

pub fn plan_manager_install(
    manager: ManagerId,
    selected_method: Option<&str>,
    options: &ManagerInstallOptions,
) -> Result<ManagerInstallPlan, ManagerInstallPlanError> {
    match manager {
        ManagerId::Mise => {
            if matches!(selected_method, Some("homebrew") | None) {
                Ok(homebrew_manager_install_plan("mise"))
            } else {
                Err(ManagerInstallPlanError::UnsupportedMethod)
            }
        }
        ManagerId::Asdf => {
            if matches!(selected_method, Some("homebrew") | None) {
                Ok(homebrew_manager_install_plan("asdf"))
            } else {
                Err(ManagerInstallPlanError::UnsupportedMethod)
            }
        }
        ManagerId::Mas => {
            if matches!(selected_method, Some("homebrew") | None) {
                Ok(homebrew_manager_install_plan("mas"))
            } else {
                Err(ManagerInstallPlanError::UnsupportedMethod)
            }
        }
        ManagerId::Rustup => match selected_method {
            Some("homebrew") => Ok(homebrew_manager_install_plan("rustup")),
            Some("rustupInstaller")
            | Some("rustup-init")
            | Some("rustup_init")
            | Some("officialInstaller")
            | None => Ok(rustup_manager_install_plan(options)?),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        _ => Err(ManagerInstallPlanError::UnsupportedManager),
    }
}

pub fn plan_manager_update(
    manager: ManagerId,
    active_instance: Option<&ManagerInstallInstance>,
) -> Result<ManagerUpdatePlan, ManagerUpdatePlanError> {
    match manager {
        ManagerId::HomebrewFormula => {
            match resolve_homebrew_manager_update_strategy(active_instance) {
                Ok(StrategyKind::HomebrewFormula) => Ok(ManagerUpdatePlan {
                    target_manager: ManagerId::HomebrewFormula,
                    target: ManagerUpdateTarget::ManagerSelf,
                }),
                Ok(_) => Err(ManagerUpdatePlanError::AmbiguousProvenance),
                Err(UpdateStrategyResolutionError::ReadOnly) => {
                    Err(ManagerUpdatePlanError::ReadOnly)
                }
                Err(UpdateStrategyResolutionError::AmbiguousProvenance) => {
                    Err(ManagerUpdatePlanError::AmbiguousProvenance)
                }
            }
        }
        ManagerId::Rustup => match resolve_rustup_update_strategy(active_instance) {
            Ok(StrategyKind::RustupSelf) => Ok(ManagerUpdatePlan {
                target_manager: ManagerId::Rustup,
                target: ManagerUpdateTarget::ManagerSelf,
            }),
            Ok(StrategyKind::HomebrewFormula) => Ok(ManagerUpdatePlan {
                target_manager: ManagerId::HomebrewFormula,
                target: ManagerUpdateTarget::HomebrewFormula {
                    formula_name: "rustup".to_string(),
                },
            }),
            Ok(_) => Err(ManagerUpdatePlanError::AmbiguousProvenance),
            Err(UpdateStrategyResolutionError::ReadOnly) => Err(ManagerUpdatePlanError::ReadOnly),
            Err(UpdateStrategyResolutionError::AmbiguousProvenance) => {
                Err(ManagerUpdatePlanError::AmbiguousProvenance)
            }
        },
        _ if manager_supports_homebrew_update_strategy_routing(manager) => {
            match resolve_homebrew_manager_update_strategy(active_instance) {
                Ok(StrategyKind::HomebrewFormula) => {
                    let formula_name =
                        resolve_homebrew_formula_name_for_manager(manager, active_instance)
                            .ok_or(ManagerUpdatePlanError::FormulaUnresolved)?;
                    Ok(ManagerUpdatePlan {
                        target_manager: ManagerId::HomebrewFormula,
                        target: ManagerUpdateTarget::HomebrewFormula { formula_name },
                    })
                }
                Ok(_) => Err(ManagerUpdatePlanError::AmbiguousProvenance),
                Err(UpdateStrategyResolutionError::ReadOnly) => {
                    Err(ManagerUpdatePlanError::ReadOnly)
                }
                Err(UpdateStrategyResolutionError::AmbiguousProvenance) => {
                    Err(ManagerUpdatePlanError::AmbiguousProvenance)
                }
            }
        }
        _ => Err(ManagerUpdatePlanError::UnsupportedManager),
    }
}

pub fn plan_manager_uninstall_route(
    manager: ManagerId,
    active_instance: Option<&ManagerInstallInstance>,
    allow_unknown_provenance: bool,
    preview_only: bool,
) -> Result<ManagerUninstallRoutePlan, ManagerUninstallRouteError> {
    if manager == ManagerId::Rustup {
        let resolution = resolve_rustup_uninstall_strategy(
            active_instance,
            allow_unknown_provenance,
            preview_only,
        )
        .map_err(|_| ManagerUninstallRouteError::AmbiguousProvenance)?;
        let (target_manager, request) = match resolution.strategy {
            StrategyKind::HomebrewFormula => (
                ManagerId::HomebrewFormula,
                AdapterRequest::Uninstall(UninstallRequest {
                    package: PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: "rustup".to_string(),
                    },
                }),
            ),
            StrategyKind::RustupSelf | StrategyKind::ReadOnly => (
                ManagerId::Rustup,
                AdapterRequest::Uninstall(UninstallRequest {
                    package: PackageRef {
                        manager: ManagerId::Rustup,
                        name: "__self__".to_string(),
                    },
                }),
            ),
            _ => return Err(ManagerUninstallRouteError::AmbiguousProvenance),
        };
        return Ok(ManagerUninstallRoutePlan {
            target_manager,
            request,
            strategy: resolution.strategy,
            unknown_override_required: resolution.unknown_override_required,
            used_unknown_override: resolution.used_unknown_override,
        });
    }

    if let Some(formula_name) = manager_homebrew_formula_name(manager) {
        let resolution = resolve_homebrew_manager_uninstall_strategy(
            active_instance,
            allow_unknown_provenance,
            preview_only,
        )
        .map_err(|_| ManagerUninstallRouteError::AmbiguousProvenance)?;
        if resolution.strategy == StrategyKind::ReadOnly {
            return Ok(read_only_uninstall_route_plan(
                manager,
                resolution.unknown_override_required,
                resolution.used_unknown_override,
            ));
        }
        return Ok(ManagerUninstallRoutePlan {
            target_manager: ManagerId::HomebrewFormula,
            request: AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: formula_name.to_string(),
                },
            }),
            strategy: resolution.strategy,
            unknown_override_required: resolution.unknown_override_required,
            used_unknown_override: resolution.used_unknown_override,
        });
    }

    if manager_supports_homebrew_formula_from_instance(manager) {
        let resolution = resolve_homebrew_manager_uninstall_strategy(
            active_instance,
            allow_unknown_provenance,
            preview_only,
        )
        .map_err(|_| ManagerUninstallRouteError::AmbiguousProvenance)?;
        if resolution.strategy == StrategyKind::ReadOnly {
            return Ok(read_only_uninstall_route_plan(
                manager,
                resolution.unknown_override_required,
                resolution.used_unknown_override,
            ));
        }
        let formula_name = resolve_homebrew_formula_name_for_manager(manager, active_instance)
            .ok_or(ManagerUninstallRouteError::FormulaUnresolved)?;
        return Ok(ManagerUninstallRoutePlan {
            target_manager: ManagerId::HomebrewFormula,
            request: AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: formula_name,
                },
            }),
            strategy: resolution.strategy,
            unknown_override_required: resolution.unknown_override_required,
            used_unknown_override: resolution.used_unknown_override,
        });
    }

    Err(ManagerUninstallRouteError::UnsupportedManager)
}

fn read_only_uninstall_route_plan(
    manager: ManagerId,
    unknown_override_required: bool,
    used_unknown_override: bool,
) -> ManagerUninstallRoutePlan {
    ManagerUninstallRoutePlan {
        target_manager: manager,
        request: AdapterRequest::Uninstall(UninstallRequest {
            package: PackageRef {
                manager,
                name: "__self__".to_string(),
            },
        }),
        strategy: StrategyKind::ReadOnly,
        unknown_override_required,
        used_unknown_override,
    }
}

fn homebrew_manager_install_plan(formula_name: &'static str) -> ManagerInstallPlan {
    ManagerInstallPlan {
        target_manager: ManagerId::HomebrewFormula,
        request: AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: formula_name.to_string(),
            },
            version: None,
        }),
        label_key: "service.task.label.install.homebrew_formula",
        label_args: vec![("package", formula_name.to_string())],
    }
}

fn rustup_manager_install_plan(
    options: &ManagerInstallOptions,
) -> Result<ManagerInstallPlan, ManagerInstallPlanError> {
    let version = rustup_install_request_version(options)?;
    Ok(ManagerInstallPlan {
        target_manager: ManagerId::Rustup,
        request: AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: ManagerId::Rustup,
                name: "__self__".to_string(),
            },
            version,
        }),
        label_key: "service.task.label.install.package",
        label_args: vec![
            ("package", "rustup".to_string()),
            ("manager", "rustup".to_string()),
        ],
    })
}

fn rustup_install_request_version(
    options: &ManagerInstallOptions,
) -> Result<Option<String>, ManagerInstallPlanError> {
    let source = options
        .rustup_install_source
        .unwrap_or(RustupInstallSource::OfficialDownload);
    match source {
        RustupInstallSource::OfficialDownload => Ok(Some("officialDownload".to_string())),
        RustupInstallSource::ExistingBinaryPath => {
            let path = options
                .rustup_binary_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or(ManagerInstallPlanError::InvalidRustupBinaryPath)?;
            Ok(Some(format!("existingBinaryPath:{path}")))
        }
    }
}

pub fn build_update_request(
    plan: &ManagerUpdatePlan,
    homebrew_package_name: Option<String>,
) -> Option<AdapterRequest> {
    match (&plan.target, plan.target_manager) {
        (ManagerUpdateTarget::ManagerSelf, ManagerId::HomebrewFormula) => {
            Some(AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: "__self__".to_string(),
                }),
            }))
        }
        (ManagerUpdateTarget::ManagerSelf, ManagerId::Rustup) => {
            Some(AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Rustup,
                    name: "__self__".to_string(),
                }),
            }))
        }
        (ManagerUpdateTarget::HomebrewFormula { .. }, ManagerId::HomebrewFormula) => {
            homebrew_package_name.map(|package_name| {
                AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: package_name,
                    }),
                })
            })
        }
        _ => None,
    }
}

pub fn manager_homebrew_formula_name(manager: ManagerId) -> Option<&'static str> {
    match manager {
        ManagerId::Asdf => Some("asdf"),
        ManagerId::Mise => Some("mise"),
        ManagerId::Mas => Some("mas"),
        ManagerId::Pnpm => Some("pnpm"),
        ManagerId::Yarn => Some("yarn"),
        ManagerId::Pipx => Some("pipx"),
        ManagerId::Poetry => Some("poetry"),
        ManagerId::CargoBinstall => Some("cargo-binstall"),
        ManagerId::Podman => Some("podman"),
        ManagerId::Colima => Some("colima"),
        _ => None,
    }
}

pub fn manager_supports_homebrew_formula_from_instance(manager: ManagerId) -> bool {
    matches!(
        manager,
        ManagerId::Npm
            | ManagerId::Pip
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::Cargo
    )
}

pub fn manager_supports_homebrew_update_strategy_routing(manager: ManagerId) -> bool {
    manager_homebrew_formula_name(manager).is_some()
        || manager_supports_homebrew_formula_from_instance(manager)
}

pub fn resolve_homebrew_formula_name_for_manager(
    manager: ManagerId,
    active_instance: Option<&ManagerInstallInstance>,
) -> Option<String> {
    if let Some(formula_name) = manager_homebrew_formula_name(manager) {
        return Some(formula_name.to_string());
    }

    if manager_supports_homebrew_formula_from_instance(manager)
        && let Some(instance) = active_instance
    {
        return homebrew_formula_name_from_instance(instance);
    }

    None
}

pub fn homebrew_formula_name_from_instance(instance: &ManagerInstallInstance) -> Option<String> {
    let path = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path);
    homebrew_formula_name_from_path(path.as_path())
}

pub fn homebrew_formula_name_from_path(path: &Path) -> Option<String> {
    let components = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    components
        .iter()
        .position(|component| component.eq_ignore_ascii_case("cellar"))
        .and_then(|index| components.get(index + 1))
        .map(|formula_name| formula_name.trim())
        .filter(|formula_name| !formula_name.is_empty())
        .map(|formula_name| formula_name.to_string())
}

pub fn resolve_homebrew_manager_uninstall_strategy(
    active_instance: Option<&ManagerInstallInstance>,
    allow_unknown_provenance: bool,
    preview_only: bool,
) -> Result<UninstallStrategyResolution, UninstallStrategyResolutionError> {
    let Some(instance) = active_instance else {
        return Ok(UninstallStrategyResolution {
            strategy: StrategyKind::HomebrewFormula,
            unknown_override_required: false,
            used_unknown_override: false,
        });
    };

    match instance.uninstall_strategy {
        StrategyKind::HomebrewFormula | StrategyKind::ReadOnly => Ok(UninstallStrategyResolution {
            strategy: instance.uninstall_strategy,
            unknown_override_required: false,
            used_unknown_override: false,
        }),
        StrategyKind::InteractivePrompt
        | StrategyKind::Unknown
        | StrategyKind::ManualRemediation
        | StrategyKind::RustupSelf => {
            if allow_unknown_provenance {
                return Ok(UninstallStrategyResolution {
                    strategy: StrategyKind::HomebrewFormula,
                    unknown_override_required: true,
                    used_unknown_override: true,
                });
            }

            if preview_only {
                return Ok(UninstallStrategyResolution {
                    strategy: StrategyKind::HomebrewFormula,
                    unknown_override_required: true,
                    used_unknown_override: false,
                });
            }

            Err(UninstallStrategyResolutionError::AmbiguousProvenance)
        }
    }
}

pub fn resolve_rustup_uninstall_strategy(
    active_instance: Option<&ManagerInstallInstance>,
    allow_unknown_provenance: bool,
    preview_only: bool,
) -> Result<UninstallStrategyResolution, UninstallStrategyResolutionError> {
    let Some(instance) = active_instance else {
        return Ok(UninstallStrategyResolution {
            strategy: StrategyKind::RustupSelf,
            unknown_override_required: false,
            used_unknown_override: false,
        });
    };

    match instance.uninstall_strategy {
        StrategyKind::HomebrewFormula | StrategyKind::RustupSelf | StrategyKind::ReadOnly => {
            Ok(UninstallStrategyResolution {
                strategy: instance.uninstall_strategy,
                unknown_override_required: false,
                used_unknown_override: false,
            })
        }
        StrategyKind::InteractivePrompt
        | StrategyKind::Unknown
        | StrategyKind::ManualRemediation => {
            let fallback = if instance.competing_provenance == Some(InstallProvenance::Homebrew)
                || rustup_instance_path_looks_homebrew(instance)
            {
                StrategyKind::HomebrewFormula
            } else {
                StrategyKind::RustupSelf
            };

            if allow_unknown_provenance {
                return Ok(UninstallStrategyResolution {
                    strategy: fallback,
                    unknown_override_required: true,
                    used_unknown_override: true,
                });
            }

            if preview_only {
                return Ok(UninstallStrategyResolution {
                    strategy: fallback,
                    unknown_override_required: true,
                    used_unknown_override: false,
                });
            }

            Err(UninstallStrategyResolutionError::AmbiguousProvenance)
        }
    }
}

pub fn resolve_rustup_update_strategy(
    active_instance: Option<&ManagerInstallInstance>,
) -> Result<StrategyKind, UpdateStrategyResolutionError> {
    let Some(instance) = active_instance else {
        return Ok(StrategyKind::RustupSelf);
    };

    match instance.update_strategy {
        StrategyKind::HomebrewFormula | StrategyKind::RustupSelf => Ok(instance.update_strategy),
        StrategyKind::ReadOnly => Err(UpdateStrategyResolutionError::ReadOnly),
        StrategyKind::InteractivePrompt
        | StrategyKind::Unknown
        | StrategyKind::ManualRemediation => {
            Err(UpdateStrategyResolutionError::AmbiguousProvenance)
        }
    }
}

pub fn resolve_homebrew_manager_update_strategy(
    active_instance: Option<&ManagerInstallInstance>,
) -> Result<StrategyKind, UpdateStrategyResolutionError> {
    let Some(instance) = active_instance else {
        return Ok(StrategyKind::HomebrewFormula);
    };

    match instance.update_strategy {
        StrategyKind::HomebrewFormula => Ok(StrategyKind::HomebrewFormula),
        StrategyKind::ReadOnly => Err(UpdateStrategyResolutionError::ReadOnly),
        StrategyKind::InteractivePrompt
        | StrategyKind::Unknown
        | StrategyKind::ManualRemediation
        | StrategyKind::RustupSelf => Err(UpdateStrategyResolutionError::AmbiguousProvenance),
    }
}

fn rustup_instance_path_looks_homebrew(instance: &ManagerInstallInstance) -> bool {
    instance
        .canonical_path
        .as_ref()
        .is_some_and(|path| path.starts_with("/opt/homebrew/") || path.starts_with("/usr/local/"))
        || instance.display_path.starts_with("/opt/homebrew/")
        || instance.display_path.starts_with("/usr/local/")
}

#[cfg(test)]
mod tests {
    use super::{
        ManagerInstallOptions, ManagerInstallPlanError, RustupInstallSource,
        UpdateStrategyResolutionError, manager_homebrew_formula_name, plan_manager_install,
        resolve_homebrew_manager_update_strategy, resolve_rustup_uninstall_strategy,
    };
    use crate::models::{
        AutomationLevel, InstallInstanceIdentityKind, InstallProvenance, ManagerId,
        ManagerInstallInstance, StrategyKind,
    };
    use std::path::PathBuf;

    fn sample_instance() -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "id".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/bin/rustup".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/bin/rustup")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
            is_active: true,
            version: Some("1.0.0".to_string()),
            provenance: InstallProvenance::Homebrew,
            confidence: 0.95,
            decision_margin: Some(0.30),
            automation_level: AutomationLevel::Automatic,
            uninstall_strategy: StrategyKind::HomebrewFormula,
            update_strategy: StrategyKind::HomebrewFormula,
            remediation_strategy: StrategyKind::HomebrewFormula,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        }
    }

    #[test]
    fn homebrew_formula_map_covers_expected_managers() {
        assert_eq!(manager_homebrew_formula_name(ManagerId::Asdf), Some("asdf"));
        assert_eq!(
            manager_homebrew_formula_name(ManagerId::CargoBinstall),
            Some("cargo-binstall")
        );
        assert_eq!(manager_homebrew_formula_name(ManagerId::Npm), None);
    }

    #[test]
    fn manager_install_plan_defaults_rustup_to_official_download() {
        let plan = plan_manager_install(ManagerId::Rustup, None, &ManagerInstallOptions::default())
            .expect("rustup install plan should resolve");
        assert_eq!(plan.target_manager, ManagerId::Rustup);
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Rustup);
                assert_eq!(install.package.name, "__self__");
                assert_eq!(install.version.as_deref(), Some("officialDownload"));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn manager_install_plan_supports_rustup_existing_binary_path() {
        let plan = plan_manager_install(
            ManagerId::Rustup,
            Some("rustupInstaller"),
            &ManagerInstallOptions {
                rustup_install_source: Some(RustupInstallSource::ExistingBinaryPath),
                rustup_binary_path: Some("/tmp/rustup-init".to_string()),
            },
        )
        .expect("rustup install plan should support existing binary source");
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(
                    install.version.as_deref(),
                    Some("existingBinaryPath:/tmp/rustup-init")
                );
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn manager_install_plan_rejects_invalid_rustup_binary_path() {
        let error = plan_manager_install(
            ManagerId::Rustup,
            Some("rustupInstaller"),
            &ManagerInstallOptions {
                rustup_install_source: Some(RustupInstallSource::ExistingBinaryPath),
                rustup_binary_path: Some("   ".to_string()),
            },
        )
        .expect_err("blank rustup binary path should fail");
        assert_eq!(error, ManagerInstallPlanError::InvalidRustupBinaryPath);
    }

    #[test]
    fn rustup_uninstall_resolution_defaults_to_self_without_active_instance() {
        let resolution = resolve_rustup_uninstall_strategy(None, false, false)
            .expect("resolution should succeed");
        assert_eq!(resolution.strategy, StrategyKind::RustupSelf);
        assert!(!resolution.unknown_override_required);
        assert!(!resolution.used_unknown_override);
    }

    #[test]
    fn homebrew_update_resolution_blocks_read_only() {
        let mut instance = sample_instance();
        instance.update_strategy = StrategyKind::ReadOnly;
        let error = resolve_homebrew_manager_update_strategy(Some(&instance))
            .expect_err("read-only should block updates");
        assert_eq!(error, UpdateStrategyResolutionError::ReadOnly);
    }
}
