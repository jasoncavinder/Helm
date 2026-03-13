use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use crate::models::{
    AutomationLevel, DetectionInfo, InstallInstanceIdentityKind, InstallProvenance, ManagerId,
    ManagerInstallInstance, StrategyKind,
};
#[cfg(test)]
use crate::provenance_policy::{AUTOMATIC_CONFIDENCE_THRESHOLD, UNKNOWN_CONFIRMATION_THRESHOLD};
use crate::provenance_policy::{
    PROVENANCE_CONFIDENCE_THRESHOLD, PROVENANCE_MARGIN_THRESHOLD, automation_level_for,
};

const EXTERNAL_EVIDENCE_TIMEOUT: Duration = Duration::from_millis(750);
type ExternalCommandRunner = fn(&str, &[&str], Duration) -> Option<String>;

#[derive(Clone, Debug)]
struct ScoreFactor {
    provenance: InstallProvenance,
    weight: f64,
    reason: String,
}

fn configured_asdf_root_paths() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".asdf"));
    }
    if let Some(path) = std::env::var_os("ASDF_DIR").map(PathBuf::from)
        && path.is_absolute()
        && !path.as_os_str().is_empty()
    {
        roots.push(path);
    }
    if let Some(path) = std::env::var_os("ASDF_DATA_DIR").map(PathBuf::from)
        && path.is_absolute()
        && !path.as_os_str().is_empty()
    {
        roots.push(path);
    }
    roots.sort();
    roots.dedup();
    roots
}

fn path_contains_asdf_root_subpath(text: &str, suffix: &str) -> bool {
    text.contains("/.asdf/")
        || configured_asdf_root_paths().iter().any(|root| {
            let needle = format!(
                "{}/{}",
                root.to_string_lossy().to_string().to_lowercase(),
                suffix
            );
            text.contains(needle.as_str())
        })
}

fn path_contains_asdf_shims(text: &str) -> bool {
    path_contains_asdf_root_subpath(text, "shims/")
}

fn path_contains_asdf_installs(text: &str) -> bool {
    path_contains_asdf_root_subpath(text, "installs/")
}

fn path_contains_asdf_bin_exec(text: &str, executable: &str) -> bool {
    text.contains(format!("/.asdf/bin/{executable}").as_str())
        || configured_asdf_root_paths().iter().any(|root| {
            let needle = format!(
                "{}/bin/{}",
                root.to_string_lossy().to_string().to_lowercase(),
                executable
            );
            text.contains(needle.as_str())
        })
}

trait ProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        context: &mut ExternalEvidenceContext,
    );
}

struct RustupProvenanceSpec;
struct HomebrewProvenanceSpec;
struct AsdfProvenanceSpec;
struct MiseProvenanceSpec;
struct MasProvenanceSpec;
struct NpmProvenanceSpec;
struct PnpmProvenanceSpec;
struct YarnProvenanceSpec;
struct PipProvenanceSpec;
struct PipxProvenanceSpec;
struct PoetryProvenanceSpec;
struct RubyGemsProvenanceSpec;
struct BundlerProvenanceSpec;
struct CargoProvenanceSpec;
struct CargoBinstallProvenanceSpec;
struct SoftwareUpdateProvenanceSpec;
struct MacportsManagerProvenanceSpec;
struct NixDarwinProvenanceSpec;
struct SparkleProvenanceSpec;
struct SetappProvenanceSpec;
struct HomebrewCaskProvenanceSpec;
struct DockerDesktopProvenanceSpec;
struct PodmanProvenanceSpec;
struct ColimaProvenanceSpec;
struct ParallelsDesktopProvenanceSpec;
struct XcodeCommandLineToolsProvenanceSpec;
struct Rosetta2ProvenanceSpec;
struct FirmwareUpdatesProvenanceSpec;

static RUSTUP_PROVENANCE_SPEC: RustupProvenanceSpec = RustupProvenanceSpec;
static HOMEBREW_PROVENANCE_SPEC: HomebrewProvenanceSpec = HomebrewProvenanceSpec;
static ASDF_PROVENANCE_SPEC: AsdfProvenanceSpec = AsdfProvenanceSpec;
static MISE_PROVENANCE_SPEC: MiseProvenanceSpec = MiseProvenanceSpec;
static MAS_PROVENANCE_SPEC: MasProvenanceSpec = MasProvenanceSpec;
static NPM_PROVENANCE_SPEC: NpmProvenanceSpec = NpmProvenanceSpec;
static PNPM_PROVENANCE_SPEC: PnpmProvenanceSpec = PnpmProvenanceSpec;
static YARN_PROVENANCE_SPEC: YarnProvenanceSpec = YarnProvenanceSpec;
static PIP_PROVENANCE_SPEC: PipProvenanceSpec = PipProvenanceSpec;
static PIPX_PROVENANCE_SPEC: PipxProvenanceSpec = PipxProvenanceSpec;
static POETRY_PROVENANCE_SPEC: PoetryProvenanceSpec = PoetryProvenanceSpec;
static RUBYGEMS_PROVENANCE_SPEC: RubyGemsProvenanceSpec = RubyGemsProvenanceSpec;
static BUNDLER_PROVENANCE_SPEC: BundlerProvenanceSpec = BundlerProvenanceSpec;
static CARGO_PROVENANCE_SPEC: CargoProvenanceSpec = CargoProvenanceSpec;
static CARGO_BINSTALL_PROVENANCE_SPEC: CargoBinstallProvenanceSpec = CargoBinstallProvenanceSpec;
static SOFTWAREUPDATE_PROVENANCE_SPEC: SoftwareUpdateProvenanceSpec = SoftwareUpdateProvenanceSpec;
static MACPORTS_MANAGER_PROVENANCE_SPEC: MacportsManagerProvenanceSpec =
    MacportsManagerProvenanceSpec;
static NIX_DARWIN_PROVENANCE_SPEC: NixDarwinProvenanceSpec = NixDarwinProvenanceSpec;
static SPARKLE_PROVENANCE_SPEC: SparkleProvenanceSpec = SparkleProvenanceSpec;
static SETAPP_PROVENANCE_SPEC: SetappProvenanceSpec = SetappProvenanceSpec;
static HOMEBREW_CASK_PROVENANCE_SPEC: HomebrewCaskProvenanceSpec = HomebrewCaskProvenanceSpec;
static DOCKER_DESKTOP_PROVENANCE_SPEC: DockerDesktopProvenanceSpec = DockerDesktopProvenanceSpec;
static PODMAN_PROVENANCE_SPEC: PodmanProvenanceSpec = PodmanProvenanceSpec;
static COLIMA_PROVENANCE_SPEC: ColimaProvenanceSpec = ColimaProvenanceSpec;
static PARALLELS_DESKTOP_PROVENANCE_SPEC: ParallelsDesktopProvenanceSpec =
    ParallelsDesktopProvenanceSpec;
static XCODE_COMMAND_LINE_TOOLS_PROVENANCE_SPEC: XcodeCommandLineToolsProvenanceSpec =
    XcodeCommandLineToolsProvenanceSpec;
static ROSETTA2_PROVENANCE_SPEC: Rosetta2ProvenanceSpec = Rosetta2ProvenanceSpec;
static FIRMWARE_UPDATES_PROVENANCE_SPEC: FirmwareUpdatesProvenanceSpec =
    FirmwareUpdatesProvenanceSpec;

#[derive(Clone, Debug, Eq, PartialEq)]
enum PkgutilFileOwner {
    Owned(String),
    NotOwned,
}

struct ExternalEvidenceContext {
    brew_prefix_by_formula: HashMap<String, Option<String>>,
    pkgutil_file_owner: HashMap<String, Option<PkgutilFileOwner>>,
    allow_external: bool,
    runner: ExternalCommandRunner,
}

impl ExternalEvidenceContext {
    fn new() -> Self {
        Self {
            brew_prefix_by_formula: HashMap::new(),
            pkgutil_file_owner: HashMap::new(),
            allow_external: true,
            runner: run_command_with_timeout,
        }
    }

    #[cfg(test)]
    fn without_external_queries() -> Self {
        Self {
            brew_prefix_by_formula: HashMap::new(),
            pkgutil_file_owner: HashMap::new(),
            allow_external: false,
            runner: run_command_with_timeout,
        }
    }

    #[cfg(test)]
    fn with_runner(runner: ExternalCommandRunner) -> Self {
        Self {
            brew_prefix_by_formula: HashMap::new(),
            pkgutil_file_owner: HashMap::new(),
            allow_external: true,
            runner,
        }
    }

    fn brew_prefix(&mut self, formula: &str) -> Option<String> {
        let key = formula.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        if let Some(cached) = self.brew_prefix_by_formula.get(&key) {
            debug!(
                probe = "brew_prefix",
                formula = %key,
                cache_hit = true,
                result_present = cached.is_some(),
                "using cached external provenance evidence"
            );
            return cached.clone();
        }
        if !self.allow_external {
            self.brew_prefix_by_formula.insert(key.clone(), None);
            debug!(
                probe = "brew_prefix",
                formula = %key,
                cache_hit = false,
                allowed = false,
                "skipping external provenance evidence query"
            );
            return None;
        }

        let value = (self.runner)(
            "brew",
            &["--prefix", key.as_str()],
            EXTERNAL_EVIDENCE_TIMEOUT,
        )
        .and_then(|output| {
            let trimmed = output.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        debug!(
            probe = "brew_prefix",
            formula = %key,
            cache_hit = false,
            result_present = value.is_some(),
            "resolved external provenance evidence"
        );
        self.brew_prefix_by_formula.insert(key, value.clone());
        value
    }

    fn pkgutil_file_owner(&mut self, path: &Path) -> Option<PkgutilFileOwner> {
        let key = path.to_string_lossy().to_string();
        if let Some(cached) = self.pkgutil_file_owner.get(&key) {
            debug!(
                probe = "pkgutil_file_owner",
                cache_hit = true,
                path = %key,
                result_present = cached.is_some(),
                "using cached pkgutil ownership evidence"
            );
            return cached.clone();
        }

        if !self.allow_external {
            self.pkgutil_file_owner.insert(key.clone(), None);
            debug!(
                probe = "pkgutil_file_owner",
                cache_hit = false,
                allowed = false,
                path = %key,
                "skipping pkgutil ownership evidence query"
            );
            return None;
        }

        let value = {
            let args_owned = ["--file-info".to_string(), key.clone()];
            let args = [args_owned[0].as_str(), args_owned[1].as_str()];
            (self.runner)("pkgutil", &args, EXTERNAL_EVIDENCE_TIMEOUT)
                .and_then(|output| parse_pkgutil_file_owner(output.as_str()))
        };

        debug!(
            probe = "pkgutil_file_owner",
            cache_hit = false,
            path = %key,
            result_present = value.is_some(),
            "resolved pkgutil ownership evidence"
        );
        self.pkgutil_file_owner.insert(key, value.clone());
        value
    }
}

#[derive(Clone, Debug)]
struct CandidateInstance {
    identity_kind: InstallInstanceIdentityKind,
    identity_value: String,
    display_path: PathBuf,
    canonical_path: Option<PathBuf>,
    alias_paths: Vec<PathBuf>,
    is_active: bool,
}

pub fn collect_manager_install_instances(
    manager: ManagerId,
    detection: &DetectionInfo,
) -> Vec<ManagerInstallInstance> {
    if !detection.installed {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    if let Some(active_path) = detection.executable_path.as_ref() {
        candidates.push(active_path.clone());
    }
    candidates.extend(
        discover_executable_paths(manager, manager_executable_candidates(manager))
            .into_iter()
            .map(PathBuf::from),
    );

    collect_manager_install_instances_from_candidates(manager, detection, &candidates)
}

fn collect_manager_install_instances_from_candidates(
    manager: ManagerId,
    detection: &DetectionInfo,
    candidates: &[PathBuf],
) -> Vec<ManagerInstallInstance> {
    let active_path = detection.executable_path.as_ref();
    let active_canonical = active_path.and_then(|path| path.canonicalize().ok());

    let mut by_identity: HashMap<String, CandidateInstance> = HashMap::new();
    for candidate in candidates {
        let canonical = candidate.canonicalize().ok();
        if !candidate_represents_executable(candidate, canonical.as_deref()) {
            continue;
        }

        let (identity_kind, identity_value) = compute_identity(candidate, canonical.as_deref());
        let key = format!("{}:{}", identity_kind.as_str(), identity_value);

        let is_active = is_active_candidate(
            candidate,
            canonical.as_deref(),
            active_path.map(PathBuf::as_path),
            active_canonical.as_deref(),
        );

        let entry = by_identity.entry(key).or_insert_with(|| CandidateInstance {
            identity_kind,
            identity_value,
            display_path: candidate.clone(),
            canonical_path: canonical.clone(),
            alias_paths: vec![candidate.clone()],
            is_active,
        });

        if is_active {
            entry.is_active = true;
            entry.display_path = candidate.clone();
        }
        if entry.canonical_path.is_none() {
            entry.canonical_path = canonical.clone();
        }
        if !entry.alias_paths.iter().any(|path| path == candidate) {
            entry.alias_paths.push(candidate.clone());
        }
    }

    let mut identities: Vec<CandidateInstance> = by_identity.into_values().collect();
    identities.sort_by(|left, right| {
        if left.is_active != right.is_active {
            return right.is_active.cmp(&left.is_active);
        }

        left.display_path
            .to_string_lossy()
            .cmp(&right.display_path.to_string_lossy())
    });

    let mut context = ExternalEvidenceContext::new();
    identities
        .into_iter()
        .map(|candidate| classify_instance(manager, detection, candidate, &mut context))
        .collect()
}

fn classify_instance(
    manager: ManagerId,
    detection: &DetectionInfo,
    candidate: CandidateInstance,
    context: &mut ExternalEvidenceContext,
) -> ManagerInstallInstance {
    let instance_id =
        stable_instance_id(manager, candidate.identity_kind, &candidate.identity_value);

    let mut base = ManagerInstallInstance {
        manager,
        instance_id,
        identity_kind: candidate.identity_kind,
        identity_value: candidate.identity_value,
        display_path: candidate.display_path,
        canonical_path: candidate.canonical_path,
        alias_paths: candidate.alias_paths,
        is_active: candidate.is_active,
        version: detection.version.clone(),
        provenance: InstallProvenance::Unknown,
        confidence: 0.0,
        decision_margin: None,
        automation_level: AutomationLevel::ReadOnly,
        uninstall_strategy: StrategyKind::InteractivePrompt,
        update_strategy: StrategyKind::InteractivePrompt,
        remediation_strategy: StrategyKind::ManualRemediation,
        explanation_primary: None,
        explanation_secondary: None,
        competing_provenance: None,
        competing_confidence: None,
    };

    provenance_spec_for(manager).classify(&mut base, context);

    base
}

fn provenance_spec_for(manager: ManagerId) -> &'static dyn ProvenanceSpec {
    match manager {
        ManagerId::Rustup => &RUSTUP_PROVENANCE_SPEC,
        ManagerId::HomebrewFormula => &HOMEBREW_PROVENANCE_SPEC,
        ManagerId::Asdf => &ASDF_PROVENANCE_SPEC,
        ManagerId::Mise => &MISE_PROVENANCE_SPEC,
        ManagerId::Mas => &MAS_PROVENANCE_SPEC,
        ManagerId::Npm => &NPM_PROVENANCE_SPEC,
        ManagerId::Pnpm => &PNPM_PROVENANCE_SPEC,
        ManagerId::Yarn => &YARN_PROVENANCE_SPEC,
        ManagerId::Pip => &PIP_PROVENANCE_SPEC,
        ManagerId::Pipx => &PIPX_PROVENANCE_SPEC,
        ManagerId::Poetry => &POETRY_PROVENANCE_SPEC,
        ManagerId::RubyGems => &RUBYGEMS_PROVENANCE_SPEC,
        ManagerId::Bundler => &BUNDLER_PROVENANCE_SPEC,
        ManagerId::Cargo => &CARGO_PROVENANCE_SPEC,
        ManagerId::CargoBinstall => &CARGO_BINSTALL_PROVENANCE_SPEC,
        ManagerId::SoftwareUpdate => &SOFTWAREUPDATE_PROVENANCE_SPEC,
        ManagerId::MacPorts => &MACPORTS_MANAGER_PROVENANCE_SPEC,
        ManagerId::NixDarwin => &NIX_DARWIN_PROVENANCE_SPEC,
        ManagerId::Sparkle => &SPARKLE_PROVENANCE_SPEC,
        ManagerId::Setapp => &SETAPP_PROVENANCE_SPEC,
        ManagerId::HomebrewCask => &HOMEBREW_CASK_PROVENANCE_SPEC,
        ManagerId::DockerDesktop => &DOCKER_DESKTOP_PROVENANCE_SPEC,
        ManagerId::Podman => &PODMAN_PROVENANCE_SPEC,
        ManagerId::Colima => &COLIMA_PROVENANCE_SPEC,
        ManagerId::ParallelsDesktop => &PARALLELS_DESKTOP_PROVENANCE_SPEC,
        ManagerId::XcodeCommandLineTools => &XCODE_COMMAND_LINE_TOOLS_PROVENANCE_SPEC,
        ManagerId::Rosetta2 => &ROSETTA2_PROVENANCE_SPEC,
        ManagerId::FirmwareUpdates => &FIRMWARE_UPDATES_PROVENANCE_SPEC,
    }
}

impl ProvenanceSpec for RustupProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        context: &mut ExternalEvidenceContext,
    ) {
        classify_rustup_instance(instance, context);
    }
}

