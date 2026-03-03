use crate::doctor::{
    DoctorFinding, FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL,
    FINDING_CODE_POST_INSTALL_SETUP_REQUIRED, ISSUE_CODE_METADATA_ONLY_INSTALL,
    ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED, fingerprint_for_metadata_only_install,
    fingerprint_for_post_install_setup_required,
};
use crate::models::ManagerId;
use serde::{Deserialize, Serialize};

pub const REPAIR_KNOWLEDGE_SOURCE: &str = "embedded_local";
pub const REPAIR_KNOWLEDGE_VERSION: &str = "v0";
pub const REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW: &str = "reinstall_manager_via_homebrew";
pub const REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY: &str = "remove_stale_package_entry";
pub const REPAIR_OPTION_APPLY_POST_INSTALL_SETUP_DEFAULTS: &str =
    "apply_post_install_setup_defaults";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAutomationLevel {
    Automatic,
    NeedsConfirmation,
    ReadOnly,
}

impl RepairAutomationLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::NeedsConfirmation => "needs_confirmation",
            Self::ReadOnly => "read_only",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairAction {
    ReinstallManagerViaHomebrew,
    RemoveStalePackageEntry,
    ApplyPostInstallSetupDefaults,
}

impl RepairAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReinstallManagerViaHomebrew => "reinstall_manager_via_homebrew",
            Self::RemoveStalePackageEntry => "remove_stale_package_entry",
            Self::ApplyPostInstallSetupDefaults => "apply_post_install_setup_defaults",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairOption {
    pub option_id: String,
    pub action: RepairAction,
    pub title: String,
    pub description: String,
    pub recommended: bool,
    pub requires_confirmation: bool,
    pub automation_level: RepairAutomationLevel,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairPlan {
    pub manager_id: String,
    pub source_manager_id: Option<String>,
    pub package_name: Option<String>,
    pub issue_code: String,
    pub finding_code: String,
    pub fingerprint: String,
    pub knowledge_source: String,
    pub knowledge_version: String,
    pub options: Vec<RepairOption>,
}

pub fn plan_for_finding(finding: &DoctorFinding) -> Option<RepairPlan> {
    if finding.finding_code == FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL
        && finding.issue_code == ISSUE_CODE_METADATA_ONLY_INSTALL
    {
        // TODO(doctor-repair): replace this embedded map with remote fingerprint
        // lookup once the Shared Brain endpoint is available.
        return Some(RepairPlan {
            manager_id: finding.manager_id.clone(),
            source_manager_id: finding.source_manager_id.clone(),
            package_name: finding.package_name.clone(),
            issue_code: finding.issue_code.clone(),
            finding_code: finding.finding_code.clone(),
            fingerprint: finding.fingerprint.clone(),
            knowledge_source: REPAIR_KNOWLEDGE_SOURCE.to_string(),
            knowledge_version: REPAIR_KNOWLEDGE_VERSION.to_string(),
            options: vec![
                RepairOption {
                    option_id: REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW.to_string(),
                    action: RepairAction::ReinstallManagerViaHomebrew,
                    title: "Repair Homebrew install".to_string(),
                    description:
                        "Run the manager install flow via Homebrew so binaries and metadata are aligned."
                            .to_string(),
                    recommended: true,
                    requires_confirmation: false,
                    automation_level: RepairAutomationLevel::Automatic,
                },
                RepairOption {
                    option_id: REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY.to_string(),
                    action: RepairAction::RemoveStalePackageEntry,
                    title: "Remove stale package metadata".to_string(),
                    description:
                        "Uninstall the stale Homebrew package entry when you do not want this manager managed via Homebrew."
                            .to_string(),
                    recommended: false,
                    requires_confirmation: true,
                    automation_level: RepairAutomationLevel::NeedsConfirmation,
                },
            ],
        });
    }

    if finding.finding_code == FINDING_CODE_POST_INSTALL_SETUP_REQUIRED
        && finding.issue_code == ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED
    {
        return Some(RepairPlan {
            manager_id: finding.manager_id.clone(),
            source_manager_id: finding.source_manager_id.clone(),
            package_name: finding.package_name.clone(),
            issue_code: finding.issue_code.clone(),
            finding_code: finding.finding_code.clone(),
            fingerprint: finding.fingerprint.clone(),
            knowledge_source: REPAIR_KNOWLEDGE_SOURCE.to_string(),
            knowledge_version: REPAIR_KNOWLEDGE_VERSION.to_string(),
            options: vec![RepairOption {
                option_id: REPAIR_OPTION_APPLY_POST_INSTALL_SETUP_DEFAULTS.to_string(),
                action: RepairAction::ApplyPostInstallSetupDefaults,
                title: "Apply recommended setup".to_string(),
                description:
                    "Apply Helm's safe default shell setup block for this manager, then verify setup."
                        .to_string(),
                recommended: true,
                requires_confirmation: true,
                automation_level: RepairAutomationLevel::NeedsConfirmation,
            }],
        });
    }

    None
}

pub fn plan_for_issue(
    manager: ManagerId,
    source_manager: ManagerId,
    package_name: &str,
    issue_code: &str,
) -> Option<RepairPlan> {
    if issue_code == ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED {
        let finding = DoctorFinding {
            finding_code: FINDING_CODE_POST_INSTALL_SETUP_REQUIRED.to_string(),
            issue_code: ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED.to_string(),
            fingerprint: fingerprint_for_post_install_setup_required(manager, &["unknown"]),
            manager_id: manager.as_str().to_string(),
            source_manager_id: Some(source_manager.as_str().to_string()),
            package_name: None,
            severity: crate::doctor::DoctorFindingSeverity::Warning,
            summary: String::new(),
            evidence_primary: None,
            evidence_secondary: None,
        };
        return plan_for_finding(&finding);
    }

    if issue_code != ISSUE_CODE_METADATA_ONLY_INSTALL
        || source_manager != ManagerId::HomebrewFormula
        || package_name.trim().is_empty()
    {
        return None;
    }

    let normalized_package = package_name.trim().to_string();
    let finding = DoctorFinding {
        finding_code: FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL.to_string(),
        issue_code: ISSUE_CODE_METADATA_ONLY_INSTALL.to_string(),
        fingerprint: fingerprint_for_metadata_only_install(
            manager,
            source_manager,
            normalized_package.as_str(),
        ),
        manager_id: manager.as_str().to_string(),
        source_manager_id: Some(source_manager.as_str().to_string()),
        package_name: Some(normalized_package),
        severity: crate::doctor::DoctorFindingSeverity::Warning,
        summary: String::new(),
        evidence_primary: None,
        evidence_secondary: None,
    };
    plan_for_finding(&finding)
}

pub fn resolve_option<'a>(plan: &'a RepairPlan, option_id: &str) -> Option<&'a RepairOption> {
    plan.options
        .iter()
        .find(|option| option.option_id == option_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::{
        DoctorFinding, DoctorFindingSeverity, FINDING_CODE_POST_INSTALL_SETUP_REQUIRED,
        ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED,
    };

    #[test]
    fn metadata_only_finding_returns_embedded_repair_plan() {
        let finding = DoctorFinding {
            finding_code: FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL.to_string(),
            issue_code: ISSUE_CODE_METADATA_ONLY_INSTALL.to_string(),
            fingerprint: "fingerprint".to_string(),
            manager_id: ManagerId::Rustup.as_str().to_string(),
            source_manager_id: Some(ManagerId::HomebrewFormula.as_str().to_string()),
            package_name: Some("rustup".to_string()),
            severity: DoctorFindingSeverity::Warning,
            summary: "summary".to_string(),
            evidence_primary: None,
            evidence_secondary: None,
        };

        let plan = plan_for_finding(&finding).expect("expected repair plan");
        assert_eq!(plan.options.len(), 2);
        assert_eq!(
            plan.options[0].option_id,
            REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW
        );
        assert_eq!(
            plan.options[1].option_id,
            REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY
        );
    }

    #[test]
    fn resolve_option_matches_expected_action() {
        let plan = plan_for_issue(
            ManagerId::Rustup,
            ManagerId::HomebrewFormula,
            "rustup",
            ISSUE_CODE_METADATA_ONLY_INSTALL,
        )
        .expect("expected plan");
        let option = resolve_option(&plan, REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY)
            .expect("expected stale-entry option");
        assert_eq!(option.action, RepairAction::RemoveStalePackageEntry);
    }

    #[test]
    fn post_install_setup_finding_returns_setup_plan() {
        let finding = DoctorFinding {
            finding_code: FINDING_CODE_POST_INSTALL_SETUP_REQUIRED.to_string(),
            issue_code: ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED.to_string(),
            fingerprint: "fingerprint-setup".to_string(),
            manager_id: ManagerId::Rustup.as_str().to_string(),
            source_manager_id: Some(ManagerId::Rustup.as_str().to_string()),
            package_name: None,
            severity: DoctorFindingSeverity::Warning,
            summary: String::new(),
            evidence_primary: None,
            evidence_secondary: None,
        };
        let plan = plan_for_finding(&finding).expect("expected setup plan");
        assert_eq!(plan.options.len(), 1);
        assert_eq!(
            plan.options[0].option_id,
            REPAIR_OPTION_APPLY_POST_INSTALL_SETUP_DEFAULTS
        );
        assert_eq!(
            plan.options[0].action,
            RepairAction::ApplyPostInstallSetupDefaults
        );
    }
}
