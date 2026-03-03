use crate::manager_lifecycle;
use crate::models::{InstallProvenance, InstalledPackage, ManagerId, ManagerInstallInstance};
use crate::post_install_setup::evaluate_manager_post_install_setup;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ISSUE_CODE_METADATA_ONLY_INSTALL: &str = "metadata_only_install";
pub const FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL: &str = "homebrew_metadata_only_install";
pub const ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED: &str = "post_install_setup_required";
pub const FINDING_CODE_POST_INSTALL_SETUP_REQUIRED: &str = "post_install_setup_required";
const FINGERPRINT_VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorFindingSeverity {
    Info,
    Warning,
    Error,
}

impl DoctorFindingSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorHealthStatus {
    Healthy,
    Attention,
    Critical,
}

impl DoctorHealthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Attention => "attention",
            Self::Critical => "critical",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorFinding {
    pub finding_code: String,
    pub issue_code: String,
    pub fingerprint: String,
    pub manager_id: String,
    pub source_manager_id: Option<String>,
    pub package_name: Option<String>,
    pub severity: DoctorFindingSeverity,
    pub summary: String,
    pub evidence_primary: Option<String>,
    pub evidence_secondary: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorSummary {
    pub manager_count: usize,
    pub total_findings: usize,
    pub warnings: usize,
    pub errors: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    pub generated_at_unix: u64,
    pub health: DoctorHealthStatus,
    pub findings: Vec<DoctorFinding>,
    pub summary: DoctorSummary,
}

pub struct ManagerPackageStateScanInput<'a> {
    pub manager: ManagerId,
    pub manager_install_instances: Option<&'a [ManagerInstallInstance]>,
    pub homebrew_installed_formulas: &'a HashSet<String>,
}

pub fn fingerprint_for_metadata_only_install(
    manager: ManagerId,
    source_manager: ManagerId,
    package_name: &str,
) -> String {
    let normalized_package = package_name.trim().to_ascii_lowercase();
    format!(
        "v{FINGERPRINT_VERSION}:manager:{}:issue:{}:source:{}:package:{}",
        manager.as_str(),
        ISSUE_CODE_METADATA_ONLY_INSTALL,
        source_manager.as_str(),
        normalized_package
    )
}

pub fn fingerprint_for_post_install_setup_required(
    manager: ManagerId,
    unmet_requirement_ids: &[&str],
) -> String {
    let mut requirement_ids = unmet_requirement_ids
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    requirement_ids.sort();
    requirement_ids.dedup();
    let encoded_requirements = requirement_ids.join(",");
    format!(
        "v{FINGERPRINT_VERSION}:manager:{}:issue:{}:requirements:{}",
        manager.as_str(),
        ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED,
        encoded_requirements
    )
}

pub fn manager_expected_homebrew_formula(manager: ManagerId) -> Option<&'static str> {
    match manager {
        // rustup intentionally supports both rustup-init and homebrew provenance.
        ManagerId::Rustup => Some("rustup"),
        _ => manager_lifecycle::manager_homebrew_formula_name(manager),
    }
}