impl ProvenanceSpec for HomebrewProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_homebrew_formula_manager_instance(instance, "brew");
    }
}

impl ProvenanceSpec for AsdfProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_asdf_instance(instance);
    }
}

impl ProvenanceSpec for MiseProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        context: &mut ExternalEvidenceContext,
    ) {
        classify_mise_instance(instance, context);
    }
}

impl ProvenanceSpec for MasProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_homebrew_formula_manager_instance(instance, "mas");
    }
}

impl ProvenanceSpec for NpmProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_node_runtime_instance(instance, "npm");
    }
}

impl ProvenanceSpec for PnpmProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_node_runtime_instance(instance, "pnpm");
    }
}

impl ProvenanceSpec for YarnProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_node_runtime_instance(instance, "yarn");
    }
}

impl ProvenanceSpec for PipProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_python_runtime_instance(instance, "pip", &["pip", "pip3", "python3"]);
    }
}

impl ProvenanceSpec for PipxProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_python_runtime_instance(instance, "pipx", &["pipx"]);
    }
}

impl ProvenanceSpec for PoetryProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_python_runtime_instance(instance, "poetry", &["poetry"]);
    }
}

impl ProvenanceSpec for RubyGemsProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_ruby_runtime_instance(instance, "rubygems", &["gem"]);
    }
}

impl ProvenanceSpec for BundlerProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_ruby_runtime_instance(instance, "bundler", &["bundle", "bundler"]);
    }
}

impl ProvenanceSpec for CargoProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_cargo_runtime_instance(instance, "cargo", &["cargo"]);
    }
}

impl ProvenanceSpec for CargoBinstallProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_cargo_runtime_instance(instance, "cargo_binstall", &["cargo-binstall"]);
    }
}

impl ProvenanceSpec for SoftwareUpdateProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_system_guarded_manager_instance(instance, "softwareupdate", &["softwareupdate"]);
    }
}

impl ProvenanceSpec for MacportsManagerProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_macports_manager_instance(instance);
    }
}

impl ProvenanceSpec for NixDarwinProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_nix_darwin_manager_instance(instance);
    }
}

impl ProvenanceSpec for SparkleProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_application_manager_instance(instance, "sparkle", &["sparkle"], "sparkle.app");
    }
}

impl ProvenanceSpec for SetappProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_application_manager_instance(instance, "setapp", &["setapp"], "setapp.app");
    }
}

impl ProvenanceSpec for HomebrewCaskProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_homebrew_formula_manager_instance(instance, "brew");
    }
}

impl ProvenanceSpec for DockerDesktopProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_application_manager_instance(
            instance,
            "docker_desktop",
            &["docker", "docker-desktop"],
            "docker.app",
        );
    }
}

impl ProvenanceSpec for PodmanProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_runtime_manager_instance(instance, "podman", &["podman"]);
    }
}

impl ProvenanceSpec for ColimaProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_runtime_manager_instance(instance, "colima", &["colima"]);
    }
}

impl ProvenanceSpec for ParallelsDesktopProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_application_manager_instance(
            instance,
            "parallels_desktop",
            &["prlctl", "parallels"],
            "parallels desktop.app",
        );
    }
}

impl ProvenanceSpec for XcodeCommandLineToolsProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_xcode_command_line_tools_instance(instance);
    }
}

impl ProvenanceSpec for Rosetta2ProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_system_guarded_manager_instance(instance, "rosetta2", &["softwareupdate"]);
    }
}

impl ProvenanceSpec for FirmwareUpdatesProvenanceSpec {
    fn classify(
        &self,
        instance: &mut ManagerInstallInstance,
        _context: &mut ExternalEvidenceContext,
    ) {
        classify_system_guarded_manager_instance(instance, "firmware_updates", &["softwareupdate"]);
    }
}

fn classify_asdf_instance(instance: &mut ManagerInstallInstance) {
    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();

    let asdf_layout = path_contains_asdf_bin_exec(canonical.as_str(), "asdf")
        || path_contains_asdf_shims(canonical.as_str())
        || path_contains_asdf_installs(canonical.as_str())
        || path_contains_asdf_bin_exec(display.as_str(), "asdf")
        || path_contains_asdf_shims(display.as_str())
        || path_contains_asdf_installs(display.as_str());

    if asdf_layout {
        let confidence = 0.92;
        instance.provenance = InstallProvenance::Asdf;
        instance.confidence = confidence;
        instance.decision_margin = Some(0.30);
        instance.automation_level = automation_level_for(instance.provenance, confidence);
        instance.uninstall_strategy = StrategyKind::AsdfSelf;
        instance.update_strategy = StrategyKind::AsdfSelf;
        instance.remediation_strategy = StrategyKind::AsdfSelf;
        instance.explanation_primary =
            Some("asdf executable path indicates asdf-managed layout".to_string());
        instance.explanation_secondary = None;
        instance.competing_provenance = Some(InstallProvenance::Homebrew);
        instance.competing_confidence = Some(0.35);
        return;
    }

    classify_homebrew_formula_manager_instance(instance, "asdf");
}

fn classify_node_runtime_instance(instance: &mut ManagerInstallInstance, tool_name: &str) {
    classify_runtime_manager_instance(instance, tool_name, &[tool_name]);
}

fn classify_python_runtime_instance(
    instance: &mut ManagerInstallInstance,
    manager_label: &str,
    executable_names: &[&str],
) {
    classify_runtime_manager_instance(instance, manager_label, executable_names);
}

fn classify_ruby_runtime_instance(
    instance: &mut ManagerInstallInstance,
    manager_label: &str,
    executable_names: &[&str],
) {
    classify_runtime_manager_instance(instance, manager_label, executable_names);
}

fn cargo_home_exec_hints(executable_names: &[&str]) -> Vec<String> {
    let mut hints = executable_names
        .iter()
        .map(|name| format!("/.cargo/bin/{name}"))
        .collect::<Vec<_>>();

    if let Some(cargo_home) = std::env::var_os("CARGO_HOME").map(PathBuf::from) {
        for executable_name in executable_names {
            let hint = cargo_home
                .join("bin")
                .join(executable_name)
                .to_string_lossy()
                .to_lowercase();
            hints.push(hint);
        }
    }

    hints
}

fn classify_cargo_runtime_instance(
    instance: &mut ManagerInstallInstance,
    manager_label: &str,
    executable_names: &[&str],
) {
    classify_runtime_manager_instance(instance, manager_label, executable_names);

    if instance.provenance != InstallProvenance::Unknown {
        return;
    }

    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();

    let canonical_matches_exec = path_matches_any_exec_name(canonical.as_str(), executable_names);
    let display_matches_exec = path_matches_any_exec_name(display.as_str(), executable_names);
    let cargo_home_hints = cargo_home_exec_hints(executable_names);

    let cargo_home_layout = ((canonical.contains("/.cargo/bin/")
        || display.contains("/.cargo/bin/"))
        && (canonical_matches_exec || display_matches_exec))
        || cargo_home_hints.iter().any(|hint| {
            canonical == *hint
                || display == *hint
                || canonical.ends_with(hint)
                || display.ends_with(hint)
        });

    if !cargo_home_layout {
        return;
    }

    let confidence = 0.78;
    instance.provenance = InstallProvenance::SourceBuild;
    instance.confidence = confidence;
    instance.decision_margin = Some(0.20);
    instance.automation_level = automation_level_for(instance.provenance, confidence);
    instance.uninstall_strategy = uninstall_strategy_for(instance.manager, instance.provenance);
    instance.update_strategy = update_strategy_for(instance.provenance);
    instance.remediation_strategy = remediation_strategy_for(instance.provenance);
    instance.explanation_primary = Some(format!(
        "{} path matches cargo-home style install layout",
        manager_label
    ));
    instance.explanation_secondary = None;
    instance.competing_provenance = None;
    instance.competing_confidence = None;
}

fn set_instance_provenance(
    instance: &mut ManagerInstallInstance,
    provenance: InstallProvenance,
    confidence: f64,
    decision_margin: Option<f64>,
    explainability: ProvenanceExplainability,
) {
    let clamped_confidence = confidence.clamp(0.0, 1.0);
    instance.provenance = provenance;
    instance.confidence = clamped_confidence;
    instance.decision_margin = decision_margin;
    instance.automation_level = automation_level_for(provenance, clamped_confidence);
    instance.uninstall_strategy = uninstall_strategy_for(instance.manager, provenance);
    instance.update_strategy = update_strategy_for(provenance);
    instance.remediation_strategy = remediation_strategy_for(provenance);
    instance.explanation_primary = Some(explainability.explanation_primary);
    instance.explanation_secondary = explainability.explanation_secondary;
    instance.competing_provenance = explainability.competing.map(|(provenance, _)| provenance);
    instance.competing_confidence = explainability
        .competing
        .map(|(_, confidence)| confidence.clamp(0.0, 1.0));
}

struct ProvenanceExplainability {
    explanation_primary: String,
    explanation_secondary: Option<String>,
    competing: Option<(InstallProvenance, f64)>,
}

fn classify_system_guarded_manager_instance(
    instance: &mut ManagerInstallInstance,
    manager_label: &str,
    executable_names: &[&str],
) {
    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let matches_exec = path_matches_any_exec_name(canonical.as_str(), executable_names)
        || path_matches_any_exec_name(display.as_str(), executable_names);

    let system_prefix = canonical.starts_with("/usr/bin/")
        || canonical.starts_with("/usr/sbin/")
        || canonical.starts_with("/system/")
        || canonical.starts_with("/bin/")
        || canonical.starts_with("/sbin/")
        || display.starts_with("/usr/bin/")
        || display.starts_with("/usr/sbin/")
        || display.starts_with("/system/")
        || display.starts_with("/bin/")
        || display.starts_with("/sbin/");

    if system_prefix && matches_exec {
        set_instance_provenance(
            instance,
            InstallProvenance::System,
            0.99,
            Some(0.60),
            ProvenanceExplainability {
                explanation_primary: format!(
                    "{} executable path is in an OS-managed system prefix",
                    manager_label
                ),
                explanation_secondary: None,
                competing: Some((InstallProvenance::Unknown, 0.30)),
            },
        );
        return;
    }

    set_instance_provenance(
        instance,
        InstallProvenance::Unknown,
        0.30,
        None,
        ProvenanceExplainability {
            explanation_primary: format!(
                "{} executable path is not a trusted OS-managed location",
                manager_label
            ),
            explanation_secondary: Some("defaulting to unknown read-only behavior".to_string()),
            competing: None,
        },
    );
}

fn classify_xcode_command_line_tools_instance(instance: &mut ManagerInstallInstance) {
    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let matches_xcode_select = path_matches_exec_name(canonical.as_str(), "xcode-select")
        || path_matches_exec_name(display.as_str(), "xcode-select");
    let matches_clt_clang = (path_matches_exec_name(canonical.as_str(), "clang")
        || path_matches_exec_name(display.as_str(), "clang"))
        && (canonical.starts_with("/library/developer/commandlinetools/")
            || display.starts_with("/library/developer/commandlinetools/"));
    let trusted_clt_prefix = canonical.starts_with("/library/developer/commandlinetools/")
        || display.starts_with("/library/developer/commandlinetools/");
    let system_prefix = canonical.starts_with("/usr/bin/")
        || canonical.starts_with("/usr/sbin/")
        || canonical.starts_with("/system/")
        || canonical.starts_with("/bin/")
        || canonical.starts_with("/sbin/")
        || display.starts_with("/usr/bin/")
        || display.starts_with("/usr/sbin/")
        || display.starts_with("/system/")
        || display.starts_with("/bin/")
        || display.starts_with("/sbin/")
        || trusted_clt_prefix;

    if (system_prefix && matches_xcode_select) || matches_clt_clang {
        set_instance_provenance(
            instance,
            InstallProvenance::System,
            0.99,
            Some(0.60),
            ProvenanceExplainability {
                explanation_primary:
                    "xcode_command_line_tools executable path is in an OS-managed system location"
                        .to_string(),
                explanation_secondary: None,
                competing: Some((InstallProvenance::Unknown, 0.30)),
            },
        );
        return;
    }

    set_instance_provenance(
        instance,
        InstallProvenance::Unknown,
        0.30,
        None,
        ProvenanceExplainability {
            explanation_primary:
                "xcode_command_line_tools executable path is not a trusted OS-managed location"
                    .to_string(),
            explanation_secondary: Some("defaulting to unknown read-only behavior".to_string()),
            competing: None,
        },
    );
}

