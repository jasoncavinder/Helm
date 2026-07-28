use crate::manager_lifecycle;
use crate::models::{
    InstallProvenance, InstalledPackage, ManagerId, ManagerInstallInstance, TaskLogRecord,
};
use crate::post_install_setup::{
    ManagerPostInstallSetupReport, evaluate_manager_post_install_setup,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const ISSUE_CODE_METADATA_ONLY_INSTALL: &str = "metadata_only_install";
pub const FINDING_CODE_HOMEBREW_METADATA_ONLY_INSTALL: &str = "homebrew_metadata_only_install";
pub const ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED: &str = "post_install_setup_required";
pub const FINDING_CODE_POST_INSTALL_SETUP_REQUIRED: &str = "post_install_setup_required";
pub const ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE: &str = "selected_executable_path_stale";
pub const FINDING_CODE_SELECTED_EXECUTABLE_PATH_STALE: &str = "selected_executable_path_stale";
pub const ISSUE_CODE_HOMEBREW_CELLAR_LOCK_CONFLICT: &str = "homebrew_cellar_lock_conflict";
pub const FINDING_CODE_HOMEBREW_CELLAR_LOCK_CONFLICT: &str = "homebrew_cellar_lock_conflict";
const TASK_FAILURE_DIAGNOSTIC_PREFIX: &str = "[diagnostic.v1] ";
const TASK_FAILURE_DIAGNOSTIC_SCHEMA: &str = "helm.task.failure_diagnostic";
pub const HOME_BREW_LOCK_DIAGNOSTIC_ISSUE_KEY: &str = "homebrew.cellar_lock_conflict";
const RECENT_TASK_FAILURE_MAX_AGE: Duration = Duration::from_secs(60 * 60);
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ManagerExecutableDoctorState {
    pub detected: bool,
    pub stored_selected_executable_path: Option<String>,
    pub default_executable_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecentTaskFailureDiagnostic {
    pub fingerprint: String,
    pub manager: ManagerId,
    pub issue_key: String,
    pub command: Option<String>,
    pub issue_summary: Option<String>,
    pub error_excerpt: Option<String>,
    pub created_at: SystemTime,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TaskFailureDiagnosticEnvelope {
    schema: String,
    fingerprint: String,
    manager_id: String,
    issue_key: String,
    issue_summary: String,
    command: Option<String>,
    error_excerpt: String,
}

pub struct ManagerPackageStateScanInput<'a> {
    pub manager: ManagerId,
    pub manager_install_instances: Option<&'a [ManagerInstallInstance]>,
    pub homebrew_installed_formulas: &'a HashSet<String>,
    pub executable_state: Option<&'a ManagerExecutableDoctorState>,
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

pub fn fingerprint_for_selected_executable_path_stale(
    manager: ManagerId,
    selected_path: &str,
) -> String {
    let normalized_path = selected_path.trim().to_ascii_lowercase();
    format!(
        "v{FINGERPRINT_VERSION}:manager:{}:issue:{}:selected_path:{}",
        manager.as_str(),
        ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE,
        normalized_path
    )
}

fn post_install_setup_required_finding(
    report: &ManagerPostInstallSetupReport,
) -> Option<DoctorFinding> {
    if !report.has_unmet_required() {
        return None;
    }

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

    Some(DoctorFinding {
        finding_code: FINDING_CODE_POST_INSTALL_SETUP_REQUIRED.to_string(),
        issue_code: ISSUE_CODE_POST_INSTALL_SETUP_REQUIRED.to_string(),
        fingerprint: fingerprint_for_post_install_setup_required(
            report.manager,
            unmet_requirement_ids.as_slice(),
        ),
        manager_id: report.manager.as_str().to_string(),
        source_manager_id: Some(report.manager.as_str().to_string()),
        package_name: None,
        severity: DoctorFindingSeverity::Warning,
        summary: format!(
            "{} is installed but requires post-install setup before Helm can enable manager actions.",
            ManagerDisplayName(report.manager)
        ),
        evidence_primary: Some(format!(
            "unmet setup requirements: {}",
            unmet_details.join("; ")
        )),
        evidence_secondary,
    })
}

pub fn parse_recent_task_failure_diagnostic(
    entry: &TaskLogRecord,
) -> Option<RecentTaskFailureDiagnostic> {
    let payload = entry
        .message
        .strip_prefix(TASK_FAILURE_DIAGNOSTIC_PREFIX)?
        .trim();
    let envelope: TaskFailureDiagnosticEnvelope = serde_json::from_str(payload).ok()?;
    if envelope.schema != TASK_FAILURE_DIAGNOSTIC_SCHEMA {
        return None;
    }
    let manager = envelope.manager_id.parse::<ManagerId>().ok()?;
    Some(RecentTaskFailureDiagnostic {
        fingerprint: envelope.fingerprint,
        manager,
        issue_key: envelope.issue_key,
        command: normalize_nonempty_owned(envelope.command),
        issue_summary: normalize_nonempty_owned(Some(envelope.issue_summary)),
        error_excerpt: normalize_nonempty_owned(Some(envelope.error_excerpt)),
        created_at: entry.created_at,
    })
}

pub fn scan_recent_task_failure_issues(
    diagnostics: &[RecentTaskFailureDiagnostic],
    observed_at: SystemTime,
) -> Vec<DoctorFinding> {
    let mut findings = Vec::new();
    let mut seen = HashSet::new();

    for diagnostic in diagnostics {
        let age = observed_at
            .duration_since(diagnostic.created_at)
            .unwrap_or(Duration::ZERO);
        if age > RECENT_TASK_FAILURE_MAX_AGE {
            continue;
        }
        if !seen.insert(diagnostic.fingerprint.clone()) {
            continue;
        }
        if diagnostic.issue_key != HOME_BREW_LOCK_DIAGNOSTIC_ISSUE_KEY {
            continue;
        }
        let source_manager = match diagnostic.manager {
            ManagerId::HomebrewFormula | ManagerId::HomebrewCask => diagnostic.manager,
            _ => continue,
        };
        let package_name =
            parse_homebrew_target_from_command(diagnostic.command.as_deref(), source_manager);
        let summary = package_name
            .as_deref()
            .map(|package| {
                format!(
                    "Homebrew {} operations for '{}' are failing because a Homebrew lock is blocking the target.",
                    if source_manager == ManagerId::HomebrewCask {
                        "cask"
                    } else {
                        "formula"
                    },
                    package
                )
            })
            .unwrap_or_else(|| {
                "A Homebrew operation is failing because a Homebrew lock is blocking the target."
                    .to_string()
            });
        let evidence_primary = diagnostic
            .issue_summary
            .clone()
            .or_else(|| diagnostic.error_excerpt.clone());
        let evidence_secondary = diagnostic
            .command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .map(str::to_string);

        findings.push(DoctorFinding {
            finding_code: FINDING_CODE_HOMEBREW_CELLAR_LOCK_CONFLICT.to_string(),
            issue_code: ISSUE_CODE_HOMEBREW_CELLAR_LOCK_CONFLICT.to_string(),
            fingerprint: diagnostic.fingerprint.clone(),
            manager_id: source_manager.as_str().to_string(),
            source_manager_id: Some(source_manager.as_str().to_string()),
            package_name,
            severity: DoctorFindingSeverity::Warning,
            summary,
            evidence_primary,
            evidence_secondary,
        });
    }

    findings
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
            && !manager_has_homebrew_instance_for_formula(
                input.manager_install_instances,
                formula_name,
            )
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

    if let Some(finding) =
        evaluate_manager_post_install_setup(input.manager, input.manager_install_instances)
            .as_ref()
            .and_then(post_install_setup_required_finding)
    {
        findings.push(finding);
    }

    if let Some(executable_state) = input.executable_state
        && let Some(selected_path) = executable_state
            .stored_selected_executable_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        && !Path::new(selected_path).is_file()
    {
        let evidence_secondary = executable_state
            .default_executable_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|path| format!("current discovered default executable path: '{}'", path))
            .or_else(|| {
                Some(if executable_state.detected {
                    "manager is still detected through another executable path".to_string()
                } else {
                    "no currently detected executable path is available for this manager"
                        .to_string()
                })
            });

        findings.push(DoctorFinding {
            finding_code: FINDING_CODE_SELECTED_EXECUTABLE_PATH_STALE.to_string(),
            issue_code: ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE.to_string(),
            fingerprint: fingerprint_for_selected_executable_path_stale(
                input.manager,
                selected_path,
            ),
            manager_id: input.manager.as_str().to_string(),
            source_manager_id: Some(input.manager.as_str().to_string()),
            package_name: None,
            severity: DoctorFindingSeverity::Warning,
            summary: format!(
                "{} is configured to use '{}', but that executable no longer exists.",
                ManagerDisplayName(input.manager),
                selected_path
            ),
            evidence_primary: Some(format!(
                "saved selected executable path: '{}'",
                selected_path
            )),
            evidence_secondary,
        });
    }

    findings
}

fn parse_homebrew_target_from_command(command: Option<&str>, manager: ManagerId) -> Option<String> {
    let command = command?.trim();
    if command.is_empty() {
        return None;
    }

    let tokens = command.split_whitespace().collect::<Vec<_>>();
    if tokens.is_empty() || !tokens[0].ends_with("brew") {
        return None;
    }

    let action_index = tokens
        .iter()
        .position(|token| matches!(*token, "upgrade" | "install" | "uninstall" | "reinstall"))?;
    let candidate = tokens[action_index + 1..]
        .iter()
        .rev()
        .find(|token| !token.starts_with('-'))
        .copied()?;

    let candidate = candidate.rsplit('/').next().unwrap_or(candidate).trim();
    if !is_valid_homebrew_lock_package_name(candidate) {
        return None;
    }

    match manager {
        ManagerId::HomebrewFormula | ManagerId::HomebrewCask => Some(candidate.to_string()),
        _ => None,
    }
}

fn is_valid_homebrew_lock_package_name(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '@' | '+' | '.' | '_' | '-')
        })
}

