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
    InvalidOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum RustupInstallSource {
    #[default]
    OfficialDownload,
    ExistingBinaryPath,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum MiseInstallSource {
    #[default]
    OfficialDownload,
    ExistingBinaryPath,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum MiseUninstallCleanupMode {
    #[default]
    ManagerOnly,
    FullCleanup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MiseUninstallConfigRemoval {
    KeepConfig,
    RemoveConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum HomebrewUninstallCleanupMode {
    #[default]
    ManagerOnly,
    FullCleanup,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ManagerInstallOptions {
    pub install_method_override: Option<String>,
    pub rustup_install_source: Option<RustupInstallSource>,
    pub rustup_binary_path: Option<String>,
    pub mise_install_source: Option<MiseInstallSource>,
    pub mise_binary_path: Option<String>,
    pub complete_post_install_setup_automatically: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ManagerUninstallOptions {
    pub homebrew_cleanup_mode: Option<HomebrewUninstallCleanupMode>,
    pub mise_cleanup_mode: Option<MiseUninstallCleanupMode>,
    pub mise_config_removal: Option<MiseUninstallConfigRemoval>,
    pub remove_helm_managed_shell_setup: Option<bool>,
}

const HOMEBREW_MANAGER_UNINSTALL_MARKER: &str = "@@helm.manager.uninstall::";
const SHELL_SETUP_CLEANUP_MARKER: &str = "removeShellSetup";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomebrewManagerUninstallRequestSpec {
    pub formula_name: String,
    pub requested_manager: ManagerId,
    pub cleanup_mode: HomebrewUninstallCleanupMode,
    pub remove_helm_managed_shell_setup: bool,
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
    InvalidMiseBinaryPath,
}

pub fn plan_manager_install(
    manager: ManagerId,
    selected_method: Option<&str>,
    options: &ManagerInstallOptions,
) -> Result<ManagerInstallPlan, ManagerInstallPlanError> {
    let selected_method = effective_manager_install_method(manager, selected_method, options);
    match manager {
        ManagerId::Mise => match selected_method {
            Some("homebrew") => Ok(homebrew_manager_install_plan("mise")),
            Some("scriptInstaller") | None => Ok(mise_manager_install_plan(options)?),
            Some("macports") => Ok(package_manager_install_plan(ManagerId::MacPorts, "mise")),
            Some("cargoInstall") => Ok(package_manager_install_plan(ManagerId::Cargo, "mise")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Asdf => match selected_method {
            Some("homebrew") => Ok(homebrew_manager_install_plan("asdf")),
            Some("scriptInstaller") | None => Ok(asdf_manager_install_plan()),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
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
        ManagerId::Npm => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("node")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Pnpm => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("pnpm")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Yarn => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("yarn")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Pipx => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("pipx")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Pip => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("python")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Poetry => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("poetry")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::RubyGems => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("ruby")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Bundler => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("ruby")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Cargo => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("rust")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::CargoBinstall => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("cargo-binstall")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Podman => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("podman")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        ManagerId::Colima => match selected_method {
            Some("homebrew") | None => Ok(homebrew_manager_install_plan("colima")),
            Some(_) => Err(ManagerInstallPlanError::UnsupportedMethod),
        },
        _ => Err(ManagerInstallPlanError::UnsupportedManager),
    }
}

pub fn manager_install_method_supported(manager: ManagerId, method: &str) -> bool {
    plan_manager_install(manager, Some(method), &ManagerInstallOptions::default()).is_ok()
}

pub fn manager_supported_install_methods(manager: ManagerId) -> Vec<&'static str> {
    crate::registry::manager_install_method_candidates(manager)
        .iter()
        .copied()
        .filter(|method| manager_install_method_supported(manager, method))
        .collect()
}

fn effective_manager_install_method<'a>(
    manager: ManagerId,
    selected_method: Option<&'a str>,
    options: &'a ManagerInstallOptions,
) -> Option<&'a str> {
    match manager {
        // Explicit install-source options are request-scoped intent and take precedence over any
        // persisted selection. This prevents stale preferences from routing install tasks through
        // the wrong manager/label.
        ManagerId::Mise
            if options.mise_install_source.is_some() || options.mise_binary_path.is_some() =>
        {
            Some("scriptInstaller")
        }
        ManagerId::Rustup
            if options.rustup_install_source.is_some() || options.rustup_binary_path.is_some() =>
        {
            Some("rustupInstaller")
        }
        _ => options
            .install_method_override
            .as_deref()
            .or(selected_method),
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
        ManagerId::Asdf => match resolve_asdf_update_strategy(active_instance) {
            Ok(StrategyKind::AsdfSelf) => Ok(ManagerUpdatePlan {
                target_manager: ManagerId::Asdf,
                target: ManagerUpdateTarget::ManagerSelf,
            }),
            Ok(StrategyKind::HomebrewFormula) => Ok(ManagerUpdatePlan {
                target_manager: ManagerId::HomebrewFormula,
                target: ManagerUpdateTarget::HomebrewFormula {
                    formula_name: "asdf".to_string(),
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
    plan_manager_uninstall_route_with_options(
        manager,
        active_instance,
        allow_unknown_provenance,
        preview_only,
        &ManagerUninstallOptions::default(),
    )
}

pub fn plan_manager_uninstall_route_with_options(
    manager: ManagerId,
    active_instance: Option<&ManagerInstallInstance>,
    allow_unknown_provenance: bool,
    preview_only: bool,
    options: &ManagerUninstallOptions,
) -> Result<ManagerUninstallRoutePlan, ManagerUninstallRouteError> {
    if manager == ManagerId::Mise {
        let resolution = resolve_mise_uninstall_strategy(
            active_instance,
            allow_unknown_provenance,
            preview_only,
        )
        .map_err(|_| ManagerUninstallRouteError::AmbiguousProvenance)?;
        let (target_manager, request) = match resolution.target {
            MiseUninstallTarget::HomebrewFormula => (
                ManagerId::HomebrewFormula,
                AdapterRequest::Uninstall(UninstallRequest {
                    package: PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: "mise".to_string(),
                    },
                }),
            ),
            MiseUninstallTarget::MacPortsPort => (
                ManagerId::MacPorts,
                AdapterRequest::Uninstall(UninstallRequest {
                    package: PackageRef {
                        manager: ManagerId::MacPorts,
                        name: "mise".to_string(),
                    },
                }),
            ),
            MiseUninstallTarget::SelfManaged | MiseUninstallTarget::ReadOnly => {
                let package_name = mise_uninstall_package_name(options)
                    .ok_or(ManagerUninstallRouteError::InvalidOptions)?;
                (
                    ManagerId::Mise,
                    AdapterRequest::Uninstall(UninstallRequest {
                        package: PackageRef {
                            manager: ManagerId::Mise,
                            name: package_name,
                        },
                    }),
                )
            }
        };

        if matches!(resolution.target, MiseUninstallTarget::HomebrewFormula) {
            if options.mise_cleanup_mode.is_some() || options.mise_config_removal.is_some() {
                return Err(ManagerUninstallRouteError::InvalidOptions);
            }
            if !homebrew_cleanup_options_are_default(options)
                || remove_shell_setup_requested(options)
            {
                let cleanup_mode = homebrew_cleanup_mode(options);
                let package_name = encode_homebrew_manager_uninstall_package_name_with_options(
                    "mise",
                    manager,
                    cleanup_mode,
                    remove_shell_setup_requested(options),
                );
                return Ok(ManagerUninstallRoutePlan {
                    target_manager: ManagerId::HomebrewFormula,
                    request: AdapterRequest::Uninstall(UninstallRequest {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: package_name,
                        },
                    }),
                    strategy: resolution.strategy,
                    unknown_override_required: resolution.unknown_override_required,
                    used_unknown_override: resolution.used_unknown_override,
                });
            }
        }

        if matches!(resolution.target, MiseUninstallTarget::MacPortsPort)
            && !manager_only_uninstall_options_are_default(options)
        {
            return Err(ManagerUninstallRouteError::InvalidOptions);
        }

        if matches!(
            resolution.target,
            MiseUninstallTarget::SelfManaged | MiseUninstallTarget::ReadOnly
        ) && !homebrew_cleanup_options_are_default(options)
        {
            return Err(ManagerUninstallRouteError::InvalidOptions);
        }

        return Ok(ManagerUninstallRoutePlan {
            target_manager,
            request,
            strategy: resolution.strategy,
            unknown_override_required: resolution.unknown_override_required,
            used_unknown_override: resolution.used_unknown_override,
        });
    }

    if manager == ManagerId::Rustup {
        let resolution = resolve_rustup_uninstall_strategy(
            active_instance,
            allow_unknown_provenance,
            preview_only,
        )
        .map_err(|_| ManagerUninstallRouteError::AmbiguousProvenance)?;
        let (target_manager, request) = match resolution.strategy {
            StrategyKind::HomebrewFormula => {
                let cleanup_mode = homebrew_cleanup_mode(options);
                (
                    ManagerId::HomebrewFormula,
                    AdapterRequest::Uninstall(UninstallRequest {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: encode_homebrew_manager_uninstall_package_name_with_options(
                                "rustup",
                                manager,
                                cleanup_mode,
                                remove_shell_setup_requested(options),
                            ),
                        },
                    }),
                )
            }
            StrategyKind::RustupSelf | StrategyKind::ReadOnly => (
                ManagerId::Rustup,
                AdapterRequest::Uninstall(UninstallRequest {
                    package: PackageRef {
                        manager: ManagerId::Rustup,
                        name: if remove_shell_setup_requested(options) {
                            "__self__:removeShellSetup".to_string()
                        } else {
                            "__self__".to_string()
                        },
                    },
                }),
            ),
            _ => return Err(ManagerUninstallRouteError::AmbiguousProvenance),
        };
        if matches!(
            resolution.strategy,
            StrategyKind::RustupSelf | StrategyKind::ReadOnly
        ) && !homebrew_cleanup_options_are_default(options)
        {
            return Err(ManagerUninstallRouteError::InvalidOptions);
        }
        if matches!(resolution.strategy, StrategyKind::HomebrewFormula)
            && (options.mise_cleanup_mode.is_some() || options.mise_config_removal.is_some())
        {
            return Err(ManagerUninstallRouteError::InvalidOptions);
        }
        return Ok(ManagerUninstallRoutePlan {
            target_manager,
            request,
            strategy: resolution.strategy,
            unknown_override_required: resolution.unknown_override_required,
            used_unknown_override: resolution.used_unknown_override,
        });
    }

    if manager == ManagerId::Asdf {
        let resolution = resolve_asdf_uninstall_strategy(
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
                        name: encode_homebrew_manager_uninstall_package_name_with_options(
                            "asdf",
                            manager,
                            homebrew_cleanup_mode(options),
                            remove_shell_setup_requested(options),
                        ),
                    },
                }),
            ),
            StrategyKind::AsdfSelf => (
                ManagerId::Asdf,
                AdapterRequest::Uninstall(UninstallRequest {
                    package: PackageRef {
                        manager: ManagerId::Asdf,
                        name: if remove_shell_setup_requested(options) {
                            "__self__:removeShellSetup".to_string()
                        } else {
                            "__self__".to_string()
                        },
                    },
                }),
            ),
            StrategyKind::ReadOnly => {
                return Ok(read_only_uninstall_route_plan(
                    manager,
                    resolution.unknown_override_required,
                    resolution.used_unknown_override,
                ));
            }
            _ => return Err(ManagerUninstallRouteError::AmbiguousProvenance),
        };
        if matches!(resolution.strategy, StrategyKind::AsdfSelf)
            && !homebrew_cleanup_options_are_default(options)
        {
            return Err(ManagerUninstallRouteError::InvalidOptions);
        }
        if options.mise_cleanup_mode.is_some() || options.mise_config_removal.is_some() {
            return Err(ManagerUninstallRouteError::InvalidOptions);
        }
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
        if options.mise_cleanup_mode.is_some() || options.mise_config_removal.is_some() {
            return Err(ManagerUninstallRouteError::InvalidOptions);
        }
        return Ok(ManagerUninstallRoutePlan {
            target_manager: ManagerId::HomebrewFormula,
            request: AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: encode_homebrew_manager_uninstall_package_name_with_options(
                        formula_name,
                        manager,
                        homebrew_cleanup_mode(options),
                        remove_shell_setup_requested(options),
                    ),
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
        if options.mise_cleanup_mode.is_some() || options.mise_config_removal.is_some() {
            return Err(ManagerUninstallRouteError::InvalidOptions);
        }
        return Ok(ManagerUninstallRoutePlan {
            target_manager: ManagerId::HomebrewFormula,
            request: AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::HomebrewFormula,
                    name: encode_homebrew_manager_uninstall_package_name_with_options(
                        &formula_name,
                        manager,
                        homebrew_cleanup_mode(options),
                        remove_shell_setup_requested(options),
                    ),
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

fn package_manager_install_plan(
    target_manager: ManagerId,
    package_name: &'static str,
) -> ManagerInstallPlan {
    ManagerInstallPlan {
        target_manager,
        request: AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: target_manager,
                name: package_name.to_string(),
            },
            version: None,
        }),
        label_key: "service.task.label.install.package",
        label_args: vec![
            ("package", package_name.to_string()),
            ("manager", target_manager.as_str().to_string()),
        ],
    }
}

fn mise_manager_install_plan(
    options: &ManagerInstallOptions,
) -> Result<ManagerInstallPlan, ManagerInstallPlanError> {
    let version = mise_install_request_version(options)?;
    Ok(ManagerInstallPlan {
        target_manager: ManagerId::Mise,
        request: AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: ManagerId::Mise,
                name: "__self__".to_string(),
            },
            version,
        }),
        label_key: "service.task.label.install.package",
        label_args: vec![
            ("package", "mise".to_string()),
            ("manager", "mise".to_string()),
        ],
    })
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

fn asdf_manager_install_plan() -> ManagerInstallPlan {
    ManagerInstallPlan {
        target_manager: ManagerId::Asdf,
        request: AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager: ManagerId::Asdf,
                name: "__self__".to_string(),
            },
            version: Some("scriptInstaller:officialDownload".to_string()),
        }),
        label_key: "service.task.label.install.package",
        label_args: vec![
            ("package", "asdf".to_string()),
            ("manager", "asdf".to_string()),
        ],
    }
}

fn mise_install_request_version(
    options: &ManagerInstallOptions,
) -> Result<Option<String>, ManagerInstallPlanError> {
    let source = options
        .mise_install_source
        .unwrap_or(MiseInstallSource::OfficialDownload);
    match source {
        MiseInstallSource::OfficialDownload => Ok(Some("scriptInstaller:officialDownload".into())),
        MiseInstallSource::ExistingBinaryPath => {
            let path = options
                .mise_binary_path
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or(ManagerInstallPlanError::InvalidMiseBinaryPath)?;
            if !Path::new(path).is_absolute() {
                return Err(ManagerInstallPlanError::InvalidMiseBinaryPath);
            }
            Ok(Some(format!("scriptInstaller:existingBinaryPath:{path}")))
        }
    }
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
            if !Path::new(path).is_absolute() {
                return Err(ManagerInstallPlanError::InvalidRustupBinaryPath);
            }
            Ok(Some(format!("existingBinaryPath:{path}")))
        }
    }
}