fn classify_macports_manager_instance(instance: &mut ManagerInstallInstance) {
    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let matches_exec = path_matches_exec_name(canonical.as_str(), "port")
        || path_matches_exec_name(display.as_str(), "port");

    if (canonical.starts_with("/opt/local/") || display.starts_with("/opt/local/")) && matches_exec
    {
        set_instance_provenance(
            instance,
            InstallProvenance::Macports,
            0.96,
            Some(0.45),
            ProvenanceExplainability {
                explanation_primary: "macports executable path is in MacPorts-managed prefix"
                    .to_string(),
                explanation_secondary: None,
                competing: Some((InstallProvenance::Unknown, 0.35)),
            },
        );
        return;
    }

    if (canonical.starts_with("/usr/bin/") || display.starts_with("/usr/bin/")) && matches_exec {
        set_instance_provenance(
            instance,
            InstallProvenance::System,
            0.90,
            Some(0.35),
            ProvenanceExplainability {
                explanation_primary: "macports executable path appears to be system-managed"
                    .to_string(),
                explanation_secondary: None,
                competing: Some((InstallProvenance::Unknown, 0.30)),
            },
        );
        return;
    }

    set_instance_provenance(
        instance,
        InstallProvenance::Unknown,
        0.32,
        None,
        ProvenanceExplainability {
            explanation_primary:
                "insufficient or conflicting macports provenance evidence; defaulting to unknown"
                    .to_string(),
            explanation_secondary: None,
            competing: None,
        },
    );
}

fn classify_nix_darwin_manager_instance(instance: &mut ManagerInstallInstance) {
    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let matches_exec = path_matches_any_exec_name(canonical.as_str(), &["darwin-rebuild", "nix"])
        || path_matches_any_exec_name(display.as_str(), &["darwin-rebuild", "nix"]);
    let nix_layout = canonical.contains("/nix/store/")
        || display.contains("/nix/store/")
        || canonical.contains("/nix/var/nix/profiles/")
        || display.contains("/nix/var/nix/profiles/")
        || canonical.contains("/run/current-system/sw/")
        || display.contains("/run/current-system/sw/");

    if nix_layout && matches_exec {
        set_instance_provenance(
            instance,
            InstallProvenance::Nix,
            0.97,
            Some(0.55),
            ProvenanceExplainability {
                explanation_primary: "nix_darwin executable path matches Nix-managed layout"
                    .to_string(),
                explanation_secondary: None,
                competing: Some((InstallProvenance::Unknown, 0.30)),
            },
        );
        return;
    }

    set_instance_provenance(
        instance,
        InstallProvenance::Unknown,
        0.28,
        None,
        ProvenanceExplainability {
            explanation_primary:
                "insufficient or conflicting nix_darwin provenance evidence; defaulting to unknown"
                    .to_string(),
            explanation_secondary: None,
            competing: None,
        },
    );
}

fn classify_application_manager_instance(
    instance: &mut ManagerInstallInstance,
    manager_label: &str,
    executable_names: &[&str],
    app_bundle_name: &str,
) {
    classify_runtime_manager_instance(instance, manager_label, executable_names);
    if instance.provenance != InstallProvenance::Unknown {
        return;
    }

    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let app_bundle = format!("/{app_bundle_name}/");
    let app_in_path =
        canonical.contains(app_bundle.as_str()) || display.contains(app_bundle.as_str());

    if app_in_path {
        let system_app = canonical.contains("/system/applications/")
            || display.contains("/system/applications/");
        let provenance = if system_app {
            InstallProvenance::System
        } else {
            InstallProvenance::SourceBuild
        };
        let confidence = if system_app { 0.95 } else { 0.78 };
        set_instance_provenance(
            instance,
            provenance,
            confidence,
            Some(0.25),
            ProvenanceExplainability {
                explanation_primary: format!(
                    "{} executable path is inside application bundle {}",
                    manager_label, app_bundle_name
                ),
                explanation_secondary: None,
                competing: Some((InstallProvenance::Unknown, 0.32)),
            },
        );
    }
}

fn path_matches_exec_name(path: &str, executable_name: &str) -> bool {
    if path.ends_with(format!("/{executable_name}").as_str()) {
        return true;
    }

    path.rsplit('/')
        .next()
        .map(|basename| {
            basename == executable_name
                || basename
                    .strip_prefix(executable_name)
                    .is_some_and(|suffix| {
                        suffix.starts_with('.')
                            || suffix
                                .chars()
                                .next()
                                .is_some_and(|character| character.is_ascii_digit())
                    })
        })
        .unwrap_or(false)
}

fn path_matches_bin_exec_name(path: &str, executable_name: &str) -> bool {
    if path.ends_with(format!("/bin/{executable_name}").as_str()) {
        return true;
    }

    path.rsplit_once("/bin/")
        .map(|(_, basename)| {
            basename == executable_name
                || basename
                    .strip_prefix(executable_name)
                    .is_some_and(|suffix| {
                        suffix.starts_with('.')
                            || suffix
                                .chars()
                                .next()
                                .is_some_and(|character| character.is_ascii_digit())
                    })
        })
        .unwrap_or(false)
}

fn path_matches_any_exec_name(path: &str, executable_names: &[&str]) -> bool {
    executable_names
        .iter()
        .any(|name| path_matches_exec_name(path, name))
}

fn path_matches_any_bin_exec_name(path: &str, executable_names: &[&str]) -> bool {
    executable_names
        .iter()
        .any(|name| path_matches_bin_exec_name(path, name))
}

fn classify_runtime_manager_instance(
    instance: &mut ManagerInstallInstance,
    manager_label: &str,
    executable_names: &[&str],
) {
    let mut scores: HashMap<InstallProvenance, f64> = HashMap::new();
    let mut factors: Vec<ScoreFactor> = Vec::new();

    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let canonical_matches_exec = path_matches_any_exec_name(canonical.as_str(), executable_names);
    let display_matches_exec = path_matches_any_exec_name(display.as_str(), executable_names);
    let canonical_matches_bin_exec =
        path_matches_any_bin_exec_name(canonical.as_str(), executable_names);
    let display_matches_bin_exec =
        path_matches_any_bin_exec_name(display.as_str(), executable_names);

    add_score(
        &mut scores,
        &mut factors,
        canonical.contains("/cellar/") && canonical_matches_bin_exec,
        InstallProvenance::Homebrew,
        0.95,
        format!(
            "canonical path is inside Homebrew Cellar for {}",
            manager_label
        ),
    );

    add_score(
        &mut scores,
        &mut factors,
        ((canonical.starts_with("/opt/homebrew/opt/") || canonical.starts_with("/usr/local/opt/"))
            && canonical_matches_bin_exec)
            || (canonical.starts_with("/opt/homebrew/bin/") && canonical_matches_exec)
            || ((display.starts_with("/opt/homebrew/opt/")
                || display.starts_with("/usr/local/opt/"))
                && display_matches_bin_exec)
            || (display.starts_with("/opt/homebrew/bin/") && display_matches_exec),
        InstallProvenance::Homebrew,
        0.85,
        format!(
            "{} path is in an explicit Homebrew prefix (excluding ambiguous /usr/local/bin)",
            manager_label
        ),
    );

    add_score(
        &mut scores,
        &mut factors,
        (path_contains_asdf_shims(canonical.as_str()) && canonical_matches_exec)
            || (path_contains_asdf_shims(display.as_str()) && display_matches_exec)
            || (path_contains_asdf_installs(canonical.as_str()) && canonical_matches_bin_exec)
            || (path_contains_asdf_installs(display.as_str()) && display_matches_bin_exec),
        InstallProvenance::Asdf,
        0.92,
        format!(
            "{} path indicates asdf-managed shim/install layout",
            manager_label
        ),
    );

    add_score(
        &mut scores,
        &mut factors,
        ((canonical.contains("/.local/share/mise/shims/")
            || canonical.contains("/.local/share/rtx/shims/"))
            && canonical_matches_exec)
            || ((display.contains("/.local/share/mise/shims/")
                || display.contains("/.local/share/rtx/shims/"))
                && display_matches_exec)
            || ((canonical.contains("/.local/share/mise/installs/")
                || canonical.contains("/.local/share/rtx/installs/"))
                && canonical_matches_bin_exec)
            || ((display.contains("/.local/share/mise/installs/")
                || display.contains("/.local/share/rtx/installs/"))
                && display_matches_bin_exec),
        InstallProvenance::Mise,
        0.92,
        format!(
            "{} path indicates mise/rtx-managed shim/install layout",
            manager_label
        ),
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical.contains("/nix/store/")
            && (canonical_matches_exec || canonical_matches_bin_exec))
            || (display.contains("/nix/store/")
                && (display_matches_exec || display_matches_bin_exec)),
        InstallProvenance::Nix,
        0.92,
        format!("{} path is inside nix store", manager_label),
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical.starts_with("/opt/local/bin/") && canonical_matches_exec)
            || (display.starts_with("/opt/local/bin/") && display_matches_exec),
        InstallProvenance::Macports,
        0.92,
        format!("{} executable is in MacPorts prefix", manager_label),
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical.starts_with("/usr/bin/") && canonical_matches_exec)
            || (display.starts_with("/usr/bin/") && display_matches_exec),
        InstallProvenance::System,
        0.95,
        format!("{} executable is in a system path", manager_label),
    );

    add_score(
        &mut scores,
        &mut factors,
        ((canonical.starts_with("/usr/local/bin/") && canonical_matches_exec)
            || (display.starts_with("/usr/local/bin/") && display_matches_exec))
            && !canonical.contains("/cellar/")
            && !display.contains("/cellar/")
            && !canonical.contains("/opt/")
            && !display.contains("/opt/"),
        InstallProvenance::SourceBuild,
        0.40,
        format!(
            "{} path is under /usr/local/bin without package-owner evidence",
            manager_label
        ),
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical.starts_with("/opt/") || display.starts_with("/opt/"))
            && (canonical_matches_exec
                || display_matches_exec
                || canonical_matches_bin_exec
                || display_matches_bin_exec)
            && !canonical.contains("/homebrew/")
            && !display.contains("/homebrew/")
            && !canonical.contains("/nix/store/")
            && !display.contains("/nix/store/"),
        InstallProvenance::EnterpriseManaged,
        0.35,
        format!(
            "{} path is in a non-default /opt prefix, possibly enterprise-managed",
            manager_label
        ),
    );

    finalize_scored_instance_provenance(instance, &scores, &factors, manager_label);
}

fn classify_homebrew_formula_manager_instance(
    instance: &mut ManagerInstallInstance,
    formula_name: &str,
) {
    let mut scores: HashMap<InstallProvenance, f64> = HashMap::new();
    let mut factors: Vec<ScoreFactor> = Vec::new();

    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string()
        .to_lowercase();
    let display = instance
        .display_path
        .to_string_lossy()
        .to_string()
        .to_lowercase();

    let cellar_fragment = format!("/cellar/{formula_name}/");
    let opt_bin_path = format!("/opt/homebrew/opt/{formula_name}/bin/{formula_name}");
    let usr_local_opt_bin_path = format!("/usr/local/opt/{formula_name}/bin/{formula_name}");
    let opt_homebrew_bin_path = format!("/opt/homebrew/bin/{formula_name}");
    let usr_local_bin_path = format!("/usr/local/bin/{formula_name}");
    let system_bin_path = format!("/usr/bin/{formula_name}");

    add_score(
        &mut scores,
        &mut factors,
        canonical.contains(cellar_fragment.as_str()),
        InstallProvenance::Homebrew,
        0.95,
        format!(
            "canonical path is inside Homebrew Cellar for {}",
            formula_name
        ),
    );

    add_score(
        &mut scores,
        &mut factors,
        canonical.starts_with(opt_homebrew_bin_path.as_str())
            || canonical.starts_with(usr_local_bin_path.as_str())
            || canonical.starts_with(opt_bin_path.as_str())
            || canonical.starts_with(usr_local_opt_bin_path.as_str())
            || display.starts_with(opt_homebrew_bin_path.as_str())
            || display.starts_with(usr_local_bin_path.as_str())
            || display.starts_with(opt_bin_path.as_str())
            || display.starts_with(usr_local_opt_bin_path.as_str()),
        InstallProvenance::Homebrew,
        0.75,
        format!(
            "{} path is inside a known Homebrew binary prefix",
            formula_name
        ),
    );

    add_score(
        &mut scores,
        &mut factors,
        canonical.starts_with(system_bin_path.as_str())
            || display.starts_with(system_bin_path.as_str()),
        InstallProvenance::System,
        0.95,
        format!("{} executable is in a system path", formula_name),
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical.starts_with("/usr/local/") || display.starts_with("/usr/local/"))
            && !canonical.contains("/cellar/")
            && !canonical.contains("/homebrew/")
            && !canonical.contains("/opt/"),
        InstallProvenance::SourceBuild,
        0.40,
        format!(
            "{} path is user-managed under /usr/local without package-owner fingerprints",
            formula_name
        ),
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical.starts_with("/opt/") || display.starts_with("/opt/"))
            && !canonical.contains("/homebrew/")
            && !display.contains("/homebrew/"),
        InstallProvenance::EnterpriseManaged,
        0.35,
        format!(
            "{} path is in a non-default /opt prefix, possibly enterprise-managed",
            formula_name
        ),
    );

    finalize_scored_instance_provenance(instance, &scores, &factors, formula_name);
}

