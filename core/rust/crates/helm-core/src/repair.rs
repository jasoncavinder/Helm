use crate::doctor::{
    DoctorFinding, FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL,
    FINDING_CODE_POST_INSTALL_SETUP_REQUIRED, FINDING_CODE_SELECTED_EXECUTABLE_PATH_STALE,
    ISSUE_CODE_METADATA_ONLY_INSTALL, ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED,
    ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE, fingerprint_for_metadata_only_install,
    fingerprint_for_post_install_setup_required, fingerprint_for_selected_executable_path_stale,
};
use crate::models::ManagerId;
use crate::persistence::doctor_persistence::EffectiveKnowledge;
use serde::{Deserialize, Serialize};

pub const REPAIR_KNOWLEDGE_SOURCE: &str = "embedded_local";
pub const REPAIR_KNOWLEDGE_VERSION: &str = "v0";
pub const REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW: &str = "reinstall_manager_via_homebrew";
pub const REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY: &str = "remove_stale_package_entry";
pub const REPAIR_OPTION_APPLY_POST_INSTALL_SETUP_DEFAULTS: &str =
    "apply_post_install_setup_defaults";
pub const REPAIR_OPTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE: &str =
    "clear_selected_executable_override";
pub const REPAIR_ACTION_HOMEBREW_REINSTALL_FORMULA: &str = "homebrew.reinstall_formula";
pub const REPAIR_ACTION_HOMEBREW_UNINSTALL_FORMULA: &str = "homebrew.uninstall_formula";
pub const REPAIR_ACTION_APPLY_POST_INSTALL_SETUP_DEFAULTS: &str =
    "manager.apply_post_install_setup_defaults";
pub const REPAIR_ACTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE: &str =
    "manager.clear_selected_executable_override";

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RepairAction {
    #[serde(rename = "homebrew.reinstall_formula")]
    ReinstallManagerViaHomebrew,
    #[serde(rename = "homebrew.uninstall_formula")]
    RemoveStalePackageEntry,
    #[serde(rename = "manager.apply_post_install_setup_defaults")]
    ApplyPostInstallSetupDefaults,
    #[serde(rename = "manager.clear_selected_executable_override")]
    ClearSelectedExecutableOverride,
}