fn normalize_nonempty_owned(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

pub fn scan_package_state_report(
    managers: impl IntoIterator<Item = ManagerId>,
    manager_install_instances: &HashMap<ManagerId, Vec<ManagerInstallInstance>>,
    installed_packages: &[InstalledPackage],
    manager_executable_states: &HashMap<ManagerId, ManagerExecutableDoctorState>,
    recent_failure_diagnostics: &[RecentTaskFailureDiagnostic],
    observed_at: SystemTime,
) -> DoctorReport {
    let homebrew_installed_formulas: HashSet<String> = installed_packages
        .iter()
        .filter(|package| package.package.manager == ManagerId::HomebrewFormula)
        .map(|package| package.package.name.to_ascii_lowercase())
        .collect();

    let manager_ids = managers.into_iter().collect::<Vec<_>>();
    let mut findings = manager_ids
        .iter()
        .flat_map(|manager| {
            let instances = manager_install_instances.get(manager).map(Vec::as_slice);
            scan_manager_package_state_issues(ManagerPackageStateScanInput {
                manager: *manager,
                manager_install_instances: instances,
                homebrew_installed_formulas: &homebrew_installed_formulas,
                executable_state: manager_executable_states.get(manager),
            })
        })
        .collect::<Vec<_>>();
    findings.extend(scan_recent_task_failure_issues(
        recent_failure_diagnostics,
        observed_at,
    ));

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
    use crate::models::{
        AutomationLevel, InstallInstanceIdentityKind, StrategyKind, TaskId, TaskLogLevel,
        TaskStatus, TaskType,
    };
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
            executable_state: None,
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
            executable_state: None,
        });

        assert!(
            findings
                .iter()
                .all(|finding| finding.issue_code != ISSUE_CODE_METADATA_ONLY_INSTALL)
        );
    }

    #[test]
    fn setup_required_finding_is_constructed_from_report() {
        let report = ManagerPostInstallSetupReport {
            manager: ManagerId::Mise,
            shell_name: "zsh".to_string(),
            rc_files: vec![PathBuf::from("/tmp/helm-doctor-test/.zshrc")],
            automation_supported: true,
            requirements: vec![crate::post_install_setup::PostInstallRequirementStatus {
                requirement_id: "mise_activate",
                met: false,
                detail: "shell startup config includes mise activation",
            }],
        };

        let finding = post_install_setup_required_finding(&report)
            .expect("setup-required issue should be constructed");
        assert_eq!(
            finding.finding_code,
            FINDING_CODE_POST_INSTALL_SETUP_REQUIRED.to_string()
        );
        assert_eq!(finding.severity, DoctorFindingSeverity::Warning);
        assert_eq!(
            finding.fingerprint,
            fingerprint_for_post_install_setup_required(ManagerId::Mise, &["mise_activate"])
        );
        assert_eq!(
            finding.evidence_secondary.as_deref(),
            Some("shell startup file to update: '/tmp/helm-doctor-test/.zshrc'")
        );
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

    #[test]
    fn stale_selected_executable_path_issue_detected_when_saved_path_missing() {
        let formulas = HashSet::new();
        let executable_state = ManagerExecutableDoctorState {
            detected: false,
            stored_selected_executable_path: Some("/tmp/helm-missing-rustup".to_string()),
            default_executable_path: Some("/usr/local/bin/rustup".to_string()),
        };
        let findings = scan_manager_package_state_issues(ManagerPackageStateScanInput {
            manager: ManagerId::Rustup,
            manager_install_instances: None,
            homebrew_installed_formulas: &formulas,
            executable_state: Some(&executable_state),
        });

        let finding = findings
            .iter()
            .find(|finding| finding.issue_code == ISSUE_CODE_SELECTED_EXECUTABLE_PATH_STALE)
            .expect("stale selected executable issue should be present");
        assert_eq!(
            finding.fingerprint,
            fingerprint_for_selected_executable_path_stale(
                ManagerId::Rustup,
                "/tmp/helm-missing-rustup"
            )
        );
        assert_eq!(finding.severity, DoctorFindingSeverity::Warning);
        assert_eq!(
            finding.evidence_secondary.as_deref(),
            Some("current discovered default executable path: '/usr/local/bin/rustup'")
        );
    }

    #[test]
    fn parses_recent_task_failure_diagnostic_from_task_log_record() {
        let entry = TaskLogRecord {
            id: 1,
            task_id: TaskId(7),
            manager: ManagerId::HomebrewFormula,
            task_type: TaskType::Upgrade,
            status: Some(TaskStatus::Failed),
            level: TaskLogLevel::Info,
            message: r#"[diagnostic.v1] {"schema":"helm.task.failure_diagnostic","schemaVersion":1,"fingerprint":"failure-v1-123","taskId":7,"managerId":"homebrew_formula","taskType":"upgrade","errorCode":"process_failure","issueKey":"homebrew.cellar_lock_conflict","issueOwner":"local_runtime","issueConfidence":"high","issueSummary":"Another Homebrew process already holds a Cellar lock for this upgrade target.","command":"brew upgrade --formula fd","errorExcerpt":"Error: A `brew upgrade fd` process has already locked /usr/local/Cellar/fd."}"#.to_string(),
            created_at: UNIX_EPOCH,
        };

        let parsed =
            parse_recent_task_failure_diagnostic(&entry).expect("expected diagnostic parse");
        assert_eq!(parsed.manager, ManagerId::HomebrewFormula);
        assert_eq!(parsed.issue_key, HOME_BREW_LOCK_DIAGNOSTIC_ISSUE_KEY);
        assert_eq!(parsed.command.as_deref(), Some("brew upgrade --formula fd"));
        assert_eq!(parsed.created_at, UNIX_EPOCH);
    }

    #[test]
    fn recent_task_failure_scan_emits_homebrew_lock_conflict_finding() {
        let findings = scan_recent_task_failure_issues(
            &[RecentTaskFailureDiagnostic {
                fingerprint: "failure-v1-123".to_string(),
                manager: ManagerId::HomebrewFormula,
                issue_key: HOME_BREW_LOCK_DIAGNOSTIC_ISSUE_KEY.to_string(),
                command: Some("brew upgrade --formula fd".to_string()),
                issue_summary: Some(
                    "Another Homebrew process already holds a Cellar lock for this upgrade target."
                        .to_string(),
                ),
                error_excerpt: Some(
                    "Error: A `brew upgrade fd` process has already locked /usr/local/Cellar/fd."
                        .to_string(),
                ),
                created_at: UNIX_EPOCH,
            }],
            UNIX_EPOCH + Duration::from_secs(60),
        );

        let finding = findings
            .iter()
            .find(|finding| finding.issue_code == ISSUE_CODE_HOMEBREW_CELLAR_LOCK_CONFLICT)
            .expect("expected homebrew lock conflict finding");
        assert_eq!(
            finding.finding_code,
            FINDING_CODE_HOMEBREW_CELLAR_LOCK_CONFLICT
        );
        assert_eq!(finding.manager_id, ManagerId::HomebrewFormula.as_str());
        assert_eq!(finding.package_name.as_deref(), Some("fd"));
        assert_eq!(finding.severity, DoctorFindingSeverity::Warning);
        assert_eq!(
            finding.evidence_secondary.as_deref(),
            Some("brew upgrade --formula fd")
        );
    }

    #[test]
    fn recent_task_failure_scan_expires_old_lock_conflicts() {
        let findings = scan_recent_task_failure_issues(
            &[RecentTaskFailureDiagnostic {
                fingerprint: "failure-v1-expired".to_string(),
                manager: ManagerId::HomebrewFormula,
                issue_key: HOME_BREW_LOCK_DIAGNOSTIC_ISSUE_KEY.to_string(),
                command: Some("brew upgrade --formula fd".to_string()),
                issue_summary: None,
                error_excerpt: None,
                created_at: UNIX_EPOCH,
            }],
            UNIX_EPOCH + RECENT_TASK_FAILURE_MAX_AGE + Duration::from_secs(1),
        );

        assert!(findings.is_empty());
    }
}