fn classify_mise_instance(
    instance: &mut ManagerInstallInstance,
    context: &mut ExternalEvidenceContext,
) {
    let mut scores: HashMap<InstallProvenance, f64> = HashMap::new();
    let mut factors: Vec<ScoreFactor> = Vec::new();

    let display = instance.display_path.to_string_lossy().to_string();
    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string();
    let display_lower = display.to_lowercase();
    let canonical_lower = canonical.to_lowercase();
    let canonical_matches_exec = path_matches_exec_name(canonical_lower.as_str(), "mise");
    let display_matches_exec = path_matches_exec_name(display_lower.as_str(), "mise");
    let canonical_matches_bin_exec = path_matches_bin_exec_name(canonical_lower.as_str(), "mise");
    let display_matches_bin_exec = path_matches_bin_exec_name(display_lower.as_str(), "mise");

    add_score(
        &mut scores,
        &mut factors,
        canonical_lower.contains("/cellar/mise/"),
        InstallProvenance::Homebrew,
        0.95,
        "canonical path is inside Homebrew Cellar for mise",
    );

    add_score(
        &mut scores,
        &mut factors,
        canonical_lower.starts_with("/opt/homebrew/bin/mise")
            || canonical_lower.starts_with("/usr/local/bin/mise")
            || canonical_lower.starts_with("/opt/homebrew/opt/mise/bin/mise")
            || canonical_lower.starts_with("/usr/local/opt/mise/bin/mise")
            || display_lower.starts_with("/opt/homebrew/bin/mise")
            || display_lower.starts_with("/usr/local/bin/mise")
            || display_lower.starts_with("/opt/homebrew/opt/mise/bin/mise")
            || display_lower.starts_with("/usr/local/opt/mise/bin/mise"),
        InstallProvenance::Homebrew,
        0.65,
        "mise path is inside a Homebrew binary prefix",
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.contains("/.local/bin/mise") && canonical_matches_exec)
            || (display_lower.contains("/.local/bin/mise") && display_matches_exec),
        InstallProvenance::SourceBuild,
        0.90,
        "mise path matches the default upstream script-installer location (~/.local/bin/mise)",
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.contains("/.cargo/bin/mise") && canonical_matches_exec)
            || (display_lower.contains("/.cargo/bin/mise") && display_matches_exec),
        InstallProvenance::SourceBuild,
        0.82,
        "mise executable is in a cargo-home bin layout",
    );

    if let Some(cargo_home_mise_hint) = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .map(|path| path.join("bin").join("mise"))
        .map(|path| path.to_string_lossy().to_lowercase())
    {
        add_score(
            &mut scores,
            &mut factors,
            canonical_lower == cargo_home_mise_hint
                || display_lower == cargo_home_mise_hint
                || canonical_lower.ends_with(cargo_home_mise_hint.as_str())
                || display_lower.ends_with(cargo_home_mise_hint.as_str()),
            InstallProvenance::SourceBuild,
            0.25,
            "mise executable path matches CARGO_HOME/bin/mise",
        );
    }

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.contains("/node_modules/@jdxcode/mise/")
            && (canonical_matches_exec || canonical_matches_bin_exec))
            || (display_lower.contains("/node_modules/@jdxcode/mise/")
                && (display_matches_exec || display_matches_bin_exec)),
        InstallProvenance::SourceBuild,
        0.80,
        "mise path indicates npm global package layout (@jdxcode/mise)",
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.starts_with("/opt/local/bin/") && canonical_matches_exec)
            || (display_lower.starts_with("/opt/local/bin/") && display_matches_exec),
        InstallProvenance::Macports,
        0.92,
        "mise executable is in MacPorts prefix",
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.contains("/nix/store/")
            || canonical_lower.contains("/.nix-profile/")
            || display_lower.contains("/nix/store/")
            || display_lower.contains("/.nix-profile/"))
            && (canonical_matches_exec
                || display_matches_exec
                || canonical_matches_bin_exec
                || display_matches_bin_exec),
        InstallProvenance::Nix,
        0.85,
        "mise executable path indicates a Nix-managed install",
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.starts_with("/usr/bin/") && canonical_matches_exec)
            || (display_lower.starts_with("/usr/bin/") && display_matches_exec),
        InstallProvenance::System,
        0.95,
        "mise executable is in a system path",
    );

    add_score(
        &mut scores,
        &mut factors,
        ((canonical_lower.starts_with("/usr/local/bin/") && canonical_matches_exec)
            || (display_lower.starts_with("/usr/local/bin/") && display_matches_exec))
            && !canonical_lower.contains("/cellar/")
            && !display_lower.contains("/cellar/")
            && !canonical_lower.contains("/opt/")
            && !display_lower.contains("/opt/"),
        InstallProvenance::SourceBuild,
        0.40,
        "mise path is under /usr/local/bin without package-owner fingerprints",
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.starts_with("/opt/") || display_lower.starts_with("/opt/"))
            && (canonical_matches_exec
                || display_matches_exec
                || canonical_matches_bin_exec
                || display_matches_bin_exec)
            && !canonical_lower.contains("/homebrew/")
            && !display_lower.contains("/homebrew/")
            && !canonical_lower.contains("/nix/store/")
            && !display_lower.contains("/nix/store/"),
        InstallProvenance::EnterpriseManaged,
        0.35,
        "mise path is in a non-default /opt prefix, possibly enterprise-managed",
    );

    let homebrew_ambiguous = provenance_score(&scores, InstallProvenance::Homebrew)
        .filter(|score| *score < PROVENANCE_CONFIDENCE_THRESHOLD)
        .is_some();
    let close_race = score_gap(
        &scores,
        InstallProvenance::Homebrew,
        InstallProvenance::SourceBuild,
    )
    .is_some_and(|gap| gap < 0.25);

    if (homebrew_ambiguous || close_race)
        && let Some(prefix) = context.brew_prefix("mise")
    {
        let prefix_lower = prefix.to_lowercase();
        add_score(
            &mut scores,
            &mut factors,
            canonical_lower.starts_with(prefix_lower.as_str())
                || display_lower.starts_with(prefix_lower.as_str()),
            InstallProvenance::Homebrew,
            0.30,
            "brew ownership query matched mise prefix",
        );
    }

    let pkgutil_probe_candidate = canonical_lower.starts_with("/usr/bin/")
        || canonical_lower.starts_with("/usr/local/")
        || canonical_lower.starts_with("/opt/");
    let pkgutil_ambiguous = rank_scores(&scores)
        .first()
        .map(|(_, score)| *score < PROVENANCE_CONFIDENCE_THRESHOLD)
        .unwrap_or(true);

    if pkgutil_probe_candidate
        && pkgutil_ambiguous
        && let Some(path_owner) = context.pkgutil_file_owner(
            instance
                .canonical_path
                .as_deref()
                .unwrap_or(&instance.display_path),
        )
    {
        match path_owner {
            PkgutilFileOwner::Owned(pkgid) => {
                let pkgid_lower = pkgid.to_lowercase();
                if pkgid_lower.starts_with("com.apple.") {
                    add_score(
                        &mut scores,
                        &mut factors,
                        true,
                        InstallProvenance::System,
                        0.90,
                        "pkgutil ownership receipt indicates system-managed mise",
                    );
                } else if pkgid_lower.contains("homebrew") {
                    add_score(
                        &mut scores,
                        &mut factors,
                        true,
                        InstallProvenance::Homebrew,
                        0.35,
                        "pkgutil ownership receipt indicates Homebrew-managed mise",
                    );
                } else if pkgid_lower.contains("macports") {
                    add_score(
                        &mut scores,
                        &mut factors,
                        true,
                        InstallProvenance::Macports,
                        0.70,
                        "pkgutil ownership receipt indicates MacPorts-managed mise",
                    );
                } else {
                    add_score(
                        &mut scores,
                        &mut factors,
                        true,
                        InstallProvenance::EnterpriseManaged,
                        0.70,
                        format!(
                            "pkgutil ownership receipt ({}) indicates managed package install",
                            pkgid
                        ),
                    );
                }
            }
            PkgutilFileOwner::NotOwned => {
                add_score(
                    &mut scores,
                    &mut factors,
                    canonical_lower.starts_with("/usr/local/")
                        || canonical_lower.starts_with("/opt/"),
                    InstallProvenance::SourceBuild,
                    0.20,
                    "pkgutil reports no owning receipt for mise path",
                );
            }
        }
    }

    finalize_scored_instance_provenance(instance, &scores, &factors, "mise");
}

fn classify_rustup_instance(
    instance: &mut ManagerInstallInstance,
    context: &mut ExternalEvidenceContext,
) {
    let mut scores: HashMap<InstallProvenance, f64> = HashMap::new();
    let mut factors: Vec<ScoreFactor> = Vec::new();

    let display = instance.display_path.to_string_lossy().to_string();
    let canonical = instance
        .canonical_path
        .as_ref()
        .unwrap_or(&instance.display_path)
        .to_string_lossy()
        .to_string();
    let display_lower = display.to_lowercase();
    let canonical_lower = canonical.to_lowercase();

    add_score(
        &mut scores,
        &mut factors,
        matches!(
            canonical_lower.as_str(),
            path if path.contains("/cellar/rustup/")
        ),
        InstallProvenance::Homebrew,
        0.95,
        "canonical path is inside Homebrew Cellar for rustup",
    );

    add_score(
        &mut scores,
        &mut factors,
        canonical_lower.starts_with("/opt/homebrew/bin/rustup")
            || canonical_lower.starts_with("/usr/local/bin/rustup")
            || canonical_lower.starts_with("/opt/homebrew/opt/rustup/bin/rustup")
            || canonical_lower.starts_with("/usr/local/opt/rustup/bin/rustup")
            || display_lower.starts_with("/opt/homebrew/bin/rustup")
            || display_lower.starts_with("/usr/local/bin/rustup")
            || display_lower.starts_with("/opt/homebrew/opt/rustup/bin/rustup")
            || display_lower.starts_with("/usr/local/opt/rustup/bin/rustup"),
        InstallProvenance::Homebrew,
        0.65,
        "rustup path is inside a Homebrew binary prefix",
    );

    add_score(
        &mut scores,
        &mut factors,
        canonical_lower.contains("/.cargo/bin/rustup")
            || display_lower.contains("/.cargo/bin/rustup"),
        InstallProvenance::RustupInit,
        0.90,
        "rustup executable is in the default cargo bin location",
    );

    add_score(
        &mut scores,
        &mut factors,
        is_custom_cargo_home_rustup_path(&canonical_lower, &display_lower),
        InstallProvenance::RustupInit,
        0.70,
        "rustup executable path matches a custom cargo-home style layout",
    );

    if let Some(cargo_home_hint) = cargo_home_rustup_hint() {
        add_score(
            &mut scores,
            &mut factors,
            canonical_lower == cargo_home_hint || display_lower == cargo_home_hint,
            InstallProvenance::RustupInit,
            0.25,
            "rustup executable path matches CARGO_HOME/bin/rustup",
        );
    }

    add_score(
        &mut scores,
        &mut factors,
        path_contains_asdf_root_subpath(canonical_lower.as_str(), "shims/rustup")
            || path_contains_asdf_root_subpath(display_lower.as_str(), "shims/rustup")
            || (path_contains_asdf_installs(canonical_lower.as_str())
                && canonical_lower.ends_with("/bin/rustup")),
        InstallProvenance::Asdf,
        0.92,
        "rustup path indicates asdf-managed shim/install layout",
    );

    add_score(
        &mut scores,
        &mut factors,
        canonical_lower.contains("/.local/share/mise/shims/rustup")
            || canonical_lower.contains("/.local/share/rtx/shims/rustup")
            || display_lower.contains("/.local/share/mise/shims/rustup")
            || display_lower.contains("/.local/share/rtx/shims/rustup")
            || (canonical_lower.contains("/.local/share/mise/installs/")
                && canonical_lower.ends_with("/bin/rustup"))
            || (canonical_lower.contains("/.local/share/rtx/installs/")
                && canonical_lower.ends_with("/bin/rustup")),
        InstallProvenance::Mise,
        0.92,
        "rustup path indicates mise/rtx-managed shim/install layout",
    );

    add_score(
        &mut scores,
        &mut factors,
        canonical_lower.contains("/nix/store/")
            || canonical_lower.contains("/.nix-profile/")
            || display_lower.contains("/nix/store/")
            || display_lower.contains("/.nix-profile/"),
        InstallProvenance::Nix,
        0.85,
        "rustup executable path indicates a Nix-managed install",
    );

    add_score(
        &mut scores,
        &mut factors,
        canonical_lower.starts_with("/usr/bin/rustup")
            || display_lower.starts_with("/usr/bin/rustup"),
        InstallProvenance::System,
        0.95,
        "rustup executable is in a system path",
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.starts_with("/usr/local/") || display_lower.starts_with("/usr/local/"))
            && !canonical_lower.contains("/cellar/")
            && !display_lower.contains("/cellar/"),
        InstallProvenance::SourceBuild,
        0.45,
        "rustup path is user-managed under /usr/local without package-owner fingerprints",
    );

    add_score(
        &mut scores,
        &mut factors,
        (canonical_lower.starts_with("/opt/") || display_lower.starts_with("/opt/"))
            && !canonical_lower.contains("/homebrew/")
            && !display_lower.contains("/homebrew/"),
        InstallProvenance::EnterpriseManaged,
        0.35,
        "rustup path is in a non-default /opt prefix, possibly enterprise-managed",
    );

    let homebrew_ambiguous = provenance_score(&scores, InstallProvenance::Homebrew)
        .filter(|score| *score < PROVENANCE_CONFIDENCE_THRESHOLD)
        .is_some();
    let close_race = score_gap(
        &scores,
        InstallProvenance::Homebrew,
        InstallProvenance::RustupInit,
    )
    .is_some_and(|gap| gap < 0.25);

    if (homebrew_ambiguous || close_race)
        && let Some(prefix) = context.brew_prefix("rustup")
    {
        let prefix_lower = prefix.to_lowercase();
        add_score(
            &mut scores,
            &mut factors,
            canonical_lower.starts_with(prefix_lower.as_str())
                || display_lower.starts_with(prefix_lower.as_str()),
            InstallProvenance::Homebrew,
            0.30,
            "brew ownership query matched rustup prefix",
        );
    }

    let pkgutil_probe_candidate = canonical_lower.starts_with("/usr/bin/")
        || canonical_lower.starts_with("/usr/local/")
        || canonical_lower.starts_with("/opt/");
    let pkgutil_ambiguous = rank_scores(&scores)
        .first()
        .map(|(_, score)| *score < PROVENANCE_CONFIDENCE_THRESHOLD)
        .unwrap_or(true);

    if pkgutil_probe_candidate
        && pkgutil_ambiguous
        && let Some(path_owner) = context.pkgutil_file_owner(
            instance
                .canonical_path
                .as_deref()
                .unwrap_or(&instance.display_path),
        )
    {
        match path_owner {
            PkgutilFileOwner::Owned(pkgid) => {
                let pkgid_lower = pkgid.to_lowercase();
                if pkgid_lower.starts_with("com.apple.") {
                    add_score(
                        &mut scores,
                        &mut factors,
                        true,
                        InstallProvenance::System,
                        0.90,
                        "pkgutil ownership receipt indicates system-managed rustup",
                    );
                } else if pkgid_lower.contains("homebrew") {
                    add_score(
                        &mut scores,
                        &mut factors,
                        true,
                        InstallProvenance::Homebrew,
                        0.35,
                        "pkgutil ownership receipt indicates Homebrew-managed rustup",
                    );
                } else {
                    add_score(
                        &mut scores,
                        &mut factors,
                        true,
                        InstallProvenance::EnterpriseManaged,
                        0.70,
                        format!(
                            "pkgutil ownership receipt ({}) indicates managed package install",
                            pkgid
                        ),
                    );
                }
            }
            PkgutilFileOwner::NotOwned => {
                add_score(
                    &mut scores,
                    &mut factors,
                    canonical_lower.starts_with("/usr/local/")
                        || canonical_lower.starts_with("/opt/"),
                    InstallProvenance::SourceBuild,
                    0.20,
                    "pkgutil reports no owning receipt for rustup path",
                );
            }
        }
    }

    finalize_scored_instance_provenance(instance, &scores, &factors, "rustup");
}

