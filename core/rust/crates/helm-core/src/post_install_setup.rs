use crate::models::{ManagerId, ManagerInstallInstance};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostInstallRequirementStatus {
    pub requirement_id: &'static str,
    pub met: bool,
    pub detail: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagerPostInstallSetupReport {
    pub manager: ManagerId,
    pub shell_name: String,
    pub rc_files: Vec<PathBuf>,
    pub automation_supported: bool,
    pub requirements: Vec<PostInstallRequirementStatus>,
}

impl ManagerPostInstallSetupReport {
    pub fn has_unmet_required(&self) -> bool {
        self.requirements.iter().any(|requirement| !requirement.met)
    }

    pub fn unmet_count(&self) -> usize {
        self.requirements
            .iter()
            .filter(|requirement| !requirement.met)
            .count()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostInstallAutomationResult {
    pub manager: ManagerId,
    pub rc_file: PathBuf,
    pub changed: bool,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ShellSetupContext {
    shell_name: String,
    rc_files: Vec<PathBuf>,
}

impl ShellSetupContext {
    fn from_environment() -> Option<Self> {
        let home = std::env::var("HOME").ok()?;
        let home = home.trim();
        if home.is_empty() {
            return None;
        }

        let shell_env = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let shell_name = shell_name_from_env(shell_env.as_str());
        let home_dir = PathBuf::from(home);
        let rc_files = rc_candidates_for_shell(home_dir.as_path(), shell_name.as_str());

        Some(Self {
            shell_name,
            rc_files,
        })
    }
}

fn shell_name_from_env(shell: &str) -> String {
    let normalized = shell.trim();
    if normalized.is_empty() {
        return "zsh".to_string();
    }
    let file_name = Path::new(normalized)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("zsh")
        .to_ascii_lowercase();
    match file_name.as_str() {
        "bash" => "bash".to_string(),
        "fish" => "fish".to_string(),
        "zsh" => "zsh".to_string(),
        _ => "zsh".to_string(),
    }
}

fn rc_candidates_for_shell(home: &Path, shell_name: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut push_unique = |path: PathBuf| {
        if !files.contains(&path) {
            files.push(path);
        }
    };

    match shell_name {
        "bash" => {
            push_unique(home.join(".bashrc"));
            push_unique(home.join(".bash_profile"));
            push_unique(home.join(".profile"));
        }
        "fish" => {
            push_unique(home.join(".config/fish/config.fish"));
        }
        _ => {
            push_unique(home.join(".zshrc"));
            push_unique(home.join(".zprofile"));
        }
    }

    // Keep fallback candidates so checks work even if SHELL is stale or non-login.
    push_unique(home.join(".zshrc"));
    push_unique(home.join(".bashrc"));
    files
}

fn read_file_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn any_rc_file_contains(rc_files: &[PathBuf], marker: &str) -> bool {
    rc_files.iter().any(|path| {
        read_file_text(path)
            .map(|text| text.contains(marker))
            .unwrap_or(false)
    })
}

fn rustup_requirement_status(ctx: &ShellSetupContext) -> PostInstallRequirementStatus {
    let met = any_rc_file_contains(&ctx.rc_files, ".cargo/env")
        || any_rc_file_contains(&ctx.rc_files, ".cargo/bin");
    PostInstallRequirementStatus {
        requirement_id: "cargo_path_or_env",
        met,
        detail: "shell startup config includes Cargo environment setup",
    }
}

fn mise_requirement_status(ctx: &ShellSetupContext) -> PostInstallRequirementStatus {
    let met = any_rc_file_contains(&ctx.rc_files, "mise activate");
    PostInstallRequirementStatus {
        requirement_id: "mise_activate",
        met,
        detail: "shell startup config includes mise activation",
    }
}

fn asdf_requirement_status(ctx: &ShellSetupContext) -> PostInstallRequirementStatus {
    let met = any_rc_file_contains(&ctx.rc_files, ".asdf/shims")
        || any_rc_file_contains(&ctx.rc_files, "ASDF_DATA_DIR")
        || any_rc_file_contains(&ctx.rc_files, "asdf/shims:$PATH");
    PostInstallRequirementStatus {
        requirement_id: "asdf_shims_path",
        met,
        detail: "shell startup config includes asdf shims PATH entry",
    }
}

fn setup_block_for_manager(manager: ManagerId, shell_name: &str) -> Option<String> {
    let block = match manager {
        ManagerId::Rustup => r#"# >>> Helm managed rustup setup >>>
source "$HOME/.cargo/env"
# <<< Helm managed rustup setup <<<
"#
        .to_string(),
        ManagerId::Mise => {
            let shell = match shell_name {
                "bash" => "bash",
                "fish" => "fish",
                _ => "zsh",
            };
            format!(
                "# >>> Helm managed mise setup >>>\neval \"$(mise activate {shell})\"\n# <<< Helm managed mise setup <<<\n"
            )
        }
        ManagerId::Asdf => r#"# >>> Helm managed asdf setup >>>
export PATH="${ASDF_DATA_DIR:-$HOME/.asdf}/shims:$PATH"
# <<< Helm managed asdf setup <<<
"#
        .to_string(),
        _ => return None,
    };
    Some(block)
}

fn report_for_manager(
    manager: ManagerId,
    instances: Option<&[ManagerInstallInstance]>,
    ctx: &ShellSetupContext,
) -> Option<ManagerPostInstallSetupReport> {
    if instances.is_none_or(|value| value.is_empty()) {
        return None;
    }
    let requirements = match manager {
        ManagerId::Rustup => vec![rustup_requirement_status(ctx)],
        ManagerId::Mise => vec![mise_requirement_status(ctx)],
        ManagerId::Asdf => vec![asdf_requirement_status(ctx)],
        _ => return None,
    };

    Some(ManagerPostInstallSetupReport {
        manager,
        shell_name: ctx.shell_name.clone(),
        rc_files: ctx.rc_files.clone(),
        automation_supported: setup_block_for_manager(manager, ctx.shell_name.as_str()).is_some(),
        requirements,
    })
}

fn apply_recommended_post_install_setup_with_context(
    manager: ManagerId,
    manager_install_instances: Option<&[ManagerInstallInstance]>,
    ctx: &ShellSetupContext,
) -> Result<PostInstallAutomationResult, String> {
    let report = report_for_manager(manager, manager_install_instances, ctx).ok_or_else(|| {
        format!(
            "manager '{}' does not expose automated post-install setup",
            manager.as_str()
        )
    })?;

    let block = setup_block_for_manager(manager, ctx.shell_name.as_str()).ok_or_else(|| {
        format!(
            "manager '{}' does not expose automated post-install setup",
            manager.as_str()
        )
    })?;

    let rc_file = report
        .rc_files
        .first()
        .cloned()
        .ok_or_else(|| "no shell rc file target resolved".to_string())?;

    let mut content = read_file_text(rc_file.as_path()).unwrap_or_default();
    if content.contains(block.as_str()) {
        return Ok(PostInstallAutomationResult {
            manager,
            rc_file,
            changed: false,
            summary: "shell setup block already present".to_string(),
        });
    }

    if content.trim().is_empty() {
        content = block;
    } else {
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push('\n');
        content.push_str(block.as_str());
    }

    if let Some(parent) = rc_file.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create shell config directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    fs::write(rc_file.as_path(), content).map_err(|error| {
        format!(
            "failed to write shell config '{}': {error}",
            rc_file.display()
        )
    })?;

    Ok(PostInstallAutomationResult {
        manager,
        rc_file,
        changed: true,
        summary: "appended Helm-managed setup block".to_string(),
    })
}

pub fn evaluate_manager_post_install_setup(
    manager: ManagerId,
    manager_install_instances: Option<&[ManagerInstallInstance]>,
) -> Option<ManagerPostInstallSetupReport> {
    let ctx = ShellSetupContext::from_environment()?;
    report_for_manager(manager, manager_install_instances, &ctx)
}

pub fn apply_recommended_post_install_setup(
    manager: ManagerId,
    manager_install_instances: Option<&[ManagerInstallInstance]>,
) -> Result<PostInstallAutomationResult, String> {
    let ctx = ShellSetupContext::from_environment().ok_or_else(|| {
        format!(
            "HOME/SHELL environment is unavailable; cannot apply {} post-install setup",
            manager.as_str()
        )
    })?;

    apply_recommended_post_install_setup_with_context(manager, manager_install_instances, &ctx)
}

#[cfg(test)]
mod tests {
    use super::{
        ShellSetupContext, apply_recommended_post_install_setup_with_context,
        rc_candidates_for_shell, report_for_manager,
    };
    use crate::models::{
        AutomationLevel, InstallInstanceIdentityKind, InstallProvenance, ManagerId,
        ManagerInstallInstance, StrategyKind,
    };
    use std::path::{Path, PathBuf};

    fn sample_instance(manager: ManagerId) -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager,
            instance_id: "instance-1".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/tmp/example".to_string(),
            display_path: PathBuf::from("/tmp/example"),
            canonical_path: Some(PathBuf::from("/tmp/example")),
            alias_paths: Vec::new(),
            is_active: true,
            version: Some("1.0.0".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.5,
            decision_margin: Some(0.0),
            automation_level: AutomationLevel::NeedsConfirmation,
            uninstall_strategy: StrategyKind::InteractivePrompt,
            update_strategy: StrategyKind::InteractivePrompt,
            remediation_strategy: StrategyKind::InteractivePrompt,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        }
    }

    fn temp_root(test_name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "helm-post-install-setup-{}-{}",
            test_name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temporary root should be creatable");
        path
    }

    fn context_with_shell(home: &Path, shell_name: &str) -> ShellSetupContext {
        ShellSetupContext {
            shell_name: shell_name.to_string(),
            rc_files: rc_candidates_for_shell(home, shell_name),
        }
    }

    #[test]
    fn rustup_requirement_reports_unmet_when_marker_missing() {
        let home = temp_root("rustup-missing");
        let ctx = context_with_shell(home.as_path(), "zsh");
        let instances = vec![sample_instance(ManagerId::Rustup)];
        let report = report_for_manager(
            ManagerId::Rustup,
            Some(instances.as_slice()),
            &ctx,
        )
        .expect("report should be present");
        assert!(report.has_unmet_required());
        assert_eq!(report.unmet_count(), 1);
    }

    #[test]
    fn asdf_requirement_passes_with_shims_marker() {
        let home = temp_root("asdf-met");
        let zshrc = home.join(".zshrc");
        std::fs::write(
            zshrc.as_path(),
            "export PATH=\"${ASDF_DATA_DIR:-$HOME/.asdf}/shims:$PATH\"\n",
        )
        .expect("zshrc should be writable");
        let ctx = context_with_shell(home.as_path(), "zsh");
        let instances = vec![sample_instance(ManagerId::Asdf)];
        let report = report_for_manager(
            ManagerId::Asdf,
            Some(instances.as_slice()),
            &ctx,
        )
        .expect("report should be present");
        assert!(!report.has_unmet_required());
    }

    #[test]
    fn mise_requirement_passes_with_activate_marker() {
        let home = temp_root("mise-met");
        let zshrc = home.join(".zshrc");
        std::fs::write(zshrc.as_path(), "eval \"$(mise activate zsh)\"\n")
            .expect("zshrc should be writable");
        let ctx = context_with_shell(home.as_path(), "zsh");
        let instances = vec![sample_instance(ManagerId::Mise)];
        let report = report_for_manager(
            ManagerId::Mise,
            Some(instances.as_slice()),
            &ctx,
        )
        .expect("report should be present");
        assert!(!report.has_unmet_required());
    }

    #[test]
    fn unsupported_manager_returns_no_report() {
        let home = temp_root("unsupported");
        let ctx = context_with_shell(home.as_path(), "zsh");
        let instances = vec![sample_instance(ManagerId::Npm)];
        assert!(
            report_for_manager(ManagerId::Npm, Some(instances.as_slice()), &ctx).is_none()
        );
    }

    #[test]
    fn apply_setup_is_idempotent() {
        let home = temp_root("apply-idempotent");
        let rc = home.join(".zshrc");
        std::fs::write(rc.as_path(), "# existing\n").expect("zshrc should be writable");
        let ctx = context_with_shell(home.as_path(), "zsh");
        let instances = vec![sample_instance(ManagerId::Rustup)];
        let first = apply_recommended_post_install_setup_with_context(
            ManagerId::Rustup,
            Some(instances.as_slice()),
            &ctx,
        )
        .expect("first setup should succeed");
        assert!(first.changed);

        let second = apply_recommended_post_install_setup_with_context(
            ManagerId::Rustup,
            Some(instances.as_slice()),
            &ctx,
        )
        .expect("second setup should succeed");
        assert!(!second.changed);
    }
}