impl RepairAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReinstallManagerViaHomebrew => REPAIR_ACTION_HOMEBREW_REINSTALL_FORMULA,
            Self::RemoveStalePackageEntry => REPAIR_ACTION_HOMEBREW_UNINSTALL_FORMULA,
            Self::ApplyPostInstallSetupDefaults => REPAIR_ACTION_APPLY_POST_INSTALL_SETUP_DEFAULTS,
            Self::ClearSelectedExecutableOverride => {
                REPAIR_ACTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepairActionDefinition {
    pub action: RepairAction,
    pub action_id: &'static str,
    pub protected_option_id: &'static str,
    pub finding_code: &'static str,
    pub issue_code: &'static str,
    pub fallback_title: &'static str,
    pub fallback_description: &'static str,
    pub requires_confirmation: bool,
    pub minimum_automation_level: RepairAutomationLevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairRegistryError {
    UnknownAction,
    ProtectedOptionRebinding,
}

const REPAIR_ACTION_DEFINITIONS: [RepairActionDefinition; 4] = [
    RepairActionDefinition {
        action: RepairAction::ReinstallManagerViaHomebrew,
        action_id: REPAIR_ACTION_HOMEBREW_REINSTALL_FORMULA,
        protected_option_id: REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW,
        finding_code: FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL,
        issue_code: ISSUE_CODE_METADATA_ONLY_INSTALL,
        fallback_title: "Repair Homebrew install",
        fallback_description: "Run the manager install flow via Homebrew so binaries and metadata are aligned.",
        requires_confirmation: false,
        minimum_automation_level: RepairAutomationLevel::Automatic,
    },
    RepairActionDefinition {
        action: RepairAction::RemoveStalePackageEntry,
        action_id: REPAIR_ACTION_HOMEBREW_UNINSTALL_FORMULA,
        protected_option_id: REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY,
        finding_code: FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL,
        issue_code: ISSUE_CODE_METADATA_ONLY_INSTALL,
        fallback_title: "Remove stale package metadata",
        fallback_description: "Uninstall the stale Homebrew package entry when you do not want this manager managed via Homebrew.",
        requires_confirmation: true,
        minimum_automation_level: RepairAutomationLevel::NeedsConfirmation,
    },
    RepairActionDefinition {
        action: RepairAction::ApplyPostInstallSetupDefaults,
        action_id: REPAIR_ACTION_APPLY_POST_INSTALL_SETUP_DEFAULTS,
        protected_option_id: REPAIR_OPTION_APPLY_POST_INSTALL_SETUP_DEFAULTS,
        finding_code: FINDING_CODE_POST_INSTALL_SETUP_REQUIRED,
        issue_code: ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED,
        fallback_title: "Apply recommended setup",
        fallback_description: "Apply Helm's safe default shell setup block for this manager, then verify setup.",
        requires_confirmation: true,
        minimum_automation_level: RepairAutomationLevel::NeedsConfirmation,
    },
    RepairActionDefinition {
        action: RepairAction::ClearSelectedExecutableOverride,
        action_id: REPAIR_ACTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE,
        protected_option_id: REPAIR_OPTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE,
        finding_code: FINDING_CODE_SELECTED_EXECUTABLE_PATH_STALE,
        issue_code: ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE,
        fallback_title: "Clear selected executable override",
        fallback_description: "Remove the saved executable override so Helm can fall back to normal executable discovery.",
        requires_confirmation: false,
        minimum_automation_level: RepairAutomationLevel::Automatic,
    },
];

pub fn repair_action_definitions() -> &'static [RepairActionDefinition] {
    &REPAIR_ACTION_DEFINITIONS
}

pub fn repair_action_definition(action: RepairAction) -> &'static RepairActionDefinition {
    REPAIR_ACTION_DEFINITIONS
        .iter()
        .find(|definition| definition.action == action)
        .expect("every RepairAction must have a compiled registry definition")
}

pub fn repair_action_from_id(action_id: &str) -> Option<RepairAction> {
    REPAIR_ACTION_DEFINITIONS
        .iter()
        .find(|definition| definition.action_id == action_id)
        .map(|definition| definition.action)
}

pub fn validate_knowledge_binding(
    option_id: &str,
    action_id: &str,
) -> Result<RepairAction, RepairRegistryError> {
    let action = repair_action_from_id(action_id).ok_or(RepairRegistryError::UnknownAction)?;
    if let Some(protected) = REPAIR_ACTION_DEFINITIONS
        .iter()
        .find(|definition| definition.protected_option_id == option_id)
        && protected.action != action
    {
        return Err(RepairRegistryError::ProtectedOptionRebinding);
    }
    Ok(action)
}

fn automation_restrictiveness(level: RepairAutomationLevel) -> u8 {
    match level {
        RepairAutomationLevel::Automatic => 0,
        RepairAutomationLevel::NeedsConfirmation => 1,
        RepairAutomationLevel::ReadOnly => 2,
    }
}

fn option_satisfies_registry(plan: &RepairPlan, option: &RepairOption) -> bool {
    let definition = repair_action_definition(option.action);
    validate_knowledge_binding(option.option_id.as_str(), definition.action_id).is_ok()
        && definition.finding_code == plan.finding_code
        && definition.issue_code == plan.issue_code
        && (!definition.requires_confirmation || option.requires_confirmation)
        && automation_restrictiveness(option.automation_level)
            >= automation_restrictiveness(definition.minimum_automation_level)
}