fn finalize_scored_instance_provenance(
    instance: &mut ManagerInstallInstance,
    scores: &HashMap<InstallProvenance, f64>,
    factors: &[ScoreFactor],
    subject_label: &str,
) {
    let ranked = rank_scores(scores);
    let best = ranked.first().copied();
    let second = ranked.get(1).copied();
    let decision_margin = match (best, second) {
        (Some((_, best_score)), Some((_, second_score))) => {
            Some((best_score - second_score).abs().clamp(0.0, 1.0))
        }
        _ => None,
    };

    let (selected_provenance, confidence) = match (best, second) {
        (Some((best_provenance, best_score)), Some((_, second_score))) => {
            let margin = best_score - second_score;
            if best_score >= PROVENANCE_CONFIDENCE_THRESHOLD
                && margin >= PROVENANCE_MARGIN_THRESHOLD
            {
                (best_provenance, best_score)
            } else {
                (InstallProvenance::Unknown, best_score)
            }
        }
        (Some((best_provenance, best_score)), None)
            if best_score >= PROVENANCE_CONFIDENCE_THRESHOLD =>
        {
            (best_provenance, best_score)
        }
        (Some((_, best_score)), None) => (InstallProvenance::Unknown, best_score),
        _ => (InstallProvenance::Unknown, 0.0),
    };

    let clamped_confidence = confidence.clamp(0.0, 1.0);
    instance.provenance = selected_provenance;
    instance.confidence = clamped_confidence;
    instance.decision_margin = decision_margin;
    instance.automation_level = automation_level_for(selected_provenance, clamped_confidence);
    instance.uninstall_strategy = uninstall_strategy_for(instance.manager, selected_provenance);
    instance.update_strategy = update_strategy_for(selected_provenance);
    instance.remediation_strategy = remediation_strategy_for(selected_provenance);

    if let Some((provenance, score)) = second {
        instance.competing_provenance = Some(provenance);
        instance.competing_confidence = Some(score.clamp(0.0, 1.0));
    }

    let selected_factors = factors_for_provenance(factors, selected_provenance);
    if let Some(primary) = selected_factors.first() {
        instance.explanation_primary = Some(primary.reason.clone());
    }
    if let Some(secondary) = selected_factors.get(1) {
        instance.explanation_secondary = Some(secondary.reason.clone());
    }

    if instance.explanation_primary.is_none() {
        if selected_provenance == InstallProvenance::Unknown {
            instance.explanation_primary = Some(format!(
                "insufficient or conflicting {} provenance evidence; defaulting to unknown",
                subject_label
            ));
        } else {
            instance.explanation_primary =
                Some("provenance selected from weak evidence set".to_string());
        }
    }
}

fn add_score(
    scores: &mut HashMap<InstallProvenance, f64>,
    factors: &mut Vec<ScoreFactor>,
    condition: bool,
    provenance: InstallProvenance,
    weight: f64,
    reason: impl Into<String>,
) {
    if !condition {
        return;
    }
    let entry = scores.entry(provenance).or_insert(0.0);
    *entry = (*entry + weight).clamp(0.0, 1.0);
    factors.push(ScoreFactor {
        provenance,
        weight,
        reason: reason.into(),
    });
}

fn rank_scores(scores: &HashMap<InstallProvenance, f64>) -> Vec<(InstallProvenance, f64)> {
    let mut ranked = scores
        .iter()
        .map(|(provenance, score)| (*provenance, *score))
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.as_str().cmp(right.0.as_str()))
    });
    ranked
}

fn provenance_score(
    scores: &HashMap<InstallProvenance, f64>,
    provenance: InstallProvenance,
) -> Option<f64> {
    scores.get(&provenance).copied()
}

fn score_gap(
    scores: &HashMap<InstallProvenance, f64>,
    left: InstallProvenance,
    right: InstallProvenance,
) -> Option<f64> {
    let left_score = scores.get(&left)?;
    let right_score = scores.get(&right)?;
    Some((left_score - right_score).abs())
}

fn factors_for_provenance(
    factors: &[ScoreFactor],
    provenance: InstallProvenance,
) -> Vec<ScoreFactor> {
    let mut selected = factors
        .iter()
        .filter(|factor| factor.provenance == provenance)
        .cloned()
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        right
            .weight
            .partial_cmp(&left.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selected
}

fn is_custom_cargo_home_rustup_path(canonical_lower: &str, display_lower: &str) -> bool {
    let looks_like_rustup_bin =
        canonical_lower.ends_with("/bin/rustup") || display_lower.ends_with("/bin/rustup");
    let cargo_home_style_path = canonical_lower.contains("/cargo/")
        || canonical_lower.contains("cargo_home")
        || display_lower.contains("/cargo/")
        || display_lower.contains("cargo_home");
    let known_non_rustup_init_layout = canonical_lower.contains("/cellar/")
        || canonical_lower.contains("/homebrew/")
        || canonical_lower.contains("/nix/store/")
        || path_contains_asdf_root_subpath(canonical_lower, "")
        || canonical_lower.contains("/.local/share/mise/")
        || canonical_lower.contains("/.local/share/rtx/");
    looks_like_rustup_bin && cargo_home_style_path && !known_non_rustup_init_layout
}

fn cargo_home_rustup_hint() -> Option<String> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .map(|path| path.join("bin").join("rustup"))
        .map(|path| path.to_string_lossy().to_lowercase())
}

fn parse_pkgutil_file_owner(output: &str) -> Option<PkgutilFileOwner> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }

    for line in trimmed.lines() {
        let line = line.trim();
        if let Some(pkgid) = line.strip_prefix("pkgid:") {
            let pkgid = pkgid.trim();
            if !pkgid.is_empty() {
                return Some(PkgutilFileOwner::Owned(pkgid.to_string()));
            }
        }
    }

    Some(PkgutilFileOwner::NotOwned)
}

fn uninstall_strategy_for(manager: ManagerId, provenance: InstallProvenance) -> StrategyKind {
    match provenance {
        InstallProvenance::Homebrew => StrategyKind::HomebrewFormula,
        InstallProvenance::RustupInit => StrategyKind::RustupSelf,
        InstallProvenance::Macports => {
            if manager == ManagerId::MacPorts {
                StrategyKind::MacportsSelf
            } else {
                StrategyKind::InteractivePrompt
            }
        }
        InstallProvenance::System
        | InstallProvenance::EnterpriseManaged
        | InstallProvenance::Nix => StrategyKind::ReadOnly,
        InstallProvenance::Unknown
        | InstallProvenance::SourceBuild
        | InstallProvenance::Asdf
        | InstallProvenance::Mise => StrategyKind::InteractivePrompt,
    }
}

fn update_strategy_for(provenance: InstallProvenance) -> StrategyKind {
    match provenance {
        InstallProvenance::Homebrew => StrategyKind::HomebrewFormula,
        InstallProvenance::RustupInit => StrategyKind::RustupSelf,
        InstallProvenance::System
        | InstallProvenance::EnterpriseManaged
        | InstallProvenance::Nix => StrategyKind::ReadOnly,
        InstallProvenance::Unknown
        | InstallProvenance::SourceBuild
        | InstallProvenance::Asdf
        | InstallProvenance::Mise
        | InstallProvenance::Macports => StrategyKind::InteractivePrompt,
    }
}

fn remediation_strategy_for(provenance: InstallProvenance) -> StrategyKind {
    match provenance {
        InstallProvenance::Homebrew => StrategyKind::HomebrewFormula,
        InstallProvenance::RustupInit => StrategyKind::RustupSelf,
        InstallProvenance::System
        | InstallProvenance::EnterpriseManaged
        | InstallProvenance::Nix => StrategyKind::ReadOnly,
        InstallProvenance::Unknown
        | InstallProvenance::SourceBuild
        | InstallProvenance::Asdf
        | InstallProvenance::Mise
        | InstallProvenance::Macports => StrategyKind::InteractivePrompt,
    }
}

fn run_command_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let start = Instant::now();
    debug!(
        probe_program = program,
        probe_args = ?args,
        timeout_ms = timeout.as_millis(),
        "starting bounded external provenance probe"
    );

    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    warn!(
                        probe_program = program,
                        probe_args = ?args,
                        elapsed_ms = start.elapsed().as_millis(),
                        exit_success = false,
                        "external provenance probe exited non-zero"
                    );
                    return None;
                }
                break;
            }
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    warn!(
                        probe_program = program,
                        probe_args = ?args,
                        elapsed_ms = start.elapsed().as_millis(),
                        "external provenance probe timed out and was terminated"
                    );
                    return None;
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                warn!(
                    probe_program = program,
                    probe_args = ?args,
                    elapsed_ms = start.elapsed().as_millis(),
                    "external provenance probe failed during wait; returning no evidence"
                );
                return None;
            }
        }
    }

    let mut output = String::new();
    let mut stdout = child.stdout.take()?;
    stdout.read_to_string(&mut output).ok()?;
    debug!(
        probe_program = program,
        probe_args = ?args,
        elapsed_ms = start.elapsed().as_millis(),
        output_bytes = output.len(),
        "external provenance probe completed successfully"
    );
    Some(output)
}

fn candidate_represents_executable(candidate: &Path, canonical: Option<&Path>) -> bool {
    candidate.is_file() || canonical.is_some_and(Path::is_file)
}

fn is_active_candidate(
    candidate: &Path,
    candidate_canonical: Option<&Path>,
    active_path: Option<&Path>,
    active_canonical: Option<&Path>,
) -> bool {
    if let Some(active_path) = active_path
        && candidate == active_path
    {
        return true;
    }

    if let (Some(candidate_canonical), Some(active_canonical)) =
        (candidate_canonical, active_canonical)
        && candidate_canonical == active_canonical
    {
        return true;
    }

    false
}

fn compute_identity(
    display_path: &Path,
    canonical_path: Option<&Path>,
) -> (InstallInstanceIdentityKind, String) {
    let metadata = fs::metadata(display_path)
        .ok()
        .or_else(|| canonical_path.and_then(|path| fs::metadata(path).ok()));

    // Unix platforms can provide a stable device+inode identity even when alias paths change.
    #[cfg(unix)]
    if let Some(metadata) = metadata.as_ref() {
        let value = format!("{}:{}", metadata.dev(), metadata.ino());
        return (InstallInstanceIdentityKind::DevInode, value);
    }

    // When inode metadata is unavailable (or unsupported), canonical path is the next-stable
    // identity. This keeps continuity across runs unless canonical ownership really changes.
    if let Some(canonical_path) = canonical_path {
        return (
            InstallInstanceIdentityKind::CanonicalPath,
            canonical_path.to_string_lossy().to_string(),
        );
    }

    // Last-resort identity uses rendered path + selected metadata. This can intentionally reset
    // when fallback metadata changes; treat it as conservative continuity for ambiguous cases.
    let size = metadata.as_ref().map(|value| value.len()).unwrap_or(0);
    let mtime = metadata
        .and_then(|value| value.modified().ok())
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .unwrap_or(0);
    let fallback = format!("{}:{}:{}", display_path.to_string_lossy(), size, mtime);
    (
        InstallInstanceIdentityKind::FallbackHash,
        format!("{:016x}", stable_hash64(&fallback)),
    )
}

fn stable_instance_id(
    manager: ManagerId,
    identity_kind: InstallInstanceIdentityKind,
    identity_value: &str,
) -> String {
    let stable = format!(
        "{}:{}:{}",
        manager.as_str(),
        identity_kind.as_str(),
        identity_value
    );
    format!(
        "{}-{:016x}",
        manager.as_str(),
        stable_hash64(stable.as_str())
    )
}

fn stable_hash64(input: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn manager_executable_candidates(id: ManagerId) -> &'static [&'static str] {
    match id {
        ManagerId::HomebrewFormula | ManagerId::HomebrewCask => {
            &["brew", "/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        }
        ManagerId::Asdf => &["asdf"],
        ManagerId::Mise => &["mise"],
        ManagerId::Rustup => &[
            "rustup",
            "/opt/homebrew/opt/rustup/bin/rustup",
            "/usr/local/opt/rustup/bin/rustup",
        ],
        ManagerId::Npm => &["npm"],
        ManagerId::Pnpm => &["pnpm"],
        ManagerId::Yarn => &["yarn"],
        ManagerId::Pip => &["python3", "pip3", "pip"],
        ManagerId::Pipx => &["pipx"],
        ManagerId::Poetry => &["poetry"],
        ManagerId::RubyGems => &["gem"],
        ManagerId::Bundler => &["bundle"],
        ManagerId::Cargo => &["cargo"],
        ManagerId::CargoBinstall => &["cargo-binstall"],
        ManagerId::MacPorts => &["port", "/opt/local/bin/port"],
        ManagerId::NixDarwin => &["darwin-rebuild", "nix"],
        ManagerId::Mas => &["mas"],
        ManagerId::DockerDesktop => &["docker"],
        ManagerId::Podman => &["podman"],
        ManagerId::Colima => &["colima"],
        ManagerId::XcodeCommandLineTools => &["/Library/Developer/CommandLineTools/usr/bin/clang"],
        ManagerId::SoftwareUpdate => &["/usr/sbin/softwareupdate"],
        _ => &[],
    }
}

fn manager_additional_bin_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
        PathBuf::from("/usr/sbin"),
        PathBuf::from("/sbin"),
    ];

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        roots.push(home.join(".local/bin"));
        roots.push(home.join(".cargo/bin"));
        roots.push(home.join(".asdf/bin"));
        roots.push(home.join(".asdf/shims"));
        roots.push(home.join(".local/share/rtx/shims"));
        roots.push(home.join(".nix-profile/bin"));
    }
    for root in configured_asdf_root_paths() {
        roots.push(root.join("bin"));
        roots.push(root.join("shims"));
    }

    roots
}

fn manager_versioned_install_roots(id: ManagerId) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if matches!(
        id,
        ManagerId::HomebrewFormula
            | ManagerId::HomebrewCask
            | ManagerId::Mise
            | ManagerId::Asdf
            | ManagerId::Rustup
            | ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
            | ManagerId::Mas
            | ManagerId::DockerDesktop
            | ManagerId::Podman
            | ManagerId::Colima
    ) {
        roots.push(PathBuf::from("/opt/homebrew/Cellar"));
        roots.push(PathBuf::from("/usr/local/Cellar"));
    }

    if matches!(
        id,
        ManagerId::Npm
            | ManagerId::Pnpm
            | ManagerId::Yarn
            | ManagerId::Pip
            | ManagerId::Pipx
            | ManagerId::Poetry
            | ManagerId::RubyGems
            | ManagerId::Bundler
            | ManagerId::Cargo
            | ManagerId::CargoBinstall
    ) && let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
    {
        roots.push(home.join(".asdf/installs"));
        roots.push(home.join(".local/share/mise/installs"));
        roots.push(home.join(".local/share/rtx/installs"));
    }
    for root in configured_asdf_root_paths() {
        roots.push(root.join("installs"));
    }

    roots
}

