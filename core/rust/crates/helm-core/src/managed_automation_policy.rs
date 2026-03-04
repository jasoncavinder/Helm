use crate::models::{AutomationLevel, ManagerInstallInstance, StrategyKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedAutomationPolicyMode {
    Automatic,
    NeedsConfirmation,
    ReadOnly,
}

impl ManagedAutomationPolicyMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::NeedsConfirmation => "needs_confirmation",
            Self::ReadOnly => "read_only",
        }
    }
}

pub fn apply_managed_automation_policy(
    instance: &ManagerInstallInstance,
    mode: ManagedAutomationPolicyMode,
) -> ManagerInstallInstance {
    let mut adjusted = instance.clone();
    match mode {
        ManagedAutomationPolicyMode::Automatic => adjusted,
        ManagedAutomationPolicyMode::NeedsConfirmation => {
            if adjusted.automation_level == AutomationLevel::Automatic {
                adjusted.automation_level = AutomationLevel::NeedsConfirmation;
                append_managed_policy_note(
                    &mut adjusted,
                    "Managed policy requires confirmation for manager mutations.",
                );
            }
            adjusted
        }
        ManagedAutomationPolicyMode::ReadOnly => {
            adjusted.automation_level = AutomationLevel::ReadOnly;
            adjusted.uninstall_strategy = StrategyKind::ReadOnly;
            adjusted.update_strategy = StrategyKind::ReadOnly;
            adjusted.remediation_strategy = StrategyKind::ReadOnly;
            append_managed_policy_note(
                &mut adjusted,
                "Managed policy blocks manager mutations (read-only mode).",
            );
            adjusted
        }
    }
}

fn append_managed_policy_note(instance: &mut ManagerInstallInstance, note: &str) {
    let existing = instance
        .explanation_secondary
        .clone()
        .unwrap_or_default()
        .trim()
        .to_string();
    if existing.contains(note) {
        return;
    }
    if existing.is_empty() {
        instance.explanation_secondary = Some(note.to_string());
    } else {
        instance.explanation_secondary = Some(format!("{existing} {note}"));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ManagedAutomationPolicyMode, apply_managed_automation_policy};
    use crate::models::{
        AutomationLevel, InstallInstanceIdentityKind, InstallProvenance, ManagerId,
        ManagerInstallInstance, StrategyKind,
    };

    fn sample_instance(
        automation_level: AutomationLevel,
        uninstall_strategy: StrategyKind,
        update_strategy: StrategyKind,
        remediation_strategy: StrategyKind,
    ) -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "rustup:dev_inode:1:2".to_string(),
            identity_kind: InstallInstanceIdentityKind::DevInode,
            identity_value: "1:2".to_string(),
            display_path: PathBuf::from("/usr/local/bin/rustup"),
            canonical_path: Some(PathBuf::from("/usr/local/bin/rustup")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Homebrew,
            confidence: 0.94,
            decision_margin: Some(0.34),
            automation_level,
            uninstall_strategy,
            update_strategy,
            remediation_strategy,
            explanation_primary: Some("Path resolves to Homebrew Cellar ownership.".to_string()),
            explanation_secondary: None,
            competing_provenance: Some(InstallProvenance::Unknown),
            competing_confidence: Some(0.11),
        }
    }

    #[test]
    fn automatic_mode_does_not_modify_instance() {
        let instance = sample_instance(
            AutomationLevel::Automatic,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
        );
        let adjusted =
            apply_managed_automation_policy(&instance, ManagedAutomationPolicyMode::Automatic);
        assert_eq!(adjusted, instance);
    }

    #[test]
    fn needs_confirmation_mode_downgrades_automatic_automation_only() {
        let instance = sample_instance(
            AutomationLevel::Automatic,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
        );
        let adjusted = apply_managed_automation_policy(
            &instance,
            ManagedAutomationPolicyMode::NeedsConfirmation,
        );
        assert_eq!(
            adjusted.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(adjusted.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(adjusted.update_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(adjusted.remediation_strategy, StrategyKind::HomebrewFormula);
        assert!(
            adjusted
                .explanation_secondary
                .as_deref()
                .is_some_and(|message| message.contains("Managed policy requires confirmation"))
        );
    }

    #[test]
    fn needs_confirmation_mode_keeps_existing_non_automatic_level() {
        let instance = sample_instance(
            AutomationLevel::NeedsConfirmation,
            StrategyKind::InteractivePrompt,
            StrategyKind::InteractivePrompt,
            StrategyKind::InteractivePrompt,
        );
        let adjusted = apply_managed_automation_policy(
            &instance,
            ManagedAutomationPolicyMode::NeedsConfirmation,
        );
        assert_eq!(
            adjusted.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(adjusted.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(adjusted.update_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(
            adjusted.remediation_strategy,
            StrategyKind::InteractivePrompt
        );
        assert_eq!(
            adjusted.explanation_secondary,
            instance.explanation_secondary
        );
    }

    #[test]
    fn read_only_mode_forces_read_only_automation_and_strategies() {
        let instance = sample_instance(
            AutomationLevel::Automatic,
            StrategyKind::HomebrewFormula,
            StrategyKind::RustupSelf,
            StrategyKind::ManualRemediation,
        );
        let adjusted =
            apply_managed_automation_policy(&instance, ManagedAutomationPolicyMode::ReadOnly);
        assert_eq!(adjusted.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(adjusted.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(adjusted.update_strategy, StrategyKind::ReadOnly);
        assert_eq!(adjusted.remediation_strategy, StrategyKind::ReadOnly);
        assert!(
            adjusted
                .explanation_secondary
                .as_deref()
                .is_some_and(|message| message.contains("read-only mode"))
        );
    }

    #[test]
    fn policy_note_is_not_duplicated_when_applied_multiple_times() {
        let instance = sample_instance(
            AutomationLevel::Automatic,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
            StrategyKind::HomebrewFormula,
        );
        let first = apply_managed_automation_policy(
            &instance,
            ManagedAutomationPolicyMode::NeedsConfirmation,
        );
        let second =
            apply_managed_automation_policy(&first, ManagedAutomationPolicyMode::NeedsConfirmation);
        let message = second.explanation_secondary.unwrap_or_default();
        assert_eq!(
            message
                .matches("Managed policy requires confirmation for manager mutations.")
                .count(),
            1
        );
    }
}