fn registered_option(action: RepairAction, recommended: bool) -> RepairOption {
    let definition = repair_action_definition(action);
    RepairOption {
        option_id: definition.protected_option_id.to_string(),
        action,
        title: definition.fallback_title.to_string(),
        description: definition.fallback_description.to_string(),
        content_keys: None,
        recommended,
        requires_confirmation: definition.requires_confirmation,
        automation_level: definition.minimum_automation_level,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepairOption {
    pub option_id: String,
    pub action: RepairAction,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_keys: Option<crate::persistence::repair_knowledge::KnowledgeContentKeys>,
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
        // Storeless compatibility fallback; runtime surfaces resolve SQLite knowledge.
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
                registered_option(RepairAction::ReinstallManagerViaHomebrew, true),
                registered_option(RepairAction::RemoveStalePackageEntry, false),
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
            options: vec![registered_option(
                RepairAction::ApplyPostInstallSetupDefaults,
                true,
            )],
        });
    }

    if finding.finding_code == FINDING_CODE_SELECTED_EXECUTABLE_PATH_STALE
        && finding.issue_code == ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE
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
            options: vec![registered_option(
                RepairAction::ClearSelectedExecutableOverride,
                true,
            )],
        });
    }

    None
}

pub fn plan_for_finding_with_knowledge(
    finding: &DoctorFinding,
    knowledge: &[EffectiveKnowledge],
) -> Option<RepairPlan> {
    if knowledge.is_empty() {
        return None;
    }

    let mut ranked_options = Vec::new();
    for entry in knowledge {
        let Ok(action) = validate_knowledge_binding(&entry.option_id, &entry.action_id) else {
            continue;
        };

        let definition = repair_action_definition(action);
        if definition.finding_code != finding.finding_code
            || definition.issue_code != finding.issue_code
        {
            continue;
        }

        let Some(automation_level) = entry.policy.automation_level() else {
            continue;
        };

        if automation_level == RepairAutomationLevel::ReadOnly {
            continue;
        }

        if (definition.requires_confirmation && !entry.policy.requires_confirmation)
            || automation_restrictiveness(automation_level)
                < automation_restrictiveness(definition.minimum_automation_level)
        {
            continue;
        }

        ranked_options.push((
            entry.recommendation_rank,
            RepairOption {
                option_id: entry.option_id.clone(),
                action,
                title: definition.fallback_title.to_string(),
                description: definition.fallback_description.to_string(),
                content_keys: Some(entry.content_keys.clone()),
                recommended: false,
                requires_confirmation: entry.policy.requires_confirmation,
                automation_level,
            },
        ));
    }

    if ranked_options.is_empty() {
        return None;
    }
    let preferred_rank = ranked_options.iter().filter_map(|(rank, _)| *rank).min();
    let options = ranked_options
        .into_iter()
        .map(|(rank, mut option)| {
            option.recommended = preferred_rank.is_some_and(|preferred| rank == Some(preferred));
            option
        })
        .collect();

    let knowledge_version = knowledge
        .iter()
        .map(|entry| format!("{}@{}", entry.knowledge_entry_id, entry.revision))
        .collect::<Vec<_>>()
        .join(",");

    Some(RepairPlan {
        manager_id: finding.manager_id.clone(),
        source_manager_id: finding.source_manager_id.clone(),
        package_name: finding.package_name.clone(),
        issue_code: finding.issue_code.clone(),
        finding_code: finding.finding_code.clone(),
        fingerprint: finding.fingerprint.clone(),
        knowledge_source: "sqlite_local".to_string(),
        knowledge_version,
        options,
    })
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

    if issue_code == ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE {
        let finding = DoctorFinding {
            finding_code: FINDING_CODE_SELECTED_EXECUTABLE_PATH_STALE.to_string(),
            issue_code: ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE.to_string(),
            fingerprint: fingerprint_for_selected_executable_path_stale(manager, package_name),
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
    plan.options.iter().find(|option| {
        option.option_id == option_id
            && option.automation_level != RepairAutomationLevel::ReadOnly
            && option_satisfies_registry(plan, option)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::{
        DoctorFinding, DoctorFindingSeverity, FINDING_CODE_POST_INSTALL_SETUP_REQUIRED,
        ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED, ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE,
    };

    use crate::persistence::doctor_persistence::{EffectiveKnowledge, KnowledgeTrustLevel};
    use crate::persistence::repair_knowledge::{KnowledgeContentKeys, KnowledgePolicy};

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
        assert_eq!(
            option.action.as_str(),
            REPAIR_ACTION_HOMEBREW_UNINSTALL_FORMULA
        );
    }

    #[test]
    fn compiled_registry_preserves_option_bindings_and_typed_action_ids() {
        let expected = [
            (
                REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW,
                REPAIR_ACTION_HOMEBREW_REINSTALL_FORMULA,
            ),
            (
                REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY,
                REPAIR_ACTION_HOMEBREW_UNINSTALL_FORMULA,
            ),
            (
                REPAIR_OPTION_APPLY_POST_INSTALL_SETUP_DEFAULTS,
                REPAIR_ACTION_APPLY_POST_INSTALL_SETUP_DEFAULTS,
            ),
            (
                REPAIR_OPTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE,
                REPAIR_ACTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE,
            ),
        ];

        for (option_id, action_id) in expected {
            let action = validate_knowledge_binding(option_id, action_id)
                .expect("compiled protected binding should validate");
            assert_eq!(action.as_str(), action_id);
        }
    }

    #[test]
    fn registry_rejects_unknown_actions_and_protected_option_rebinding() {
        assert_eq!(
            validate_knowledge_binding("future_option", "shell.execute"),
            Err(RepairRegistryError::UnknownAction)
        );
        assert_eq!(
            validate_knowledge_binding(
                REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY,
                REPAIR_ACTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE,
            ),
            Err(RepairRegistryError::ProtectedOptionRebinding)
        );
    }

    #[test]
    fn resolver_rejects_policy_weakening_and_wrong_finding_classes() {
        let mut metadata_plan = plan_for_issue(
            ManagerId::Rustup,
            ManagerId::HomebrewFormula,
            "rustup",
            ISSUE_CODE_METADATA_ONLY_INSTALL,
        )
        .expect("expected metadata plan");
        let destructive = metadata_plan
            .options
            .iter_mut()
            .find(|option| option.option_id == REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY)
            .expect("expected destructive option");
        destructive.requires_confirmation = false;
        destructive.automation_level = RepairAutomationLevel::Automatic;
        assert!(
            resolve_option(&metadata_plan, REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY,).is_none()
        );

        let mut wrong_finding_plan = plan_for_issue(
            ManagerId::Rustup,
            ManagerId::HomebrewFormula,
            "rustup",
            ISSUE_CODE_METADATA_ONLY_INSTALL,
        )
        .expect("expected metadata plan");
        wrong_finding_plan.finding_code = FINDING_CODE_POST_INSTALL_SETUP_REQUIRED.to_string();
        assert!(
            resolve_option(
                &wrong_finding_plan,
                REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW,
            )
            .is_none()
        );
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

    #[test]
    fn stale_selected_executable_finding_returns_clear_override_plan() {
        let finding = DoctorFinding {
            finding_code: FINDING_CODE_SELECTED_EXECUTABLE_PATH_STALE.to_string(),
            issue_code: ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE.to_string(),
            fingerprint: "fingerprint-selected-executable".to_string(),
            manager_id: ManagerId::Rustup.as_str().to_string(),
            source_manager_id: Some(ManagerId::Rustup.as_str().to_string()),
            package_name: None,
            severity: DoctorFindingSeverity::Warning,
            summary: String::new(),
            evidence_primary: None,
            evidence_secondary: None,
        };

        let plan = plan_for_finding(&finding).expect("expected selected executable repair plan");
        assert_eq!(plan.options.len(), 1);
        assert_eq!(
            plan.options[0].option_id,
            REPAIR_OPTION_CLEAR_SELECTED_EXECUTABLE_OVERRIDE
        );
        assert_eq!(
            plan.options[0].action,
            RepairAction::ClearSelectedExecutableOverride
        );
    }

    #[test]
    fn knowledge_directly_creates_plan() {
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

        let knowledge = vec![EffectiveKnowledge {
            source_key: "sqlite".to_string(),
            trust_level: KnowledgeTrustLevel::Bundled,
            knowledge_entry_id: "homebrew_reinstall".to_string(),
            revision: 1,
            option_id: REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW.to_string(),
            action_id: REPAIR_ACTION_HOMEBREW_REINSTALL_FORMULA.to_string(),
            recommendation_rank: Some(0),
            policy: KnowledgePolicy {
                requires_confirmation: true,               // Requires confirmation
                automation_level: "automatic".to_string(), // Keep automatic
                enabled: Some(true),
            },
            content_keys: KnowledgeContentKeys {
                title: "app.repair.test.title".to_string(),
                description: "app.repair.test.desc".to_string(),
                impact: None,
                guidance: None,
            },
        }];

        let plan = plan_for_finding_with_knowledge(&finding, &knowledge).expect("expected plan");
        assert_eq!(plan.options.len(), 1);
        let option = &plan.options[0];
        assert_eq!(
            option.option_id,
            REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW
        );
        assert_eq!(option.title, "Repair Homebrew install");
        assert_eq!(
            option.content_keys.as_ref().unwrap().title,
            "app.repair.test.title"
        );
        assert!(option.recommended);
        assert!(option.requires_confirmation);
    }

    #[test]
    fn knowledge_rejects_incompatible_weakening_and_read_only() {
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

        let knowledge = vec![
            EffectiveKnowledge {
                // Incompatible finding code
                source_key: "sqlite".to_string(),
                trust_level: KnowledgeTrustLevel::Bundled,
                knowledge_entry_id: "incompatible".to_string(),
                revision: 1,
                option_id: REPAIR_OPTION_APPLY_POST_INSTALL_SETUP_DEFAULTS.to_string(),
                action_id: REPAIR_ACTION_APPLY_POST_INSTALL_SETUP_DEFAULTS.to_string(),
                recommendation_rank: Some(0),
                policy: KnowledgePolicy {
                    requires_confirmation: true,
                    automation_level: "needs_confirmation".to_string(),
                    enabled: Some(true),
                },
                content_keys: KnowledgeContentKeys {
                    title: "app.repair.test.title".to_string(),
                    description: "app.repair.test.desc".to_string(),
                    impact: None,
                    guidance: None,
                },
            },
            EffectiveKnowledge {
                // Weaken registry (needs confirmation -> automatic)
                source_key: "sqlite".to_string(),
                trust_level: KnowledgeTrustLevel::Bundled,
                knowledge_entry_id: "weaken".to_string(),
                revision: 1,
                option_id: REPAIR_OPTION_REMOVE_STALE_PACKAGE_ENTRY.to_string(),
                action_id: REPAIR_ACTION_HOMEBREW_UNINSTALL_FORMULA.to_string(),
                recommendation_rank: Some(1),
                policy: KnowledgePolicy {
                    requires_confirmation: false, // Trying to bypass confirmation
                    automation_level: "automatic".to_string(), // Trying to bypass automation level
                    enabled: Some(true),
                },
                content_keys: KnowledgeContentKeys {
                    title: "app.repair.test.title".to_string(),
                    description: "app.repair.test.desc".to_string(),
                    impact: None,
                    guidance: None,
                },
            },
            EffectiveKnowledge {
                // Read only
                source_key: "sqlite".to_string(),
                trust_level: KnowledgeTrustLevel::Bundled,
                knowledge_entry_id: "readonly".to_string(),
                revision: 1,
                option_id: REPAIR_OPTION_REINSTALL_MANAGER_VIA_HOMEBREW.to_string(),
                action_id: REPAIR_ACTION_HOMEBREW_REINSTALL_FORMULA.to_string(),
                recommendation_rank: Some(0),
                policy: KnowledgePolicy {
                    requires_confirmation: false,
                    automation_level: "read_only".to_string(),
                    enabled: Some(true),
                },
                content_keys: KnowledgeContentKeys {
                    title: "app.repair.test.title".to_string(),
                    description: "app.repair.test.desc".to_string(),
                    impact: None,
                    guidance: None,
                },
            },
        ];

        let plan = plan_for_finding_with_knowledge(&finding, &knowledge);
        assert!(plan.is_none());
    }
}