fn discover_executable_paths(id: ManagerId, candidates: &[&str]) -> Vec<String> {
    let mut discovered = Vec::new();
    let mut seen = HashSet::new();

    let path_dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .as_deref()
        .map(std::env::split_paths)
        .map(|iter| iter.collect::<Vec<_>>())
        .unwrap_or_default();

    for candidate in candidates {
        if candidate.contains('/') {
            push_discovered_path(Path::new(candidate), &mut discovered, &mut seen);
            continue;
        }

        for path_dir in &path_dirs {
            push_discovered_path(&path_dir.join(candidate), &mut discovered, &mut seen);
        }

        for root in manager_additional_bin_roots() {
            push_discovered_path(&root.join(candidate), &mut discovered, &mut seen);
        }

        for root in manager_versioned_install_roots(id) {
            let Ok(tool_dirs) = fs::read_dir(root) else {
                continue;
            };
            for tool_dir in tool_dirs.flatten() {
                let tool_path = tool_dir.path();
                if !tool_path.is_dir() {
                    continue;
                }

                let Ok(version_dirs) = fs::read_dir(&tool_path) else {
                    continue;
                };
                for version_dir in version_dirs.flatten() {
                    let version_path = version_dir.path();
                    if !version_path.is_dir() {
                        continue;
                    }
                    push_discovered_path(
                        &version_path.join("bin").join(candidate),
                        &mut discovered,
                        &mut seen,
                    );
                }
            }
        }
    }

    discovered
}