pub fn scan_manager_package_state_issues(
    input: ManagerPackageStateScanInput<'_>,
) -> Vec<DoctorFinding> {
    let mut findings = Vec::new();

    if let Some(formula_name) = manager_expected_homebrew_formula(input.manager) {
        let normalized_formula = formula_name.to_ascii_lowercase();
        if input
            .homebrew_installed_formulas
            .contains(&normalized_formula)
            && !manager_has_homebrew_instance_for_formula(input.manager_install_instances, formula_name)
        {
            findings.push(DoctorFinding {
                finding_code: FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL.to_string(),
                issue_code: ISSUE_CODE_METADATA_ONLY_INSTALL.to_string(),
                fingerprint: fingerprint_for_metadata_only_install(
                    input.manager,
                    ManagerId::HomebrewFormula,
                    formula_name,
                ),
                manager_id: input.manager.as_str().to_string(),
                source_manager_id: Some(ManagerId::HomebrewFormula.as_str().to_string()),
                package_name: Some(formula_name.to_string()),
                severity: DoctorFindingSeverity::Warning,
                summary: format!(
                    "{} metadata shows '{}' as installed, but no matching executable instance was detected.",
                    ManagerDisplayName(input.manager),
                    formula_name
                ),
                evidence_primary: Some(format!(
                    "homebrew formula '{}' appears in installed package metadata",
                    formula_name
                )),
                evidence_secondary: Some(
                    "detected install instances do not include homebrew-owned executable paths"
                        .to_string(),
                ),
            });
        }
    }

    if let Some(report) =
        evaluate_manager_post_install_setup(input.manager, input.manager_install_instances)
            .filter(|report| report.has_unmet_required())
    {
        let unmet = report
            .requirements
            .iter()
            .filter(|requirement| !requirement.met)
            .collect::<Vec<_>>();
        let unmet_requirement_ids = unmet
            .iter()
            .map(|requirement| requirement.requirement_id)
            .collect::<Vec<_>>();
        let unmet_details = unmet
            .iter()
            .map(|requirement| requirement.detail)
            .collect::<Vec<_>>();
        let evidence_secondary = report
            .rc_files
            .first()
            .map(|path| format!("shell startup file to update: '{}'", path.display()));

        findings.push(DoctorFinding {
            finding_code: FINDING_CODE_POST_INSTALL_SETUP_REQUIRED.to_string(),
            issue_code: ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED.to_string(),
            fingerprint: fingerprint_for_post_install_setup_required(
                input.manager,
                unmet_requirement_ids.as_slice(),
            ),
            manager_id: input.manager.as_str().to_string(),
            source_manager_id: Some(input.manager.as_str().to_string()),
            package_name: None,
            severity: DoctorFindingSeverity::Warning,
            summary: format!(
                "{} is installed but requires post-install setup before Helm can enable manager actions.",
                ManagerDisplayName(input.manager)
            ),
            evidence_primary: Some(format!(
                "unmet setup requirements: {}",
                unmet_details.join("; ")
            )),
            evidence_secondary,
        });
    }

    findings
}

pub fn scan_package_state_report(
    managers: impl IntoIterator<Item = ManagerId>,
    manager_install_instances: &HashMap<ManagerId, Vec<ManagerInstallInstance>>,
    installed_packages: &[InstalledPackage],
) -> DoctorReport {
    let homebrew_installed_formulas: HashSet<String> = installed_packages
        .iter()
        .filter(|package| package.package.manager == ManagerId::HomebrewFormula)
        .map(|package| package.package.name.to_ascii_lowercase())
        .collect();

    let manager_ids = managers.into_iter().collect::<Vec<_>>();
    let findings = manager_ids
        .iter()
        .flat_map(|manager| {
            let instances = manager_install_instances.get(manager).map(Vec::as_slice);
            scan_manager_package_state_issues(ManagerPackageStateScanInput {
                manager: *manager,
                manager_install_instances: instances,
                homebrew_installed_formulas: &homebrew_installed_formulas,
            })
        })
        .collect::<Vec<_>>();

    build_report(manager_ids.len(), findings)
}

pub fn build_report(manager_count: usize, findings: Vec<DoctorFinding>) -> DoctorReport {
    let warnings = findings
        .iter()
        .filter(|finding| finding.severity == DoctorFindingSeverity::Warning)
        .count();
    let errors = findings
        .iter()
        .filter(|finding| finding.severity == DoctorFindingSeverity::Error)
        .count();
    let total_findings = findings.len();
    let health = if errors > 0 {
        DoctorHealthStatus::Critical
    } else if warnings > 0 {
        DoctorHealthStatus::Attention
    } else {
        DoctorHealthStatus::Healthy
    };

    let generated_at_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    DoctorReport {
        generated_at_unix,
        health,
        findings,
        summary: DoctorSummary {
            manager_count,
            total_findings,
            warnings,
            errors,
        },
    }
}

fn manager_has_homebrew_instance_for_formula(
    manager_install_instances: Option<&[ManagerInstallInstance]>,
    expected_formula: &str,
) -> bool {
    manager_install_instances.is_some_and(|instances| {
        instances.iter().any(|instance| {
            if instance.provenance != InstallProvenance::Homebrew {
                return false;
            }
            manager_lifecycle::homebrew_formula_name_from_instance(instance)
                .is_none_or(|name| name.eq_ignore_ascii_case(expected_formula))
        })
    })
}