fn homebrew_cleanup_mode(options: &ManagerUninstallOptions) -> HomebrewUninstallCleanupMode {
    options
        .homebrew_cleanup_mode
        .unwrap_or(HomebrewUninstallCleanupMode::ManagerOnly)
}

fn homebrew_cleanup_options_are_default(options: &ManagerUninstallOptions) -> bool {
    matches!(
        homebrew_cleanup_mode(options),
        HomebrewUninstallCleanupMode::ManagerOnly
    )
}

fn manager_only_uninstall_options_are_default(options: &ManagerUninstallOptions) -> bool {
    homebrew_cleanup_options_are_default(options)
        && options.mise_cleanup_mode.is_none()
        && options.mise_config_removal.is_none()
        && options.remove_helm_managed_shell_setup.is_none()
}

fn remove_shell_setup_requested(options: &ManagerUninstallOptions) -> bool {
    options.remove_helm_managed_shell_setup.unwrap_or(false)
}

pub fn strip_shell_setup_cleanup_suffix(value: &str) -> (&str, bool) {
    let trimmed = value.trim();
    if let Some(stripped) = trimmed.strip_suffix(":removeShellSetup") {
        (stripped, true)
    } else {
        (trimmed, false)
    }
}

pub fn encode_homebrew_manager_uninstall_package_name(
    formula_name: &str,
    requested_manager: ManagerId,
    cleanup_mode: HomebrewUninstallCleanupMode,
) -> String {
    encode_homebrew_manager_uninstall_package_name_with_options(
        formula_name,
        requested_manager,
        cleanup_mode,
        false,
    )
}