fn push_discovered_path(
    candidate: &Path,
    discovered: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    if !candidate.is_file() {
        return;
    }
    let rendered = candidate.to_string_lossy().to_string();
    if rendered.trim().is_empty() {
        return;
    }
    if seen.insert(rendered.clone()) {
        discovered.push(rendered);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rustup_detection(path: PathBuf) -> DetectionInfo {
        DetectionInfo {
            installed: true,
            executable_path: Some(path),
            version: Some("1.28.2".to_string()),
        }
    }

    fn rustup_instance(display_path: &str, canonical_path: &str) -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "id".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: canonical_path.to_string(),
            display_path: PathBuf::from(display_path),
            canonical_path: Some(PathBuf::from(canonical_path)),
            alias_paths: vec![PathBuf::from(display_path)],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.0,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::Unknown,
            update_strategy: StrategyKind::Unknown,
            remediation_strategy: StrategyKind::Unknown,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        }
    }

    fn manager_instance(
        manager: ManagerId,
        display_path: &str,
        canonical_path: &str,
    ) -> ManagerInstallInstance {
        ManagerInstallInstance {
            manager,
            instance_id: "id".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: canonical_path.to_string(),
            display_path: PathBuf::from(display_path),
            canonical_path: Some(PathBuf::from(canonical_path)),
            alias_paths: vec![PathBuf::from(display_path)],
            is_active: true,
            version: Some("1.0.0".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.0,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::Unknown,
            update_strategy: StrategyKind::Unknown,
            remediation_strategy: StrategyKind::Unknown,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        }
    }

    fn classify_manager_path(manager: ManagerId, path: &str) -> ManagerInstallInstance {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from(path)),
            version: Some("1.0.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: path.to_string(),
            display_path: PathBuf::from(path),
            canonical_path: Some(PathBuf::from(path)),
            alias_paths: vec![PathBuf::from(path)],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();
        classify_instance(manager, &detection, candidate, &mut context)
    }

    #[test]
    #[cfg(unix)]
    fn dedupes_symlink_aliases_into_single_instance() {
        let root = std::env::temp_dir().join(format!(
            "helm-install-instance-symlink-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let target = root.join("rustup-target");
        fs::write(&target, b"binary").unwrap();
        let alias = root.join("rustup");
        std::os::unix::fs::symlink(&target, &alias).unwrap();

        let detection = rustup_detection(alias.clone());
        let instances = collect_manager_install_instances_from_candidates(
            ManagerId::Rustup,
            &detection,
            &[alias.clone(), target.clone()],
        );

        assert_eq!(instances.len(), 1);
        assert!(instances[0].alias_paths.iter().any(|path| path == &alias));
        assert!(instances[0].alias_paths.iter().any(|path| path == &target));

        let _ = fs::remove_file(alias);
        let _ = fs::remove_file(target);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rustup_cellar_path_classifies_as_homebrew() {
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
            canonical_path: Some(PathBuf::from(
                "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
            )),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
            is_active: true,
        };

        let mut instance = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "id".to_string(),
            identity_kind: candidate.identity_kind,
            identity_value: candidate.identity_value,
            display_path: candidate.display_path,
            canonical_path: candidate.canonical_path,
            alias_paths: candidate.alias_paths,
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.0,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::Unknown,
            update_strategy: StrategyKind::Unknown,
            remediation_strategy: StrategyKind::Unknown,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        };

        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::without_external_queries(),
        );
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.decision_margin, None);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn rustup_candidates_include_homebrew_keg_only_paths() {
        let candidates = manager_executable_candidates(ManagerId::Rustup);
        assert!(candidates.contains(&"rustup"));
        assert!(candidates.contains(&"/opt/homebrew/opt/rustup/bin/rustup"));
        assert!(candidates.contains(&"/usr/local/opt/rustup/bin/rustup"));
    }

    #[test]
    fn mise_cellar_path_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/mise")),
            version: Some("2024.11.6".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/Cellar/mise/2024.11.6/bin/mise".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/mise"),
            canonical_path: Some(PathBuf::from(
                "/opt/homebrew/Cellar/mise/2024.11.6/bin/mise",
            )),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/mise")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Mise, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
    }

    #[test]
    fn mise_usr_local_bin_defaults_to_unknown_when_ambiguous() {
        let mut instance = manager_instance(
            ManagerId::Mise,
            "/usr/local/bin/mise",
            "/usr/local/bin/mise",
        );
        classify_mise_instance(
            &mut instance,
            &mut ExternalEvidenceContext::without_external_queries(),
        );

        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("mise provenance evidence"))
        );
    }

    #[test]
    fn mise_usr_local_bin_resolves_homebrew_when_brew_prefix_matches() {
        fn brew_runner(program: &str, args: &[&str], _timeout: Duration) -> Option<String> {
            if program == "brew" && args == ["--prefix", "mise"] {
                return Some("/usr/local".to_string());
            }
            None
        }

        let mut instance = manager_instance(
            ManagerId::Mise,
            "/usr/local/bin/mise",
            "/usr/local/bin/mise",
        );
        classify_mise_instance(
            &mut instance,
            &mut ExternalEvidenceContext::with_runner(brew_runner),
        );

        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= AUTOMATIC_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert_eq!(
            instance.competing_provenance,
            Some(InstallProvenance::SourceBuild)
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn mise_default_script_path_classifies_as_source_build() {
        let instance = classify_manager_path(ManagerId::Mise, "/Users/test/.local/bin/mise");
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert!(instance.confidence >= AUTOMATIC_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn mise_cargo_home_path_classifies_as_source_build() {
        let instance = classify_manager_path(ManagerId::Mise, "/Users/test/.cargo/bin/mise");
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn mise_npm_global_path_classifies_as_source_build() {
        let path = "/usr/local/lib/node_modules/@jdxcode/mise/bin/mise";
        let instance = classify_manager_path(ManagerId::Mise, path);
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn mise_macports_path_classifies_as_macports() {
        let instance = classify_manager_path(ManagerId::Mise, "/opt/local/bin/mise");
        assert_eq!(instance.provenance, InstallProvenance::Macports);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn mise_nix_store_path_classifies_as_nix() {
        let path = "/nix/store/abc123-mise-2026.2.7/bin/mise";
        let instance = classify_manager_path(ManagerId::Mise, path);
        assert_eq!(instance.provenance, InstallProvenance::Nix);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
    }

    #[test]
    fn mas_nonstandard_path_defaults_to_unknown() {
        let mut instance = manager_instance(
            ManagerId::Mas,
            "/Users/test/tools/mas",
            "/Users/test/tools/mas",
        );
        classify_homebrew_formula_manager_instance(&mut instance, "mas");

        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("insufficient or conflicting"))
        );
    }

    #[test]
    fn asdf_home_layout_classifies_as_asdf() {
        let mut instance = manager_instance(
            ManagerId::Asdf,
            "/Users/test/.asdf/bin/asdf",
            "/Users/test/.asdf/bin/asdf",
        );
        classify_asdf_instance(&mut instance);

        assert_eq!(instance.provenance, InstallProvenance::Asdf);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::AsdfSelf);
        assert_eq!(instance.update_strategy, StrategyKind::AsdfSelf);
    }

    #[test]
    fn asdf_cellar_path_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/asdf")),
            version: Some("0.15.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/Cellar/asdf/0.15.0/bin/asdf".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/asdf"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/Cellar/asdf/0.15.0/bin/asdf")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/asdf")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Asdf, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
    }

    #[test]
    fn homebrew_cellar_path_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
            version: Some("4.4.1".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/Cellar/brew/4.4.1/bin/brew".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/brew"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/Cellar/brew/4.4.1/bin/brew")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/brew")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::HomebrewFormula,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.update_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn homebrew_nonstandard_path_defaults_to_unknown() {
        let mut instance = manager_instance(
            ManagerId::HomebrewFormula,
            "/Users/test/tools/brew",
            "/Users/test/tools/brew",
        );
        classify_homebrew_formula_manager_instance(&mut instance, "brew");

        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn rustup_ambiguous_prefix_defaults_to_unknown() {
        let mut instance = rustup_instance("/usr/local/bin/rustup", "/usr/local/bin/rustup");

        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::without_external_queries(),
        );
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert!(
            instance
                .decision_margin
                .is_some_and(|value| value > 0.0 && value < 0.30)
        );
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
    }

    #[test]
    fn rustup_ambiguous_usr_local_resolves_homebrew_when_brew_prefix_matches() {
        fn brew_runner(program: &str, args: &[&str], _timeout: Duration) -> Option<String> {
            if program == "brew" && args == ["--prefix", "rustup"] {
                return Some("/usr/local".to_string());
            }
            None
        }

        let mut instance = rustup_instance("/usr/local/bin/rustup", "/usr/local/bin/rustup");
        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::with_runner(brew_runner),
        );

        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= AUTOMATIC_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert_eq!(
            instance.competing_provenance,
            Some(InstallProvenance::SourceBuild)
        );
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("Homebrew binary prefix"))
        );
    }

    #[test]
    fn rustup_ambiguous_usr_local_homebrew_pkgutil_receipt_calibrates_to_homebrew() {
        fn pkgutil_homebrew_runner(
            program: &str,
            args: &[&str],
            _timeout: Duration,
        ) -> Option<String> {
            if program == "brew" && args == ["--prefix", "rustup"] {
                return None;
            }
            if program == "pkgutil"
                && args.first().copied() == Some("--file-info")
                && args.get(1).copied() == Some("/usr/local/bin/rustup")
            {
                return Some(
                    "volume: /\npath: /usr/local/bin/rustup\npkgid: org.homebrew.rustup\n"
                        .to_string(),
                );
            }
            None
        }

        let mut instance = rustup_instance("/usr/local/bin/rustup", "/usr/local/bin/rustup");
        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::with_runner(pkgutil_homebrew_runner),
        );

        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= AUTOMATIC_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert!(
            instance
                .explanation_secondary
                .as_deref()
                .is_some_and(|text| text.contains("Homebrew-managed rustup"))
        );
    }

    #[test]
    fn rustup_ambiguous_usr_local_apple_pkgutil_receipt_calibrates_to_system() {
        fn pkgutil_system_runner(
            program: &str,
            args: &[&str],
            _timeout: Duration,
        ) -> Option<String> {
            if program == "brew" && args == ["--prefix", "rustup"] {
                return None;
            }
            if program == "pkgutil"
                && args.first().copied() == Some("--file-info")
                && args.get(1).copied() == Some("/usr/local/bin/rustup")
            {
                return Some(
                    "volume: /\npath: /usr/local/bin/rustup\npkgid: com.apple.pkg.rustup\n"
                        .to_string(),
                );
            }
            None
        }

        let mut instance = rustup_instance("/usr/local/bin/rustup", "/usr/local/bin/rustup");
        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::with_runner(pkgutil_system_runner),
        );

        assert_eq!(instance.provenance, InstallProvenance::System);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("system-managed rustup"))
        );
    }

    #[test]
    fn rustup_default_cargo_path_classifies_as_rustup_init() {
        let mut instance = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "id".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
            display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
            canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
            alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.0,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::Unknown,
            update_strategy: StrategyKind::Unknown,
            remediation_strategy: StrategyKind::Unknown,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        };

        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::without_external_queries(),
        );
        assert_eq!(instance.provenance, InstallProvenance::RustupInit);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.uninstall_strategy, StrategyKind::RustupSelf);
    }

    #[test]
    #[cfg(unix)]
    fn instance_id_is_stable_when_active_alias_path_changes() {
        let root = std::env::temp_dir().join(format!(
            "helm-install-instance-alias-switch-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let target = root.join("rustup-target");
        fs::write(&target, b"binary").unwrap();
        let alias_a = root.join("rustup-a");
        let alias_b = root.join("rustup-b");
        std::os::unix::fs::symlink(&target, &alias_a).unwrap();
        std::os::unix::fs::symlink(&target, &alias_b).unwrap();

        let first_detection = rustup_detection(alias_a.clone());
        let first = collect_manager_install_instances_from_candidates(
            ManagerId::Rustup,
            &first_detection,
            std::slice::from_ref(&alias_a),
        );

        let second_detection = rustup_detection(alias_b.clone());
        let second = collect_manager_install_instances_from_candidates(
            ManagerId::Rustup,
            &second_detection,
            std::slice::from_ref(&alias_b),
        );

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_eq!(first[0].instance_id, second[0].instance_id);

        let _ = fs::remove_file(alias_a);
        let _ = fs::remove_file(alias_b);
        let _ = fs::remove_file(target);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn compute_identity_fallback_hash_is_stable_for_missing_path() {
        let missing_path = std::env::temp_dir().join(format!(
            "helm-install-instance-missing-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&missing_path);

        let (first_kind, first_value) = compute_identity(&missing_path, None);
        let (second_kind, second_value) = compute_identity(&missing_path, None);

        assert_eq!(first_kind, InstallInstanceIdentityKind::FallbackHash);
        assert_eq!(second_kind, InstallInstanceIdentityKind::FallbackHash);
        assert_eq!(first_value, second_value);
    }

    #[test]
    fn compute_identity_prefers_canonical_path_when_available_without_metadata() {
        let display_path = std::env::temp_dir().join(format!(
            "helm-install-instance-display-missing-{}",
            std::process::id()
        ));
        let canonical_path = std::env::temp_dir().join(format!(
            "helm-install-instance-canonical-target-{}",
            std::process::id()
        ));
        let _ = fs::remove_file(&display_path);
        let _ = fs::remove_file(&canonical_path);

        let (identity_kind, identity_value) =
            compute_identity(&display_path, Some(canonical_path.as_path()));

        assert_eq!(identity_kind, InstallInstanceIdentityKind::CanonicalPath);
        assert_eq!(identity_value, canonical_path.to_string_lossy());
    }

    #[cfg(not(unix))]
    #[test]
    fn non_unix_fallback_identity_changes_when_metadata_changes() {
        let root = std::env::temp_dir().join(format!(
            "helm-install-instance-non-unix-fallback-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp identity root");

        let file_path = root.join("tool");
        fs::write(&file_path, b"v1").expect("write initial bytes");
        let (first_kind, first_value) = compute_identity(&file_path, None);

        fs::write(&file_path, b"v1-expanded").expect("write updated bytes");
        let (second_kind, second_value) = compute_identity(&file_path, None);

        assert_eq!(first_kind, InstallInstanceIdentityKind::FallbackHash);
        assert_eq!(second_kind, InstallInstanceIdentityKind::FallbackHash);
        assert_ne!(first_value, second_value);

        let _ = fs::remove_file(file_path);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_brew_probe_result_is_cached_per_context() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);

        fn fake_runner(program: &str, args: &[&str], _timeout: Duration) -> Option<String> {
            assert_eq!(program, "brew");
            assert_eq!(args, &["--prefix", "rustup"]);
            CALLS.fetch_add(1, Ordering::SeqCst);
            Some("/opt/homebrew/opt/rustup".to_string())
        }

        CALLS.store(0, Ordering::SeqCst);
        let mut context = ExternalEvidenceContext::with_runner(fake_runner);
        assert_eq!(
            context.brew_prefix("rustup").as_deref(),
            Some("/opt/homebrew/opt/rustup")
        );
        assert_eq!(
            context.brew_prefix("rustup").as_deref(),
            Some("/opt/homebrew/opt/rustup")
        );
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn rustup_asdf_shim_classifies_as_asdf() {
        let mut instance = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "id".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.asdf/shims/rustup".to_string(),
            display_path: PathBuf::from("/Users/test/.asdf/shims/rustup"),
            canonical_path: Some(PathBuf::from("/Users/test/.asdf/shims/rustup")),
            alias_paths: vec![PathBuf::from("/Users/test/.asdf/shims/rustup")],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.0,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::Unknown,
            update_strategy: StrategyKind::Unknown,
            remediation_strategy: StrategyKind::Unknown,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        };

        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::without_external_queries(),
        );
        assert_eq!(instance.provenance, InstallProvenance::Asdf);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
    }

    #[test]
    fn rustup_mise_install_layout_classifies_as_mise() {
        let mut instance = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "id".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.local/share/mise/installs/rust/1.93.0/bin/rustup"
                .to_string(),
            display_path: PathBuf::from(
                "/Users/test/.local/share/mise/installs/rust/1.93.0/bin/rustup",
            ),
            canonical_path: Some(PathBuf::from(
                "/Users/test/.local/share/mise/installs/rust/1.93.0/bin/rustup",
            )),
            alias_paths: vec![PathBuf::from(
                "/Users/test/.local/share/mise/installs/rust/1.93.0/bin/rustup",
            )],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.0,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::Unknown,
            update_strategy: StrategyKind::Unknown,
            remediation_strategy: StrategyKind::Unknown,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        };

        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::without_external_queries(),
        );
        assert_eq!(instance.provenance, InstallProvenance::Mise);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
    }

    #[test]
    fn external_pkgutil_probe_result_is_cached_per_context() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);

        fn fake_runner(program: &str, args: &[&str], _timeout: Duration) -> Option<String> {
            assert_eq!(program, "pkgutil");
            assert_eq!(args.first().copied(), Some("--file-info"));
            assert_eq!(args.get(1).copied(), Some("/usr/local/bin/rustup"));
            CALLS.fetch_add(1, Ordering::SeqCst);
            Some("volume: /\npath: /usr/local/bin/rustup\npkgid: com.example.rustup\n".to_string())
        }

        CALLS.store(0, Ordering::SeqCst);
        let mut context = ExternalEvidenceContext::with_runner(fake_runner);
        let first = context.pkgutil_file_owner(Path::new("/usr/local/bin/rustup"));
        let second = context.pkgutil_file_owner(Path::new("/usr/local/bin/rustup"));
        assert_eq!(
            first,
            Some(PkgutilFileOwner::Owned("com.example.rustup".to_string()))
        );
        assert_eq!(
            second,
            Some(PkgutilFileOwner::Owned("com.example.rustup".to_string()))
        );
        assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn npm_cellar_path_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/npm")),
            version: Some("10.9.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/Cellar/npm/10.9.0/bin/npm".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/npm"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/Cellar/npm/10.9.0/bin/npm")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/npm")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Npm, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= AUTOMATIC_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.update_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn pnpm_asdf_shim_classifies_as_asdf() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/Users/test/.asdf/shims/pnpm")),
            version: Some("9.10.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.asdf/shims/pnpm".to_string(),
            display_path: PathBuf::from("/Users/test/.asdf/shims/pnpm"),
            canonical_path: Some(PathBuf::from("/Users/test/.asdf/shims/pnpm")),
            alias_paths: vec![PathBuf::from("/Users/test/.asdf/shims/pnpm")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Pnpm, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Asdf);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn yarn_usr_local_bin_defaults_to_unknown_when_ambiguous() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/local/bin/yarn")),
            version: Some("1.22.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/local/bin/yarn".to_string(),
            display_path: PathBuf::from("/usr/local/bin/yarn"),
            canonical_path: Some(PathBuf::from("/usr/local/bin/yarn")),
            alias_paths: vec![PathBuf::from("/usr/local/bin/yarn")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Yarn, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("yarn provenance evidence"))
        );
    }

    #[test]
    fn pip_system_python3_path_classifies_as_system() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/bin/python3")),
            version: Some("24.3.1".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/bin/python3".to_string(),
            display_path: PathBuf::from("/usr/bin/python3"),
            canonical_path: Some(PathBuf::from("/usr/bin/python3")),
            alias_paths: vec![PathBuf::from("/usr/bin/python3")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Pip, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::System);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(instance.update_strategy, StrategyKind::ReadOnly);
    }

    #[test]
    fn pipx_cellar_path_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/pipx")),
            version: Some("1.7.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/Cellar/pipx/1.7.0/bin/pipx".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/pipx"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/Cellar/pipx/1.7.0/bin/pipx")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/pipx")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Pipx, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= AUTOMATIC_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.update_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn poetry_mise_layout_classifies_as_mise() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from(
                "/Users/test/.local/share/mise/installs/poetry/1.8.4/bin/poetry",
            )),
            version: Some("1.8.4".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.local/share/mise/installs/poetry/1.8.4/bin/poetry"
                .to_string(),
            display_path: PathBuf::from(
                "/Users/test/.local/share/mise/installs/poetry/1.8.4/bin/poetry",
            ),
            canonical_path: Some(PathBuf::from(
                "/Users/test/.local/share/mise/installs/poetry/1.8.4/bin/poetry",
            )),
            alias_paths: vec![PathBuf::from(
                "/Users/test/.local/share/mise/installs/poetry/1.8.4/bin/poetry",
            )],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Poetry, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Mise);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn pip_usr_local_bin_defaults_to_unknown_when_ambiguous() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/local/bin/pip3")),
            version: Some("24.3.1".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/local/bin/pip3".to_string(),
            display_path: PathBuf::from("/usr/local/bin/pip3"),
            canonical_path: Some(PathBuf::from("/usr/local/bin/pip3")),
            alias_paths: vec![PathBuf::from("/usr/local/bin/pip3")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Pip, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("pip provenance evidence"))
        );
    }

    #[test]
    fn rubygems_cellar_path_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/gem")),
            version: Some("3.6.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/Cellar/ruby/3.4.0/bin/gem".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/gem"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/Cellar/ruby/3.4.0/bin/gem")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/gem")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::RubyGems, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert!(instance.confidence >= AUTOMATIC_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.update_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn rubygems_system_bin_classifies_as_system() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/bin/gem")),
            version: Some("3.3.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/bin/gem".to_string(),
            display_path: PathBuf::from("/usr/bin/gem"),
            canonical_path: Some(PathBuf::from("/usr/bin/gem")),
            alias_paths: vec![PathBuf::from("/usr/bin/gem")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::RubyGems, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::System);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(instance.update_strategy, StrategyKind::ReadOnly);
    }

    #[test]
    fn bundler_asdf_shim_classifies_as_asdf() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/Users/test/.asdf/shims/bundle")),
            version: Some("2.6.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.asdf/shims/bundle".to_string(),
            display_path: PathBuf::from("/Users/test/.asdf/shims/bundle"),
            canonical_path: Some(PathBuf::from("/Users/test/.asdf/shims/bundle")),
            alias_paths: vec![PathBuf::from("/Users/test/.asdf/shims/bundle")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Bundler, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Asdf);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn bundler_usr_local_versioned_bin_defaults_to_unknown_when_ambiguous() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/local/bin/bundle3.4")),
            version: Some("2.6.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/local/bin/bundle3.4".to_string(),
            display_path: PathBuf::from("/usr/local/bin/bundle3.4"),
            canonical_path: Some(PathBuf::from("/usr/local/bin/bundle3.4")),
            alias_paths: vec![PathBuf::from("/usr/local/bin/bundle3.4")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Bundler, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("bundler provenance evidence"))
        );
    }

    #[test]
    fn cargo_home_layout_classifies_as_source_build() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/Users/test/.cargo/bin/cargo")),
            version: Some("1.84.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.cargo/bin/cargo".to_string(),
            display_path: PathBuf::from("/Users/test/.cargo/bin/cargo"),
            canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/cargo")),
            alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/cargo")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Cargo, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn cargo_binstall_home_layout_classifies_as_source_build() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/Users/test/.cargo/bin/cargo-binstall")),
            version: Some("1.13.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.cargo/bin/cargo-binstall".to_string(),
            display_path: PathBuf::from("/Users/test/.cargo/bin/cargo-binstall"),
            canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/cargo-binstall")),
            alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/cargo-binstall")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::CargoBinstall,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert!(instance.confidence >= PROVENANCE_CONFIDENCE_THRESHOLD);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn cargo_binstall_usr_local_bin_defaults_to_unknown_when_ambiguous() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/local/bin/cargo-binstall")),
            version: Some("1.13.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/local/bin/cargo-binstall".to_string(),
            display_path: PathBuf::from("/usr/local/bin/cargo-binstall"),
            canonical_path: Some(PathBuf::from("/usr/local/bin/cargo-binstall")),
            alias_paths: vec![PathBuf::from("/usr/local/bin/cargo-binstall")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::CargoBinstall,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("cargo_binstall provenance evidence"))
        );
    }

    #[test]
    fn softwareupdate_usr_sbin_path_classifies_as_system() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
            version: Some("1.0.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/sbin/softwareupdate".to_string(),
            display_path: PathBuf::from("/usr/sbin/softwareupdate"),
            canonical_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
            alias_paths: vec![PathBuf::from("/usr/sbin/softwareupdate")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::SoftwareUpdate,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::System);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(instance.update_strategy, StrategyKind::ReadOnly);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("OS-managed system prefix"))
        );
    }

    #[test]
    fn macports_opt_local_path_classifies_as_macports() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/local/bin/port")),
            version: Some("2.9.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/local/bin/port".to_string(),
            display_path: PathBuf::from("/opt/local/bin/port"),
            canonical_path: Some(PathBuf::from("/opt/local/bin/port")),
            alias_paths: vec![PathBuf::from("/opt/local/bin/port")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::MacPorts, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Macports);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::MacportsSelf);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn nix_darwin_store_path_classifies_as_nix() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from(
                "/nix/store/abc123-darwin-system/bin/darwin-rebuild",
            )),
            version: Some("24.11".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/nix/store/abc123-darwin-system/bin/darwin-rebuild".to_string(),
            display_path: PathBuf::from("/run/current-system/sw/bin/darwin-rebuild"),
            canonical_path: Some(PathBuf::from(
                "/nix/store/abc123-darwin-system/bin/darwin-rebuild",
            )),
            alias_paths: vec![PathBuf::from("/run/current-system/sw/bin/darwin-rebuild")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::NixDarwin, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Nix);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(instance.update_strategy, StrategyKind::ReadOnly);
    }

    #[test]
    fn sparkle_app_bundle_path_classifies_as_source_build() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from(
                "/Applications/Sparkle.app/Contents/MacOS/sparkle",
            )),
            version: Some("2.7.1".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Applications/Sparkle.app/Contents/MacOS/sparkle".to_string(),
            display_path: PathBuf::from("/Applications/Sparkle.app/Contents/MacOS/sparkle"),
            canonical_path: Some(PathBuf::from(
                "/Applications/Sparkle.app/Contents/MacOS/sparkle",
            )),
            alias_paths: vec![PathBuf::from(
                "/Applications/Sparkle.app/Contents/MacOS/sparkle",
            )],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Sparkle, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("application bundle sparkle.app"))
        );
    }

    #[test]
    fn xcode_select_usr_bin_path_classifies_as_system() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/bin/xcode-select")),
            version: Some("2397".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/bin/xcode-select".to_string(),
            display_path: PathBuf::from("/usr/bin/xcode-select"),
            canonical_path: Some(PathBuf::from("/usr/bin/xcode-select")),
            alias_paths: vec![PathBuf::from("/usr/bin/xcode-select")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::XcodeCommandLineTools,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::System);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(instance.update_strategy, StrategyKind::ReadOnly);
    }

    #[test]
    fn xcode_clt_clang_path_classifies_as_system() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from(
                "/Library/Developer/CommandLineTools/usr/bin/clang",
            )),
            version: Some("2397".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Library/Developer/CommandLineTools/usr/bin/clang".to_string(),
            display_path: PathBuf::from("/Library/Developer/CommandLineTools/usr/bin/clang"),
            canonical_path: Some(PathBuf::from(
                "/Library/Developer/CommandLineTools/usr/bin/clang",
            )),
            alias_paths: vec![PathBuf::from(
                "/Library/Developer/CommandLineTools/usr/bin/clang",
            )],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::XcodeCommandLineTools,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::System);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(instance.update_strategy, StrategyKind::ReadOnly);
    }

    #[test]
    fn rosetta2_softwareupdate_path_classifies_as_system() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
            version: Some("1.0.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/sbin/softwareupdate".to_string(),
            display_path: PathBuf::from("/usr/sbin/softwareupdate"),
            canonical_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
            alias_paths: vec![PathBuf::from("/usr/sbin/softwareupdate")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Rosetta2, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::System);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(instance.update_strategy, StrategyKind::ReadOnly);
    }

    #[test]
    fn firmware_updates_softwareupdate_path_classifies_as_system() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
            version: Some("1.0.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/sbin/softwareupdate".to_string(),
            display_path: PathBuf::from("/usr/sbin/softwareupdate"),
            canonical_path: Some(PathBuf::from("/usr/sbin/softwareupdate")),
            alias_paths: vec![PathBuf::from("/usr/sbin/softwareupdate")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::FirmwareUpdates,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::System);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::ReadOnly);
        assert_eq!(instance.update_strategy, StrategyKind::ReadOnly);
    }

    #[test]
    fn setapp_app_bundle_path_classifies_as_source_build() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from(
                "/Applications/Setapp.app/Contents/MacOS/Setapp",
            )),
            version: Some("4.0.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Applications/Setapp.app/Contents/MacOS/Setapp".to_string(),
            display_path: PathBuf::from("/Applications/Setapp.app/Contents/MacOS/Setapp"),
            canonical_path: Some(PathBuf::from(
                "/Applications/Setapp.app/Contents/MacOS/Setapp",
            )),
            alias_paths: vec![PathBuf::from(
                "/Applications/Setapp.app/Contents/MacOS/Setapp",
            )],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Setapp, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("application bundle setapp.app"))
        );
    }

    #[test]
    fn docker_desktop_app_bundle_path_classifies_as_source_build() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from(
                "/Applications/Docker.app/Contents/Resources/bin/docker",
            )),
            version: Some("4.0.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Applications/Docker.app/Contents/Resources/bin/docker".to_string(),
            display_path: PathBuf::from("/Applications/Docker.app/Contents/Resources/bin/docker"),
            canonical_path: Some(PathBuf::from(
                "/Applications/Docker.app/Contents/Resources/bin/docker",
            )),
            alias_paths: vec![PathBuf::from(
                "/Applications/Docker.app/Contents/Resources/bin/docker",
            )],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::DockerDesktop,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn application_managers_usr_local_bin_default_to_unknown_when_ambiguous() {
        let cases = [
            (ManagerId::Sparkle, "sparkle", "sparkle"),
            (ManagerId::Setapp, "setapp", "setapp"),
            (ManagerId::DockerDesktop, "docker", "docker_desktop"),
            (ManagerId::ParallelsDesktop, "prlctl", "parallels_desktop"),
        ];

        for (manager, executable_name, expected_label) in cases {
            let path = format!("/usr/local/bin/{}", executable_name);
            let detection = DetectionInfo {
                installed: true,
                executable_path: Some(PathBuf::from(path.clone())),
                version: Some("1.0.0".to_string()),
            };
            let candidate = CandidateInstance {
                identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                identity_value: path.clone(),
                display_path: PathBuf::from(path.clone()),
                canonical_path: Some(PathBuf::from(path.clone())),
                alias_paths: vec![PathBuf::from(path.clone())],
                is_active: true,
            };
            let mut context = ExternalEvidenceContext::without_external_queries();

            let instance = classify_instance(manager, &detection, candidate, &mut context);
            assert_eq!(instance.provenance, InstallProvenance::Unknown);
            assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
            assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
            assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
            assert!(
                instance
                    .explanation_primary
                    .as_deref()
                    .is_some_and(|text| text.contains(expected_label))
            );
        }
    }

    #[test]
    fn podman_homebrew_prefix_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/podman")),
            version: Some("5.0.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/bin/podman".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/podman"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/bin/podman")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/podman")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Podman, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.update_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn podman_usr_local_bin_defaults_to_unknown_when_ambiguous() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/local/bin/podman")),
            version: Some("5.0.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/local/bin/podman".to_string(),
            display_path: PathBuf::from("/usr/local/bin/podman"),
            canonical_path: Some(PathBuf::from("/usr/local/bin/podman")),
            alias_paths: vec![PathBuf::from("/usr/local/bin/podman")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Podman, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("podman provenance evidence"))
        );
    }

    #[test]
    fn colima_homebrew_prefix_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/colima")),
            version: Some("0.7.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/bin/colima".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/colima"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/bin/colima")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/colima")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Colima, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert_eq!(instance.automation_level, AutomationLevel::Automatic);
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.update_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn colima_usr_local_bin_defaults_to_unknown_when_ambiguous() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/usr/local/bin/colima")),
            version: Some("0.7.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/local/bin/colima".to_string(),
            display_path: PathBuf::from("/usr/local/bin/colima"),
            canonical_path: Some(PathBuf::from("/usr/local/bin/colima")),
            alias_paths: vec![PathBuf::from("/usr/local/bin/colima")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(ManagerId::Colima, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("colima provenance evidence"))
        );
    }

    #[test]
    fn parallels_app_bundle_path_classifies_as_source_build() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from(
                "/Applications/Parallels Desktop.app/Contents/MacOS/prlctl",
            )),
            version: Some("20.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Applications/Parallels Desktop.app/Contents/MacOS/prlctl".to_string(),
            display_path: PathBuf::from(
                "/Applications/Parallels Desktop.app/Contents/MacOS/prlctl",
            ),
            canonical_path: Some(PathBuf::from(
                "/Applications/Parallels Desktop.app/Contents/MacOS/prlctl",
            )),
            alias_paths: vec![PathBuf::from(
                "/Applications/Parallels Desktop.app/Contents/MacOS/prlctl",
            )],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance = classify_instance(
            ManagerId::ParallelsDesktop,
            &detection,
            candidate,
            &mut context,
        );
        assert_eq!(instance.provenance, InstallProvenance::SourceBuild);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
    }

    #[test]
    fn homebrew_cask_brew_path_classifies_as_homebrew() {
        let detection = DetectionInfo {
            installed: true,
            executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
            version: Some("4.6.0".to_string()),
        };
        let candidate = CandidateInstance {
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/bin/brew".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/brew"),
            canonical_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/brew")],
            is_active: true,
        };
        let mut context = ExternalEvidenceContext::without_external_queries();

        let instance =
            classify_instance(ManagerId::HomebrewCask, &detection, candidate, &mut context);
        assert_eq!(instance.provenance, InstallProvenance::Homebrew);
        assert_eq!(
            instance.automation_level,
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(instance.uninstall_strategy, StrategyKind::HomebrewFormula);
        assert_eq!(instance.update_strategy, StrategyKind::HomebrewFormula);
    }

    #[test]
    fn additional_runtime_managers_usr_local_bin_default_to_unknown_when_ambiguous() {
        let cases = [
            (
                ManagerId::Npm,
                "/usr/local/bin/npm",
                "npm provenance evidence",
            ),
            (
                ManagerId::Pnpm,
                "/usr/local/bin/pnpm",
                "pnpm provenance evidence",
            ),
            (
                ManagerId::Pipx,
                "/usr/local/bin/pipx",
                "pipx provenance evidence",
            ),
            (
                ManagerId::Poetry,
                "/usr/local/bin/poetry",
                "poetry provenance evidence",
            ),
            (
                ManagerId::RubyGems,
                "/usr/local/bin/gem",
                "rubygems provenance evidence",
            ),
            (
                ManagerId::Cargo,
                "/usr/local/bin/cargo",
                "cargo provenance evidence",
            ),
        ];

        for (manager, path, expected_label) in cases {
            let instance = classify_manager_path(manager, path);
            assert_eq!(instance.provenance, InstallProvenance::Unknown);
            assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
            assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
            assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
            assert!(
                instance
                    .explanation_primary
                    .as_deref()
                    .is_some_and(|text| text.contains(expected_label))
            );
        }
    }

    #[test]
    fn guarded_system_managers_non_system_path_default_to_unknown() {
        let cases = [
            (
                ManagerId::SoftwareUpdate,
                "/usr/local/bin/softwareupdate",
                "softwareupdate executable path is not a trusted OS-managed location",
            ),
            (
                ManagerId::XcodeCommandLineTools,
                "/usr/local/bin/xcode-select",
                "xcode_command_line_tools executable path is not a trusted OS-managed location",
            ),
            (
                ManagerId::Rosetta2,
                "/usr/local/bin/softwareupdate",
                "rosetta2 executable path is not a trusted OS-managed location",
            ),
            (
                ManagerId::FirmwareUpdates,
                "/usr/local/bin/softwareupdate",
                "firmware_updates executable path is not a trusted OS-managed location",
            ),
        ];

        for (manager, path, expected_explanation) in cases {
            let instance = classify_manager_path(manager, path);
            assert_eq!(instance.provenance, InstallProvenance::Unknown);
            assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
            assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
            assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
            assert_eq!(
                instance.explanation_primary.as_deref(),
                Some(expected_explanation)
            );
        }
    }

    #[test]
    fn macports_nonstandard_path_defaults_to_unknown() {
        let instance = classify_manager_path(ManagerId::MacPorts, "/usr/local/bin/port");
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("macports provenance evidence"))
        );
    }

    #[test]
    fn nix_darwin_non_nix_path_defaults_to_unknown() {
        let instance = classify_manager_path(ManagerId::NixDarwin, "/usr/local/bin/darwin-rebuild");
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("nix_darwin provenance evidence"))
        );
    }

    #[test]
    fn homebrew_cask_nonstandard_path_defaults_to_unknown() {
        let instance = classify_manager_path(ManagerId::HomebrewCask, "/Users/test/tools/brew");
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert_eq!(instance.automation_level, AutomationLevel::ReadOnly);
        assert_eq!(instance.uninstall_strategy, StrategyKind::InteractivePrompt);
        assert_eq!(instance.update_strategy, StrategyKind::InteractivePrompt);
        assert!(
            instance
                .explanation_primary
                .as_deref()
                .is_some_and(|text| text.contains("brew provenance evidence"))
        );
    }

    #[test]
    fn rustup_conflicting_signals_default_to_unknown() {
        let mut instance = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "id".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/cargo-home/bin/rustup".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
            canonical_path: Some(PathBuf::from("/Users/test/cargo-home/bin/rustup")),
            alias_paths: vec![
                PathBuf::from("/opt/homebrew/bin/rustup"),
                PathBuf::from("/Users/test/cargo-home/bin/rustup"),
            ],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.0,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::Unknown,
            update_strategy: StrategyKind::Unknown,
            remediation_strategy: StrategyKind::Unknown,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        };

        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::without_external_queries(),
        );
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
        assert!(instance.confidence < PROVENANCE_CONFIDENCE_THRESHOLD);
    }

    #[test]
    fn automation_level_policy_boundary_mapping_is_stable() {
        assert_eq!(
            automation_level_for(InstallProvenance::Homebrew, AUTOMATIC_CONFIDENCE_THRESHOLD),
            AutomationLevel::Automatic
        );
        assert_eq!(
            automation_level_for(
                InstallProvenance::Homebrew,
                AUTOMATIC_CONFIDENCE_THRESHOLD - 0.01
            ),
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(
            automation_level_for(InstallProvenance::Unknown, UNKNOWN_CONFIRMATION_THRESHOLD),
            AutomationLevel::NeedsConfirmation
        );
        assert_eq!(
            automation_level_for(
                InstallProvenance::Unknown,
                UNKNOWN_CONFIRMATION_THRESHOLD - 0.01
            ),
            AutomationLevel::ReadOnly
        );
        assert_eq!(
            automation_level_for(InstallProvenance::System, 1.0),
            AutomationLevel::ReadOnly
        );
        assert_eq!(
            automation_level_for(InstallProvenance::Asdf, 1.0),
            AutomationLevel::NeedsConfirmation
        );
    }

    #[test]
    fn pkgutil_probe_failure_fails_closed_without_blocking() {
        fn failing_runner(_program: &str, _args: &[&str], _timeout: Duration) -> Option<String> {
            None
        }

        let mut context = ExternalEvidenceContext::with_runner(failing_runner);
        let first = context.pkgutil_file_owner(Path::new("/usr/local/bin/rustup"));
        let second = context.pkgutil_file_owner(Path::new("/usr/local/bin/rustup"));
        assert_eq!(first, None);
        assert_eq!(second, None);
    }

    #[test]
    fn external_probe_timeout_is_bounded() {
        let timeout = Duration::from_millis(50);
        let started = Instant::now();
        let result = run_command_with_timeout("sh", &["-c", "sleep 5"], timeout);
        let elapsed = started.elapsed();

        assert_eq!(result, None);
        assert!(
            elapsed < Duration::from_secs(3),
            "expected bounded timeout, got {elapsed:?}"
        );
    }

    #[test]
    fn rustup_classification_remains_responsive_with_hung_external_probes() {
        fn timeout_runner(_program: &str, _args: &[&str], timeout: Duration) -> Option<String> {
            run_command_with_timeout("sh", &["-c", "sleep 5"], timeout)
        }

        let mut instance = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "id".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/usr/local/bin/rustup".to_string(),
            display_path: PathBuf::from("/usr/local/bin/rustup"),
            canonical_path: Some(PathBuf::from("/usr/local/bin/rustup")),
            alias_paths: vec![PathBuf::from("/usr/local/bin/rustup")],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Unknown,
            confidence: 0.0,
            decision_margin: None,
            automation_level: AutomationLevel::ReadOnly,
            uninstall_strategy: StrategyKind::Unknown,
            update_strategy: StrategyKind::Unknown,
            remediation_strategy: StrategyKind::Unknown,
            explanation_primary: None,
            explanation_secondary: None,
            competing_provenance: None,
            competing_confidence: None,
        };

        let started = Instant::now();
        classify_rustup_instance(
            &mut instance,
            &mut ExternalEvidenceContext::with_runner(timeout_runner),
        );
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_secs(3),
            "expected bounded classification time, got {elapsed:?}"
        );
        assert_eq!(instance.provenance, InstallProvenance::Unknown);
    }

    #[test]
    fn remediation_strategy_matrix_handles_blocked_and_unknown_cases() {
        assert_eq!(
            remediation_strategy_for(InstallProvenance::Unknown),
            StrategyKind::InteractivePrompt
        );
        assert_eq!(
            remediation_strategy_for(InstallProvenance::System),
            StrategyKind::ReadOnly
        );
        assert_eq!(
            remediation_strategy_for(InstallProvenance::EnterpriseManaged),
            StrategyKind::ReadOnly
        );
        assert_eq!(
            remediation_strategy_for(InstallProvenance::Homebrew),
            StrategyKind::HomebrewFormula
        );
        assert_eq!(
            remediation_strategy_for(InstallProvenance::RustupInit),
            StrategyKind::RustupSelf
        );
    }
}