struct ManagerDisplayName(ManagerId);

impl std::fmt::Display for ManagerDisplayName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AutomationLevel, InstallInstanceIdentityKind, StrategyKind};
    use std::path::PathBuf;

    fn sample_homebrew_instance(manager: ManagerId, path: &str) -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager,
            instance_id: "instance-1".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: path.to_string(),
            display_path: PathBuf::from(path),
            canonical_path: Some(PathBuf::from(path)),
            alias_paths: Vec::new(),
            is_active: true,
            version: Some("1.0.0".to_string()),
            provenance: InstallProvenance::Homebrew,
            confidence: 0.95,
            decision_margin: Some(0.40),
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
    fn metadata_only_issue_detected_when_formula_installed_without_homebrew_instance() {
        let formulas = HashSet::from([String::from("rustup")]);
        let findings = scan_manager_package_state_issues(ManagerPackageStateScanInput {
            manager: ManagerId::Rustup,
            manager_install_instances: None,
            homebrew_installed_formulas: &formulas,
        });

        let finding = findings
            .iter()
            .find(|finding| finding.issue_code == ISSUE_CODE_METADATA_ONLY_INSTALL)
            .expect("metadata-only issue should be present");
        assert_eq!(finding.issue_code, ISSUE_CODE_METADATA_ONLY_INSTALL);
        assert_eq!(
            finding.fingerprint,
            fingerprint_for_metadata_only_install(
                ManagerId::Rustup,
                ManagerId::HomebrewFormula,
                "rustup"
            )
        );
    }

    #[test]
    fn metadata_only_issue_skipped_when_homebrew_instance_present() {
        let formulas = HashSet::from([String::from("rustup")]);
        let instances = vec![sample_homebrew_instance(
            ManagerId::Rustup,
            "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
        )];
        let findings = scan_manager_package_state_issues(ManagerPackageStateScanInput {
            manager: ManagerId::Rustup,
            manager_install_instances: Some(&instances),
            homebrew_installed_formulas: &formulas,
        });

        assert!(
            findings
                .iter()
                .all(|finding| finding.issue_code != ISSUE_CODE_METADATA_ONLY_INSTALL)
        );
    }

    #[test]
    fn setup_required_issue_detected_when_requirements_unmet() {
        let formulas = HashSet::new();
        let instances = vec![sample_homebrew_instance(
            ManagerId::Mise,
            "/Users/test/.local/bin/mise",
        )];
        let findings = scan_manager_package_state_issues(ManagerPackageStateScanInput {
            manager: ManagerId::Mise,
            manager_install_instances: Some(&instances),
            homebrew_installed_formulas: &formulas,
        });

        let finding = findings
            .iter()
            .find(|finding| finding.issue_code == ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED)
            .expect("setup-required issue should be present");
        assert_eq!(
            finding.finding_code,
            FINDING_CODE_POST_INSTALL_SETUP_REQUIRED.to_string()
        );
        assert_eq!(finding.severity, DoctorFindingSeverity::Warning);
    }

    #[test]
    fn report_marks_attention_for_warning_findings() {
        let findings = vec![DoctorFinding {
            finding_code: FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL.to_string(),
            issue_code: ISSUE_CODE_METADATA_ONLY_INSTALL.to_string(),
            fingerprint: "f1".to_string(),
            manager_id: ManagerId::Rustup.as_str().to_string(),
            source_manager_id: Some(ManagerId::HomebrewFormula.as_str().to_string()),
            package_name: Some("rustup".to_string()),
            severity: DoctorFindingSeverity::Warning,
            summary: "warning".to_string(),
            evidence_primary: None,
            evidence_secondary: None,
        }];

        let report = build_report(1, findings);
        assert_eq!(report.health, DoctorHealthStatus::Attention);
        assert_eq!(report.summary.total_findings, 1);
        assert_eq!(report.summary.warnings, 1);
        assert_eq!(report.summary.errors, 0);
    }
}