pub fn encode_homebrew_manager_uninstall_package_name_with_options(
    formula_name: &str,
    requested_manager: ManagerId,
    cleanup_mode: HomebrewUninstallCleanupMode,
    remove_helm_managed_shell_setup: bool,
) -> String {
    if matches!(cleanup_mode, HomebrewUninstallCleanupMode::ManagerOnly)
        && !remove_helm_managed_shell_setup
    {
        return formula_name.to_string();
    }
    let cleanup_token = match cleanup_mode {
        HomebrewUninstallCleanupMode::ManagerOnly => "managerOnly",
        HomebrewUninstallCleanupMode::FullCleanup => "fullCleanup",
    };
    format!(
        "{HOMEBREW_MANAGER_UNINSTALL_MARKER}{formula_name}::{}::{cleanup_token}::{}",
        requested_manager.as_str(),
        if remove_helm_managed_shell_setup {
            SHELL_SETUP_CLEANUP_MARKER
        } else {
            "keepShellSetup"
        }
    )
}

pub fn parse_homebrew_manager_uninstall_package_name(
    value: &str,
) -> Option<HomebrewManagerUninstallRequestSpec> {
    let stripped = value.strip_prefix(HOMEBREW_MANAGER_UNINSTALL_MARKER)?;
    let mut parts = stripped.splitn(4, "::");
    let formula_name = parts.next()?.trim();
    let manager_raw = parts.next()?.trim();
    let cleanup_raw = parts.next()?.trim();
    let shell_cleanup_raw = parts.next().map(str::trim);
    if formula_name.is_empty() {
        return None;
    }
    let requested_manager = manager_raw.parse::<ManagerId>().ok()?;
    let cleanup_mode = match cleanup_raw {
        "fullCleanup" => HomebrewUninstallCleanupMode::FullCleanup,
        "managerOnly" => HomebrewUninstallCleanupMode::ManagerOnly,
        _ => return None,
    };
    let remove_helm_managed_shell_setup = match shell_cleanup_raw {
        Some(SHELL_SETUP_CLEANUP_MARKER) => true,
        Some("keepShellSetup") | None => false,
        _ => return None,
    };
    Some(HomebrewManagerUninstallRequestSpec {
        formula_name: formula_name.to_string(),
        requested_manager,
        cleanup_mode,
        remove_helm_managed_shell_setup,
    })
}

fn mise_uninstall_package_name(options: &ManagerUninstallOptions) -> Option<String> {
    let cleanup_mode = options
        .mise_cleanup_mode
        .unwrap_or(MiseUninstallCleanupMode::ManagerOnly);
    let remove_shell_setup = remove_shell_setup_requested(options);
    let with_shell_cleanup = |base: String| {
        if remove_shell_setup {
            format!("{base}:removeShellSetup")
        } else {
            base
        }
    };
    match cleanup_mode {
        MiseUninstallCleanupMode::ManagerOnly => {
            if options.mise_config_removal.is_some() {
                return None;
            }
            Some(with_shell_cleanup("__self__".to_string()))
        }
        MiseUninstallCleanupMode::FullCleanup => match options.mise_config_removal {
            Some(MiseUninstallConfigRemoval::KeepConfig) => Some(with_shell_cleanup(
                "__self__:fullCleanup:keepConfig".to_string(),
            )),
            Some(MiseUninstallConfigRemoval::RemoveConfig) => Some(with_shell_cleanup(
                "__self__:fullCleanup:removeConfig".to_string(),
            )),
            None => None,
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MiseUninstallTarget {
    HomebrewFormula,
    MacPortsPort,
    SelfManaged,
    ReadOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MiseUninstallResolution {
    target: MiseUninstallTarget,
    strategy: StrategyKind,
    unknown_override_required: bool,
    used_unknown_override: bool,
}

fn resolve_mise_uninstall_strategy(
    active_instance: Option<&ManagerInstallInstance>,
    allow_unknown_provenance: bool,
    preview_only: bool,
) -> Result<MiseUninstallResolution, UninstallStrategyResolutionError> {
    let Some(instance) = active_instance else {
        return Ok(MiseUninstallResolution {
            target: MiseUninstallTarget::SelfManaged,
            strategy: StrategyKind::InteractivePrompt,
            unknown_override_required: false,
            used_unknown_override: false,
        });
    };

    match instance.provenance {
        InstallProvenance::Homebrew => Ok(MiseUninstallResolution {
            target: MiseUninstallTarget::HomebrewFormula,
            strategy: StrategyKind::HomebrewFormula,
            unknown_override_required: false,
            used_unknown_override: false,
        }),
        InstallProvenance::Macports => Ok(MiseUninstallResolution {
            target: MiseUninstallTarget::MacPortsPort,
            strategy: StrategyKind::InteractivePrompt,
            unknown_override_required: false,
            used_unknown_override: false,
        }),
        InstallProvenance::System
        | InstallProvenance::EnterpriseManaged
        | InstallProvenance::Nix => Ok(MiseUninstallResolution {
            target: MiseUninstallTarget::ReadOnly,
            strategy: StrategyKind::ReadOnly,
            unknown_override_required: false,
            used_unknown_override: false,
        }),
        InstallProvenance::Unknown => {
            if allow_unknown_provenance {
                return Ok(MiseUninstallResolution {
                    target: MiseUninstallTarget::SelfManaged,
                    strategy: StrategyKind::InteractivePrompt,
                    unknown_override_required: true,
                    used_unknown_override: true,
                });
            }

            if preview_only {
                return Ok(MiseUninstallResolution {
                    target: MiseUninstallTarget::SelfManaged,
                    strategy: StrategyKind::InteractivePrompt,
                    unknown_override_required: true,
                    used_unknown_override: false,
                });
            }

            Err(UninstallStrategyResolutionError::AmbiguousProvenance)
        }
        InstallProvenance::SourceBuild
        | InstallProvenance::Asdf
        | InstallProvenance::Mise
        | InstallProvenance::RustupInit => Ok(MiseUninstallResolution {
            target: MiseUninstallTarget::SelfManaged,
            strategy: StrategyKind::InteractivePrompt,
            unknown_override_required: false,
            used_unknown_override: false,
        }),
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
        (ManagerUpdateTarget::ManagerSelf, ManagerId::Mise) => {
            Some(AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Mise,
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
        | StrategyKind::RustupSelf
        | StrategyKind::AsdfSelf => {
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
        | StrategyKind::ManualRemediation
        | StrategyKind::AsdfSelf => {
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

pub fn resolve_asdf_uninstall_strategy(
    active_instance: Option<&ManagerInstallInstance>,
    allow_unknown_provenance: bool,
    preview_only: bool,
) -> Result<UninstallStrategyResolution, UninstallStrategyResolutionError> {
    let Some(instance) = active_instance else {
        return Ok(UninstallStrategyResolution {
            strategy: StrategyKind::AsdfSelf,
            unknown_override_required: false,
            used_unknown_override: false,
        });
    };

    match instance.uninstall_strategy {
        StrategyKind::HomebrewFormula | StrategyKind::AsdfSelf | StrategyKind::ReadOnly => {
            Ok(UninstallStrategyResolution {
                strategy: instance.uninstall_strategy,
                unknown_override_required: false,
                used_unknown_override: false,
            })
        }
        StrategyKind::InteractivePrompt
        | StrategyKind::Unknown
        | StrategyKind::ManualRemediation
        | StrategyKind::RustupSelf => {
            let fallback = if instance.competing_provenance == Some(InstallProvenance::Homebrew)
                || asdf_instance_path_looks_homebrew(instance)
            {
                StrategyKind::HomebrewFormula
            } else {
                StrategyKind::AsdfSelf
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
        | StrategyKind::ManualRemediation
        | StrategyKind::AsdfSelf => Err(UpdateStrategyResolutionError::AmbiguousProvenance),
    }
}

pub fn resolve_asdf_update_strategy(
    active_instance: Option<&ManagerInstallInstance>,
) -> Result<StrategyKind, UpdateStrategyResolutionError> {
    let Some(instance) = active_instance else {
        return Ok(StrategyKind::AsdfSelf);
    };

    match instance.update_strategy {
        StrategyKind::HomebrewFormula | StrategyKind::AsdfSelf => Ok(instance.update_strategy),
        StrategyKind::ReadOnly => Err(UpdateStrategyResolutionError::ReadOnly),
        StrategyKind::InteractivePrompt
        | StrategyKind::Unknown
        | StrategyKind::ManualRemediation
        | StrategyKind::RustupSelf => Err(UpdateStrategyResolutionError::AmbiguousProvenance),
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
        | StrategyKind::RustupSelf
        | StrategyKind::AsdfSelf => Err(UpdateStrategyResolutionError::AmbiguousProvenance),
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

fn asdf_instance_path_looks_homebrew(instance: &ManagerInstallInstance) -> bool {
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
        HomebrewUninstallCleanupMode, ManagerInstallOptions, ManagerInstallPlanError,
        ManagerUninstallOptions, ManagerUninstallRouteError, MiseInstallSource,
        MiseUninstallCleanupMode, MiseUninstallConfigRemoval, RustupInstallSource,
        UpdateStrategyResolutionError, encode_homebrew_manager_uninstall_package_name,
        encode_homebrew_manager_uninstall_package_name_with_options, manager_homebrew_formula_name,
        manager_supported_install_methods, parse_homebrew_manager_uninstall_package_name,
        plan_manager_install, plan_manager_uninstall_route_with_options,
        resolve_asdf_update_strategy, resolve_homebrew_manager_update_strategy,
        resolve_rustup_uninstall_strategy,
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
    fn manager_supported_install_methods_filters_to_planner_supported_subset() {
        assert_eq!(
            manager_supported_install_methods(ManagerId::Npm),
            vec!["homebrew"]
        );
        assert_eq!(
            manager_supported_install_methods(ManagerId::Pnpm),
            vec!["homebrew"]
        );
        assert_eq!(
            manager_supported_install_methods(ManagerId::Poetry),
            vec!["homebrew"]
        );
        assert_eq!(
            manager_supported_install_methods(ManagerId::Cargo),
            vec!["homebrew"]
        );
        assert_eq!(
            manager_supported_install_methods(ManagerId::DockerDesktop),
            Vec::<&'static str>::new()
        );
    }

    #[test]
    fn manager_install_plan_routes_supported_homebrew_manager_set() {
        let cases = [
            (ManagerId::Npm, "node"),
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Pip, "python"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::RubyGems, "ruby"),
            (ManagerId::Bundler, "ruby"),
            (ManagerId::Cargo, "rust"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, expected_formula) in cases {
            let plan =
                plan_manager_install(manager, Some("homebrew"), &ManagerInstallOptions::default())
                    .expect("homebrew manager install plan should resolve");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            match plan.request {
                crate::adapters::AdapterRequest::Install(install) => {
                    assert_eq!(install.package.manager, ManagerId::HomebrewFormula);
                    assert_eq!(install.package.name, expected_formula);
                }
                other => panic!("unexpected request for {}: {other:?}", manager.as_str()),
            }
        }
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
                ..ManagerInstallOptions::default()
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
                ..ManagerInstallOptions::default()
            },
        )
        .expect_err("blank rustup binary path should fail");
        assert_eq!(error, ManagerInstallPlanError::InvalidRustupBinaryPath);
    }

    #[test]
    fn manager_install_plan_defaults_mise_to_script_installer() {
        let plan = plan_manager_install(ManagerId::Mise, None, &ManagerInstallOptions::default())
            .expect("mise install plan should resolve");
        assert_eq!(plan.target_manager, ManagerId::Mise);
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Mise);
                assert_eq!(install.package.name, "__self__");
                assert_eq!(
                    install.version.as_deref(),
                    Some("scriptInstaller:officialDownload")
                );
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn manager_install_plan_defaults_asdf_to_script_installer() {
        let plan = plan_manager_install(ManagerId::Asdf, None, &ManagerInstallOptions::default())
            .expect("asdf install plan should resolve");
        assert_eq!(plan.target_manager, ManagerId::Asdf);
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Asdf);
                assert_eq!(install.package.name, "__self__");
                assert_eq!(
                    install.version.as_deref(),
                    Some("scriptInstaller:officialDownload")
                );
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn manager_install_plan_supports_mise_existing_binary_path() {
        let plan = plan_manager_install(
            ManagerId::Mise,
            Some("scriptInstaller"),
            &ManagerInstallOptions {
                mise_install_source: Some(MiseInstallSource::ExistingBinaryPath),
                mise_binary_path: Some("/tmp/mise".to_string()),
                ..ManagerInstallOptions::default()
            },
        )
        .expect("mise existing binary install should resolve");
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(
                    install.version.as_deref(),
                    Some("scriptInstaller:existingBinaryPath:/tmp/mise")
                );
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn manager_install_plan_prefers_mise_options_over_stale_homebrew_selection() {
        let plan = plan_manager_install(
            ManagerId::Mise,
            Some("homebrew"),
            &ManagerInstallOptions {
                mise_install_source: Some(MiseInstallSource::OfficialDownload),
                ..ManagerInstallOptions::default()
            },
        )
        .expect("mise install options should override stale selected method");
        assert_eq!(plan.target_manager, ManagerId::Mise);
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Mise);
                assert_eq!(
                    install.version.as_deref(),
                    Some("scriptInstaller:officialDownload")
                );
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn manager_install_plan_prefers_rustup_options_over_stale_homebrew_selection() {
        let plan = plan_manager_install(
            ManagerId::Rustup,
            Some("homebrew"),
            &ManagerInstallOptions {
                rustup_install_source: Some(RustupInstallSource::OfficialDownload),
                ..ManagerInstallOptions::default()
            },
        )
        .expect("rustup install options should override stale selected method");
        assert_eq!(plan.target_manager, ManagerId::Rustup);
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Rustup);
                assert_eq!(install.version.as_deref(), Some("officialDownload"));
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn manager_install_plan_prefers_request_scoped_method_override() {
        let plan = plan_manager_install(
            ManagerId::Rustup,
            Some("rustupInstaller"),
            &ManagerInstallOptions {
                install_method_override: Some("homebrew".to_string()),
                ..ManagerInstallOptions::default()
            },
        )
        .expect("request-scoped method override should be honored");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(install.package.name, "rustup");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn manager_install_plan_keeps_rustup_source_options_precedence_over_method_override() {
        let plan = plan_manager_install(
            ManagerId::Rustup,
            Some("rustupInstaller"),
            &ManagerInstallOptions {
                install_method_override: Some("homebrew".to_string()),
                rustup_install_source: Some(RustupInstallSource::OfficialDownload),
                ..ManagerInstallOptions::default()
            },
        )
        .expect("rustup source options should take precedence");
        assert_eq!(plan.target_manager, ManagerId::Rustup);
        match plan.request {
            crate::adapters::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Rustup);
                assert_eq!(install.version.as_deref(), Some("officialDownload"));
            }
            other => panic!("unexpected request: {other:?}"),
        }
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

    #[test]
    fn asdf_update_resolution_supports_asdf_self_strategy() {
        let mut instance = sample_instance();
        instance.manager = ManagerId::Asdf;
        instance.display_path = PathBuf::from("/Users/example/.asdf/bin/asdf");
        instance.canonical_path = Some(PathBuf::from("/Users/example/.asdf/bin/asdf"));
        instance.update_strategy = StrategyKind::AsdfSelf;
        let strategy = resolve_asdf_update_strategy(Some(&instance))
            .expect("asdf self strategy should resolve");
        assert_eq!(strategy, StrategyKind::AsdfSelf);
    }

    #[test]
    fn mise_uninstall_full_cleanup_requires_explicit_config_choice() {
        let error = plan_manager_uninstall_route_with_options(
            ManagerId::Mise,
            None,
            false,
            false,
            &ManagerUninstallOptions {
                homebrew_cleanup_mode: None,
                mise_cleanup_mode: Some(MiseUninstallCleanupMode::FullCleanup),
                mise_config_removal: None,
                remove_helm_managed_shell_setup: None,
            },
        )
        .expect_err("full cleanup should require config selection");
        assert_eq!(error, ManagerUninstallRouteError::InvalidOptions);
    }

    #[test]
    fn mise_uninstall_manager_only_rejects_config_choice() {
        let error = plan_manager_uninstall_route_with_options(
            ManagerId::Mise,
            None,
            false,
            false,
            &ManagerUninstallOptions {
                homebrew_cleanup_mode: None,
                mise_cleanup_mode: Some(MiseUninstallCleanupMode::ManagerOnly),
                mise_config_removal: Some(MiseUninstallConfigRemoval::RemoveConfig),
                remove_helm_managed_shell_setup: None,
            },
        )
        .expect_err("manager-only uninstall should not accept config removal options");
        assert_eq!(error, ManagerUninstallRouteError::InvalidOptions);
    }

    #[test]
    fn mise_uninstall_full_cleanup_routes_self_request_with_mode_suffix() {
        let route = plan_manager_uninstall_route_with_options(
            ManagerId::Mise,
            None,
            false,
            false,
            &ManagerUninstallOptions {
                homebrew_cleanup_mode: None,
                mise_cleanup_mode: Some(MiseUninstallCleanupMode::FullCleanup),
                mise_config_removal: Some(MiseUninstallConfigRemoval::KeepConfig),
                remove_helm_managed_shell_setup: None,
            },
        )
        .expect("full cleanup with config choice should route");
        match route.request {
            crate::adapters::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::Mise);
                assert_eq!(uninstall.package.name, "__self__:fullCleanup:keepConfig");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn homebrew_uninstall_package_name_roundtrips_full_cleanup() {
        let encoded = encode_homebrew_manager_uninstall_package_name(
            "rustup",
            ManagerId::Rustup,
            HomebrewUninstallCleanupMode::FullCleanup,
        );
        let parsed = parse_homebrew_manager_uninstall_package_name(encoded.as_str())
            .expect("encoded uninstall package marker should parse");
        assert_eq!(parsed.formula_name, "rustup");
        assert_eq!(parsed.requested_manager, ManagerId::Rustup);
        assert_eq!(
            parsed.cleanup_mode,
            HomebrewUninstallCleanupMode::FullCleanup
        );
        assert!(!parsed.remove_helm_managed_shell_setup);
    }

    #[test]
    fn rustup_homebrew_uninstall_route_encodes_full_cleanup_marker() {
        let route = plan_manager_uninstall_route_with_options(
            ManagerId::Rustup,
            Some(&sample_instance()),
            false,
            false,
            &ManagerUninstallOptions {
                homebrew_cleanup_mode: Some(HomebrewUninstallCleanupMode::FullCleanup),
                ..ManagerUninstallOptions::default()
            },
        )
        .expect("rustup homebrew uninstall should route");
        assert_eq!(route.target_manager, ManagerId::HomebrewFormula);
        match route.request {
            crate::adapters::AdapterRequest::Uninstall(uninstall) => {
                let parsed =
                    parse_homebrew_manager_uninstall_package_name(uninstall.package.name.as_str())
                        .expect("homebrew uninstall request should include cleanup metadata");
                assert_eq!(parsed.formula_name, "rustup");
                assert_eq!(parsed.requested_manager, ManagerId::Rustup);
                assert_eq!(
                    parsed.cleanup_mode,
                    HomebrewUninstallCleanupMode::FullCleanup
                );
                assert!(!parsed.remove_helm_managed_shell_setup);
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn rustup_self_uninstall_route_can_request_shell_setup_cleanup() {
        let mut instance = sample_instance();
        instance.display_path = PathBuf::from("/Users/example/.cargo/bin/rustup");
        instance.canonical_path = Some(PathBuf::from("/Users/example/.cargo/bin/rustup"));
        instance.uninstall_strategy = StrategyKind::RustupSelf;
        let route = plan_manager_uninstall_route_with_options(
            ManagerId::Rustup,
            Some(&instance),
            false,
            false,
            &ManagerUninstallOptions {
                remove_helm_managed_shell_setup: Some(true),
                ..ManagerUninstallOptions::default()
            },
        )
        .expect("rustup self uninstall should route");
        match route.request {
            crate::adapters::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.name, "__self__:removeShellSetup");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn homebrew_uninstall_package_name_roundtrips_shell_setup_cleanup_option() {
        let encoded = encode_homebrew_manager_uninstall_package_name_with_options(
            "rustup",
            ManagerId::Rustup,
            HomebrewUninstallCleanupMode::ManagerOnly,
            true,
        );
        let parsed = parse_homebrew_manager_uninstall_package_name(encoded.as_str())
            .expect("encoded uninstall package marker should parse");
        assert_eq!(parsed.formula_name, "rustup");
        assert_eq!(
            parsed.cleanup_mode,
            HomebrewUninstallCleanupMode::ManagerOnly
        );
        assert!(parsed.remove_helm_managed_shell_setup);
    }

    #[test]
    fn rustup_self_uninstall_route_rejects_homebrew_cleanup_options() {
        let mut instance = sample_instance();
        instance.display_path = PathBuf::from("/Users/example/.cargo/bin/rustup");
        instance.canonical_path = Some(PathBuf::from("/Users/example/.cargo/bin/rustup"));
        instance.uninstall_strategy = StrategyKind::RustupSelf;
        let error = plan_manager_uninstall_route_with_options(
            ManagerId::Rustup,
            Some(&instance),
            false,
            false,
            &ManagerUninstallOptions {
                homebrew_cleanup_mode: Some(HomebrewUninstallCleanupMode::FullCleanup),
                ..ManagerUninstallOptions::default()
            },
        )
        .expect_err("rustup-self strategy should reject homebrew cleanup options");
        assert_eq!(error, ManagerUninstallRouteError::InvalidOptions);
    }

    #[test]
    fn asdf_self_uninstall_routes_to_asdf_manager() {
        let mut instance = sample_instance();
        instance.manager = ManagerId::Asdf;
        instance.display_path = PathBuf::from("/Users/example/.asdf/bin/asdf");
        instance.canonical_path = Some(PathBuf::from("/Users/example/.asdf/bin/asdf"));
        instance.uninstall_strategy = StrategyKind::AsdfSelf;
        instance.update_strategy = StrategyKind::AsdfSelf;
        let route = plan_manager_uninstall_route_with_options(
            ManagerId::Asdf,
            Some(&instance),
            false,
            false,
            &ManagerUninstallOptions::default(),
        )
        .expect("asdf self uninstall should route");
        assert_eq!(route.target_manager, ManagerId::Asdf);
        match route.request {
            crate::adapters::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::Asdf);
                assert_eq!(uninstall.package.name, "__self__");
            }
            other => panic!("unexpected request: {other:?}"),
        }
    }

    #[test]
    fn asdf_self_uninstall_rejects_homebrew_cleanup_options() {
        let mut instance = sample_instance();
        instance.manager = ManagerId::Asdf;
        instance.display_path = PathBuf::from("/Users/example/.asdf/bin/asdf");
        instance.canonical_path = Some(PathBuf::from("/Users/example/.asdf/bin/asdf"));
        instance.uninstall_strategy = StrategyKind::AsdfSelf;
        let error = plan_manager_uninstall_route_with_options(
            ManagerId::Asdf,
            Some(&instance),
            false,
            false,
            &ManagerUninstallOptions {
                homebrew_cleanup_mode: Some(HomebrewUninstallCleanupMode::FullCleanup),
                ..ManagerUninstallOptions::default()
            },
        )
        .expect_err("asdf self uninstall should reject homebrew cleanup options");
        assert_eq!(error, ManagerUninstallRouteError::InvalidOptions);
    }
}
