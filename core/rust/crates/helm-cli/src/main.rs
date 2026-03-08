use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{IsTerminal, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use helm_core::adapters::manager::{
    RustupAddComponentRequest, RustupAddTargetRequest, RustupRemoveComponentRequest,
    RustupRemoveTargetRequest, RustupSetDefaultToolchainRequest, RustupSetOverrideRequest,
    RustupSetProfileRequest, RustupUnsetOverrideRequest,
};
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, AsdfAdapter, BundlerAdapter, CargoAdapter,
    CargoBinstallAdapter, ColimaAdapter, DetectRequest, DockerDesktopAdapter,
    FirmwareUpdatesAdapter, HomebrewAdapter, HomebrewCaskAdapter, InstallRequest,
    ListInstalledRequest, ListOutdatedRequest, MacPortsAdapter, ManagerAdapter, MasAdapter,
    MiseAdapter, NixDarwinAdapter, NpmAdapter, ParallelsDesktopAdapter, PinRequest, PipAdapter,
    PipxAdapter, PnpmAdapter, PodmanAdapter, PoetryAdapter, ProcessAsdfSource,
    ProcessBundlerSource, ProcessCargoBinstallSource, ProcessCargoSource, ProcessColimaSource,
    ProcessDockerDesktopSource, ProcessFirmwareUpdatesSource, ProcessHomebrewCaskSource,
    ProcessHomebrewSource, ProcessMacPortsSource, ProcessMasSource, ProcessMiseSource,
    ProcessNixDarwinSource, ProcessNpmSource, ProcessParallelsDesktopSource, ProcessPipSource,
    ProcessPipxSource, ProcessPnpmSource, ProcessPodmanSource, ProcessPoetrySource,
    ProcessRosetta2Source, ProcessRubyGemsSource, ProcessRustupSource, ProcessSetappSource,
    ProcessSoftwareUpdateSource, ProcessSparkleSource, ProcessXcodeCommandLineToolsSource,
    ProcessYarnSource, Rosetta2Adapter, RubyGemsAdapter, RustupAdapter, SearchRequest,
    SetappAdapter, SoftwareUpdateAdapter, SparkleAdapter, UninstallRequest, UnpinRequest,
    UpgradeRequest, XcodeCommandLineToolsAdapter, YarnAdapter,
    load_rustup_toolchain_detail_with_runtime,
};
use helm_core::execution::{
    ManagerTimeoutProfile, TaskOutputRecord, TokioProcessExecutor,
    replace_manager_execution_preferences,
};
use helm_core::managed_automation_policy::{
    ManagedAutomationPolicyMode, apply_managed_automation_policy,
};
use helm_core::manager_dependencies::provenance_dependency_manager;
use helm_core::manager_instances::{install_instance_fingerprint, resolve_multi_instance_state};
use helm_core::manager_policy::manager_enablement_eligibility;
use helm_core::models::{
    CachedSearchResult, Capability, DetectionInfo, HomebrewKegPolicy, InstalledPackage,
    ManagerAuthority, ManagerId, ManagerInstallInstance, ManagerUninstallPreview, OutdatedPackage,
    PackageRef, PackageRuntimeState, PackageUninstallPreview, PinKind, PinRecord, SearchQuery,
    StrategyKind, TaskId, TaskLogLevel, TaskRecord, TaskStatus,
};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState, CancellationMode};
use helm_core::persistence::{DetectionStore, PackageStore, PinStore, SearchCacheStore, TaskStore};
use helm_core::registry;
use helm_core::sqlite::SqliteStore;
use helm_core::uninstall_preview::{
    DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD, ManagerUninstallPreviewContext,
    PackageUninstallPreviewContext, build_manager_uninstall_preview,
    build_package_uninstall_preview,
};
use helm_core::versioning::PackageCoordinate;
use semver::Version;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

mod cli_errors;
mod command_dispatch;
mod coordinator_transport;
mod json_output;
mod provenance;
mod tui;

use provenance::{
    InstallChannel, InstallMarker, UpdatePolicy, can_self_update as provenance_can_self_update,
    detect_install_provenance, install_marker_path, read_install_marker,
    recommended_action as provenance_recommended_action, write_install_marker,
};

const TASK_FETCH_LIMIT: usize = 400;
const TASK_FOLLOW_MAX_WAIT_MS: u64 = 30_000;
const JSON_SCHEMA_VERSION: u32 = 1;
const CLI_ONBOARDING_REQUIRED_EXIT_CODE: u8 = 5;
const CLI_LICENSE_ACCEPTANCE_REQUIRED_EXIT_CODE: u8 = 6;
const TASKS_FOLLOW_MACHINE_MODE_UNSUPPORTED_ERROR: &str = "tasks follow does not support --json/--ndjson. Run without machine mode or use 'helm tasks logs <task-id>'.";
const CLI_LICENSE_TERMS_VERSION: &str = "helm-source-available-license-v1.0-pre1.0";
const CLI_LICENSE_TERMS_URL: &str = "https://github.com/jasoncavinder/Helm/blob/main/LICENSE";
const CLI_ACCEPT_LICENSE_ENV: &str = "HELM_ACCEPT_LICENSE";
const CLI_ACCEPT_DEFAULTS_ENV: &str = "HELM_ACCEPT_DEFAULTS";
const MANAGED_INSTALL_METHOD_POLICY_ENV: &str = "HELM_MANAGED_INSTALL_METHOD_POLICY";
const MANAGED_INSTALL_METHOD_POLICY_ALLOW_RESTRICTED_ENV: &str =
    "HELM_MANAGED_INSTALL_METHOD_POLICY_ALLOW_RESTRICTED";
const MANAGED_AUTOMATION_POLICY_ENV: &str = "HELM_MANAGED_AUTOMATION_POLICY";
static CLI_TOKIO_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static EXECUTABLE_DISCOVERY_CACHE: OnceLock<Mutex<HashMap<ManagerId, Vec<String>>>> =
    OnceLock::new();
static MANAGER_INSTALL_METHOD_POLICY_CONTEXT: OnceLock<ManagerInstallMethodPolicyContext> =
    OnceLock::new();
static MANAGER_AUTOMATION_POLICY_CONTEXT: OnceLock<ManagerAutomationPolicyContext> =
    OnceLock::new();
static COORDINATOR_REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static CLI_VERBOSE: AtomicBool = AtomicBool::new(false);
static CLI_REQUEST_TIMEOUT_SECONDS: AtomicU64 = AtomicU64::new(30);
static CLI_NDJSON: AtomicBool = AtomicBool::new(false);
const BASH_COMPLETION_SCRIPT: &str = r#"_helm_complete() {
    local cur
    cur="${COMP_WORDS[COMP_CWORD]}"
    local commands="status refresh search ls packages updates tasks managers settings diagnostics doctor onboarding self completion help"
    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "${commands}" -- "${cur}") )
        return 0
    fi

    case "${COMP_WORDS[1]}" in
        packages)
            COMPREPLY=( $(compgen -W "list search show install uninstall upgrade pin unpin rustup keg-policy help" -- "${cur}") )
            ;;
        updates)
            COMPREPLY=( $(compgen -W "list summary preview run help" -- "${cur}") )
            ;;
        tasks)
            COMPREPLY=( $(compgen -W "list show logs output follow cancel help" -- "${cur}") )
            ;;
        managers)
            COMPREPLY=( $(compgen -W "list show detect enable disable install update uninstall executables install-methods instances priority help" -- "${cur}") )
            ;;
        settings)
            COMPREPLY=( $(compgen -W "list get set reset help" -- "${cur}") )
            ;;
        diagnostics)
            COMPREPLY=( $(compgen -W "summary task manager provenance export help" -- "${cur}") )
            ;;
        doctor)
            COMPREPLY=( $(compgen -W "scan repair help" -- "${cur}") )
            ;;
        onboarding)
            COMPREPLY=( $(compgen -W "status run reset help" -- "${cur}") )
            ;;
        self)
            if [[ ${COMP_CWORD} -ge 3 && "${COMP_WORDS[2]}" == "auto-check" ]]; then
                COMPREPLY=( $(compgen -W "status enable disable frequency help" -- "${cur}") )
            else
                COMPREPLY=( $(compgen -W "status check update uninstall auto-check help" -- "${cur}") )
            fi
            ;;
        completion)
            COMPREPLY=( $(compgen -W "bash zsh fish help" -- "${cur}") )
            ;;
    esac
}
complete -F _helm_complete helm
"#;
const ZSH_COMPLETION_SCRIPT: &str = r#"#compdef helm

local -a commands
commands=(
  status
  refresh
  search
  ls
  packages
  updates
  tasks
  managers
  settings
  diagnostics
  doctor
  onboarding
  self
  completion
  help
)

if (( CURRENT == 2 )); then
  _describe 'command' commands
  return
fi

case $words[2] in
  packages)
    _values 'subcommand' list search show install uninstall upgrade pin unpin rustup keg-policy help
    ;;
  updates)
    _values 'subcommand' list summary preview run help
    ;;
  tasks)
    _values 'subcommand' list show logs output follow cancel help
    ;;
  managers)
    _values 'subcommand' list show detect enable disable install update uninstall executables install-methods instances priority help
    ;;
  settings)
    _values 'subcommand' list get set reset help
    ;;
  diagnostics)
    _values 'subcommand' summary task manager provenance export help
    ;;
  doctor)
    _values 'subcommand' scan repair help
    ;;
  onboarding)
    _values 'subcommand' status run reset help
    ;;
  self)
    if [[ "$words[3]" == "auto-check" ]]; then
      _values 'subcommand' status enable disable frequency help
    else
      _values 'subcommand' status check update uninstall auto-check help
    fi
    ;;
  completion)
    _values 'shell' bash zsh fish help
    ;;
esac
"#;
const FISH_COMPLETION_SCRIPT: &str = r#"complete -c helm -f
complete -c helm -n "__fish_use_subcommand" -a "status refresh search ls packages updates tasks managers settings diagnostics doctor onboarding self completion help"
complete -c helm -n "__fish_seen_subcommand_from packages" -a "list search show install uninstall upgrade pin unpin rustup keg-policy help"
complete -c helm -n "__fish_seen_subcommand_from updates" -a "list summary preview run help"
complete -c helm -n "__fish_seen_subcommand_from tasks" -a "list show logs output follow cancel help"
complete -c helm -n "__fish_seen_subcommand_from managers" -a "list show detect enable disable install update uninstall executables install-methods instances priority help"
complete -c helm -n "__fish_seen_subcommand_from settings" -a "list get set reset help"
complete -c helm -n "__fish_seen_subcommand_from diagnostics" -a "summary task manager provenance export help"
complete -c helm -n "__fish_seen_subcommand_from doctor" -a "scan repair help"
complete -c helm -n "__fish_seen_subcommand_from onboarding" -a "status run reset help"
complete -c helm -n "__fish_seen_subcommand_from self" -a "status check update uninstall auto-check help"
complete -c helm -n "__fish_seen_subcommand_from auto-check" -a "status enable disable frequency help"
complete -c helm -n "__fish_seen_subcommand_from completion" -a "bash zsh fish help"
"#;

#[derive(Default, Debug, Clone)]
struct GlobalOptions {
    json: bool,
    ndjson: bool,
    verbose: bool,
    quiet: bool,
    no_color: bool,
    locale: Option<String>,
    timeout_seconds: Option<u64>,
    execution_mode: ExecutionMode,
    accept_license: bool,
    accept_defaults: bool,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionMode {
    #[default]
    Wait,
    Detach,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Tui,
    Help,
    Version,
    Status,
    Refresh,
    Ls,
    Search,
    Packages,
    Updates,
    Tasks,
    Managers,
    Settings,
    Diagnostics,
    Doctor,
    Onboarding,
    SelfCmd,
    Completion,
    InternalCoordinator,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliTaskRecord {
    id: u64,
    manager: String,
    task_type: String,
    status: String,
    created_at_unix: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliTaskLogRecord {
    id: u64,
    task_id: u64,
    manager: String,
    task_type: String,
    status: Option<String>,
    level: String,
    message: String,
    created_at_unix: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliDiagnosticsSummary {
    installed_packages: usize,
    updatable_packages: usize,
    enabled_managers: usize,
    detected_enabled_managers: usize,
    queued_tasks: usize,
    running_tasks: usize,
    completed_tasks: usize,
    failed_tasks: usize,
    cancelled_tasks: usize,
    failed_task_ids: Vec<u64>,
    undetected_enabled_managers: Vec<String>,
    failure_classes: BTreeMap<String, usize>,
    failure_class_hints: BTreeMap<String, String>,
    coordinator: CliCoordinatorHealthSummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliCoordinatorHealthSummary {
    state_dir: String,
    ready_file_present: bool,
    pid: Option<u32>,
    pid_alive: Option<bool>,
    executable_path: Option<String>,
    executable_exists: Option<bool>,
    last_heartbeat_unix: Option<i64>,
    stale_reasons: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliTaskDiagnosticsOutput {
    available: bool,
    command: Option<String>,
    cwd: Option<String>,
    program_path: Option<String>,
    path_snippet: Option<String>,
    started_at_unix_ms: Option<i64>,
    finished_at_unix_ms: Option<i64>,
    duration_ms: Option<u64>,
    exit_code: Option<i32>,
    termination_reason: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
    stdout: Option<String>,
    stderr: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CliTaskDiagnosticsError {
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliManagerStatus {
    manager_id: String,
    display_name: String,
    authority: String,
    detected: bool,
    version: Option<String>,
    executable_path: Option<String>,
    enabled: bool,
    is_implemented: bool,
    is_optional: bool,
    is_detection_only: bool,
    supports_remote_search: bool,
    supports_package_install: bool,
    supports_package_uninstall: bool,
    supports_package_upgrade: bool,
    selected_executable_path: Option<String>,
    selected_executable_differs_from_default: bool,
    executable_path_diagnostic: String,
    selected_install_method: Option<String>,
    is_eligible: bool,
    ineligible_reason_code: Option<String>,
    ineligible_reason_message: Option<String>,
    install_instance_count: usize,
    multi_instance_state: String,
    multi_instance_acknowledged: bool,
    multi_instance_fingerprint: Option<String>,
    active_provenance: Option<String>,
    active_confidence: Option<f64>,
    active_decision_margin: Option<f64>,
    active_automation_level: Option<String>,
    active_uninstall_strategy: Option<String>,
    active_update_strategy: Option<String>,
    active_remediation_strategy: Option<String>,
    active_explanation_primary: Option<String>,
    active_explanation_secondary: Option<String>,
    competing_provenance: Option<String>,
    competing_confidence: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliManagerResult {
    manager_id: String,
    success: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliPackageManagerView {
    manager_id: String,
    installed_version: Option<String>,
    candidate_version: Option<String>,
    pinned: bool,
    restart_required: bool,
    runtime_state: PackageRuntimeState,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliPackageShowResult {
    name: String,
    manager: CliPackageManagerView,
    rustup_toolchain_detail: Option<helm_core::adapters::rustup::RustupToolchainDetail>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliManagerExecutableStatus {
    manager_id: String,
    active_executable_path: Option<String>,
    executable_paths: Vec<String>,
    default_executable_path: Option<String>,
    selected_executable_path: Option<String>,
    selected_executable_differs_from_default: bool,
    executable_path_diagnostic: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliManagerInstallMethodsStatus {
    manager_id: String,
    install_methods: Vec<String>,
    selected_install_method: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliManagerInstallInstance {
    manager_id: String,
    instance_id: String,
    identity_kind: String,
    identity_value: String,
    display_path: String,
    canonical_path: Option<String>,
    alias_paths: Vec<String>,
    is_active: bool,
    version: Option<String>,
    provenance: String,
    confidence: f64,
    decision_margin: Option<f64>,
    automation_level: String,
    uninstall_strategy: String,
    update_strategy: String,
    remediation_strategy: String,
    explanation_primary: Option<String>,
    explanation_secondary: Option<String>,
    competing_provenance: Option<String>,
    competing_confidence: Option<f64>,
}

#[derive(Debug, Clone)]
struct ParsedManagerMutationArgs {
    manager: ManagerId,
    preview: bool,
    yes: bool,
    allow_unknown_provenance: bool,
    install_method_override: Option<String>,
    install_options: helm_core::manager_lifecycle::ManagerInstallOptions,
    uninstall_options: helm_core::manager_lifecycle::ManagerUninstallOptions,
}

#[derive(Debug, Clone)]
struct ManagerUninstallPlan {
    target_manager: ManagerId,
    request: AdapterRequest,
    preview: ManagerUninstallPreview,
}

#[derive(Clone, Debug)]
struct UpgradeExecutionStep {
    manager: ManagerId,
    package_name: String,
    cleanup_old_kegs: bool,
    pinned: bool,
    restart_required: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliUpgradePlanStep {
    step_id: String,
    order_index: u64,
    manager_id: String,
    authority: String,
    action: String,
    package_name: String,
    pinned: bool,
    restart_required: bool,
    cleanup_old_kegs: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliUpgradeRunStepResult {
    step_id: String,
    manager_id: String,
    package_name: String,
    task_id: Option<u64>,
    success: bool,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliManagerPriorityEntry {
    manager_id: String,
    display_name: String,
    authority: String,
    rank: usize,
    default_rank: usize,
    overridden: bool,
    detected: bool,
    enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManagerTarget {
    All,
    One(ManagerId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoordinatorRequest {
    Ping,
    Submit {
        manager_id: String,
        request: CoordinatorSubmitRequest,
        wait: bool,
    },
    Cancel {
        task_id: u64,
    },
    StartWorkflow {
        workflow: CoordinatorWorkflowRequest,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoordinatorSubmitRequest {
    Detect,
    Search {
        query: String,
    },
    RustupAddComponent {
        toolchain: String,
        component: String,
    },
    RustupRemoveComponent {
        toolchain: String,
        component: String,
    },
    RustupAddTarget {
        toolchain: String,
        target: String,
    },
    RustupRemoveTarget {
        toolchain: String,
        target: String,
    },
    RustupSetDefaultToolchain {
        toolchain: String,
    },
    RustupSetOverride {
        toolchain: String,
        path: String,
    },
    RustupUnsetOverride {
        toolchain: String,
        path: String,
    },
    RustupSetProfile {
        profile: String,
    },
    Install {
        package_name: String,
        version: Option<String>,
    },
    Uninstall {
        package_name: String,
    },
    Upgrade {
        package_name: Option<String>,
    },
    Pin {
        package_name: String,
        version: Option<String>,
    },
    Unpin {
        package_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoordinatorWorkflowRequest {
    RefreshAll,
    RefreshManager {
        manager_id: String,
    },
    DetectAll,
    UpdatesRun {
        include_pinned: bool,
        allow_os_updates: bool,
        manager_id: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoordinatorResponse {
    ok: bool,
    task_id: Option<u64>,
    job_id: Option<String>,
    payload: Option<CoordinatorPayload>,
    #[serde(default)]
    exit_code: Option<u8>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CoordinatorPayload {
    Detection {
        installed: bool,
        version: Option<String>,
        executable_path: Option<String>,
    },
    Mutation {
        manager_id: String,
        package_name: String,
        action: String,
        before_version: Option<String>,
        after_version: Option<String>,
    },
    Refreshed,
    InstalledPackages {
        count: usize,
    },
    OutdatedPackages {
        count: usize,
    },
    SearchResults {
        count: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoordinatorReadyState {
    pid: u32,
    started_at: i64,
    heartbeat_unix: i64,
    #[serde(default)]
    executable_path: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct CoordinatorStateHealth {
    ready_file_present: bool,
    pid: Option<u32>,
    pid_alive: Option<bool>,
    executable_path: Option<String>,
    executable_exists: Option<bool>,
    last_heartbeat_unix: Option<i64>,
    stale_reasons: Vec<String>,
}

impl CoordinatorStateHealth {
    fn is_stale(&self) -> bool {
        !self.stale_reasons.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoordinatorClientTransport {
    LocalInProcess,
    ExternalFileIpc,
}

fn coordinator_transport_for_submit(execution_mode: ExecutionMode) -> CoordinatorClientTransport {
    if execution_mode == ExecutionMode::Detach {
        CoordinatorClientTransport::ExternalFileIpc
    } else {
        CoordinatorClientTransport::LocalInProcess
    }
}

fn coordinator_transport_for_workflow(execution_mode: ExecutionMode) -> CoordinatorClientTransport {
    if execution_mode == ExecutionMode::Detach {
        CoordinatorClientTransport::ExternalFileIpc
    } else {
        CoordinatorClientTransport::LocalInProcess
    }
}

fn coordinator_transport_for_cancel() -> CoordinatorClientTransport {
    CoordinatorClientTransport::ExternalFileIpc
}

fn coordinator_request_kind(request: &CoordinatorRequest) -> &'static str {
    match request {
        CoordinatorRequest::Ping => "ping",
        CoordinatorRequest::Submit { .. } => "submit",
        CoordinatorRequest::Cancel { .. } => "cancel",
        CoordinatorRequest::StartWorkflow { .. } => "start_workflow",
    }
}

fn coordinator_workflow_kind(workflow: &CoordinatorWorkflowRequest) -> &'static str {
    match workflow {
        CoordinatorWorkflowRequest::RefreshAll => "refresh_all",
        CoordinatorWorkflowRequest::RefreshManager { .. } => "refresh_manager",
        CoordinatorWorkflowRequest::DetectAll => "detect_all",
        CoordinatorWorkflowRequest::UpdatesRun { .. } => "updates_run",
    }
}

fn main() -> ExitCode {
    let raw_args: Vec<String> = env::args().skip(1).collect();
    set_ndjson_enabled(raw_args_request_ndjson(&raw_args));
    let (options, command, command_args) = match parse_args(raw_args.clone()) {
        Ok(parsed) => parsed,
        Err(error) => {
            let exit_code = exit_code_for_error(error.as_str());
            if raw_args_request_json(&raw_args) {
                emit_cli_error_json("helm.cli.v1.error", error.as_str(), exit_code);
            } else {
                eprintln!("helm: {error}");
            }
            return ExitCode::from(exit_code);
        }
    };
    set_cli_request_timeout_seconds(options.timeout_seconds);
    set_verbose_enabled(options.verbose);
    set_ndjson_enabled(options.ndjson);
    emit_version_metadata_if_verbose();
    verbose_log(format!(
        "parsed invocation: command={:?}, args={:?}, json={}, ndjson={}, execution_mode={:?}, verbose={}, quiet={}, no_color={}, locale={:?}, timeout_seconds={:?}, accept_license={}, accept_defaults={}",
        command,
        command_args,
        options.json,
        options.ndjson,
        options.execution_mode,
        options.verbose,
        options.quiet,
        options.no_color,
        options.locale,
        options.timeout_seconds,
        options.accept_license,
        options.accept_defaults
    ));

    if matches!(command, Command::Help) {
        if options.json {
            if command_args.is_empty() {
                emit_help_json_payload(None, &[], true);
            } else if let Some(topic_command) = parse_top_level_command(command_args[0].as_str()) {
                let topic_path = &command_args[1..];
                let resolved = command_help_topic_exists(topic_command, topic_path);
                emit_help_json_payload(Some(topic_command), topic_path, resolved);
            } else {
                emit_help_json_payload(None, &command_args, false);
            }
        } else if !print_help_topic(&command_args) {
            print_help();
        }
        return ExitCode::SUCCESS;
    }

    if matches!(command, Command::Version) {
        if options.json {
            emit_json_payload("helm.cli.v1.version", build_version_payload());
        } else {
            println!("{}", env!("CARGO_PKG_VERSION"));
        }
        return ExitCode::SUCCESS;
    }

    if matches!(command, Command::Completion) {
        return cmd_completion(options.clone(), &command_args)
            .map(|_| ExitCode::SUCCESS)
            .unwrap_or_else(|error| {
                let (json_emitted, normalized_error) = strip_json_error_marker(error.as_str());
                let (marked_exit_code, normalized_error) = strip_exit_code_marker(normalized_error);
                let exit_code =
                    marked_exit_code.unwrap_or_else(|| exit_code_for_error(normalized_error));
                if options.json {
                    if !json_emitted {
                        emit_cli_error_json("helm.cli.v1.error", normalized_error, exit_code);
                    }
                } else if !json_emitted {
                    eprintln!("helm: {normalized_error}");
                }
                ExitCode::from(exit_code)
            });
    }

    if let Some(help_path) = extract_help_path(&command_args) {
        if options.json {
            if command_help_topic_exists(command, &help_path) {
                emit_help_json_payload(Some(command), &help_path, true);
                return ExitCode::SUCCESS;
            }
        } else if print_command_help_topic(command, &help_path) {
            return ExitCode::SUCCESS;
        }
    }

    let store = match open_store() {
        Ok(store) => store,
        Err(error) => {
            let rendered = format!("failed to open state store: {error}");
            let exit_code = exit_code_for_error(rendered.as_str());
            if options.json {
                emit_cli_error_json("helm.cli.v1.error", rendered.as_str(), exit_code);
            } else {
                eprintln!("helm: {rendered}");
            }
            return ExitCode::from(exit_code);
        }
    };
    if let Err(error) = apply_manager_enablement_self_heal(store.as_ref()) {
        verbose_log(format!("manager policy self-heal skipped: {}", error));
    }
    if command_requires_cli_onboarding(command)
        && let Err(error) = ensure_cli_onboarding_completed(store.as_ref(), &options)
    {
        let (json_emitted, normalized_error) = strip_json_error_marker(error.as_str());
        let (marked_exit_code, normalized_error) = strip_exit_code_marker(normalized_error);
        let exit_code = marked_exit_code.unwrap_or_else(|| exit_code_for_error(normalized_error));
        if options.json {
            if !json_emitted {
                emit_cli_error_json("helm.cli.v1.error", normalized_error, exit_code);
            }
        } else if !json_emitted {
            eprintln!("helm: {normalized_error}");
        }
        return ExitCode::from(exit_code);
    }

    let options_for_error = options.clone();
    command_dispatch::execute_command(command, store.clone(), options, &command_args)
        .map(|_| ExitCode::SUCCESS)
        .unwrap_or_else(|error| {
            let (json_emitted, normalized_error) = strip_json_error_marker(error.as_str());
            let (marked_exit_code, normalized_error) = strip_exit_code_marker(normalized_error);
            let exit_code =
                marked_exit_code.unwrap_or_else(|| exit_code_for_error(normalized_error));
            if options_for_error.json {
                if !json_emitted {
                    emit_cli_error_json("helm.cli.v1.error", normalized_error, exit_code);
                }
            } else if !json_emitted {
                eprintln!("helm: {normalized_error}");
            }
            ExitCode::from(exit_code)
        })
}

fn cmd_tui(store: Arc<SqliteStore>, options: GlobalOptions) -> Result<(), String> {
    if options.json {
        return Err("TUI mode does not support --json or --ndjson".to_string());
    }
    tui::run(store, options.no_color, options.quiet)
}

fn parse_args(args: Vec<String>) -> Result<(GlobalOptions, Command, Vec<String>), String> {
    parse_args_with_tty(args, std::io::stdout().is_terminal())
}

fn parse_args_with_tty(
    args: Vec<String>,
    stdout_is_tty: bool,
) -> Result<(GlobalOptions, Command, Vec<String>), String> {
    if args.is_empty() {
        if stdout_is_tty {
            return Ok((global_options_from_env(), Command::Tui, Vec::new()));
        }
        return Ok((global_options_from_env(), Command::Help, Vec::new()));
    }

    let mut options = global_options_from_env();
    let mut filtered = Vec::new();
    let mut wait_flag = false;
    let mut detach_flag = false;
    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--json" => {
                options.json = true;
                index += 1;
                continue;
            }
            "--ndjson" => {
                options.ndjson = true;
                options.json = true;
                index += 1;
                continue;
            }
            "-v" | "--verbose" => {
                options.verbose = true;
                index += 1;
                continue;
            }
            "-q" | "--quiet" => {
                options.quiet = true;
                index += 1;
                continue;
            }
            "--no-color" => {
                options.no_color = true;
                index += 1;
                continue;
            }
            "--wait" => {
                wait_flag = true;
                options.execution_mode = ExecutionMode::Wait;
                index += 1;
                continue;
            }
            "--detach" => {
                detach_flag = true;
                options.execution_mode = ExecutionMode::Detach;
                index += 1;
                continue;
            }
            "--accept-license" => {
                options.accept_license = true;
                index += 1;
                continue;
            }
            "--accept-defaults" => {
                options.accept_defaults = true;
                index += 1;
                continue;
            }
            _ => {}
        }
        if filtered.is_empty() && parse_combined_short_flags(arg, &mut options, &mut filtered)? {
            index += 1;
            continue;
        }
        if let Some((key, value)) = arg.split_once('=')
            && key == "--locale"
        {
            if value.trim().is_empty() {
                return Err("--locale requires a non-empty value".to_string());
            }
            options.locale = Some(value.trim().to_string());
            index += 1;
            continue;
        }
        if arg == "--locale" {
            if index + 1 >= args.len() {
                return Err("--locale requires a value".to_string());
            }
            let value = args[index + 1].trim();
            if value.is_empty() {
                return Err("--locale requires a non-empty value".to_string());
            }
            options.locale = Some(value.to_string());
            index += 2;
            continue;
        }
        if let Some((key, value)) = arg.split_once('=')
            && key == "--timeout"
        {
            let seconds = value
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("invalid --timeout value '{}'", value.trim()))?;
            if seconds == 0 {
                return Err("--timeout must be greater than 0".to_string());
            }
            options.timeout_seconds = Some(seconds);
            index += 1;
            continue;
        }
        if arg == "--timeout" {
            if index + 1 >= args.len() {
                return Err("--timeout requires a value".to_string());
            }
            let seconds = args[index + 1]
                .trim()
                .parse::<u64>()
                .map_err(|_| format!("invalid --timeout value '{}'", args[index + 1].trim()))?;
            if seconds == 0 {
                return Err("--timeout must be greater than 0".to_string());
            }
            options.timeout_seconds = Some(seconds);
            index += 2;
            continue;
        }
        filtered.push(args[index].clone());
        index += 1;
    }

    if wait_flag && detach_flag {
        return Err("flags --wait and --detach are mutually exclusive".to_string());
    }
    if options.verbose && options.quiet {
        return Err("flags --verbose and --quiet are mutually exclusive".to_string());
    }

    if filtered.is_empty() {
        return Ok((options, Command::Help, Vec::new()));
    }

    let first = filtered[0].as_str();
    if matches!(first, "-h" | "--help" | "help") {
        return Ok((options, Command::Help, filtered[1..].to_vec()));
    }
    if matches!(first, "-V" | "--version") {
        return Ok((options, Command::Version, filtered[1..].to_vec()));
    }

    let command = parse_top_level_command(first)
        .ok_or_else(|| format!("unknown command '{first}'. Run 'helm help' for usage."))?;

    Ok((options, command, filtered[1..].to_vec()))
}

fn parse_combined_short_flags(
    arg: &str,
    options: &mut GlobalOptions,
    filtered: &mut Vec<String>,
) -> Result<bool, String> {
    if !arg.starts_with('-') || arg.starts_with("--") || arg.len() <= 2 {
        return Ok(false);
    }

    let bundle = &arg[1..];
    if !bundle.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return Ok(false);
    }

    let mut parsed_any = false;
    for ch in bundle.chars() {
        match ch {
            'v' => {
                options.verbose = true;
                parsed_any = true;
            }
            'q' => {
                options.quiet = true;
                parsed_any = true;
            }
            'h' => {
                filtered.push("-h".to_string());
                parsed_any = true;
            }
            'V' => {
                filtered.push("-V".to_string());
                parsed_any = true;
            }
            _ => {
                return Err(format!(
                    "unknown short flag '-{}' in combined flags '{}'",
                    ch, arg
                ));
            }
        }
    }

    Ok(parsed_any)
}

fn env_flag_enabled(key: &str) -> bool {
    let Ok(value) = env::var(key) else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn global_options_from_env() -> GlobalOptions {
    GlobalOptions {
        verbose: env_flag_enabled("HELM_CLI_VERBOSE"),
        accept_license: env_flag_enabled(CLI_ACCEPT_LICENSE_ENV),
        accept_defaults: env_flag_enabled(CLI_ACCEPT_DEFAULTS_ENV),
        ..GlobalOptions::default()
    }
}

fn set_verbose_enabled(enabled: bool) {
    CLI_VERBOSE.store(enabled, Ordering::Relaxed);
}

fn verbose_enabled() -> bool {
    CLI_VERBOSE.load(Ordering::Relaxed)
}

fn verbose_log(message: impl AsRef<str>) {
    if verbose_enabled() {
        eprintln!("helm[verbose]: {}", message.as_ref());
    }
}

fn set_cli_request_timeout_seconds(value: Option<u64>) {
    CLI_REQUEST_TIMEOUT_SECONDS.store(value.unwrap_or(30), Ordering::Relaxed);
}

fn set_ndjson_enabled(enabled: bool) {
    CLI_NDJSON.store(enabled, Ordering::Relaxed);
}

fn ndjson_enabled() -> bool {
    CLI_NDJSON.load(Ordering::Relaxed)
}

fn coordinator_request_timeout() -> Duration {
    coordinator_transport::coordinator_request_timeout(
        CLI_REQUEST_TIMEOUT_SECONDS.load(Ordering::Relaxed),
    )
}

fn coordinator_response_poll_interval(elapsed: Duration) -> Duration {
    coordinator_transport::coordinator_response_poll_interval(elapsed)
}

fn coordinator_server_idle_poll_interval(empty_iterations: u32) -> Duration {
    coordinator_transport::coordinator_server_idle_poll_interval(empty_iterations)
}

fn coordinator_bootstrap_lock_poll_interval(elapsed: Duration) -> Duration {
    coordinator_transport::coordinator_bootstrap_lock_poll_interval(elapsed)
}

fn coordinator_startup_poll_interval(elapsed: Duration) -> Duration {
    coordinator_transport::coordinator_startup_poll_interval(elapsed)
}

fn exit_code_for_error(error: &str) -> u8 {
    cli_errors::exit_code_for_error(error)
}

fn mark_json_error_emitted(error: impl AsRef<str>) -> String {
    cli_errors::mark_json_error_emitted(error)
}

fn strip_json_error_marker(error: &str) -> (bool, &str) {
    cli_errors::strip_json_error_marker(error)
}

fn mark_exit_code(error: impl AsRef<str>, exit_code: u8) -> String {
    cli_errors::mark_exit_code(error, exit_code)
}

fn strip_exit_code_marker(error: &str) -> (Option<u8>, &str) {
    cli_errors::strip_exit_code_marker(error)
}

fn parse_top_level_command(raw: &str) -> Option<Command> {
    match raw {
        "status" => Some(Command::Status),
        "refresh" => Some(Command::Refresh),
        "ls" => Some(Command::Ls),
        "search" => Some(Command::Search),
        "packages" => Some(Command::Packages),
        "updates" | "up" => Some(Command::Updates),
        "tasks" | "ps" => Some(Command::Tasks),
        "mgr" | "managers" => Some(Command::Managers),
        "config" | "settings" => Some(Command::Settings),
        "diagnostics" => Some(Command::Diagnostics),
        "doctor" => Some(Command::Doctor),
        "onboarding" => Some(Command::Onboarding),
        "self" => Some(Command::SelfCmd),
        "completion" => Some(Command::Completion),
        "__coordinator__" => Some(Command::InternalCoordinator),
        _ => None,
    }
}

fn extract_help_path(command_args: &[String]) -> Option<Vec<String>> {
    if command_args.is_empty() {
        return None;
    }

    if is_help_token(&command_args[0]) {
        return Some(command_args[1..].to_vec());
    }

    if command_args
        .last()
        .is_some_and(|value| is_help_token(value.as_str()))
    {
        return Some(command_args[..command_args.len() - 1].to_vec());
    }

    None
}

fn raw_args_request_json(args: &[String]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.as_str(), "--json" | "--ndjson"))
}

fn raw_args_request_ndjson(args: &[String]) -> bool {
    args.iter().any(|arg| arg == "--ndjson")
}

fn emit_cli_error_json(schema: &str, message: &str, exit_code: u8) {
    emit_json_payload(
        schema,
        json!({
            "message": message,
            "exit_code": exit_code
        }),
    );
}

fn build_version_payload() -> serde_json::Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "build_profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "build_channel": option_env!("HELM_BUILD_CHANNEL")
    })
}

fn emit_version_metadata_if_verbose() {
    if !verbose_enabled() {
        return;
    }
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let channel = option_env!("HELM_BUILD_CHANNEL").unwrap_or("unknown");
    verbose_log(format!(
        "build metadata: version={}, profile={}, channel={}",
        env!("CARGO_PKG_VERSION"),
        profile,
        channel
    ));
}

fn command_label(command: Command) -> &'static str {
    match command {
        Command::Tui => "tui",
        Command::Help => "help",
        Command::Version => "version",
        Command::Status => "status",
        Command::Refresh => "refresh",
        Command::Ls => "ls",
        Command::Search => "search",
        Command::Packages => "packages",
        Command::Updates => "updates",
        Command::Tasks => "tasks",
        Command::Managers => "managers",
        Command::Settings => "settings",
        Command::Diagnostics => "diagnostics",
        Command::Doctor => "doctor",
        Command::Onboarding => "onboarding",
        Command::SelfCmd => "self",
        Command::Completion => "completion",
        Command::InternalCoordinator => "__coordinator__",
    }
}

fn emit_help_json_payload(command: Option<Command>, path: &[String], resolved: bool) {
    let available_commands = [
        "status",
        "refresh",
        "search",
        "ls",
        "packages",
        "updates",
        "tasks",
        "managers",
        "settings",
        "diagnostics",
        "doctor",
        "onboarding",
        "self",
        "completion",
        "help",
    ];
    let topic = path
        .iter()
        .map(|segment| segment.as_str())
        .collect::<Vec<_>>();
    emit_json_payload(
        "helm.cli.v1.help",
        json!({
            "command": command.map(command_label),
            "topic": topic,
            "resolved": resolved,
            "available_commands": available_commands,
            "hint": "Run help without --json/--ndjson for formatted text output."
        }),
    );
}

fn command_help_topic_exists(command: Command, path: &[String]) -> bool {
    fn first(path: &[String]) -> Option<&str> {
        path.first().map(|value| value.as_str())
    }

    match command {
        Command::Tui => path.is_empty(),
        Command::Status | Command::Refresh | Command::Search | Command::Ls => path.is_empty(),
        Command::Packages => {
            if path.is_empty() {
                return true;
            }
            if path.len() == 1 {
                return matches!(
                    first(path),
                    Some(
                        "list"
                            | "search"
                            | "show"
                            | "install"
                            | "uninstall"
                            | "upgrade"
                            | "pin"
                            | "unpin"
                            | "rustup"
                            | "keg-policy"
                    )
                );
            }
            if path.len() == 2 {
                return matches!(
                    (path[0].as_str(), path[1].as_str()),
                    ("rustup", "show")
                        | ("rustup", "component")
                        | ("rustup", "target")
                        | ("rustup", "default")
                        | ("rustup", "override")
                        | ("rustup", "profile")
                        | ("keg-policy", "list")
                        | ("keg-policy", "get")
                        | ("keg-policy", "set")
                        | ("keg-policy", "reset")
                );
            }
            if path.len() == 3 {
                return matches!(
                    (path[0].as_str(), path[1].as_str(), path[2].as_str()),
                    ("rustup", "component", "add")
                        | ("rustup", "component", "remove")
                        | ("rustup", "target", "add")
                        | ("rustup", "target", "remove")
                        | ("rustup", "override", "set")
                        | ("rustup", "override", "unset")
                        | ("rustup", "profile", "set")
                );
            }
            false
        }
        Command::Updates => {
            path.is_empty()
                || (path.len() == 1
                    && matches!(first(path), Some("list" | "summary" | "preview" | "run")))
        }
        Command::Tasks => {
            path.is_empty()
                || (path.len() == 1
                    && matches!(
                        first(path),
                        Some("list" | "show" | "logs" | "output" | "follow" | "cancel")
                    ))
        }
        Command::Managers => {
            if path.is_empty() {
                return true;
            }
            if path.len() == 1 {
                return matches!(
                    first(path),
                    Some(
                        "list"
                            | "show"
                            | "detect"
                            | "enable"
                            | "disable"
                            | "install"
                            | "update"
                            | "uninstall"
                            | "executables"
                            | "install-methods"
                            | "instances"
                            | "priority"
                    )
                );
            }
            if path.len() == 2 {
                return matches!(
                    (path[0].as_str(), path[1].as_str()),
                    ("executables", "list")
                        | ("executables", "set")
                        | ("install-methods", "list")
                        | ("install-methods", "set")
                        | ("instances", "ack")
                        | ("instances", "clear-ack")
                        | ("priority", "list")
                        | ("priority", "set")
                        | ("priority", "reset")
                );
            }
            if path.len() == 3 {
                return (path[0].as_str(), path[1].as_str()) == ("instances", "set-active");
            }
            false
        }
        Command::Settings => {
            path.is_empty()
                || (path.len() == 1
                    && matches!(first(path), Some("list" | "get" | "set" | "reset")))
        }
        Command::Diagnostics => {
            path.is_empty()
                || (path.len() == 1
                    && matches!(
                        first(path),
                        Some("summary" | "task" | "manager" | "provenance" | "export")
                    ))
        }
        Command::Doctor => {
            if path.is_empty() {
                return true;
            }
            if path.len() == 1 {
                return matches!(first(path), Some("scan" | "repair"));
            }
            if path.len() == 2 {
                return (path[0].as_str(), path[1].as_str()) == ("repair", "plan")
                    || (path[0].as_str(), path[1].as_str()) == ("repair", "apply");
            }
            false
        }
        Command::Onboarding => {
            path.is_empty()
                || (path.len() == 1 && matches!(first(path), Some("status" | "run" | "reset")))
        }
        Command::SelfCmd => {
            if path.is_empty() {
                return true;
            }
            if path.len() == 1 {
                return matches!(
                    first(path),
                    Some("status" | "check" | "update" | "uninstall" | "auto-check")
                );
            }
            if path.len() == 2 {
                return path[0] == "auto-check"
                    && matches!(
                        path[1].as_str(),
                        "status" | "enable" | "disable" | "frequency"
                    );
            }
            false
        }
        Command::Completion => path.is_empty(),
        Command::Help | Command::Version | Command::InternalCoordinator => false,
    }
}

fn open_store() -> Result<Arc<SqliteStore>, String> {
    let db_path = database_path()?;
    verbose_log(format!("opening sqlite store at '{}'", db_path));
    let store = Arc::new(SqliteStore::new(db_path));
    store
        .migrate_to_latest()
        .map_err(|error| format!("failed to migrate sqlite store: {error}"))?;
    verbose_log("sqlite store ready");
    Ok(store)
}

#[derive(Debug, Clone)]
struct CliOnboardingState {
    completed: bool,
    accepted_license_terms_version: Option<String>,
}

impl CliOnboardingState {
    fn current_license_accepted(&self) -> bool {
        self.accepted_license_terms_version.as_deref() == Some(CLI_LICENSE_TERMS_VERSION)
    }

    fn fully_complete(&self) -> bool {
        self.completed && self.current_license_accepted()
    }
}

fn command_requires_cli_onboarding(command: Command) -> bool {
    !matches!(
        command,
        Command::Help
            | Command::Version
            | Command::Completion
            | Command::Onboarding
            | Command::InternalCoordinator
    )
}

fn load_cli_onboarding_state(store: &SqliteStore) -> Result<CliOnboardingState, String> {
    let completed = store
        .cli_onboarding_completed()
        .map_err(|error| format!("failed to read CLI onboarding completion state: {error}"))?;
    let accepted_license_terms_version = store
        .cli_accepted_license_terms_version()
        .map_err(|error| format!("failed to read CLI accepted license terms version: {error}"))?;
    Ok(CliOnboardingState {
        completed,
        accepted_license_terms_version,
    })
}

fn set_cli_license_accepted(store: &SqliteStore) -> Result<(), String> {
    store
        .set_cli_accepted_license_terms_version(Some(CLI_LICENSE_TERMS_VERSION))
        .map_err(|error| format!("failed to persist CLI accepted license terms version: {error}"))
}

fn set_cli_onboarding_completed(store: &SqliteStore, completed: bool) -> Result<(), String> {
    store
        .set_cli_onboarding_completed(completed)
        .map_err(|error| format!("failed to persist CLI onboarding completion state: {error}"))
}

fn apply_cli_default_settings(store: &SqliteStore) -> Result<(), String> {
    store
        .set_safe_mode(false)
        .map_err(|error| format!("failed to apply CLI default safe_mode setting: {error}"))?;
    store
        .set_homebrew_keg_policy(HomebrewKegPolicy::Keep)
        .map_err(|error| {
            format!("failed to apply CLI default homebrew_keg_auto_cleanup setting: {error}")
        })?;
    store.set_auto_check_for_updates(false).map_err(|error| {
        format!("failed to apply CLI default auto_check_for_updates setting: {error}")
    })?;
    store
        .set_auto_check_frequency_minutes(1_440)
        .map_err(|error| {
            format!("failed to apply CLI default auto_check_frequency_minutes setting: {error}")
        })?;
    Ok(())
}

fn can_run_interactive_onboarding() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn onboarding_block_error_message(state: &CliOnboardingState) -> String {
    if !state.current_license_accepted() {
        "CLI onboarding requires license acceptance before commands can run. Run Helm interactively and accept the license, or rerun with --accept-license --accept-defaults (or HELM_ACCEPT_LICENSE=1 HELM_ACCEPT_DEFAULTS=1).".to_string()
    } else {
        "CLI onboarding has not completed. Run Helm interactively to complete onboarding, or rerun with --accept-defaults (or HELM_ACCEPT_DEFAULTS=1).".to_string()
    }
}

fn onboarding_block_exit_code(state: &CliOnboardingState) -> u8 {
    if !state.current_license_accepted() {
        CLI_LICENSE_ACCEPTANCE_REQUIRED_EXIT_CODE
    } else {
        CLI_ONBOARDING_REQUIRED_EXIT_CODE
    }
}

fn ensure_cli_onboarding_completed(
    store: &SqliteStore,
    options: &GlobalOptions,
) -> Result<(), String> {
    let mut state = load_cli_onboarding_state(store)?;
    if state.fully_complete() {
        return Ok(());
    }

    if !state.current_license_accepted() && options.accept_license {
        set_cli_license_accepted(store)?;
    }
    if !state.completed && options.accept_defaults {
        apply_cli_default_settings(store)?;
        set_cli_onboarding_completed(store, true)?;
    }

    state = load_cli_onboarding_state(store)?;
    if state.fully_complete() {
        return Ok(());
    }

    if options.json {
        return Err(mark_exit_code(
            onboarding_block_error_message(&state),
            onboarding_block_exit_code(&state),
        ));
    }
    if !can_run_interactive_onboarding() {
        return Err(mark_exit_code(
            onboarding_block_error_message(&state),
            onboarding_block_exit_code(&state),
        ));
    }
    run_cli_onboarding_interactive(store, options.quiet)
}

fn prompt_line(prompt: &str) -> Result<String, String> {
    print!("{prompt}");
    std::io::stdout()
        .flush()
        .map_err(|error| format!("failed to flush stdout: {error}"))?;

    let mut input = String::new();
    let bytes = std::io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("failed to read stdin: {error}"))?;
    if bytes == 0 {
        return Err("stdin closed while reading onboarding input".to_string());
    }
    Ok(input.trim().to_string())
}

fn prompt_yes_no(prompt: &str, default: bool) -> Result<bool, String> {
    loop {
        let suffix = if default { "[Y/n]" } else { "[y/N]" };
        let input = prompt_line(&format!("{prompt} {suffix} "))?;
        if input.is_empty() {
            return Ok(default);
        }
        match input.to_ascii_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => {
                eprintln!("Please enter y or n.");
            }
        }
    }
}

fn prompt_menu_choice(prompt: &str, choices: &[&str], default: &str) -> Result<String, String> {
    loop {
        let input = prompt_line(prompt)?;
        let value = if input.is_empty() {
            default.to_string()
        } else {
            input
        };
        if choices.iter().any(|choice| *choice == value) {
            return Ok(value);
        }
        eprintln!("Please select one of: {}", choices.join(", "));
    }
}

fn prompt_frequency_minutes() -> Result<u32, String> {
    loop {
        let input = prompt_line("Update-check frequency in minutes [1440]: ")?;
        let value = if input.is_empty() {
            "1440".to_string()
        } else {
            input
        };
        let parsed = match value.parse::<u32>() {
            Ok(parsed) if parsed > 0 => parsed,
            _ => {
                eprintln!("Please provide a positive integer.");
                continue;
            }
        };
        return Ok(parsed);
    }
}

fn run_cli_onboarding_interactive(store: &SqliteStore, quiet: bool) -> Result<(), String> {
    let mut state = load_cli_onboarding_state(store)?;

    if !quiet {
        println!("Helm CLI setup");
        println!("  Version: {}", env!("CARGO_PKG_VERSION"));
        println!("  This setup runs once, then your original command continues.");
    }

    if !state.current_license_accepted() {
        if !quiet {
            println!();
            println!("License terms");
            println!("  Version: {CLI_LICENSE_TERMS_VERSION}");
            println!("  URL: {CLI_LICENSE_TERMS_URL}");
        }
        let accepted = prompt_yes_no("Accept license terms to continue?", false)?;
        if !accepted {
            return Err(mark_exit_code(
                "license acceptance is required to continue",
                CLI_LICENSE_ACCEPTANCE_REQUIRED_EXIT_CODE,
            ));
        }
        set_cli_license_accepted(store)?;
        state = load_cli_onboarding_state(store)?;
    }

    if !state.completed {
        let (safe_mode, homebrew_auto_cleanup, auto_check_for_updates, auto_check_frequency) =
            if quiet {
                (false, false, false, 1_440)
            } else {
                println!();
                println!("Onboarding profile");
                println!(
                    "  1) Recommended defaults (safe mode off, keg auto-cleanup off, auto-check off)"
                );
                println!("  2) Custom settings");
                let profile =
                    prompt_menu_choice("Select profile [1-2] (default 1): ", &["1", "2"], "1")?;
                if profile == "1" {
                    (false, false, false, 1_440)
                } else {
                    let safe_mode = prompt_yes_no("Enable safe mode?", false)?;
                    let homebrew_auto_cleanup =
                        prompt_yes_no("Enable Homebrew keg auto-cleanup?", false)?;
                    let auto_check_for_updates =
                        prompt_yes_no("Enable auto-check for updates?", false)?;
                    let auto_check_frequency = if auto_check_for_updates {
                        prompt_frequency_minutes()?
                    } else {
                        1_440
                    };
                    (
                        safe_mode,
                        homebrew_auto_cleanup,
                        auto_check_for_updates,
                        auto_check_frequency,
                    )
                }
            };

        store
            .set_safe_mode(safe_mode)
            .map_err(|error| format!("failed to persist safe_mode setting: {error}"))?;
        store
            .set_homebrew_keg_policy(if homebrew_auto_cleanup {
                HomebrewKegPolicy::Cleanup
            } else {
                HomebrewKegPolicy::Keep
            })
            .map_err(|error| {
                format!("failed to persist homebrew_keg_auto_cleanup setting: {error}")
            })?;
        store
            .set_auto_check_for_updates(auto_check_for_updates)
            .map_err(|error| {
                format!("failed to persist auto_check_for_updates setting: {error}")
            })?;
        store
            .set_auto_check_frequency_minutes(auto_check_frequency)
            .map_err(|error| {
                format!("failed to persist auto_check_frequency_minutes setting: {error}")
            })?;
        set_cli_onboarding_completed(store, true)?;
    }

    if !quiet {
        println!("CLI onboarding complete.");
    }
    Ok(())
}

fn database_path() -> Result<String, String> {
    if let Ok(path) = env::var("HELM_DB_PATH")
        && !path.trim().is_empty()
    {
        return Ok(path);
    }

    let home = env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
    let path = PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Helm")
        .join("helm.db");
    Ok(path.to_string_lossy().into_owned())
}

fn cmd_status(store: &SqliteStore, options: GlobalOptions) -> Result<(), String> {
    let enabled_map = manager_enabled_map(store)?;
    let installed = list_installed_for_enabled(store, &enabled_map)?;
    let outdated = list_outdated_for_enabled(store, &enabled_map)?;
    let tasks = list_tasks_for_enabled(store, &enabled_map)?;
    let managers = list_managers(store)?;

    let running_tasks = tasks
        .iter()
        .filter(|task| task.status == "running" || task.status == "queued")
        .count();
    let enabled_managers = managers.iter().filter(|manager| manager.enabled).count();
    let detected_managers = managers
        .iter()
        .filter(|manager| manager.enabled && manager.detected)
        .count();

    if options.json {
        emit_json_payload(
            "helm.cli.v1.status",
            json!({
                "installed_packages": installed.len(),
                "updatable_packages": outdated.len(),
                "running_or_queued_tasks": running_tasks,
                "enabled_managers": enabled_managers,
                "detected_enabled_managers": detected_managers
            }),
        );
        return Ok(());
    }

    println!("Helm Status");
    println!("  Installed packages: {}", installed.len());
    println!("  Available updates: {}", outdated.len());
    println!("  Running/queued tasks: {running_tasks}");
    println!("  Enabled managers: {enabled_managers}");
    println!("  Detected enabled managers: {detected_managers}");
    Ok(())
}

fn cmd_refresh(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let target = parse_manager_target(command_args, "refresh")?;
    verbose_log(format!(
        "refresh requested target={:?} mode={:?}",
        target, options.execution_mode
    ));
    let tokio_runtime = cli_tokio_runtime()?;
    let runtime = build_adapter_runtime(store.clone())?;

    match target {
        ManagerTarget::All => {
            if options.execution_mode == ExecutionMode::Detach {
                let response = coordinator_start_workflow(
                    store.as_ref(),
                    CoordinatorWorkflowRequest::RefreshAll,
                    options.execution_mode,
                )?;
                let job_id = response
                    .job_id
                    .ok_or_else(|| "coordinator workflow response missing job id".to_string())?;
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.refresh.detach",
                        json!({
                            "accepted": true,
                            "mode": "detach",
                            "scope": "all",
                            "job_id": job_id
                        }),
                    );
                } else {
                    println!(
                        "Refresh workflow submitted for all managers (job {}).",
                        job_id
                    );
                }
                return Ok(());
            }
            let rows = tokio_runtime.block_on(refresh_all_no_timeout(&runtime));
            let failures = emit_manager_results(
                options,
                "helm.cli.v1.refresh.all",
                "Refresh completed",
                rows,
            );
            if let Some(error) = manager_operation_failure_error("refresh", failures) {
                return Err(error);
            }
            Ok(())
        }
        ManagerTarget::One(manager) => {
            if options.execution_mode == ExecutionMode::Detach {
                let response = coordinator_start_workflow(
                    store.as_ref(),
                    CoordinatorWorkflowRequest::RefreshManager {
                        manager_id: manager.as_str().to_string(),
                    },
                    options.execution_mode,
                )?;
                let job_id = response
                    .job_id
                    .ok_or_else(|| "coordinator workflow response missing job id".to_string())?;
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.refresh.detach",
                        json!({
                            "accepted": true,
                            "mode": "detach",
                            "scope": "manager",
                            "manager_id": manager.as_str(),
                            "job_id": job_id
                        }),
                    );
                } else {
                    println!(
                        "Refresh workflow submitted for '{}' (job {}).",
                        manager.as_str(),
                        job_id
                    );
                }
                return Ok(());
            }

            let result = tokio_runtime.block_on(refresh_single_manager(&runtime, manager));
            let failure = result.as_ref().err().cloned();
            let rows = vec![CliManagerResult {
                manager_id: manager.as_str().to_string(),
                success: result.is_ok(),
                error: result.err(),
            }];
            let failures = emit_manager_results(
                options,
                "helm.cli.v1.refresh.manager",
                "Refresh completed",
                rows,
            );
            if failures > 0 {
                return Err(mark_exit_code(
                    failure.unwrap_or_else(|| "refresh failed".to_string()),
                    2,
                ));
            }
            Ok(())
        }
    }
}

fn cmd_packages(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return cmd_packages_list(store.as_ref(), options, &[]);
    }

    if command_args[0] == "list" {
        return cmd_packages_list(store.as_ref(), options, &command_args[1..]);
    }

    if command_args[0].starts_with("--") {
        return cmd_packages_list(store.as_ref(), options, command_args);
    }

    if command_args[0] == "search" {
        return cmd_search_query(store.clone(), options, &command_args[1..]);
    }

    if command_args[0] == "show" {
        return cmd_packages_show(store.as_ref(), options, &command_args[1..]);
    }

    if command_args[0] == "rustup" {
        return cmd_packages_rustup(store, options, &command_args[1..]);
    }

    if command_args[0] == "keg-policy" {
        return cmd_packages_keg_policy(store.as_ref(), options, &command_args[1..]);
    }

    if matches!(
        command_args[0].as_str(),
        "install" | "uninstall" | "upgrade" | "pin" | "unpin"
    ) {
        return cmd_packages_mutation(store, options, command_args[0].as_str(), &command_args[1..]);
    }

    Err(format!(
        "unsupported packages subcommand '{}'; currently supported: list, search, show, install, uninstall, upgrade, pin, unpin, rustup, keg-policy",
        command_args[0]
    ))
}

fn cmd_search(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    cmd_search_query(store, options, command_args)
}

fn cmd_search_query(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_search_args(command_args)?;
    let enabled_map = manager_enabled_map(store.as_ref())?;
    let local_results = if parsed.remote_only {
        Vec::new()
    } else {
        let mut rows = search_local_for_enabled(store.as_ref(), &enabled_map, &parsed.query)?;
        if let Some(manager) = parsed.manager_filter {
            rows.retain(|result| result.result.package.manager == manager);
        }
        rows
    };

    let (remote_results, remote_errors) = if parsed.local_only {
        (Vec::new(), Vec::new())
    } else {
        search_remote_for_enabled(
            store.clone(),
            &parsed.query,
            parsed.manager_filter,
            &enabled_map,
        )?
    };
    let mut merged_results = merge_search_results(local_results.clone(), remote_results.clone());
    if let Some(limit) = parsed.limit {
        merged_results.truncate(limit);
    }

    if options.json {
        emit_json_payload(
            "helm.cli.v1.search",
            json!({
                "query": parsed.query,
                "manager_filter": parsed.manager_filter.map(|manager| manager.as_str().to_string()),
                "local_only": parsed.local_only,
                "remote_only": parsed.remote_only,
                "limit": parsed.limit,
                "local_results": local_results,
                "remote_results": remote_results,
                "merged_results": merged_results,
                "remote_errors": remote_errors
            }),
        );
        return Ok(());
    }

    if merged_results.is_empty() {
        println!("No search results for query '{}'.", parsed.query);
        return Ok(());
    }

    if parsed.local_only {
        println!("Search Results (local cache)");
    } else if parsed.remote_only {
        println!("Search Results (remote)");
    } else {
        println!("Search Results (progressive local + remote)");
    }
    for result in merged_results {
        let version = result
            .result
            .version
            .as_deref()
            .unwrap_or("unknown-version");
        let summary = result.result.summary.as_deref().unwrap_or("-");
        println!(
            "  {}:{} @ {} ({}) source={}",
            result.result.package.manager.as_str(),
            result.result.package.name,
            version,
            summary,
            result.source_manager.as_str()
        );
    }
    if !remote_errors.is_empty() {
        println!();
        println!("Remote search warnings:");
        for warning in remote_errors {
            println!("  - {}", warning);
        }
    }
    Ok(())
}

fn cmd_packages_list(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_list_limit_args(command_args, "packages list")?;
    let enabled_map = manager_enabled_map(store)?;
    let mut installed = list_installed_for_enabled(store, &enabled_map)?;
    if let Some(limit) = parsed.limit {
        installed.truncate(limit);
    }
    if options.json {
        emit_json_payload(
            "helm.cli.v1.packages.list",
            json!({
                "packages": installed
            }),
        );
        return Ok(());
    }

    if installed.is_empty() {
        println!("No installed packages found.");
        return Ok(());
    }

    println!("Installed Packages");
    for package in installed {
        let version = package
            .installed_version
            .as_deref()
            .unwrap_or("unknown-version");
        let pinned = if package.pinned { " [pinned]" } else { "" };
        println!(
            "  {}:{} @ {}{}",
            package.package.manager.as_str(),
            package.package.name,
            version,
            pinned
        );
    }
    Ok(())
}

fn cmd_packages_show(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_package_show_args(command_args)?;
    let mut package_name = parsed.package_name;
    let requested_manager = parsed.manager;
    let mut requested_version: Option<String> = None;

    let enabled_map = manager_enabled_map(store)?;
    let installed = list_installed_for_enabled(store, &enabled_map)?;
    let outdated = list_outdated_for_enabled(store, &enabled_map)?;
    let mut rows = collect_package_show_rows(
        installed.as_slice(),
        outdated.as_slice(),
        package_name.as_str(),
        None,
    );
    if rows.is_empty() {
        let coordinate_hint_allowed = requested_manager
            .map(manager_supports_package_coordinate_versions)
            .unwrap_or(false);
        if coordinate_hint_allowed
            && let Some((coordinate_package_name, coordinate_version)) = parsed.coordinate_hint
        {
            let hinted_rows = collect_package_show_rows(
                installed.as_slice(),
                outdated.as_slice(),
                coordinate_package_name.as_str(),
                Some(coordinate_version.as_str()),
            );
            if !hinted_rows.is_empty() {
                package_name = coordinate_package_name;
                requested_version = Some(coordinate_version);
                rows = hinted_rows;
            }
        }
    }

    if rows.is_empty() {
        let requested_package_display = if let Some(version) = requested_version.as_deref() {
            format!("{}@{}", package_name, version)
        } else {
            package_name.clone()
        };
        if let Some(manager) = requested_manager {
            let managers = list_managers(store)?;
            let detected = managers
                .iter()
                .find(|row| row.manager_id == manager.as_str())
                .map(|row| row.detected)
                .unwrap_or(false);
            if !detected {
                return Err(format!(
                    "manager '{}' is not currently detected/installed",
                    manager.as_str()
                ));
            }
            return Err(format!(
                "package '{}' not found under manager '{}'",
                requested_package_display,
                manager.as_str()
            ));
        }
        return Err(format!("package '{}' not found", requested_package_display));
    }

    if let Some(manager) = requested_manager {
        rows.retain(|row| row.manager_id == manager.as_str());
        if rows.is_empty() {
            return Err(format!(
                "package '{}' not found under manager '{}'",
                if let Some(version) = requested_version.as_deref() {
                    format!("{}@{}", package_name, version)
                } else {
                    package_name.clone()
                },
                manager.as_str()
            ));
        }
    } else if rows.len() > 1 {
        let preference_key =
            package_manager_preference_key(package_name.as_str(), requested_version.as_deref());
        let fallback_preference_key = package_manager_preference_key(package_name.as_str(), None);
        let preferred_manager = store
            .package_manager_preference(preference_key.as_str())
            .map_err(|error| format!("failed to read package manager preference: {error}"))?;
        let preferred_manager = preferred_manager.or_else(|| {
            if preference_key == fallback_preference_key {
                return None;
            }
            store
                .package_manager_preference(fallback_preference_key.as_str())
                .ok()
                .flatten()
        });

        if let Some(preferred_manager) = preferred_manager {
            rows.retain(|row| row.manager_id == preferred_manager.as_str());
        }
    }

    if rows.len() > 1 {
        let managers = rows
            .iter()
            .map(|row| row.manager_id.clone())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "package '{}' is ambiguous across managers; specify --manager <id> (candidates: {})",
            package_name, managers
        ));
    }

    let manager = rows.into_iter().next().ok_or_else(|| {
        format!(
            "package '{}' could not be resolved for display due to empty manager set",
            package_name
        )
    })?;
    let rustup_toolchain_detail =
        resolve_rustup_package_show_detail(package_name.as_str(), &manager);

    if options.json {
        emit_json_payload(
            "helm.cli.v1.packages.show",
            json!({
                "package": CliPackageShowResult {
                    name: package_name,
                    manager,
                    rustup_toolchain_detail,
                }
            }),
        );
        return Ok(());
    }

    println!("Package: {}", package_name);
    println!("  manager: {}", manager.manager_id);
    println!(
        "  installed_version: {}",
        manager.installed_version.as_deref().unwrap_or("-")
    );
    println!(
        "  candidate_version: {}",
        manager.candidate_version.as_deref().unwrap_or("-")
    );
    println!("  pinned: {}", manager.pinned);
    println!("  restart_required: {}", manager.restart_required);
    if !manager.runtime_state.is_empty() {
        println!(
            "  runtime_state: {}",
            render_package_runtime_state(&manager.runtime_state)
        );
    }
    if let Some(detail) = rustup_toolchain_detail.as_ref() {
        print_rustup_toolchain_detail(detail);
    }
    Ok(())
}

fn resolve_rustup_package_show_detail(
    package_name: &str,
    manager: &CliPackageManagerView,
) -> Option<helm_core::adapters::rustup::RustupToolchainDetail> {
    if manager.manager_id != ManagerId::Rustup.as_str() {
        return None;
    }
    load_rustup_toolchain_detail_for_cli(package_name, "packages show")
}

pub(crate) fn load_rustup_toolchain_detail_for_cli(
    toolchain: &str,
    context: &str,
) -> Option<helm_core::adapters::rustup::RustupToolchainDetail> {
    let runtime = match cli_tokio_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("{context}: failed to initialize tokio runtime: {error}");
            return None;
        }
    };

    match load_rustup_toolchain_detail_with_runtime(runtime.handle(), toolchain) {
        Ok(detail) => Some(detail),
        Err(error) => {
            eprintln!(
                "{context}: failed to fetch rustup detail for '{}': {}",
                toolchain, error
            );
            None
        }
    }
}

fn print_rustup_toolchain_detail(detail: &helm_core::adapters::rustup::RustupToolchainDetail) {
    println!(
        "  profile: {}",
        detail.current_profile.as_deref().unwrap_or("-")
    );
    if detail.override_paths.is_empty() {
        println!("  overrides: none");
    } else {
        println!("  overrides: {}", detail.override_paths.join(", "));
    }
    print_rustup_toolchain_detail_group("components", detail.components.as_slice());
    print_rustup_toolchain_detail_group("targets", detail.targets.as_slice());
}

fn print_rustup_toolchain_detail_group(
    label: &str,
    entries: &[helm_core::adapters::rustup::RustupToolchainDetailEntry],
) {
    let installed = entries
        .iter()
        .filter(|entry| entry.installed)
        .map(|entry| entry.name.as_str())
        .collect::<Vec<_>>();
    println!(
        "  {label}: {} installed of {} available",
        installed.len(),
        entries.len()
    );
    if installed.is_empty() {
        println!("    none installed");
    } else {
        println!("    installed: {}", installed.join(", "));
    }
}

fn render_package_runtime_state(state: &PackageRuntimeState) -> String {
    let mut parts = Vec::new();
    if state.is_active {
        parts.push("active");
    }
    if state.is_default {
        parts.push("default");
    }
    if state.has_override {
        parts.push("override");
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(", ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RustupPackagesCommand {
    Show {
        toolchain: String,
    },
    ComponentAdd {
        toolchain: String,
        component: String,
    },
    ComponentRemove {
        toolchain: String,
        component: String,
    },
    TargetAdd {
        toolchain: String,
        target: String,
    },
    TargetRemove {
        toolchain: String,
        target: String,
    },
    Default {
        toolchain: String,
    },
    OverrideSet {
        toolchain: String,
        path: String,
    },
    OverrideUnset {
        toolchain: String,
        path: String,
    },
    ProfileSet {
        profile: String,
    },
}

impl RustupPackagesCommand {
    fn submit_request(&self) -> Option<CoordinatorSubmitRequest> {
        match self {
            Self::Show { .. } => None,
            Self::ComponentAdd {
                toolchain,
                component,
            } => Some(CoordinatorSubmitRequest::RustupAddComponent {
                toolchain: toolchain.clone(),
                component: component.clone(),
            }),
            Self::ComponentRemove {
                toolchain,
                component,
            } => Some(CoordinatorSubmitRequest::RustupRemoveComponent {
                toolchain: toolchain.clone(),
                component: component.clone(),
            }),
            Self::TargetAdd { toolchain, target } => {
                Some(CoordinatorSubmitRequest::RustupAddTarget {
                    toolchain: toolchain.clone(),
                    target: target.clone(),
                })
            }
            Self::TargetRemove { toolchain, target } => {
                Some(CoordinatorSubmitRequest::RustupRemoveTarget {
                    toolchain: toolchain.clone(),
                    target: target.clone(),
                })
            }
            Self::Default { toolchain } => {
                Some(CoordinatorSubmitRequest::RustupSetDefaultToolchain {
                    toolchain: toolchain.clone(),
                })
            }
            Self::OverrideSet { toolchain, path } => {
                Some(CoordinatorSubmitRequest::RustupSetOverride {
                    toolchain: toolchain.clone(),
                    path: path.clone(),
                })
            }
            Self::OverrideUnset { toolchain, path } => {
                Some(CoordinatorSubmitRequest::RustupUnsetOverride {
                    toolchain: toolchain.clone(),
                    path: path.clone(),
                })
            }
            Self::ProfileSet { profile } => Some(CoordinatorSubmitRequest::RustupSetProfile {
                profile: profile.clone(),
            }),
        }
    }

    fn schema(&self) -> &'static str {
        match self {
            Self::Show { .. } => "helm.cli.v1.packages.rustup.show",
            Self::ComponentAdd { .. } => "helm.cli.v1.packages.rustup.component.add",
            Self::ComponentRemove { .. } => "helm.cli.v1.packages.rustup.component.remove",
            Self::TargetAdd { .. } => "helm.cli.v1.packages.rustup.target.add",
            Self::TargetRemove { .. } => "helm.cli.v1.packages.rustup.target.remove",
            Self::Default { .. } => "helm.cli.v1.packages.rustup.default",
            Self::OverrideSet { .. } => "helm.cli.v1.packages.rustup.override.set",
            Self::OverrideUnset { .. } => "helm.cli.v1.packages.rustup.override.unset",
            Self::ProfileSet { .. } => "helm.cli.v1.packages.rustup.profile.set",
        }
    }

    fn json_fields(&self) -> serde_json::Value {
        match self {
            Self::Show { toolchain } => json!({
                "manager_id": ManagerId::Rustup.as_str(),
                "toolchain": toolchain,
            }),
            Self::ComponentAdd {
                toolchain,
                component,
            }
            | Self::ComponentRemove {
                toolchain,
                component,
            } => json!({
                "manager_id": ManagerId::Rustup.as_str(),
                "toolchain": toolchain,
                "component": component,
            }),
            Self::TargetAdd { toolchain, target } | Self::TargetRemove { toolchain, target } => {
                json!({
                    "manager_id": ManagerId::Rustup.as_str(),
                    "toolchain": toolchain,
                    "target": target,
                })
            }
            Self::Default { toolchain } => json!({
                "manager_id": ManagerId::Rustup.as_str(),
                "toolchain": toolchain,
            }),
            Self::OverrideSet { toolchain, path } | Self::OverrideUnset { toolchain, path } => {
                json!({
                    "manager_id": ManagerId::Rustup.as_str(),
                    "toolchain": toolchain,
                    "path": path,
                })
            }
            Self::ProfileSet { profile } => json!({
                "manager_id": ManagerId::Rustup.as_str(),
                "profile": profile,
            }),
        }
    }

    fn wait_message(&self, task_id: u64) -> String {
        match self {
            Self::Show { toolchain } => {
                format!("rustup toolchain detail loaded for '{toolchain}' (task #{task_id})")
            }
            Self::ComponentAdd {
                toolchain,
                component,
            } => format!("rustup component '{component}' added to '{toolchain}' (task #{task_id})"),
            Self::ComponentRemove {
                toolchain,
                component,
            } => format!(
                "rustup component '{component}' removed from '{toolchain}' (task #{task_id})"
            ),
            Self::TargetAdd { toolchain, target } => {
                format!("rustup target '{target}' added to '{toolchain}' (task #{task_id})")
            }
            Self::TargetRemove { toolchain, target } => {
                format!("rustup target '{target}' removed from '{toolchain}' (task #{task_id})")
            }
            Self::Default { toolchain } => {
                format!("rustup default toolchain set to '{toolchain}' (task #{task_id})")
            }
            Self::OverrideSet { toolchain, path } => {
                format!("rustup override for '{toolchain}' set at '{path}' (task #{task_id})")
            }
            Self::OverrideUnset { toolchain, path } => {
                format!("rustup override for '{toolchain}' cleared at '{path}' (task #{task_id})")
            }
            Self::ProfileSet { profile } => {
                format!("rustup profile set to '{profile}' (task #{task_id})")
            }
        }
    }

    fn detach_message(&self, task_id: u64) -> String {
        match self {
            Self::Show { toolchain } => {
                format!("rustup toolchain detail requested for '{toolchain}' (task #{task_id})")
            }
            Self::ComponentAdd {
                toolchain,
                component,
            } => format!(
                "rustup component '{component}' add submitted for '{toolchain}' (task #{task_id})"
            ),
            Self::ComponentRemove {
                toolchain,
                component,
            } => format!(
                "rustup component '{component}' removal submitted for '{toolchain}' (task #{task_id})"
            ),
            Self::TargetAdd { toolchain, target } => format!(
                "rustup target '{target}' add submitted for '{toolchain}' (task #{task_id})"
            ),
            Self::TargetRemove { toolchain, target } => format!(
                "rustup target '{target}' removal submitted for '{toolchain}' (task #{task_id})"
            ),
            Self::Default { toolchain } => format!(
                "rustup default toolchain change submitted for '{toolchain}' (task #{task_id})"
            ),
            Self::OverrideSet { toolchain, path } => format!(
                "rustup override set submitted for '{toolchain}' at '{path}' (task #{task_id})"
            ),
            Self::OverrideUnset { toolchain, path } => format!(
                "rustup override clear submitted for '{toolchain}' at '{path}' (task #{task_id})"
            ),
            Self::ProfileSet { profile } => {
                format!("rustup profile change submitted for '{profile}' (task #{task_id})")
            }
        }
    }
}

fn collect_package_show_rows(
    installed: &[InstalledPackage],
    outdated: &[OutdatedPackage],
    package_name: &str,
    version_filter: Option<&str>,
) -> Vec<CliPackageManagerView> {
    let mut rows: Vec<CliPackageManagerView> = Vec::new();

    let mut installed_map: HashMap<ManagerId, &InstalledPackage> = HashMap::new();
    for package in installed {
        if package.package.name != package_name {
            continue;
        }
        if let Some(version) = version_filter
            && package.installed_version.as_deref() != Some(version)
        {
            continue;
        }
        let entry = installed_map
            .entry(package.package.manager)
            .or_insert(package);
        if prefer_installed_package_for_show(package, entry) {
            *entry = package;
        }
    }

    let mut outdated_map: HashMap<ManagerId, &OutdatedPackage> = HashMap::new();
    for package in outdated {
        if package.package.name != package_name {
            continue;
        }
        if let Some(version) = version_filter
            && package.installed_version.as_deref() != Some(version)
            && package.candidate_version != version
        {
            continue;
        }
        let entry = outdated_map
            .entry(package.package.manager)
            .or_insert(package);
        if prefer_outdated_package_for_show(package, entry) {
            *entry = package;
        }
    }

    let mut candidate_managers: Vec<ManagerId> = installed_map
        .keys()
        .copied()
        .chain(outdated_map.keys().copied())
        .collect();
    candidate_managers.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    candidate_managers.dedup();

    for manager in candidate_managers {
        let installed_row = installed_map.get(&manager);
        let outdated_row = outdated_map.get(&manager);
        rows.push(CliPackageManagerView {
            manager_id: manager.as_str().to_string(),
            installed_version: installed_row.and_then(|row| row.installed_version.clone()),
            candidate_version: outdated_row.map(|row| row.candidate_version.clone()),
            pinned: installed_row
                .map(|row| row.pinned)
                .unwrap_or_else(|| outdated_row.map(|row| row.pinned).unwrap_or(false)),
            restart_required: outdated_row
                .map(|row| row.restart_required)
                .unwrap_or(false),
            runtime_state: installed_row
                .map(|row| row.runtime_state.clone())
                .unwrap_or_else(|| {
                    outdated_row
                        .map(|row| row.runtime_state.clone())
                        .unwrap_or_default()
                }),
        });
    }

    rows
}

fn prefer_installed_package_for_show(
    candidate: &InstalledPackage,
    current: &InstalledPackage,
) -> bool {
    runtime_state_rank(&candidate.runtime_state).cmp(&runtime_state_rank(&current.runtime_state))
        == std::cmp::Ordering::Greater
        || (candidate.runtime_state == current.runtime_state
            && version_text_rank(candidate.installed_version.as_deref())
                > version_text_rank(current.installed_version.as_deref()))
}

fn prefer_outdated_package_for_show(
    candidate: &OutdatedPackage,
    current: &OutdatedPackage,
) -> bool {
    runtime_state_rank(&candidate.runtime_state).cmp(&runtime_state_rank(&current.runtime_state))
        == std::cmp::Ordering::Greater
        || (candidate.runtime_state == current.runtime_state
            && version_text_rank(candidate.installed_version.as_deref())
                > version_text_rank(current.installed_version.as_deref()))
}

fn runtime_state_rank(runtime_state: &PackageRuntimeState) -> (u8, u8, u8) {
    (
        u8::from(runtime_state.is_active),
        u8::from(runtime_state.is_default),
        u8::from(!runtime_state.has_override),
    )
}

fn version_text_rank(version: Option<&str>) -> String {
    version
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_default()
}

fn cmd_packages_mutation(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    subcommand: &str,
    command_args: &[String],
) -> Result<(), String> {
    let allow_version = matches!(subcommand, "install" | "pin");
    let parsed = parse_package_mutation_args(subcommand, command_args, allow_version)?;
    let package = PackageRef {
        manager: parsed.manager,
        name: parsed.package_name.clone(),
    };
    verbose_log(format!(
        "package mutation requested action={} manager={} package={} mode={:?}",
        subcommand,
        parsed.manager.as_str(),
        parsed.package_name,
        options.execution_mode
    ));

    let supports_native_pin = registry::manager(parsed.manager)
        .map(|descriptor| descriptor.capabilities.contains(&Capability::Pin))
        .unwrap_or(false);
    let supports_native_unpin = registry::manager(parsed.manager)
        .map(|descriptor| descriptor.capabilities.contains(&Capability::Unpin))
        .unwrap_or(false);
    let package_uninstall_preview = if subcommand == "uninstall" {
        Some(build_package_uninstall_preview_for_package(
            store.as_ref(),
            &package,
        )?)
    } else {
        None
    };

    if let Some(preview) = package_uninstall_preview.as_ref() {
        if parsed.preview {
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.packages.uninstall.preview",
                    json!({ "preview": preview }),
                );
            } else {
                print_package_uninstall_preview(preview);
            }
            return Ok(());
        }

        if preview.requires_yes && !parsed.yes {
            return Err(
                "packages uninstall requires --yes for this blast radius. Run 'helm packages uninstall <name|name@manager> --manager <id> --preview' first, then rerun with --yes."
                    .to_string(),
            );
        }
    }

    let coordinator_request = match subcommand {
        "install" => Some(CoordinatorSubmitRequest::Install {
            package_name: parsed.package_name.clone(),
            version: parsed.version.clone(),
        }),
        "uninstall" => Some(CoordinatorSubmitRequest::Uninstall {
            package_name: parsed.package_name.clone(),
        }),
        "upgrade" => Some(CoordinatorSubmitRequest::Upgrade {
            package_name: Some(parsed.package_name.clone()),
        }),
        "pin" if supports_native_pin => Some(CoordinatorSubmitRequest::Pin {
            package_name: parsed.package_name.clone(),
            version: parsed.version.clone(),
        }),
        "unpin" if supports_native_unpin => Some(CoordinatorSubmitRequest::Unpin {
            package_name: parsed.package_name.clone(),
        }),
        "pin" | "unpin" => None,
        _ => {
            return Err(format!(
                "unsupported package mutation subcommand '{}'",
                subcommand
            ));
        }
    };

    let response = if let Some(request) = coordinator_request {
        Some(coordinator_submit_request(
            store.as_ref(),
            parsed.manager,
            request,
            options.execution_mode,
        )?)
    } else {
        None
    };

    if subcommand == "pin" {
        let pin_kind = if supports_native_pin {
            PinKind::Native
        } else {
            PinKind::Virtual
        };
        store
            .upsert_pin(&PinRecord {
                package: package.clone(),
                kind: pin_kind,
                pinned_version: parsed.version.clone(),
                created_at: SystemTime::now(),
            })
            .map_err(|error| format!("failed to persist pin record: {error}"))?;
        store
            .set_snapshot_pinned(&package, true)
            .map_err(|error| format!("failed to mark package pinned in snapshot: {error}"))?;
    }

    if subcommand == "unpin" {
        let package_key = format!("{}:{}", parsed.manager.as_str(), parsed.package_name);
        store
            .remove_pin(&package_key)
            .map_err(|error| format!("failed to remove pin record: {error}"))?;
        store
            .set_snapshot_pinned(&package, false)
            .map_err(|error| format!("failed to unmark package pinned in snapshot: {error}"))?;
    }

    if options.execution_mode == ExecutionMode::Detach {
        if let Some(response) = response {
            let task_id = response
                .task_id
                .ok_or_else(|| "coordinator detach response missing task id".to_string())?;
            if options.json {
                emit_json_payload(
                    &format!("helm.cli.v1.packages.{}", subcommand),
                    json!({
                        "task_id": task_id,
                        "manager_id": parsed.manager.as_str(),
                        "package_name": parsed.package_name,
                        "action": subcommand,
                        "accepted": true,
                        "mode": "detach",
                        "uninstall_preview": package_uninstall_preview
                    }),
                );
            } else {
                println!(
                    "Package {} {} submitted via manager '{}' (task #{})",
                    parsed.package_name,
                    subcommand,
                    parsed.manager.as_str(),
                    task_id
                );
                if let Some(preview) = package_uninstall_preview.as_ref() {
                    println!(
                        "  blast_radius_score: {} (requires_confirmation={})",
                        preview.blast_radius_score, preview.requires_yes
                    );
                }
            }
        } else if options.json {
            emit_json_payload(
                &format!("helm.cli.v1.packages.{}", subcommand),
                json!({
                    "task_id": null,
                    "manager_id": parsed.manager.as_str(),
                    "package_name": parsed.package_name,
                    "action": subcommand,
                    "accepted": true,
                    "mode": "detach"
                }),
            );
        } else {
            println!(
                "Package {} {} accepted immediately via manager '{}'.",
                parsed.package_name,
                subcommand,
                parsed.manager.as_str()
            );
        }
        return Ok(());
    }

    if let Some(response) = response {
        let task_id = response
            .task_id
            .ok_or_else(|| "coordinator wait response missing task id".to_string())?;
        match response.payload {
            Some(CoordinatorPayload::Mutation {
                manager_id,
                package_name,
                action,
                before_version,
                after_version,
            }) => {
                if options.json {
                    emit_json_payload(
                        &format!("helm.cli.v1.packages.{}", subcommand),
                        json!({
                                "task_id": task_id,
                                "manager_id": manager_id,
                            "package_name": package_name,
                            "action": action,
                            "before_version": before_version,
                            "after_version": after_version,
                            "uninstall_preview": package_uninstall_preview
                        }),
                    );
                } else {
                    println!(
                        "Package {} {} via manager '{}' (task #{})",
                        package_name, subcommand, manager_id, task_id
                    );
                    if let Some(preview) = package_uninstall_preview.as_ref() {
                        println!(
                            "  blast_radius_score: {} (requires_confirmation={})",
                            preview.blast_radius_score, preview.requires_yes
                        );
                    }
                }
            }
            _ => {
                return Err(format!(
                    "packages {} returned unexpected coordinator payload",
                    subcommand
                ));
            }
        }
    } else if options.json {
        emit_json_payload(
            &format!("helm.cli.v1.packages.{}", subcommand),
            json!({
                "task_id": null,
                "manager_id": parsed.manager.as_str(),
                "package_name": parsed.package_name,
                "action": subcommand,
                "uninstall_preview": package_uninstall_preview
            }),
        );
    } else {
        println!(
            "Package {} {} via manager '{}'",
            parsed.package_name,
            subcommand,
            parsed.manager.as_str()
        );
        if let Some(preview) = package_uninstall_preview.as_ref() {
            println!(
                "  blast_radius_score: {} (requires_confirmation={})",
                preview.blast_radius_score, preview.requires_yes
            );
        }
    }

    Ok(())
}

fn cmd_packages_rustup(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_packages_rustup_args(command_args)?;
    if let RustupPackagesCommand::Show { toolchain } = &parsed {
        return cmd_packages_show(
            store.as_ref(),
            options,
            &[
                toolchain.clone(),
                "--manager".to_string(),
                ManagerId::Rustup.as_str().to_string(),
            ],
        );
    }

    cmd_packages_rustup_submit(store, options, parsed)
}

fn cmd_packages_rustup_submit(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command: RustupPackagesCommand,
) -> Result<(), String> {
    let request = command
        .submit_request()
        .ok_or_else(|| "rustup command does not produce a submit request".to_string())?;
    let response = coordinator_submit_request(
        store.as_ref(),
        ManagerId::Rustup,
        request,
        options.execution_mode,
    )?;

    if options.execution_mode == ExecutionMode::Detach {
        let task_id = response
            .task_id
            .ok_or_else(|| "coordinator detach response missing task id".to_string())?;
        if options.json {
            let mut payload = command.json_fields();
            payload["accepted"] = json!(true);
            payload["mode"] = json!("detach");
            payload["task_id"] = json!(task_id);
            emit_json_payload(command.schema(), payload);
        } else {
            println!("{}", command.detach_message(task_id));
        }
        return Ok(());
    }

    let task_id = response
        .task_id
        .ok_or_else(|| "coordinator wait response missing task id".to_string())?;
    match response.payload {
        Some(CoordinatorPayload::Refreshed) | Some(CoordinatorPayload::Mutation { .. }) => {
            if options.json {
                let mut payload = command.json_fields();
                payload["accepted"] = json!(true);
                payload["mode"] = json!("wait");
                payload["task_id"] = json!(task_id);
                payload["refreshed"] = json!(true);
                emit_json_payload(command.schema(), payload);
            } else {
                println!("{}", command.wait_message(task_id));
            }
            Ok(())
        }
        _ => Err("packages rustup returned unexpected coordinator payload".to_string()),
    }
}

fn cmd_packages_keg_policy(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return Err("packages keg-policy requires a subcommand: list, get, set, reset".to_string());
    }

    match command_args[0].as_str() {
        "list" => {
            if command_args.len() != 1 {
                return Err(
                    "packages keg-policy list does not accept additional arguments".to_string(),
                );
            }
            let default_policy = store
                .homebrew_keg_policy()
                .map_err(|error| format!("failed to read homebrew keg policy: {error}"))?;
            let mut overrides = store
                .list_package_keg_policies()
                .map_err(|error| format!("failed to list package keg policies: {error}"))?
                .into_iter()
                .filter(|entry| entry.package.manager == ManagerId::HomebrewFormula)
                .map(|entry| {
                    json!({
                        "package_name": entry.package.name,
                        "manager_id": entry.package.manager.as_str(),
                        "policy": entry.policy.as_str()
                    })
                })
                .collect::<Vec<_>>();
            overrides.sort_by(|left, right| {
                left["package_name"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(right["package_name"].as_str().unwrap_or(""))
            });

            if options.json {
                emit_json_payload(
                    "helm.cli.v1.packages.keg_policy.list",
                    json!({
                        "default_policy": default_policy.as_str(),
                        "overrides": overrides
                    }),
                );
                return Ok(());
            }

            println!("Homebrew keg policy");
            println!("  default_policy: {}", default_policy.as_str());
            if overrides.is_empty() {
                println!("  overrides: -");
            } else {
                println!("  overrides:");
                for row in overrides {
                    println!(
                        "    - {} => {}",
                        row["package_name"].as_str().unwrap_or("-"),
                        row["policy"].as_str().unwrap_or("-")
                    );
                }
            }
            Ok(())
        }
        "get" => {
            if command_args.len() != 2 {
                return Err(
                    "packages keg-policy get requires <package-name|package@homebrew_formula>"
                        .to_string(),
                );
            }
            let package = parse_homebrew_keg_policy_package_selector(&command_args[1])?;
            let default_policy = store
                .homebrew_keg_policy()
                .map_err(|error| format!("failed to read homebrew keg policy: {error}"))?;
            let override_policy = store
                .package_keg_policy(&package)
                .map_err(|error| format!("failed to read package keg policy: {error}"))?;
            let effective_policy = override_policy.unwrap_or(default_policy);

            if options.json {
                emit_json_payload(
                    "helm.cli.v1.packages.keg_policy.get",
                    json!({
                        "package_name": package.name,
                        "manager_id": package.manager.as_str(),
                        "default_policy": default_policy.as_str(),
                        "override_policy": override_policy.map(|value| value.as_str().to_string()),
                        "effective_policy": effective_policy.as_str()
                    }),
                );
                return Ok(());
            }

            println!("Homebrew keg policy for '{}'", package.name);
            println!("  manager: {}", package.manager.as_str());
            println!("  default_policy: {}", default_policy.as_str());
            println!(
                "  override_policy: {}",
                override_policy
                    .map(|value| value.as_str().to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
            println!("  effective_policy: {}", effective_policy.as_str());
            Ok(())
        }
        "set" => {
            if command_args.len() != 3 {
                return Err("packages keg-policy set requires <package-name|package@homebrew_formula> <keep|cleanup|default>".to_string());
            }
            let package = parse_homebrew_keg_policy_package_selector(&command_args[1])?;
            let policy = parse_homebrew_keg_policy_arg(&command_args[2])?;
            store
                .set_package_keg_policy(&package, policy)
                .map_err(|error| format!("failed to persist package keg policy: {error}"))?;
            let effective_policy = effective_homebrew_keg_policy(store, &package.name);

            if options.json {
                emit_json_payload(
                    "helm.cli.v1.packages.keg_policy.set",
                    json!({
                        "package_name": package.name,
                        "manager_id": package.manager.as_str(),
                        "override_policy": policy.map(|value| value.as_str().to_string()),
                        "effective_policy": effective_policy.as_str()
                    }),
                );
                return Ok(());
            }

            let override_rendered = policy
                .map(|value| value.as_str().to_string())
                .unwrap_or_else(|| "default".to_string());
            println!(
                "Homebrew keg policy override for '{}' set to '{}' (effective: {}).",
                package.name,
                override_rendered,
                effective_policy.as_str()
            );
            Ok(())
        }
        "reset" => {
            if command_args.len() != 2 {
                return Err(
                    "packages keg-policy reset requires <package-name|package@homebrew_formula>"
                        .to_string(),
                );
            }
            let package = parse_homebrew_keg_policy_package_selector(&command_args[1])?;
            store
                .set_package_keg_policy(&package, None)
                .map_err(|error| format!("failed to reset package keg policy: {error}"))?;
            let effective_policy = effective_homebrew_keg_policy(store, &package.name);

            if options.json {
                emit_json_payload(
                    "helm.cli.v1.packages.keg_policy.reset",
                    json!({
                        "package_name": package.name,
                        "manager_id": package.manager.as_str(),
                        "override_policy": serde_json::Value::Null,
                        "effective_policy": effective_policy.as_str()
                    }),
                );
                return Ok(());
            }

            println!(
                "Homebrew keg policy override for '{}' cleared (effective: {}).",
                package.name,
                effective_policy.as_str()
            );
            Ok(())
        }
        other => Err(format!(
            "unsupported packages keg-policy subcommand '{}'; currently supported: list, get, set, reset",
            other
        )),
    }
}

fn cmd_updates(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return cmd_updates_list(store.as_ref(), options, &[]);
    }

    if command_args[0] == "list" {
        return cmd_updates_list(store.as_ref(), options, &command_args[1..]);
    }

    if command_args[0].starts_with("--") {
        return cmd_updates_list(store.as_ref(), options, command_args);
    }

    match command_args[0].as_str() {
        "summary" => cmd_updates_summary(store.as_ref(), options),
        "preview" => cmd_updates_preview(store.clone(), options, &command_args[1..]),
        "run" => cmd_updates_run(store.clone(), options, &command_args[1..]),
        _ => Err(format!(
            "unsupported updates subcommand '{}'; currently supported: list, summary, preview, run",
            command_args[0]
        )),
    }
}

fn cmd_updates_list(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_updates_list_args(command_args)?;
    let enabled_map = manager_enabled_map(store)?;
    let mut outdated = list_outdated_for_enabled(store, &enabled_map)?;
    if let Some(manager) = parsed.manager_filter {
        outdated.retain(|package| package.package.manager == manager);
    }
    if let Some(limit) = parsed.limit {
        outdated.truncate(limit);
    }

    if options.json {
        emit_json_payload(
            "helm.cli.v1.updates.list",
            json!({
                "updates": outdated
            }),
        );
        return Ok(());
    }

    if outdated.is_empty() {
        println!("No updates available.");
        return Ok(());
    }

    println!("Available Updates");
    for package in outdated {
        let from_version = package
            .installed_version
            .as_deref()
            .unwrap_or("unknown-version");
        let pinned = if package.pinned { " [pinned]" } else { "" };
        let restart = if package.restart_required {
            " [restart-required]"
        } else {
            ""
        };
        println!(
            "  {}:{} {} -> {}{}{}",
            package.package.manager.as_str(),
            package.package.name,
            from_version,
            package.candidate_version,
            pinned,
            restart
        );
    }
    Ok(())
}

fn cmd_updates_summary(store: &SqliteStore, options: GlobalOptions) -> Result<(), String> {
    let enabled_map = manager_enabled_map(store)?;
    let outdated = list_outdated_for_enabled(store, &enabled_map)?;
    let pinned_updates = outdated.iter().filter(|package| package.pinned).count();
    let restart_required_updates = outdated
        .iter()
        .filter(|package| package.restart_required)
        .count();
    let mut by_manager: HashMap<String, usize> = HashMap::new();
    for package in &outdated {
        let manager = package.package.manager.as_str().to_string();
        let count = by_manager.entry(manager).or_insert(0);
        *count = count.saturating_add(1);
    }

    if options.json {
        emit_json_payload(
            "helm.cli.v1.updates.summary",
            json!({
                "total_updates": outdated.len(),
                "pinned_updates": pinned_updates,
                "restart_required_updates": restart_required_updates,
                "by_manager": by_manager
            }),
        );
        return Ok(());
    }

    println!("Update Summary");
    println!("  total_updates: {}", outdated.len());
    println!("  pinned_updates: {}", pinned_updates);
    println!("  restart_required_updates: {}", restart_required_updates);
    println!("  by_manager:");
    if by_manager.is_empty() {
        println!("    -");
    } else {
        let mut entries = by_manager.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        for (manager, count) in entries {
            println!("    {}: {}", manager, count);
        }
    }
    Ok(())
}

fn cmd_updates_preview(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_updates_run_preview_args(command_args, false)?;
    let runtime = build_adapter_runtime(store.clone())?;
    let steps = collect_upgrade_execution_steps(
        store.as_ref(),
        &runtime,
        parsed.include_pinned,
        parsed.allow_os_updates,
        parsed.manager_filter,
    )?;
    let plan_steps = serialize_upgrade_plan_steps(&steps);

    if options.json {
        emit_json_payload(
            "helm.cli.v1.updates.preview",
            json!({
                "include_pinned": parsed.include_pinned,
                "allow_os_updates": parsed.allow_os_updates,
                "manager_filter": parsed.manager_filter.map(|manager| manager.as_str().to_string()),
                "total_steps": plan_steps.len(),
                "steps": plan_steps
            }),
        );
        return Ok(());
    }

    if plan_steps.is_empty() {
        println!("No upgrade steps available for current snapshot.");
        return Ok(());
    }

    println!("Upgrade Plan Preview");
    println!("  include_pinned: {}", parsed.include_pinned);
    println!("  allow_os_updates: {}", parsed.allow_os_updates);
    println!(
        "  manager_filter: {}",
        parsed
            .manager_filter
            .map(|manager| manager.as_str().to_string())
            .unwrap_or_else(|| "all".to_string())
    );
    for step in plan_steps {
        let pinned = if step.pinned { " [pinned]" } else { "" };
        let restart = if step.restart_required {
            " [restart-required]"
        } else {
            ""
        };
        let cleanup = if step.cleanup_old_kegs {
            " [cleanup-old-kegs]"
        } else {
            ""
        };
        println!(
            "  [{}] {}:{}{}{}{}",
            step.order_index, step.manager_id, step.package_name, pinned, restart, cleanup
        );
    }
    Ok(())
}

fn cmd_updates_run(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_updates_run_preview_args(command_args, true)?;
    verbose_log(format!(
        "updates run requested include_pinned={} allow_os_updates={} manager_filter={:?} mode={:?}",
        parsed.include_pinned,
        parsed.allow_os_updates,
        parsed.manager_filter,
        options.execution_mode
    ));
    if !parsed.yes {
        return Err(
            "updates run requires --yes. Run 'helm updates preview' first, then rerun with --yes."
                .to_string(),
        );
    }

    if options.execution_mode == ExecutionMode::Detach {
        let response = coordinator_start_workflow(
            store.as_ref(),
            CoordinatorWorkflowRequest::UpdatesRun {
                include_pinned: parsed.include_pinned,
                allow_os_updates: parsed.allow_os_updates,
                manager_id: parsed
                    .manager_filter
                    .map(|manager| manager.as_str().to_string()),
            },
            options.execution_mode,
        )?;
        let job_id = response
            .job_id
            .ok_or_else(|| "coordinator workflow response missing job id".to_string())?;
        if options.json {
            emit_json_payload(
                "helm.cli.v1.updates.run",
                json!({
                    "accepted": true,
                    "mode": "detach",
                    "job_id": job_id,
                    "include_pinned": parsed.include_pinned,
                    "allow_os_updates": parsed.allow_os_updates,
                    "manager_filter": parsed.manager_filter.map(|manager| manager.as_str().to_string())
                }),
            );
        } else if let Some(manager) = parsed.manager_filter {
            println!(
                "Upgrade workflow submitted for manager '{}' (job {}).",
                manager.as_str(),
                job_id
            );
        } else {
            println!("Upgrade workflow submitted (job {}).", job_id);
        }
        return Ok(());
    }

    let tokio_runtime = cli_tokio_runtime()?;
    let runtime = build_adapter_runtime(store.clone())?;
    let steps = collect_upgrade_execution_steps(
        store.as_ref(),
        &runtime,
        parsed.include_pinned,
        parsed.allow_os_updates,
        parsed.manager_filter,
    )?;

    if steps.is_empty() {
        if options.json {
            emit_json_payload(
                "helm.cli.v1.updates.run",
                json!({
                    "include_pinned": parsed.include_pinned,
                    "allow_os_updates": parsed.allow_os_updates,
                    "manager_filter": parsed.manager_filter.map(|manager| manager.as_str().to_string()),
                    "results": [],
                    "total_steps": 0,
                    "failed_steps": 0
                }),
            );
        } else {
            println!("No upgrade steps available for current snapshot.");
        }
        return Ok(());
    }

    let mut results: Vec<CliUpgradeRunStepResult> = Vec::with_capacity(steps.len());
    for step in &steps {
        let request = AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(PackageRef {
                manager: step.manager,
                name: upgrade_request_name(step),
            }),
        });
        let response = tokio_runtime.block_on(submit_request_wait(&runtime, step.manager, request));
        match response {
            Ok((task_id, _)) => results.push(CliUpgradeRunStepResult {
                step_id: upgrade_plan_step_id(step.manager, &step.package_name),
                manager_id: step.manager.as_str().to_string(),
                package_name: step.package_name.clone(),
                task_id: Some(task_id.0),
                success: true,
                error: None,
            }),
            Err(error) => results.push(CliUpgradeRunStepResult {
                step_id: upgrade_plan_step_id(step.manager, &step.package_name),
                manager_id: step.manager.as_str().to_string(),
                package_name: step.package_name.clone(),
                task_id: None,
                success: false,
                error: Some(error),
            }),
        }
    }

    let failures = results.iter().filter(|row| !row.success).count();

    if options.json {
        emit_json_payload(
            "helm.cli.v1.updates.run",
            json!({
                "include_pinned": parsed.include_pinned,
                "allow_os_updates": parsed.allow_os_updates,
                "manager_filter": parsed.manager_filter.map(|manager| manager.as_str().to_string()),
                "results": results,
                "total_steps": steps.len(),
                "failed_steps": failures
            }),
        );
    } else {
        println!("Upgrade Run Results");
        if let Some(manager) = parsed.manager_filter {
            println!("  manager_filter: {}", manager.as_str());
        }
        for row in &results {
            if row.success {
                println!(
                    "  {}:{} ok (task #{})",
                    row.manager_id,
                    row.package_name,
                    row.task_id.unwrap_or(0)
                );
            } else {
                println!(
                    "  {}:{} failed ({})",
                    row.manager_id,
                    row.package_name,
                    row.error
                        .clone()
                        .unwrap_or_else(|| "unknown error".to_string())
                );
            }
        }
    }

    if let Some(error) = manager_operation_failure_error("upgrade", failures) {
        return Err(error);
    }

    Ok(())
}

fn cmd_tasks(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return cmd_tasks_list(store, options, &[]);
    }

    if command_args[0] == "list" {
        return cmd_tasks_list(store, options, &command_args[1..]);
    }

    if command_args[0].starts_with("--") {
        return cmd_tasks_list(store, options, command_args);
    }

    match command_args[0].as_str() {
        "show" => cmd_tasks_show(store, options, &command_args[1..]),
        "logs" => cmd_tasks_logs(store, options, &command_args[1..]),
        "output" => cmd_tasks_output(options, &command_args[1..]),
        "follow" => cmd_tasks_follow(store, options, &command_args[1..]),
        "cancel" => cmd_tasks_cancel(options, &command_args[1..]),
        _ => Err(format!(
            "unsupported tasks subcommand '{}'; currently supported: list, show, logs, output, follow, cancel",
            command_args[0]
        )),
    }
}

fn cmd_tasks_list(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_tasks_list_args(command_args)?;
    let enabled_map = manager_enabled_map(store)?;
    let mut tasks = list_tasks_for_enabled(store, &enabled_map)?;
    if let Some(status_filter) = parsed.status_filter.as_deref() {
        tasks.retain(|task| task.status == status_filter);
    }
    if let Some(limit) = parsed.limit {
        tasks.truncate(limit);
    }
    if options.json {
        emit_json_payload(
            "helm.cli.v1.tasks.list",
            json!({
                "status_filter": parsed.status_filter,
                "tasks": tasks
            }),
        );
        return Ok(());
    }

    if tasks.is_empty() {
        println!("No tasks found.");
        return Ok(());
    }

    println!("Tasks");
    if let Some(status_filter) = parsed.status_filter.as_deref() {
        println!("  status_filter: {}", status_filter);
    }
    for task in tasks {
        println!(
            "  #{} [{}] {} {}",
            task.id, task.status, task.manager, task.task_type
        );
    }
    Ok(())
}

fn cmd_tasks_show(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return Err("tasks show requires a task id".to_string());
    }
    let task_id = command_args[0]
        .parse::<u64>()
        .map_err(|_| format!("invalid task id '{}'", command_args[0]))?;

    let enabled_map = manager_enabled_map(store)?;
    let tasks = list_tasks_for_enabled(store, &enabled_map)?;
    let task = tasks
        .into_iter()
        .find(|task| task.id == task_id)
        .ok_or_else(|| format!("task '{}' not found in recent task window", task_id))?;

    if options.json {
        emit_json_payload(
            "helm.cli.v1.tasks.show",
            json!({
                "task": task
            }),
        );
        return Ok(());
    }

    println!("Task #{}", task.id);
    println!("  manager: {}", task.manager);
    println!("  task_type: {}", task.task_type);
    println!("  status: {}", task.status);
    println!("  created_at_unix: {}", task.created_at_unix);
    Ok(())
}

fn cmd_tasks_logs(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_tasks_log_options(command_args, "logs")?;
    let entries = store
        .list_task_logs(TaskId(parsed.task_id), parsed.limit)
        .map_err(|error| format!("failed to list task logs: {error}"))?;
    let mut logs = entries
        .into_iter()
        .map(task_log_to_cli_record)
        .collect::<Vec<_>>();
    logs.retain(|record| {
        parsed
            .level_filter
            .as_ref()
            .map(|level| &record.level == level)
            .unwrap_or(true)
            && parsed
                .status_filter
                .as_ref()
                .map(|status| record.status.as_deref() == Some(status.as_str()))
                .unwrap_or(true)
    });

    if options.json {
        emit_json_payload(
            "helm.cli.v1.tasks.logs",
            json!({
                "task_id": parsed.task_id,
                "logs": logs
            }),
        );
        return Ok(());
    }

    if logs.is_empty() {
        println!("No logs found for task #{}.", parsed.task_id);
        return Ok(());
    }

    logs.reverse();
    println!("Task Logs #{}", parsed.task_id);
    for record in logs {
        let status = record.status.unwrap_or_else(|| "-".to_string());
        println!(
            "  [{}] [{}] [{}] {}",
            record.created_at_unix, record.level, status, record.message
        );
    }
    Ok(())
}

fn cmd_tasks_output(options: GlobalOptions, command_args: &[String]) -> Result<(), String> {
    let task_id = parse_task_id_argument(command_args, "tasks output")?;
    let output = helm_core::execution::task_output(TaskId(task_id));
    let output_payload = task_output_to_cli_diagnostics(output.as_ref());

    if options.json {
        emit_json_payload(
            "helm.cli.v1.tasks.output",
            json!({
                "task_id": task_id,
                "available": output_payload.available,
                "command": output_payload.command,
                "cwd": output_payload.cwd,
                "started_at_unix_ms": output_payload.started_at_unix_ms,
                "finished_at_unix_ms": output_payload.finished_at_unix_ms,
                "duration_ms": output_payload.duration_ms,
                "exit_code": output_payload.exit_code,
                "termination_reason": output_payload.termination_reason,
                "error_code": output_payload.error_code,
                "error_message": output_payload.error_message,
                "stdout": output_payload.stdout,
                "stderr": output_payload.stderr
            }),
        );
        return Ok(());
    }

    if !output_payload.available {
        println!(
            "Task output for #{} is not available in this CLI process session.",
            task_id
        );
        println!("Use 'tasks logs' for persisted lifecycle logs.");
        return Ok(());
    }

    println!("Task Output #{}", task_id);
    println!(
        "  command: {}",
        output_payload.command.as_deref().unwrap_or("-")
    );
    println!("  cwd: {}", output_payload.cwd.as_deref().unwrap_or("-"));
    println!(
        "  started_at_unix_ms: {}",
        output_payload
            .started_at_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  finished_at_unix_ms: {}",
        output_payload
            .finished_at_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  duration_ms: {}",
        output_payload
            .duration_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  exit_code: {}",
        output_payload
            .exit_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  termination_reason: {}",
        output_payload.termination_reason.as_deref().unwrap_or("-")
    );
    println!(
        "  error_code: {}",
        output_payload.error_code.as_deref().unwrap_or("-")
    );
    println!(
        "  error_message: {}",
        output_payload.error_message.as_deref().unwrap_or("-")
    );
    println!("  stdout:");
    println!("{}", output_payload.stdout.as_deref().unwrap_or(""));
    println!("  stderr:");
    println!("{}", output_payload.stderr.as_deref().unwrap_or(""));
    Ok(())
}

fn cmd_tasks_follow(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if options.json {
        return Err(tasks_follow_machine_mode_error());
    }

    let parsed = parse_tasks_log_options(command_args, "follow")?;
    let poll_ms = parsed.poll_ms.unwrap_or(500);
    let timeout_ms = parsed.timeout_ms.unwrap_or(TASK_FOLLOW_MAX_WAIT_MS);
    let mut last_seen_log_id = 0u64;
    let started_at = Instant::now();
    if find_task_status(store, parsed.task_id)?.is_none() {
        return Err(format!(
            "task '{}' not found in recent task window",
            parsed.task_id
        ));
    }

    println!(
        "Following task #{} logs (poll={}ms)...",
        parsed.task_id, poll_ms
    );
    loop {
        if started_at.elapsed() >= Duration::from_millis(timeout_ms) {
            return Err(format!(
                "timed out while following task '{}' after {} ms",
                parsed.task_id, timeout_ms
            ));
        }

        let entries = store
            .list_task_logs(TaskId(parsed.task_id), parsed.limit)
            .map_err(|error| format!("failed to list task logs: {error}"))?;
        let mut logs = entries
            .into_iter()
            .map(task_log_to_cli_record)
            .collect::<Vec<_>>();
        logs.retain(|record| record.id > last_seen_log_id);
        logs.retain(|record| {
            parsed
                .level_filter
                .as_ref()
                .map(|level| &record.level == level)
                .unwrap_or(true)
                && parsed
                    .status_filter
                    .as_ref()
                    .map(|status| record.status.as_deref() == Some(status.as_str()))
                    .unwrap_or(true)
        });
        logs.sort_by_key(|record| record.id);

        for record in logs {
            if record.id > last_seen_log_id {
                last_seen_log_id = record.id;
            }
            let status = record.status.unwrap_or_else(|| "-".to_string());
            println!(
                "  [{}] [{}] [{}] {}",
                record.created_at_unix, record.level, status, record.message
            );
        }

        let status = find_task_status(store, parsed.task_id)?;
        if status.is_none() {
            return Err(format!(
                "task '{}' disappeared from recent task window while following logs",
                parsed.task_id
            ));
        }
        if matches!(
            status,
            Some(TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled)
        ) {
            println!(
                "Task #{} reached terminal state: {}",
                parsed.task_id,
                task_status_str(status.expect("checked as Some"))
            );
            return Ok(());
        }

        thread::sleep(Duration::from_millis(poll_ms));
    }
}

fn tasks_follow_machine_mode_error() -> String {
    mark_exit_code(TASKS_FOLLOW_MACHINE_MODE_UNSUPPORTED_ERROR, 1)
}

fn cmd_tasks_cancel(options: GlobalOptions, command_args: &[String]) -> Result<(), String> {
    let task_id = parse_task_id_argument(command_args, "tasks cancel")?;

    match coordinator_cancel_task(task_id) {
        Ok(()) => {
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.tasks.cancel",
                    json!({
                        "task_id": task_id,
                        "accepted": true
                    }),
                );
            } else {
                println!("Task '{}' cancel request accepted.", task_id);
            }
            Ok(())
        }
        Err(error) => {
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.tasks.cancel",
                    json!({
                        "task_id": task_id,
                        "accepted": false,
                        "reason": error
                    }),
                );
            }
            Err(format!("tasks cancel for '{}' failed: {}", task_id, error))
        }
    }
}

fn cmd_managers(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() || command_args[0] == "list" {
        return cmd_managers_list(store.as_ref(), options);
    }

    match command_args[0].as_str() {
        "detect" => cmd_managers_detect(store.clone(), options, &command_args[1..]),
        "executables" => cmd_managers_executables(store.as_ref(), options, &command_args[1..]),
        "install-methods" => {
            cmd_managers_install_methods(store.as_ref(), options, &command_args[1..])
        }
        "instances" => cmd_managers_instances(store.as_ref(), options, &command_args[1..]),
        "priority" => cmd_managers_priority(store.as_ref(), options, &command_args[1..]),
        "install" | "update" | "uninstall" => cmd_managers_mutation(
            store.clone(),
            options,
            command_args[0].as_str(),
            &command_args[1..],
        ),
        "show" => {
            if command_args.len() < 2 {
                return Err("managers show requires a manager id".to_string());
            }
            let manager_id = parse_manager_id(&command_args[1])?;
            let managers = list_managers(store.as_ref())?;
            let row = managers
                .into_iter()
                .find(|row| row.manager_id == manager_id.as_str())
                .ok_or_else(|| format!("manager '{}' not found", command_args[1]))?;
            if options.json {
                emit_json_payload("helm.cli.v1.managers.show", json!(row));
            } else {
                println!("Manager: {}", row.manager_id);
                println!("  display_name: {}", row.display_name);
                println!("  authority: {}", row.authority);
                println!("  enabled: {}", row.enabled);
                println!("  detected: {}", row.detected);
                println!("  version: {}", row.version.as_deref().unwrap_or("-"));
                println!(
                    "  executable_path: {}",
                    row.executable_path.as_deref().unwrap_or("-")
                );
                println!(
                    "  selected_executable_path: {}",
                    row.selected_executable_path.as_deref().unwrap_or("-")
                );
                println!(
                    "  selected_executable_differs_from_default: {}",
                    row.selected_executable_differs_from_default
                );
                println!(
                    "  executable_path_diagnostic: {}",
                    row.executable_path_diagnostic
                );
                println!(
                    "  selected_install_method: {}",
                    row.selected_install_method.as_deref().unwrap_or("-")
                );
                println!("  install_instance_count: {}", row.install_instance_count);
                println!("  multi_instance_state: {}", row.multi_instance_state);
                println!(
                    "  multi_instance_acknowledged: {}",
                    row.multi_instance_acknowledged
                );
                println!(
                    "  multi_instance_fingerprint: {}",
                    row.multi_instance_fingerprint.as_deref().unwrap_or("-")
                );
                println!(
                    "  active_provenance: {}",
                    row.active_provenance.as_deref().unwrap_or("-")
                );
                println!(
                    "  active_confidence: {}",
                    row.active_confidence
                        .map(|value| format!("{value:.2}"))
                        .unwrap_or_else(|| "-".to_string())
                );
                println!(
                    "  active_decision_margin: {}",
                    row.active_decision_margin
                        .map(|value| format!("{value:.2}"))
                        .unwrap_or_else(|| "-".to_string())
                );
                println!(
                    "  active_automation_level: {}",
                    row.active_automation_level.as_deref().unwrap_or("-")
                );
                println!(
                    "  active_uninstall_strategy: {}",
                    row.active_uninstall_strategy.as_deref().unwrap_or("-")
                );
                println!(
                    "  active_update_strategy: {}",
                    row.active_update_strategy.as_deref().unwrap_or("-")
                );
                println!(
                    "  active_remediation_strategy: {}",
                    row.active_remediation_strategy.as_deref().unwrap_or("-")
                );
                println!(
                    "  active_explanation_primary: {}",
                    row.active_explanation_primary.as_deref().unwrap_or("-")
                );
                println!(
                    "  active_explanation_secondary: {}",
                    row.active_explanation_secondary.as_deref().unwrap_or("-")
                );
                println!(
                    "  competing_provenance: {}",
                    row.competing_provenance.as_deref().unwrap_or("-")
                );
                println!(
                    "  competing_confidence: {}",
                    row.competing_confidence
                        .map(|value| format!("{value:.2}"))
                        .unwrap_or_else(|| "-".to_string())
                );
                println!("  eligible: {}", row.is_eligible);
                if !row.is_eligible {
                    println!(
                        "  ineligible_reason_code: {}",
                        row.ineligible_reason_code.as_deref().unwrap_or("-")
                    );
                    println!(
                        "  ineligible_reason_message: {}",
                        row.ineligible_reason_message.as_deref().unwrap_or("-")
                    );
                }
            }
            Ok(())
        }
        "enable" | "disable" => {
            if command_args.len() < 2 {
                return Err(format!(
                    "managers {} requires a manager id",
                    command_args[0]
                ));
            }
            let manager_id = parse_manager_id(&command_args[1])?;
            let enabled = command_args[0] == "enable";
            if enabled {
                let eligibility =
                    manager_enablement_eligibility_for_store(store.as_ref(), manager_id)?;
                if !eligibility.is_eligible {
                    let reason = eligibility.reason_message.unwrap_or(
                        "manager is not eligible to be enabled with the current executable selection",
                    );
                    let code = eligibility.reason_code.unwrap_or("manager.ineligible");
                    return Err(format!("{reason} (reason_code={code})"));
                }
            } else {
                let enabled_map = manager_enabled_map(store.as_ref())?;
                let dependents =
                    enabled_dependents_for_manager(store.as_ref(), &enabled_map, manager_id)?;
                if !dependents.is_empty() {
                    return Err(format!(
                        "cannot disable manager '{}': enabled managers depend on it ({})",
                        manager_id.as_str(),
                        dependents
                            .iter()
                            .map(|id| id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            }
            store
                .set_manager_enabled(manager_id, enabled)
                .map_err(|error| format!("failed to set manager enabled state: {error}"))?;
            let (cancelled_task_ids, cancellation_errors) = if enabled {
                (Vec::new(), Vec::new())
            } else {
                cancel_inflight_tasks_for_manager(store.as_ref(), manager_id)
            };

            if options.json {
                emit_json_payload(
                    "helm.cli.v1.managers.set_enabled",
                    json!({
                        "manager_id": manager_id.as_str(),
                        "enabled": enabled,
                        "cancelled_task_ids": cancelled_task_ids,
                        "cancellation_errors": cancellation_errors
                    }),
                );
            } else {
                println!(
                    "Manager {} is now {}.",
                    manager_id.as_str(),
                    if enabled { "enabled" } else { "disabled" }
                );
                if !cancelled_task_ids.is_empty() {
                    println!(
                        "  cancelled tasks: {}",
                        cancelled_task_ids
                            .iter()
                            .map(|task_id| task_id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                if !cancellation_errors.is_empty() {
                    println!("  cancellation warnings:");
                    for warning in cancellation_errors {
                        println!("    - {}", warning);
                    }
                }
            }
            Ok(())
        }
        _ => Err(format!(
            "unsupported managers subcommand '{}'; currently supported: list, show, detect, enable, disable, install, update, uninstall, executables, install-methods, instances, priority",
            command_args[0]
        )),
    }
}

fn cmd_managers_list(store: &SqliteStore, options: GlobalOptions) -> Result<(), String> {
    let managers = list_managers(store)?;
    if options.json {
        emit_json_payload(
            "helm.cli.v1.managers.list",
            json!({
                "managers": managers
            }),
        );
        return Ok(());
    }

    if managers.is_empty() {
        println!("No managers found.");
        return Ok(());
    }

    println!("Managers");
    for manager in managers {
        let state = if manager.enabled {
            "enabled"
        } else {
            "disabled"
        };
        let detected = if manager.detected {
            "detected"
        } else {
            "not-detected"
        };
        let version = manager.version.as_deref().unwrap_or("unknown-version");
        let executable = manager
            .selected_executable_path
            .as_deref()
            .or(manager.executable_path.as_deref())
            .unwrap_or("-");
        let method = manager.selected_install_method.as_deref().unwrap_or("-");
        let provenance = manager.active_provenance.as_deref().unwrap_or("-");
        let confidence = manager
            .active_confidence
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string());
        let margin = manager
            .active_decision_margin
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string());
        let flags = format!(
            "{}{}{}",
            if manager.is_detection_only {
                " detection-only"
            } else {
                ""
            },
            if manager.is_optional { " optional" } else { "" },
            if !manager.is_implemented {
                " not-implemented"
            } else {
                ""
            }
        );
        println!(
            "  {} [{}|{}] {} exec={} method={} prov={} conf={} margin={} inst={} multi={} exec_diag={}{}",
            manager.manager_id,
            state,
            detected,
            version,
            executable,
            method,
            provenance,
            confidence,
            margin,
            manager.install_instance_count,
            manager.multi_instance_state,
            manager.executable_path_diagnostic,
            flags
        );
    }
    Ok(())
}

fn cmd_managers_detect(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let target = parse_manager_target(command_args, "managers detect")?;
    verbose_log(format!(
        "manager detection requested target={:?} mode={:?}",
        target, options.execution_mode
    ));

    match target {
        ManagerTarget::All => {
            if options.execution_mode == ExecutionMode::Detach {
                let response = coordinator_start_workflow(
                    store.as_ref(),
                    CoordinatorWorkflowRequest::DetectAll,
                    options.execution_mode,
                )?;
                let job_id = response
                    .job_id
                    .ok_or_else(|| "coordinator workflow response missing job id".to_string())?;
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.managers.detect.detach",
                        json!({
                            "accepted": true,
                            "mode": "detach",
                            "scope": "all",
                            "job_id": job_id
                        }),
                    );
                } else {
                    println!(
                        "Detection workflow submitted for all managers (job {}).",
                        job_id
                    );
                }
                return Ok(());
            }
            let tokio_runtime = cli_tokio_runtime()?;
            let runtime = build_adapter_runtime(store.clone())?;
            let rows = tokio_runtime.block_on(detect_all_no_timeout(&runtime));
            let failures = emit_manager_results(
                options,
                "helm.cli.v1.managers.detect.all",
                "Detection completed",
                rows,
            );
            if let Some(error) = manager_operation_failure_error("detection", failures) {
                return Err(error);
            }
            Ok(())
        }
        ManagerTarget::One(manager) => {
            let response = coordinator_submit_request(
                store.as_ref(),
                manager,
                CoordinatorSubmitRequest::Detect,
                options.execution_mode,
            )?;
            let task_id = response
                .task_id
                .ok_or_else(|| "coordinator response missing task id".to_string())?;

            if options.execution_mode == ExecutionMode::Detach {
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.managers.detect.detach",
                        json!({
                            "task_id": task_id,
                            "manager_id": manager.as_str(),
                            "accepted": true,
                            "mode": "detach"
                        }),
                    );
                } else {
                    println!(
                        "Detection submitted for '{}' (task #{})",
                        manager.as_str(),
                        task_id
                    );
                }
                return Ok(());
            }

            match response.payload {
                Some(CoordinatorPayload::Detection {
                    installed,
                    version,
                    executable_path,
                }) => {
                    let info = DetectionInfo {
                        installed,
                        version,
                        executable_path: executable_path.map(PathBuf::from),
                    };
                    emit_detection_result(options, TaskId(task_id), manager, &info);
                    Ok(())
                }
                _ => Err(format!(
                    "detection task {} completed with unexpected coordinator payload",
                    task_id
                )),
            }
        }
    }
}

fn cmd_managers_mutation(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    subcommand: &str,
    command_args: &[String],
) -> Result<(), String> {
    let parsed = parse_manager_mutation_args(subcommand, command_args)?;
    let manager = parsed.manager;
    let install_method_override = if subcommand == "install" {
        resolve_install_method_override_for_install(
            store.as_ref(),
            manager,
            &options,
            parsed.install_method_override.clone(),
        )?
    } else {
        parsed.install_method_override.clone()
    };

    let (target_manager, request, uninstall_preview) = if subcommand == "uninstall" {
        if parsed.preview {
            let preview_plan = build_manager_uninstall_plan_with_options(
                store.as_ref(),
                manager,
                parsed.allow_unknown_provenance,
                true,
                parsed.uninstall_options.clone(),
            )?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.managers.uninstall.preview",
                    json!({ "preview": preview_plan.preview }),
                );
            } else {
                print_manager_uninstall_preview(&preview_plan.preview);
            }
            if !parsed.yes {
                return Ok(());
            }
        }

        let plan = build_manager_uninstall_plan_with_options(
            store.as_ref(),
            manager,
            parsed.allow_unknown_provenance,
            false,
            parsed.uninstall_options.clone(),
        )?;
        if plan.preview.requires_yes && !parsed.yes {
            return Err(
                "managers uninstall requires --yes for this blast radius. Run 'helm managers uninstall <manager-id> --preview' first, then rerun with --yes."
                    .to_string(),
            );
        }

        (plan.target_manager, plan.request, Some(plan.preview))
    } else {
        let (target_manager, request) = build_manager_mutation_request_with_options(
            store.as_ref(),
            manager,
            subcommand,
            install_method_override,
            parsed.install_options.clone(),
        )?;
        (target_manager, request, None)
    };

    let submit_request = adapter_request_to_coordinator_submit(request)?;
    let response = coordinator_submit_request(
        store.as_ref(),
        target_manager,
        submit_request,
        options.execution_mode,
    )?;
    let task_id = response
        .task_id
        .ok_or_else(|| "coordinator response missing task id".to_string())?;

    if options.execution_mode == ExecutionMode::Detach {
        if options.json {
            emit_json_payload(
                &format!("helm.cli.v1.managers.{}", subcommand),
                json!({
                    "task_id": task_id,
                    "manager_id": manager.as_str(),
                    "target_manager_id": target_manager.as_str(),
                    "action": subcommand,
                    "accepted": true,
                    "mode": "detach",
                    "uninstall_preview": uninstall_preview
                }),
            );
        } else {
            println!(
                "Manager '{}' {} submitted via '{}' (task #{})",
                manager.as_str(),
                subcommand,
                target_manager.as_str(),
                task_id
            );
            if let Some(preview) = uninstall_preview.as_ref() {
                println!(
                    "  blast_radius_score: {} (requires_yes={})",
                    preview.blast_radius_score, preview.requires_yes
                );
            }
        }
        return Ok(());
    }

    match response.payload {
        Some(CoordinatorPayload::Mutation {
            manager_id: target_manager_id,
            package_name,
            action,
            before_version,
            after_version,
        }) => {
            if options.json {
                emit_json_payload(
                    &format!("helm.cli.v1.managers.{}", subcommand),
                    json!({
                        "task_id": task_id,
                        "manager_id": manager.as_str(),
                        "target_manager_id": target_manager_id,
                        "package_name": package_name,
                        "action": action,
                        "before_version": before_version,
                        "after_version": after_version,
                        "uninstall_preview": uninstall_preview
                    }),
                );
            } else {
                println!(
                    "Manager '{}' {} submitted via '{}' (task #{})",
                    manager.as_str(),
                    subcommand,
                    target_manager.as_str(),
                    task_id
                );
                if let Some(preview) = uninstall_preview.as_ref() {
                    println!(
                        "  blast_radius_score: {} (requires_yes={})",
                        preview.blast_radius_score, preview.requires_yes
                    );
                }
            }
            Ok(())
        }
        _ => Err(format!(
            "managers {} returned unexpected coordinator payload",
            subcommand
        )),
    }
}

fn print_manager_uninstall_preview(preview: &ManagerUninstallPreview) {
    println!("Manager Uninstall Preview");
    println!("  manager_id: {}", preview.requested_manager_id);
    println!("  target_manager_id: {}", preview.target_manager_id);
    println!("  package_name: {}", preview.package_name);
    println!("  strategy: {}", preview.strategy);
    println!(
        "  provenance: {}",
        preview.provenance.as_deref().unwrap_or("-")
    );
    println!(
        "  automation_level: {}",
        preview.automation_level.as_deref().unwrap_or("-")
    );
    println!(
        "  confidence: {}",
        preview
            .confidence
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  decision_margin: {}",
        preview
            .decision_margin
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );
    println!("  blast_radius_score: {}", preview.blast_radius_score);
    println!("  requires_yes: {}", preview.requires_yes);
    println!(
        "  confidence_requires_confirmation: {}",
        preview.confidence_requires_confirmation
    );
    println!("  unknown_provenance: {}", preview.unknown_provenance);
    println!(
        "  unknown_override_required: {}",
        preview.unknown_override_required
    );
    println!("  used_unknown_override: {}", preview.used_unknown_override);
    println!("  legacy_fallback_used: {}", preview.legacy_fallback_used);
    println!("  read_only_blocked: {}", preview.read_only_blocked);
    println!(
        "  explanation_primary: {}",
        preview.explanation_primary.as_deref().unwrap_or("-")
    );
    println!(
        "  explanation_secondary: {}",
        preview.explanation_secondary.as_deref().unwrap_or("-")
    );
    println!(
        "  competing_provenance: {}",
        preview.competing_provenance.as_deref().unwrap_or("-")
    );
    println!(
        "  competing_confidence: {}",
        preview
            .competing_confidence
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );
    if preview.summary_lines.is_empty() {
        println!("  summary_lines: -");
    } else {
        println!("  summary_lines:");
        for line in &preview.summary_lines {
            println!("    - {line}");
        }
    }
    if preview.files_removed.is_empty() {
        println!("  files_removed: -");
    } else {
        println!("  files_removed:");
        for entry in &preview.files_removed {
            println!(
                "    - {} ({})",
                entry.path,
                if entry.exists { "exists" } else { "missing" }
            );
        }
    }
    if preview.directories_removed.is_empty() {
        println!("  directories_removed: -");
    } else {
        println!("  directories_removed:");
        for entry in &preview.directories_removed {
            println!(
                "    - {} ({})",
                entry.path,
                if entry.exists { "exists" } else { "missing" }
            );
        }
    }
    if preview.secondary_effects.is_empty() {
        println!("  secondary_effects: -");
    } else {
        println!("  secondary_effects:");
        for effect in &preview.secondary_effects {
            println!("    - {}", effect);
        }
    }
}

fn print_package_uninstall_preview(preview: &PackageUninstallPreview) {
    println!("Package Uninstall Preview");
    println!("  manager_id: {}", preview.manager_id);
    println!("  package_name: {}", preview.package_name);
    println!("  blast_radius_score: {}", preview.blast_radius_score);
    println!("  requires_yes: {}", preview.requires_yes);
    println!(
        "  confidence_requires_confirmation: {}",
        preview.confidence_requires_confirmation
    );
    println!(
        "  manager_provenance: {}",
        preview.manager_provenance.as_deref().unwrap_or("-")
    );
    println!(
        "  manager_automation_level: {}",
        preview.manager_automation_level.as_deref().unwrap_or("-")
    );
    println!(
        "  manager_uninstall_strategy: {}",
        preview.manager_uninstall_strategy.as_deref().unwrap_or("-")
    );
    println!(
        "  explanation_primary: {}",
        preview.explanation_primary.as_deref().unwrap_or("-")
    );
    println!(
        "  explanation_secondary: {}",
        preview.explanation_secondary.as_deref().unwrap_or("-")
    );
    println!(
        "  competing_provenance: {}",
        preview.competing_provenance.as_deref().unwrap_or("-")
    );
    println!(
        "  competing_confidence: {}",
        preview
            .competing_confidence
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );
    if preview.summary_lines.is_empty() {
        println!("  summary_lines: -");
    } else {
        println!("  summary_lines:");
        for line in &preview.summary_lines {
            println!("    - {line}");
        }
    }
    if preview.files_removed.is_empty() {
        println!("  files_removed: -");
    } else {
        println!("  files_removed:");
        for entry in &preview.files_removed {
            println!(
                "    - {} ({})",
                entry.path,
                if entry.exists { "exists" } else { "missing" }
            );
        }
    }
    if preview.directories_removed.is_empty() {
        println!("  directories_removed: -");
    } else {
        println!("  directories_removed:");
        for entry in &preview.directories_removed {
            println!(
                "    - {} ({})",
                entry.path,
                if entry.exists { "exists" } else { "missing" }
            );
        }
    }
    if preview.secondary_effects.is_empty() {
        println!("  secondary_effects: -");
    } else {
        println!("  secondary_effects:");
        for effect in &preview.secondary_effects {
            println!("    - {}", effect);
        }
    }
}

fn cmd_managers_executables(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return Err(
            "managers executables requires a subcommand: list <manager-id> or set <manager-id> <path|path-default>".to_string(),
        );
    }

    match command_args[0].as_str() {
        "list" => {
            if command_args.len() != 2 {
                return Err("managers executables list requires exactly one manager id".to_string());
            }
            let manager = parse_manager_id(&command_args[1])?;
            let details = manager_executable_status(store, manager)?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.managers.executables.list",
                    json!({ "manager": details }),
                );
                return Ok(());
            }

            println!("Manager executables: {}", details.manager_id);
            println!(
                "  active_executable_path: {}",
                details.active_executable_path.as_deref().unwrap_or("-")
            );
            println!(
                "  default_executable_path: {}",
                details.default_executable_path.as_deref().unwrap_or("-")
            );
            println!(
                "  selected_executable_path: {}",
                details.selected_executable_path.as_deref().unwrap_or("-")
            );
            println!(
                "  selected_executable_differs_from_default: {}",
                details.selected_executable_differs_from_default
            );
            println!(
                "  executable_path_diagnostic: {}",
                details.executable_path_diagnostic
            );
            if details.executable_paths.is_empty() {
                println!("  executable_paths: -");
            } else {
                println!("  executable_paths:");
                for path in details.executable_paths {
                    println!("    - {path}");
                }
            }
            Ok(())
        }
        "set" => {
            if command_args.len() != 3 {
                return Err(
                    "managers executables set requires <manager-id> and <path|path-default>"
                        .to_string(),
                );
            }
            let manager = parse_manager_id(&command_args[1])?;
            let selected_path = parse_selected_executable_arg(&command_args[2])?;
            let selected_ref = selected_path.as_deref();
            store
                .set_manager_selected_executable_path(manager, selected_ref)
                .map_err(|error| format!("failed to set selected executable path: {error}"))?;

            if options.json {
                emit_json_payload(
                    "helm.cli.v1.managers.executables.set",
                    json!({
                        "manager_id": manager.as_str(),
                        "selected_executable_path": selected_path
                    }),
                );
            } else if let Some(path) = selected_ref {
                println!(
                    "Manager '{}' selected executable path set to '{}'.",
                    manager.as_str(),
                    path
                );
            } else {
                println!(
                    "Manager '{}' executable path reset to default resolution.",
                    manager.as_str()
                );
            }
            Ok(())
        }
        _ => Err(format!(
            "unsupported managers executables subcommand '{}'; currently supported: list, set",
            command_args[0]
        )),
    }
}

fn cmd_managers_install_methods(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return Err(
            "managers install-methods requires a subcommand: list <manager-id> or set <manager-id> <method-id|default>".to_string(),
        );
    }

    match command_args[0].as_str() {
        "list" => {
            if command_args.len() != 2 {
                return Err(
                    "managers install-methods list requires exactly one manager id".to_string(),
                );
            }
            let manager = parse_manager_id(&command_args[1])?;
            let details = manager_install_methods_status(store, manager)?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.managers.install_methods.list",
                    json!({ "manager": details }),
                );
                return Ok(());
            }

            println!("Manager install methods: {}", details.manager_id);
            println!(
                "  selected_install_method: {}",
                details.selected_install_method.as_deref().unwrap_or("-")
            );
            if details.install_methods.is_empty() {
                println!("  install_methods: -");
            } else {
                println!("  install_methods:");
                for method in details.install_methods {
                    println!("    - {method}");
                }
            }
            Ok(())
        }
        "set" => {
            if command_args.len() != 3 {
                return Err(
                    "managers install-methods set requires <manager-id> and <method-id|default>"
                        .to_string(),
                );
            }
            let manager = parse_manager_id(&command_args[1])?;
            let selected_method = parse_selected_install_method_arg(manager, &command_args[2])?;
            let selected_ref = selected_method.as_deref();
            if let Some(method) = selected_ref
                && !manager_install_method_allowed_for_selection(manager, method)
            {
                return Err(format!(
                    "manager '{}' install method '{}' is blocked by managed policy",
                    manager.as_str(),
                    method
                ));
            }
            store
                .set_manager_selected_install_method(manager, selected_ref)
                .map_err(|error| format!("failed to set selected install method: {error}"))?;

            if options.json {
                emit_json_payload(
                    "helm.cli.v1.managers.install_methods.set",
                    json!({
                        "manager_id": manager.as_str(),
                        "selected_install_method": selected_method
                    }),
                );
            } else if let Some(method) = selected_ref {
                println!(
                    "Manager '{}' selected install method set to '{}'.",
                    manager.as_str(),
                    method
                );
            } else {
                println!(
                    "Manager '{}' install method reset to default resolution.",
                    manager.as_str()
                );
            }
            Ok(())
        }
        _ => Err(format!(
            "unsupported managers install-methods subcommand '{}'; currently supported: list, set",
            command_args[0]
        )),
    }
}

fn cmd_managers_instances(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if let Some(subcommand) = command_args.first().map(String::as_str) {
        match subcommand {
            "ack" => {
                if command_args.len() != 2 {
                    return Err(
                        "managers instances ack requires exactly one manager id".to_string()
                    );
                }
                let manager = parse_manager_id(&command_args[1])?;
                let message = acknowledge_manager_multi_instance_state(store, manager)?;
                let manager_status = list_managers(store)?
                    .into_iter()
                    .find(|row| row.manager_id == manager.as_str())
                    .ok_or_else(|| format!("manager '{}' not found", manager.as_str()))?;
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.managers.instances.ack",
                        json!({
                            "manager_id": manager.as_str(),
                            "acknowledged": true,
                            "multi_instance_state": manager_status.multi_instance_state,
                            "multi_instance_acknowledged": manager_status.multi_instance_acknowledged,
                            "multi_instance_fingerprint": manager_status.multi_instance_fingerprint
                        }),
                    );
                } else {
                    println!("{message}");
                }
                return Ok(());
            }
            "clear-ack" => {
                if command_args.len() != 2 {
                    return Err(
                        "managers instances clear-ack requires exactly one manager id".to_string(),
                    );
                }
                let manager = parse_manager_id(&command_args[1])?;
                let message = clear_manager_multi_instance_ack(store, manager)?;
                let manager_status = list_managers(store)?
                    .into_iter()
                    .find(|row| row.manager_id == manager.as_str())
                    .ok_or_else(|| format!("manager '{}' not found", manager.as_str()))?;
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.managers.instances.clear_ack",
                        json!({
                            "manager_id": manager.as_str(),
                            "acknowledged": false,
                            "multi_instance_state": manager_status.multi_instance_state,
                            "multi_instance_acknowledged": manager_status.multi_instance_acknowledged,
                            "multi_instance_fingerprint": manager_status.multi_instance_fingerprint
                        }),
                    );
                } else {
                    println!("{message}");
                }
                return Ok(());
            }
            "set-active" => {
                if command_args.len() != 3 {
                    return Err(
                        "managers instances set-active requires <manager-id> and <instance-id>"
                            .to_string(),
                    );
                }
                let manager = parse_manager_id(&command_args[1])?;
                let instance_id = command_args[2].trim();
                let message = set_manager_active_install_instance(store, manager, instance_id)?;
                let manager_status = list_managers(store)?
                    .into_iter()
                    .find(|row| row.manager_id == manager.as_str())
                    .ok_or_else(|| format!("manager '{}' not found", manager.as_str()))?;
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.managers.instances.set_active",
                        json!({
                            "manager_id": manager.as_str(),
                            "instance_id": instance_id,
                            "selected_executable_path": manager_status.selected_executable_path,
                            "multi_instance_state": manager_status.multi_instance_state,
                            "multi_instance_acknowledged": manager_status.multi_instance_acknowledged
                        }),
                    );
                } else {
                    println!("{message}");
                }
                return Ok(());
            }
            _ => {}
        }
    }

    if command_args.len() > 1 {
        return Err("managers instances accepts zero or one manager id".to_string());
    }

    let manager_filter = if let Some(raw) = command_args.first() {
        Some(parse_manager_id(raw)?)
    } else {
        None
    };

    let instances = list_manager_install_instances(store, manager_filter)?;

    if options.json {
        emit_json_payload(
            "helm.cli.v1.managers.instances",
            json!({
                "manager_id": manager_filter.as_ref().map(|manager| manager.as_str().to_string()),
                "instances": instances
            }),
        );
        return Ok(());
    }

    match manager_filter {
        Some(manager) => println!("Manager instances: {}", manager.as_str()),
        None => println!("Manager instances"),
    }

    if instances.is_empty() {
        println!("  -");
        return Ok(());
    }

    let manager_multi_instance_state: HashMap<String, (String, bool)> = list_managers(store)?
        .into_iter()
        .map(|manager| {
            (
                manager.manager_id,
                (
                    manager.multi_instance_state,
                    manager.multi_instance_acknowledged,
                ),
            )
        })
        .collect();

    for instance in instances {
        let (multi_state, multi_acknowledged) = manager_multi_instance_state
            .get(&instance.manager_id)
            .map(|(state, acknowledged)| (state.as_str(), *acknowledged))
            .unwrap_or(("none", false));
        println!(
            "  {} [{}] prov={} conf={:.2} margin={} automation={} active={} multi={} ack={}",
            instance.manager_id,
            instance.instance_id,
            instance.provenance,
            instance.confidence,
            instance
                .decision_margin
                .map(|value| format!("{value:.2}"))
                .unwrap_or_else(|| "-".to_string()),
            instance.automation_level,
            instance.is_active,
            multi_state,
            multi_acknowledged
        );
        println!(
            "    path={} canonical={}",
            instance.display_path,
            instance.canonical_path.as_deref().unwrap_or("-")
        );
        println!(
            "    identity={}({}) uninstall={} update={} remediation={}",
            instance.identity_kind,
            instance.identity_value,
            instance.uninstall_strategy,
            instance.update_strategy,
            instance.remediation_strategy
        );
        if !instance.alias_paths.is_empty() {
            println!("    aliases={}", instance.alias_paths.join(", "));
        }
        if let Some(primary) = instance.explanation_primary.as_deref() {
            println!("    why={primary}");
        }
        if let Some(secondary) = instance.explanation_secondary.as_deref() {
            println!("    why2={secondary}");
        }
        if let Some(competing) = instance.competing_provenance.as_deref() {
            let competing_confidence = instance
                .competing_confidence
                .map(|value| format!("{value:.2}"))
                .unwrap_or_else(|| "-".to_string());
            println!("    competing={competing} ({competing_confidence})");
        }
    }

    Ok(())
}

fn cmd_managers_priority(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() || command_args[0] == "list" {
        let rows = manager_priority_entries(store)?;
        if options.json {
            emit_json_payload(
                "helm.cli.v1.managers.priority.list",
                json!({ "managers": rows }),
            );
            return Ok(());
        }

        if rows.is_empty() {
            println!("No managers found.");
            return Ok(());
        }

        println!("Manager Priority");
        for authority in ["authoritative", "standard", "guarded", "detection_only"] {
            let mut any = false;
            for row in rows.iter().filter(|row| row.authority == authority) {
                if !any {
                    println!("  {authority}:");
                    any = true;
                }
                println!(
                    "    {:>2}. {}{}{}",
                    row.rank + 1,
                    row.manager_id,
                    if row.detected { "" } else { " [not-detected]" },
                    if row.enabled { "" } else { " [disabled]" }
                );
            }
        }
        return Ok(());
    }

    match command_args[0].as_str() {
        "set" => {
            let (manager, rank) = parse_manager_priority_set_args(&command_args[1..])?;
            let effective_rank = set_manager_priority_rank(store, manager, rank)?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.managers.priority.set",
                    json!({
                        "manager_id": manager.as_str(),
                        "rank": effective_rank
                    }),
                );
            } else {
                println!(
                    "Manager '{}' priority set to rank {}.",
                    manager.as_str(),
                    effective_rank + 1
                );
            }
            Ok(())
        }
        "reset" => {
            store
                .set_manager_priority_overrides_json(None)
                .map_err(|error| format!("failed to reset manager priority overrides: {error}"))?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.managers.priority.reset",
                    json!({ "reset": true }),
                );
            } else {
                println!("Manager priority overrides reset to defaults.");
            }
            Ok(())
        }
        _ => Err(format!(
            "unsupported managers priority subcommand '{}'; currently supported: list, set, reset",
            command_args[0]
        )),
    }
}

fn cmd_settings(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() || command_args[0] == "list" {
        return cmd_settings_list(store, options);
    }

    match command_args[0].as_str() {
        "get" => {
            if command_args.len() < 2 {
                return Err("settings get requires a key".to_string());
            }
            let key = command_args[1].as_str();
            let value = read_setting(store, key)?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.settings.get",
                    json!({
                        "key": key,
                        "value": value
                    }),
                );
            } else {
                println!("{key}={value}");
            }
            Ok(())
        }
        "set" => {
            if command_args.len() < 3 {
                return Err("settings set requires a key and value".to_string());
            }
            let key = command_args[1].as_str();
            let value = command_args[2].as_str();
            write_setting(store, key, value)?;
            let current = read_setting(store, key)?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.settings.set",
                    json!({
                        "key": key,
                        "value": current
                    }),
                );
            } else {
                println!("{key} set to {current}");
            }
            Ok(())
        }
        "reset" => {
            if command_args.len() < 2 {
                return Err("settings reset requires a key".to_string());
            }
            let key = command_args[1].as_str();
            reset_setting(store, key)?;
            let current = read_setting(store, key)?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.settings.reset",
                    json!({
                        "key": key,
                        "value": current
                    }),
                );
            } else {
                println!("{key} reset to {current}");
            }
            Ok(())
        }
        _ => Err(format!(
            "unsupported settings subcommand '{}'; currently supported: list, get, set, reset",
            command_args[0]
        )),
    }
}

fn cmd_onboarding(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() || is_help_token(&command_args[0]) {
        if options.json {
            emit_help_json_payload(Some(Command::Onboarding), &[], true);
        } else {
            print_onboarding_help();
        }
        return Ok(());
    }

    match command_args[0].as_str() {
        "status" => {
            let state = load_cli_onboarding_state(store)?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.onboarding.status",
                    json!({
                        "completed": state.completed,
                        "license_terms_version": state.accepted_license_terms_version,
                        "current_license_terms_version": CLI_LICENSE_TERMS_VERSION,
                        "license_accepted_for_current_version": state.current_license_accepted()
                    }),
                );
            } else {
                println!("CLI Onboarding");
                println!("  completed: {}", state.completed);
                println!(
                    "  license_terms_version: {}",
                    state
                        .accepted_license_terms_version
                        .as_deref()
                        .unwrap_or("none")
                );
                println!("  current_license_terms_version: {CLI_LICENSE_TERMS_VERSION}");
                println!(
                    "  license_accepted_for_current_version: {}",
                    state.current_license_accepted()
                );
            }
            Ok(())
        }
        "run" => {
            let before = load_cli_onboarding_state(store)?;
            if !before.fully_complete() {
                ensure_cli_onboarding_completed(store, &options)?;
            }
            let after = load_cli_onboarding_state(store)?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.onboarding.run",
                    json!({
                        "completed": after.completed,
                        "license_terms_version": after.accepted_license_terms_version,
                        "license_accepted_for_current_version": after.current_license_accepted()
                    }),
                );
            } else if before.fully_complete() {
                println!("CLI onboarding is already complete.");
            } else {
                println!("CLI onboarding complete.");
            }
            Ok(())
        }
        "reset" => {
            set_cli_onboarding_completed(store, false)?;
            store
                .set_cli_accepted_license_terms_version(None)
                .map_err(|error| {
                    format!("failed to clear CLI accepted license terms version: {error}")
                })?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.onboarding.reset",
                    json!({
                        "completed": false,
                        "license_terms_version": serde_json::Value::Null
                    }),
                );
            } else {
                println!("CLI onboarding state reset.");
            }
            Ok(())
        }
        _ => Err(format!(
            "unsupported onboarding subcommand '{}'; currently supported: status, run, reset",
            command_args[0]
        )),
    }
}

const DEFAULT_CLI_UPDATE_ENDPOINT: &str = "https://helmapp.dev/updates/cli/latest.json";
const SELF_UPDATE_ALLOW_INSECURE_ENV: &str = "HELM_CLI_ALLOW_INSECURE_UPDATE_URLS";
const SELF_UPDATE_ALLOW_ROOT_ENV: &str = "HELM_ALLOW_ROOT_SELF_UPDATE";
const SELF_UPDATE_MAX_DOWNLOAD_BYTES_ENV: &str = "HELM_CLI_SELF_UPDATE_MAX_DOWNLOAD_BYTES";
const SELF_UPDATE_HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;
const SELF_UPDATE_HTTP_READ_TIMEOUT_SECS: u64 = 30;
const SELF_UPDATE_HTTP_WRITE_TIMEOUT_SECS: u64 = 30;
const SELF_UPDATE_MAX_DOWNLOAD_BYTES_DEFAULT: usize = 64 * 1024 * 1024;
const APP_BUNDLE_SHIM_SENTINEL: &str = "# helm-cli-shim: app-bundle";
const DEFAULT_HELM_CLI_SHIM_RELATIVE_PATH: &str = ".local/bin/helm";
const SELF_UPDATE_MAX_REDIRECT_HOPS: usize = 5;
const SELF_UPDATE_ALLOWED_HOSTS: [&str; 5] = [
    "helmapp.dev",
    "github.com",
    "objects.githubusercontent.com",
    "github-releases.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

#[derive(Debug, Clone, Default)]
struct ParsedSelfUpdateArgs {
    check_only: bool,
    force: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct CliUpdateManifest {
    version: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    downloads: CliUpdateDownloads,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CliUpdateDownloads {
    #[serde(default)]
    universal: Option<CliUpdateAsset>,
    #[serde(default)]
    arm64: Option<CliUpdateAsset>,
    #[serde(default, rename = "x86_64")]
    x86_64: Option<CliUpdateAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct CliUpdateAsset {
    url: String,
    sha256: String,
}

#[derive(Debug, Clone)]
struct SelfUpdateCheckStatus {
    checked: bool,
    update_available: Option<bool>,
    latest_version: Option<String>,
    published_at: Option<String>,
    source: String,
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct SelfUpdateApplyResult {
    updated: bool,
    latest_version: Option<String>,
    published_at: Option<String>,
    source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelfUpdateErrorKind {
    UrlPolicy,
    ManifestHttp,
    ManifestTransport,
    ManifestRead,
    ManifestParse,
    ManifestContract,
    AssetHttp,
    AssetTransport,
    AssetRead,
    AssetContract,
    Replace,
    Other,
}

impl SelfUpdateErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::UrlPolicy => "url_policy",
            Self::ManifestHttp => "manifest_http",
            Self::ManifestTransport => "manifest_transport",
            Self::ManifestRead => "manifest_read",
            Self::ManifestParse => "manifest_parse",
            Self::ManifestContract => "manifest_contract",
            Self::AssetHttp => "asset_http",
            Self::AssetTransport => "asset_transport",
            Self::AssetRead => "asset_read",
            Self::AssetContract => "asset_contract",
            Self::Replace => "replace",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone)]
struct SelfUpdateCommandError {
    kind: SelfUpdateErrorKind,
    message: String,
    endpoint: Option<String>,
    asset_url: Option<String>,
    http_status: Option<u16>,
}

impl SelfUpdateCommandError {
    fn new(kind: SelfUpdateErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            endpoint: None,
            asset_url: None,
            http_status: None,
        }
    }

    fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    fn with_asset_url(mut self, url: impl Into<String>) -> Self {
        self.asset_url = Some(url.into());
        self
    }

    fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    fn actionable_guidance(&self, recommended_action: &str) -> String {
        match self.kind {
            SelfUpdateErrorKind::UrlPolicy => {
                "use the official Helm update endpoint and allowlisted release hosts".to_string()
            }
            SelfUpdateErrorKind::ManifestHttp
            | SelfUpdateErrorKind::ManifestTransport
            | SelfUpdateErrorKind::ManifestRead
            | SelfUpdateErrorKind::ManifestParse
            | SelfUpdateErrorKind::ManifestContract => format!(
                "verify CLI update endpoint availability and metadata integrity, then retry; if this persists, {}",
                recommended_action
            ),
            SelfUpdateErrorKind::AssetHttp
            | SelfUpdateErrorKind::AssetTransport
            | SelfUpdateErrorKind::AssetRead
            | SelfUpdateErrorKind::AssetContract => format!(
                "verify download URL/reachability and checksum metadata, then retry; if this persists, {}",
                recommended_action
            ),
            SelfUpdateErrorKind::Replace => {
                "re-run from a user-writable install path or reinstall Helm CLI directly"
                    .to_string()
            }
            SelfUpdateErrorKind::Other => recommended_action.to_string(),
        }
    }
}

impl std::fmt::Display for SelfUpdateCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<String> for SelfUpdateCommandError {
    fn from(value: String) -> Self {
        Self::new(SelfUpdateErrorKind::Other, value)
    }
}

fn current_cli_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn self_update_endpoint() -> String {
    env::var("HELM_CLI_UPDATE_ENDPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CLI_UPDATE_ENDPOINT.to_string())
}

fn self_update_allow_insecure_urls() -> bool {
    env_flag_enabled(SELF_UPDATE_ALLOW_INSECURE_ENV)
}

fn self_update_http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(SELF_UPDATE_HTTP_CONNECT_TIMEOUT_SECS))
        .timeout_read(Duration::from_secs(SELF_UPDATE_HTTP_READ_TIMEOUT_SECS))
        .timeout_write(Duration::from_secs(SELF_UPDATE_HTTP_WRITE_TIMEOUT_SECS))
        .redirects(0)
        .build()
}

fn self_update_max_download_bytes() -> usize {
    let Ok(raw) = env::var(SELF_UPDATE_MAX_DOWNLOAD_BYTES_ENV) else {
        return SELF_UPDATE_MAX_DOWNLOAD_BYTES_DEFAULT;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return SELF_UPDATE_MAX_DOWNLOAD_BYTES_DEFAULT;
    }
    match trimmed.parse::<u64>() {
        Ok(0) => {
            verbose_log(format!(
                "{SELF_UPDATE_MAX_DOWNLOAD_BYTES_ENV} ignored: value must be greater than 0"
            ));
            SELF_UPDATE_MAX_DOWNLOAD_BYTES_DEFAULT
        }
        Ok(value) if value as u128 > usize::MAX as u128 => {
            verbose_log(format!(
                "{SELF_UPDATE_MAX_DOWNLOAD_BYTES_ENV} ignored: value {value} exceeds platform maximum"
            ));
            SELF_UPDATE_MAX_DOWNLOAD_BYTES_DEFAULT
        }
        Ok(value) => value as usize,
        Err(_) => {
            verbose_log(format!(
                "{SELF_UPDATE_MAX_DOWNLOAD_BYTES_ENV} ignored: invalid integer '{}'",
                trimmed
            ));
            SELF_UPDATE_MAX_DOWNLOAD_BYTES_DEFAULT
        }
    }
}

fn is_running_as_root() -> bool {
    if let Ok(euid) = env::var("EUID")
        && euid.trim() == "0"
    {
        return true;
    }

    if let Ok(output) = std::process::Command::new("id").arg("-u").output()
        && output.status.success()
    {
        let rendered = String::from_utf8_lossy(&output.stdout);
        if rendered.trim() == "0" {
            return true;
        }
    }

    false
}

fn parse_url_scheme_host(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    let (scheme, remainder) = trimmed.split_once("://")?;
    if scheme.is_empty() || remainder.is_empty() {
        return None;
    }
    let authority = remainder.split('/').next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let host = authority.split(':').next()?.trim();
    if host.is_empty() {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), host.to_ascii_lowercase()))
}

fn parse_url_base(raw: &str) -> Option<(String, String, String)> {
    let trimmed = raw.trim();
    let (scheme, remainder) = trimmed.split_once("://")?;
    if scheme.is_empty() || remainder.is_empty() {
        return None;
    }
    let (authority, path_with_query) = match remainder.split_once('/') {
        Some((authority, path)) => (authority, format!("/{}", path)),
        None => (remainder, "/".to_string()),
    };
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    Some((
        scheme.to_ascii_lowercase(),
        authority.to_string(),
        path_with_query,
    ))
}

fn resolve_redirect_url(current_url: &str, location: &str) -> Option<String> {
    let location = location.trim();
    if location.is_empty() {
        return None;
    }
    if location.starts_with("http://")
        || location.starts_with("https://")
        || location.starts_with("file://")
    {
        return Some(location.to_string());
    }

    let (scheme, authority, path_with_query) = parse_url_base(current_url)?;
    if location.starts_with("//") {
        return Some(format!("{scheme}:{location}"));
    }
    if location.starts_with('/') {
        return Some(format!("{scheme}://{authority}{location}"));
    }

    let path_only = path_with_query.split(['?', '#']).next().unwrap_or("/");
    let base_dir = path_only.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
    if base_dir.is_empty() {
        Some(format!("{scheme}://{authority}/{location}"))
    } else {
        Some(format!("{scheme}://{authority}{base_dir}/{location}"))
    }
}

fn is_http_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn resolve_update_redirect_target(
    current_url: &str,
    location: &str,
    field_name: &'static str,
    endpoint: Option<&str>,
    error_kind: SelfUpdateErrorKind,
) -> Result<String, SelfUpdateCommandError> {
    let Some(resolved) = resolve_redirect_url(current_url, location) else {
        let mut error = SelfUpdateCommandError::new(
            error_kind,
            format!(
                "self-update {field_name} redirect location is invalid: '{}'",
                location
            ),
        )
        .with_asset_url(current_url.to_string());
        if let Some(endpoint) = endpoint {
            error = error.with_endpoint(endpoint.to_string());
        }
        return Err(error);
    };

    validate_update_url(&resolved, field_name, endpoint)?;
    Ok(resolved)
}

fn file_url_path(url: &str) -> Option<PathBuf> {
    let trimmed = url.trim();
    let raw = trimmed.strip_prefix("file://")?;
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

fn is_allowed_update_host(host: &str) -> bool {
    SELF_UPDATE_ALLOWED_HOSTS
        .iter()
        .any(|candidate| host.eq_ignore_ascii_case(candidate))
}

fn validate_update_url(
    url: &str,
    field_name: &'static str,
    endpoint: Option<&str>,
) -> Result<(), SelfUpdateCommandError> {
    let trimmed = url.trim();
    if trimmed.starts_with("file://") {
        if self_update_allow_insecure_urls() {
            return Ok(());
        }
        let mut error = SelfUpdateCommandError::new(
            SelfUpdateErrorKind::UrlPolicy,
            format!(
                "self-update {field_name} URL uses file:// but {SELF_UPDATE_ALLOW_INSECURE_ENV}=1 is not set"
            ),
        )
        .with_asset_url(url.to_string());
        if let Some(endpoint) = endpoint {
            error = error.with_endpoint(endpoint.to_string());
        }
        return Err(error);
    }

    let Some((scheme, host)) = parse_url_scheme_host(url) else {
        let mut error = SelfUpdateCommandError::new(
            SelfUpdateErrorKind::UrlPolicy,
            format!("self-update {field_name} URL is invalid: '{url}'"),
        )
        .with_asset_url(url.to_string());
        if let Some(endpoint) = endpoint {
            error = error.with_endpoint(endpoint.to_string());
        }
        return Err(error);
    };

    if scheme != "https" {
        let mut error = SelfUpdateCommandError::new(
            SelfUpdateErrorKind::UrlPolicy,
            format!(
                "self-update {field_name} URL must use https (or file when {SELF_UPDATE_ALLOW_INSECURE_ENV}=1): '{}'",
                url
            ),
        )
        .with_asset_url(url.to_string());
        if let Some(endpoint) = endpoint {
            error = error.with_endpoint(endpoint.to_string());
        }
        return Err(error);
    }

    if !is_allowed_update_host(&host) {
        let mut error = SelfUpdateCommandError::new(
            SelfUpdateErrorKind::UrlPolicy,
            format!(
                "self-update {field_name} URL host '{}' is not allowlisted",
                host
            ),
        )
        .with_asset_url(url.to_string());
        if let Some(endpoint) = endpoint {
            error = error.with_endpoint(endpoint.to_string());
        }
        return Err(error);
    }

    Ok(())
}

fn parse_self_update_args(command_args: &[String]) -> Result<ParsedSelfUpdateArgs, String> {
    let mut parsed = ParsedSelfUpdateArgs::default();
    for arg in command_args {
        match arg.as_str() {
            "--check" => parsed.check_only = true,
            "--force" => parsed.force = true,
            other => {
                return Err(format!(
                    "unsupported self update argument '{}' (supported: --check, --force)",
                    other
                ));
            }
        }
    }
    Ok(parsed)
}

fn fetch_cli_update_manifest(endpoint: &str) -> Result<CliUpdateManifest, SelfUpdateCommandError> {
    validate_update_url(endpoint, "endpoint", Some(endpoint))?;
    if let Some(path) = file_url_path(endpoint) {
        let body = fs::read_to_string(path.as_path()).map_err(|error| {
            SelfUpdateCommandError::new(
                SelfUpdateErrorKind::ManifestRead,
                format!(
                    "failed to read self-update endpoint payload from '{}': {error}",
                    path.display()
                ),
            )
            .with_endpoint(endpoint.to_string())
        })?;
        let manifest: CliUpdateManifest = serde_json::from_str(&body).map_err(|error| {
            SelfUpdateCommandError::new(
                SelfUpdateErrorKind::ManifestParse,
                format!(
                    "failed to parse self-update endpoint payload for '{}': {error}",
                    endpoint
                ),
            )
            .with_endpoint(endpoint.to_string())
        })?;
        if manifest.version.trim().is_empty() {
            return Err(SelfUpdateCommandError::new(
                SelfUpdateErrorKind::ManifestContract,
                "self-update endpoint payload is missing non-empty 'version'",
            )
            .with_endpoint(endpoint.to_string()));
        }
        return Ok(manifest);
    }

    let mut current_url = endpoint.trim().to_string();
    let mut redirect_hops = 0usize;
    loop {
        let response = match self_update_http_agent().get(&current_url).call() {
            Ok(response) => response,
            Err(ureq::Error::Status(code, response)) => {
                if is_http_redirect_status(code) {
                    let location = response
                        .header("Location")
                        .map(str::trim)
                        .unwrap_or_default();
                    if location.is_empty() {
                        return Err(SelfUpdateCommandError::new(
                            SelfUpdateErrorKind::ManifestHttp,
                            format!(
                                "self-update endpoint redirect from '{}' is missing Location header",
                                current_url
                            ),
                        )
                        .with_endpoint(endpoint.to_string())
                        .with_http_status(code)
                        .with_asset_url(current_url.clone()));
                    }
                    if redirect_hops >= SELF_UPDATE_MAX_REDIRECT_HOPS {
                        return Err(SelfUpdateCommandError::new(
                            SelfUpdateErrorKind::ManifestHttp,
                            format!(
                                "self-update endpoint exceeded redirect limit ({} hops)",
                                SELF_UPDATE_MAX_REDIRECT_HOPS
                            ),
                        )
                        .with_endpoint(endpoint.to_string())
                        .with_http_status(code)
                        .with_asset_url(current_url.clone()));
                    }
                    current_url = resolve_update_redirect_target(
                        current_url.as_str(),
                        location,
                        "endpoint",
                        Some(endpoint),
                        SelfUpdateErrorKind::ManifestHttp,
                    )?;
                    redirect_hops += 1;
                    continue;
                }

                let body = response.into_string().unwrap_or_default();
                let body = body.replace('\n', " ");
                let summary = if body.len() > 200 {
                    format!("{}...", &body[..200])
                } else {
                    body
                };
                return Err(SelfUpdateCommandError::new(
                    SelfUpdateErrorKind::ManifestHttp,
                    format!(
                        "self-update endpoint returned HTTP {} for '{}': {}",
                        code, current_url, summary
                    ),
                )
                .with_endpoint(endpoint.to_string())
                .with_http_status(code)
                .with_asset_url(current_url));
            }
            Err(error) => {
                return Err(SelfUpdateCommandError::new(
                    SelfUpdateErrorKind::ManifestTransport,
                    format!(
                        "failed to reach self-update endpoint '{}': {error}",
                        current_url
                    ),
                )
                .with_endpoint(endpoint.to_string())
                .with_asset_url(current_url));
            }
        };

        if is_http_redirect_status(response.status()) {
            let status = response.status();
            let location = response
                .header("Location")
                .map(str::trim)
                .unwrap_or_default();
            if location.is_empty() {
                return Err(SelfUpdateCommandError::new(
                    SelfUpdateErrorKind::ManifestHttp,
                    format!(
                        "self-update endpoint redirect from '{}' is missing Location header",
                        current_url
                    ),
                )
                .with_endpoint(endpoint.to_string())
                .with_http_status(status)
                .with_asset_url(current_url));
            }
            if redirect_hops >= SELF_UPDATE_MAX_REDIRECT_HOPS {
                return Err(SelfUpdateCommandError::new(
                    SelfUpdateErrorKind::ManifestHttp,
                    format!(
                        "self-update endpoint exceeded redirect limit ({} hops)",
                        SELF_UPDATE_MAX_REDIRECT_HOPS
                    ),
                )
                .with_endpoint(endpoint.to_string())
                .with_http_status(status)
                .with_asset_url(current_url));
            }
            current_url = resolve_update_redirect_target(
                current_url.as_str(),
                location,
                "endpoint",
                Some(endpoint),
                SelfUpdateErrorKind::ManifestHttp,
            )?;
            redirect_hops += 1;
            continue;
        }

        let body = response.into_string().map_err(|error| {
            SelfUpdateCommandError::new(
                SelfUpdateErrorKind::ManifestRead,
                format!("failed to read self-update endpoint payload: {error}"),
            )
            .with_endpoint(endpoint.to_string())
            .with_asset_url(current_url.clone())
        })?;
        let manifest: CliUpdateManifest = serde_json::from_str(&body).map_err(|error| {
            SelfUpdateCommandError::new(
                SelfUpdateErrorKind::ManifestParse,
                format!(
                    "failed to parse self-update endpoint payload for '{}': {error}",
                    current_url
                ),
            )
            .with_endpoint(endpoint.to_string())
            .with_asset_url(current_url.clone())
        })?;
        if manifest.version.trim().is_empty() {
            return Err(SelfUpdateCommandError::new(
                SelfUpdateErrorKind::ManifestContract,
                "self-update endpoint payload is missing non-empty 'version'",
            )
            .with_endpoint(endpoint.to_string())
            .with_asset_url(current_url));
        }
        return Ok(manifest);
    }
}

fn parse_semver_lossy(raw: &str) -> Option<Version> {
    let trimmed = raw.trim().trim_start_matches('v');
    if trimmed.is_empty() {
        return None;
    }
    Version::parse(trimmed).ok()
}

fn update_available(current_version: &str, latest_version: &str) -> bool {
    match (
        parse_semver_lossy(current_version),
        parse_semver_lossy(latest_version),
    ) {
        (Some(current), Some(latest)) => latest > current,
        _ => latest_version.trim() != current_version.trim(),
    }
}

fn select_update_asset(manifest: &CliUpdateManifest) -> Result<&CliUpdateAsset, String> {
    if let Some(universal) = manifest.downloads.universal.as_ref() {
        return Ok(universal);
    }

    match env::consts::ARCH {
        "aarch64" => manifest.downloads.arm64.as_ref().ok_or_else(|| {
            "self-update endpoint payload is missing an arm64 download entry".to_string()
        }),
        "x86_64" => manifest.downloads.x86_64.as_ref().ok_or_else(|| {
            "self-update endpoint payload is missing an x86_64 download entry".to_string()
        }),
        other => Err(format!(
            "unsupported architecture for self-update: {}",
            other
        )),
    }
}

fn download_update_bytes(url: &str) -> Result<Vec<u8>, SelfUpdateCommandError> {
    validate_update_url(url, "download", None)?;
    let max_bytes = self_update_max_download_bytes();
    if let Some(path) = file_url_path(url) {
        let metadata = fs::metadata(path.as_path()).map_err(|error| {
            SelfUpdateCommandError::new(
                SelfUpdateErrorKind::AssetRead,
                format!(
                    "failed to inspect update binary from '{}': {error}",
                    path.display()
                ),
            )
            .with_asset_url(url.to_string())
        })?;
        if metadata.len() > max_bytes as u64 {
            return Err(SelfUpdateCommandError::new(
                SelfUpdateErrorKind::AssetContract,
                format!(
                    "downloaded update binary exceeds maximum allowed size ({} bytes)",
                    max_bytes
                ),
            )
            .with_asset_url(url.to_string()));
        }
        let file = fs::File::open(path.as_path()).map_err(|error| {
            SelfUpdateCommandError::new(
                SelfUpdateErrorKind::AssetRead,
                format!(
                    "failed to open update binary from '{}': {error}",
                    path.display()
                ),
            )
            .with_asset_url(url.to_string())
        })?;
        return read_update_bytes_with_limit(file, max_bytes, url);
    }

    let mut current_url = url.trim().to_string();
    let mut redirect_hops = 0usize;
    loop {
        let response = match self_update_http_agent().get(&current_url).call() {
            Ok(response) => response,
            Err(ureq::Error::Status(code, response)) => {
                if is_http_redirect_status(code) {
                    let location = response
                        .header("Location")
                        .map(str::trim)
                        .unwrap_or_default();
                    if location.is_empty() {
                        return Err(SelfUpdateCommandError::new(
                            SelfUpdateErrorKind::AssetHttp,
                            format!(
                                "update download redirect from '{}' is missing Location header",
                                current_url
                            ),
                        )
                        .with_asset_url(current_url)
                        .with_http_status(code));
                    }
                    if redirect_hops >= SELF_UPDATE_MAX_REDIRECT_HOPS {
                        return Err(SelfUpdateCommandError::new(
                            SelfUpdateErrorKind::AssetHttp,
                            format!(
                                "update download exceeded redirect limit ({} hops)",
                                SELF_UPDATE_MAX_REDIRECT_HOPS
                            ),
                        )
                        .with_asset_url(current_url)
                        .with_http_status(code));
                    }
                    current_url = resolve_update_redirect_target(
                        current_url.as_str(),
                        location,
                        "download",
                        None,
                        SelfUpdateErrorKind::AssetHttp,
                    )?;
                    redirect_hops += 1;
                    continue;
                }
                return Err(SelfUpdateCommandError::new(
                    SelfUpdateErrorKind::AssetHttp,
                    format!("failed to download update binary (HTTP {})", code),
                )
                .with_asset_url(current_url)
                .with_http_status(code));
            }
            Err(error) => {
                return Err(SelfUpdateCommandError::new(
                    SelfUpdateErrorKind::AssetTransport,
                    format!("failed to download update binary: {error}"),
                )
                .with_asset_url(current_url));
            }
        };

        if is_http_redirect_status(response.status()) {
            let status = response.status();
            let location = response
                .header("Location")
                .map(str::trim)
                .unwrap_or_default();
            if location.is_empty() {
                return Err(SelfUpdateCommandError::new(
                    SelfUpdateErrorKind::AssetHttp,
                    format!(
                        "update download redirect from '{}' is missing Location header",
                        current_url
                    ),
                )
                .with_asset_url(current_url)
                .with_http_status(status));
            }
            if redirect_hops >= SELF_UPDATE_MAX_REDIRECT_HOPS {
                return Err(SelfUpdateCommandError::new(
                    SelfUpdateErrorKind::AssetHttp,
                    format!(
                        "update download exceeded redirect limit ({} hops)",
                        SELF_UPDATE_MAX_REDIRECT_HOPS
                    ),
                )
                .with_asset_url(current_url)
                .with_http_status(status));
            }
            current_url = resolve_update_redirect_target(
                current_url.as_str(),
                location,
                "download",
                None,
                SelfUpdateErrorKind::AssetHttp,
            )?;
            redirect_hops += 1;
            continue;
        }

        let mut reader = response.into_reader();
        return read_update_bytes_with_limit(&mut reader, max_bytes, current_url.as_str());
    }
}

fn read_update_bytes_with_limit<R: Read>(
    reader: R,
    max_bytes: usize,
    url: &str,
) -> Result<Vec<u8>, SelfUpdateCommandError> {
    let mut limited = reader.take((max_bytes as u64).saturating_add(1));
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).map_err(|error| {
        SelfUpdateCommandError::new(
            SelfUpdateErrorKind::AssetRead,
            format!("failed to read downloaded update binary: {error}"),
        )
        .with_asset_url(url.to_string())
    })?;
    if bytes.is_empty() {
        return Err(SelfUpdateCommandError::new(
            SelfUpdateErrorKind::AssetContract,
            "downloaded update binary is empty",
        )
        .with_asset_url(url.to_string()));
    }
    if bytes.len() > max_bytes {
        return Err(SelfUpdateCommandError::new(
            SelfUpdateErrorKind::AssetContract,
            format!(
                "downloaded update binary exceeds maximum allowed size ({} bytes)",
                max_bytes
            ),
        )
        .with_asset_url(url.to_string()));
    }
    Ok(bytes)
}

fn normalize_sha256(raw: &str) -> String {
    raw.trim()
        .trim_start_matches("sha256:")
        .trim()
        .to_ascii_lowercase()
}

fn verify_sha256(bytes: &[u8], expected_sha256: &str) -> Result<(), String> {
    let expected = normalize_sha256(expected_sha256);
    if expected.is_empty() {
        return Err("self-update checksum is empty".to_string());
    }

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = format!("{:x}", hasher.finalize());
    if digest != expected {
        return Err(format!(
            "self-update checksum mismatch (expected {}, got {})",
            expected, digest
        ));
    }
    Ok(())
}

fn update_temp_path(executable_path: &Path) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let name = executable_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| "helm".to_string());
    executable_path.with_file_name(format!("{name}.tmp-update-{suffix}"))
}

fn apply_update_bytes(executable_path: &Path, bytes: &[u8]) -> Result<(), String> {
    let metadata = fs::symlink_metadata(executable_path).map_err(|error| {
        format!(
            "failed to inspect executable metadata '{}': {error}",
            executable_path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to replace symlink executable path '{}'; reinstall to a real file path",
            executable_path.display()
        ));
    }
    if !metadata.file_type().is_file() {
        return Err(format!(
            "refusing to replace non-file executable path '{}'",
            executable_path.display()
        ));
    }

    let temp_path = update_temp_path(executable_path);
    let mut temp_file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp_path)
        .map_err(|error| {
            format!(
                "failed to create temporary update file '{}': {error}",
                temp_path.display()
            )
        })?;

    use std::io::Write;
    temp_file
        .write_all(bytes)
        .map_err(|error| format!("failed to write temporary update file: {error}"))?;
    temp_file
        .sync_all()
        .map_err(|error| format!("failed to flush temporary update file: {error}"))?;

    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    fs::set_permissions(&temp_path, permissions).map_err(|error| {
        format!(
            "failed to set permissions on temporary update file '{}': {error}",
            temp_path.display()
        )
    })?;

    if let Err(error) = fs::rename(&temp_path, executable_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!(
            "failed to replace '{}' with updated binary: {}. \
If this path is not writable, reinstall with a user-writable direct install path.",
            executable_path.display(),
            error
        ));
    }

    sync_parent_directory(executable_path)?;
    Ok(())
}

fn sync_parent_directory(path: &Path) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let directory = OpenOptions::new()
        .read(true)
        .open(parent)
        .map_err(|error| {
            format!(
                "failed to open parent directory '{}': {error}",
                parent.display()
            )
        })?;
    directory.sync_all().map_err(|error| {
        format!(
            "failed to sync parent directory '{}': {error}",
            parent.display()
        )
    })
}

fn persist_direct_update_marker(version: &str) -> Result<(), String> {
    let installed_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
    let marker = InstallMarker {
        channel: "direct-script".to_string(),
        artifact: "helm-cli".to_string(),
        installed_at,
        update_policy: "self".to_string(),
        version: Some(version.to_string()),
    };
    let marker_path = install_marker_path()?;
    write_install_marker(&marker_path, &marker)
}

fn default_helm_cli_shim_path() -> Result<PathBuf, String> {
    let home = env::var("HOME").map_err(|_| "HOME is not set".to_string())?;
    Ok(PathBuf::from(home).join(DEFAULT_HELM_CLI_SHIM_RELATIVE_PATH))
}

fn self_uninstall_recommended_action(channel: InstallChannel, executable_path: &Path) -> String {
    match channel {
        InstallChannel::DirectScript => "helm self uninstall".to_string(),
        InstallChannel::AppBundleShim => {
            "remove the managed Helm CLI shim via GUI Settings (Install Helm CLI)".to_string()
        }
        InstallChannel::Brew => "brew uninstall helm-cli".to_string(),
        InstallChannel::Macports => "sudo port uninstall helm-cli".to_string(),
        InstallChannel::Cargo => "cargo uninstall helm-cli".to_string(),
        InstallChannel::Unknown => format!(
            "remove the binary manually: rm '{}'",
            executable_path.display()
        ),
        InstallChannel::Managed => "follow managed organizational uninstall policy".to_string(),
    }
}

fn remove_regular_file(path: &Path, label: &str) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "failed to inspect {} '{}': {}",
                label,
                path.display(),
                error
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to remove {} symlink path '{}'",
            label,
            path.display()
        ));
    }
    if !metadata.file_type().is_file() {
        return Err(format!(
            "refusing to remove non-file {} path '{}'",
            label,
            path.display()
        ));
    }
    fs::remove_file(path)
        .map_err(|error| format!("failed to remove {} '{}': {}", label, path.display(), error))?;
    sync_parent_directory(path)?;
    Ok(true)
}

fn remove_install_marker_if_channel(
    marker_path: &Path,
    expected_channel: InstallChannel,
) -> Result<(bool, Option<String>), String> {
    let metadata = match fs::symlink_metadata(marker_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((false, None)),
        Err(error) => {
            return Err(format!(
                "failed to inspect install marker '{}': {}",
                marker_path.display(),
                error
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to remove install marker symlink path '{}'",
            marker_path.display()
        ));
    }
    if !metadata.file_type().is_file() {
        return Err(format!(
            "refusing to remove non-file install marker path '{}'",
            marker_path.display()
        ));
    }

    let Some(marker) = read_install_marker(marker_path) else {
        return Ok((
            false,
            Some("install marker exists but is not schema-valid; left in place".to_string()),
        ));
    };
    if marker.channel != expected_channel.as_str() {
        return Ok((
            false,
            Some(format!(
                "install marker channel '{}' does not match '{}' and was left in place",
                marker.channel,
                expected_channel.as_str()
            )),
        ));
    }

    remove_regular_file(marker_path, "install marker").map(|removed| (removed, None))
}

fn remove_managed_app_bundle_shim(shim_path: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(shim_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "failed to inspect Helm CLI shim '{}': {}",
                shim_path.display(),
                error
            ));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to remove Helm CLI shim symlink path '{}'",
            shim_path.display()
        ));
    }
    if !metadata.file_type().is_file() {
        return Err(format!(
            "refusing to remove non-file Helm CLI shim path '{}'",
            shim_path.display()
        ));
    }

    let script = fs::read_to_string(shim_path).map_err(|error| {
        format!(
            "failed to read Helm CLI shim '{}': {}",
            shim_path.display(),
            error
        )
    })?;
    if !script.contains(APP_BUNDLE_SHIM_SENTINEL) {
        return Err(format!(
            "refusing to remove non-managed Helm CLI shim at '{}'",
            shim_path.display()
        ));
    }

    fs::remove_file(shim_path).map_err(|error| {
        format!(
            "failed to remove Helm CLI shim '{}': {}",
            shim_path.display(),
            error
        )
    })?;
    sync_parent_directory(shim_path)?;
    Ok(true)
}

fn direct_update_check_status(
    current_version: &str,
) -> Result<SelfUpdateCheckStatus, SelfUpdateCommandError> {
    let endpoint = self_update_endpoint();
    let manifest = fetch_cli_update_manifest(&endpoint)?;
    let available = update_available(current_version, &manifest.version);
    Ok(SelfUpdateCheckStatus {
        checked: true,
        update_available: Some(available),
        latest_version: Some(manifest.version),
        published_at: manifest.published_at,
        source: endpoint,
        reason: None,
    })
}

fn run_due_auto_check(store: &SqliteStore) -> Result<(), String> {
    if !store
        .auto_check_for_updates()
        .map_err(|error| format!("failed to read auto-check setting: {error}"))?
    {
        return Ok(());
    }

    let frequency_minutes = store
        .auto_check_frequency_minutes()
        .map_err(|error| format!("failed to read auto-check frequency: {error}"))?
        .max(1);
    let now_unix = json_generated_at_unix();
    let last_checked = store
        .auto_check_last_checked_unix()
        .map_err(|error| format!("failed to read auto-check last-run timestamp: {error}"))?;
    if let Some(last_checked) = last_checked {
        let elapsed = now_unix.saturating_sub(last_checked);
        let required = (frequency_minutes as i64).saturating_mul(60);
        if elapsed < required {
            return Ok(());
        }
    }

    let current_version = current_cli_version();
    let executable_path = env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    let provenance = detect_install_provenance(&executable_path);
    let mut attempted_check = false;
    if provenance_can_self_update(provenance.update_policy) {
        attempted_check = true;
        match direct_update_check_status(&current_version) {
            Ok(status) => {
                verbose_log(format!(
                    "auto-check completed: update_available={:?}, latest_version={:?}, source={}",
                    status.update_available, status.latest_version, status.source
                ));
            }
            Err(error) => {
                verbose_log(format!("auto-check failed: {}", error));
            }
        }
    } else {
        verbose_log(format!(
            "auto-check skipped for channel-managed install '{}'",
            provenance.channel.as_str()
        ));
    }

    if attempted_check {
        store
            .set_auto_check_last_checked_unix(now_unix)
            .map_err(|error| format!("failed to persist auto-check timestamp: {error}"))?;
    } else {
        verbose_log("auto-check timestamp unchanged (no direct self-managed check attempted)");
    }
    Ok(())
}

fn direct_update_apply(
    current_version: &str,
    executable_path: &Path,
) -> Result<SelfUpdateApplyResult, SelfUpdateCommandError> {
    let endpoint = self_update_endpoint();
    let manifest = fetch_cli_update_manifest(&endpoint)?;
    let available = update_available(current_version, &manifest.version);
    if !available {
        return Ok(SelfUpdateApplyResult {
            updated: false,
            latest_version: Some(manifest.version),
            published_at: manifest.published_at,
            source: endpoint,
        });
    }

    let asset = select_update_asset(&manifest).map_err(SelfUpdateCommandError::from)?;
    validate_update_url(&asset.url, "download", Some(&endpoint))?;
    let bytes = download_update_bytes(&asset.url)?;
    verify_sha256(&bytes, &asset.sha256).map_err(SelfUpdateCommandError::from)?;
    apply_update_bytes(executable_path, &bytes)
        .map_err(|error| SelfUpdateCommandError::new(SelfUpdateErrorKind::Replace, error))?;
    let _ = persist_direct_update_marker(&manifest.version);

    Ok(SelfUpdateApplyResult {
        updated: true,
        latest_version: Some(manifest.version),
        published_at: manifest.published_at,
        source: endpoint,
    })
}

fn channel_managed_check_status(reason: String) -> SelfUpdateCheckStatus {
    SelfUpdateCheckStatus {
        checked: false,
        update_available: None,
        latest_version: None,
        published_at: None,
        source: "channel".to_string(),
        reason: Some(reason),
    }
}

fn emit_self_check_error_json(
    schema: &str,
    checked_at: i64,
    provenance: &provenance::InstallProvenance,
    can_self_update: bool,
    recommended_action: &str,
    current_version: &str,
    error: &SelfUpdateCommandError,
) {
    emit_json_payload(
        schema,
        json!({
            "checked": false,
            "checked_at": checked_at,
            "channel": provenance.channel.as_str(),
            "update_policy": provenance.update_policy.as_str(),
            "can_self_update": can_self_update,
            "recommended_action": recommended_action,
            "current_version": current_version,
            "update_available": serde_json::Value::Null,
            "latest_version": serde_json::Value::Null,
            "published_at": serde_json::Value::Null,
            "source": error.endpoint.as_deref().unwrap_or("self-update"),
            "reason": error.message.clone(),
            "error_kind": error.kind.as_str(),
            "error_http_status": error.http_status,
            "error_endpoint": error.endpoint.clone(),
            "error_asset_url": error.asset_url.clone(),
            "actionable_guidance": error.actionable_guidance(recommended_action)
        }),
    );
}

fn emit_self_update_apply_error_json(
    provenance: &provenance::InstallProvenance,
    parsed: &ParsedSelfUpdateArgs,
    can_self_update: bool,
    recommended_action: &str,
    current_version: &str,
    error: &SelfUpdateCommandError,
) {
    emit_json_payload(
        "helm.cli.v1.self.update",
        json!({
            "accepted": false,
            "updated": false,
            "channel": provenance.channel.as_str(),
            "update_policy": provenance.update_policy.as_str(),
            "force": parsed.force,
            "can_self_update": can_self_update,
            "recommended_action": recommended_action,
            "current_version": current_version,
            "latest_version": serde_json::Value::Null,
            "published_at": serde_json::Value::Null,
            "source": error.endpoint.as_deref().unwrap_or("self-update"),
            "reason": error.message.clone(),
            "error_kind": error.kind.as_str(),
            "error_http_status": error.http_status,
            "error_endpoint": error.endpoint.clone(),
            "error_asset_url": error.asset_url.clone(),
            "actionable_guidance": error.actionable_guidance(recommended_action)
        }),
    );
}

fn cmd_self(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() || is_help_token(&command_args[0]) {
        if options.json {
            emit_help_json_payload(Some(Command::SelfCmd), &[], true);
        } else {
            print_self_help();
        }
        return Ok(());
    }

    let executable_path = env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    let provenance = detect_install_provenance(&executable_path);
    let recommended_action = provenance_recommended_action(provenance.channel).to_string();
    let uninstall_recommended_action =
        self_uninstall_recommended_action(provenance.channel, &provenance.executable_path);
    let current_version = current_cli_version();
    let can_self_update = provenance_can_self_update(provenance.update_policy);
    let can_self_uninstall = provenance.update_policy != UpdatePolicy::Managed
        && matches!(
            provenance.channel,
            InstallChannel::DirectScript | InstallChannel::AppBundleShim
        );
    let force_direct_override =
        |force: bool| force && provenance.channel == InstallChannel::DirectScript;
    verbose_log(format!(
        "self command provenance channel={} policy={} source={} executable={}",
        provenance.channel.as_str(),
        provenance.update_policy.as_str(),
        provenance.source.as_str(),
        provenance.executable_path.display()
    ));

    match command_args[0].as_str() {
        "auto-check" => cmd_self_auto_check(store.as_ref(), options, &command_args[1..]),
        "status" => {
            let auto_check_for_updates = store
                .auto_check_for_updates()
                .map_err(|error| format!("failed to read auto-check setting: {error}"))?;
            let auto_check_frequency_minutes = store
                .auto_check_frequency_minutes()
                .map_err(|error| format!("failed to read auto-check frequency: {error}"))?;
            let auto_check_last_checked_unix =
                store.auto_check_last_checked_unix().map_err(|error| {
                    format!("failed to read auto-check last-run timestamp: {error}")
                })?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.status",
                    json!({
                        "current_version": current_version,
                        "channel": provenance.channel.as_str(),
                        "update_policy": provenance.update_policy.as_str(),
                        "source": provenance.source.as_str(),
                        "executable_path": provenance.executable_path.to_string_lossy().to_string(),
                        "marker_path": provenance.marker_path.to_string_lossy().to_string(),
                        "artifact": provenance.artifact,
                        "auto_check_for_updates": auto_check_for_updates,
                        "auto_check_frequency_minutes": auto_check_frequency_minutes,
                        "auto_check_last_checked_unix": auto_check_last_checked_unix,
                        "can_self_update": can_self_update,
                        "recommended_action": recommended_action,
                        "latest_version": serde_json::Value::Null,
                        "update_available": serde_json::Value::Null
                    }),
                );
            } else {
                println!("Helm Self Status");
                println!("  current_version: {}", current_version);
                println!("  channel: {}", provenance.channel.as_str());
                println!("  update_policy: {}", provenance.update_policy.as_str());
                println!("  source: {}", provenance.source.as_str());
                println!(
                    "  executable_path: {}",
                    provenance.executable_path.display()
                );
                println!("  marker_path: {}", provenance.marker_path.display());
                println!("  auto_check_for_updates: {auto_check_for_updates}");
                println!("  auto_check_frequency_minutes: {auto_check_frequency_minutes}");
                println!(
                    "  auto_check_last_checked_unix: {}",
                    auto_check_last_checked_unix
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "null".to_string())
                );
                println!("  can_self_update: {}", can_self_update);
                println!("  recommended_action: {}", recommended_action);
            }
            Ok(())
        }
        "check" => {
            let checked_at = json_generated_at_unix();
            let status = if can_self_update {
                match direct_update_check_status(&current_version) {
                    Ok(status) => status,
                    Err(error) => {
                        if options.json {
                            emit_self_check_error_json(
                                "helm.cli.v1.self.check",
                                checked_at,
                                &provenance,
                                can_self_update,
                                &recommended_action,
                                &current_version,
                                &error,
                            );
                            return Err(mark_json_error_emitted(error.to_string()));
                        }
                        return Err(error.to_string());
                    }
                }
            } else {
                channel_managed_check_status(format!(
                    "self-update check is channel-managed for '{}' installs; {}",
                    provenance.channel.as_str(),
                    recommended_action
                ))
            };
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.check",
                    json!({
                        "checked": status.checked,
                        "checked_at": checked_at,
                        "channel": provenance.channel.as_str(),
                        "update_policy": provenance.update_policy.as_str(),
                        "can_self_update": can_self_update,
                        "recommended_action": recommended_action,
                        "current_version": current_version,
                        "update_available": status.update_available,
                        "latest_version": status.latest_version,
                        "published_at": status.published_at,
                        "source": status.source,
                        "reason": status.reason
                    }),
                );
            } else {
                println!("Self-update check completed.");
                println!("  checked_at: {checked_at}");
                println!("  channel: {}", provenance.channel.as_str());
                println!("  update_policy: {}", provenance.update_policy.as_str());
                println!("  can_self_update: {}", can_self_update);
                println!("  recommended_action: {}", recommended_action);
                println!(
                    "  update_available: {}",
                    status
                        .update_available
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                );
                println!(
                    "  latest_version: {}",
                    status.latest_version.unwrap_or_else(|| "-".to_string())
                );
                if let Some(reason) = status.reason {
                    println!("  reason: {reason}");
                }
            }
            Ok(())
        }
        "update" => {
            let parsed = parse_self_update_args(&command_args[1..])?;
            if parsed.check_only {
                let checked_at = json_generated_at_unix();
                let status = if provenance.channel == InstallChannel::Managed {
                    channel_managed_check_status(
                        "self-update checks are blocked by managed policy".to_string(),
                    )
                } else if provenance.channel == InstallChannel::AppBundleShim {
                    channel_managed_check_status(
                        "app-bundled CLI cannot self-update; update the Helm GUI through its channel"
                            .to_string(),
                    )
                } else if can_self_update || force_direct_override(parsed.force) {
                    match direct_update_check_status(&current_version) {
                        Ok(status) => status,
                        Err(error) => {
                            if options.json {
                                emit_self_check_error_json(
                                    "helm.cli.v1.self.update.check",
                                    checked_at,
                                    &provenance,
                                    can_self_update,
                                    &recommended_action,
                                    &current_version,
                                    &error,
                                );
                                return Err(mark_json_error_emitted(error.to_string()));
                            }
                            return Err(error.to_string());
                        }
                    }
                } else {
                    channel_managed_check_status(format!(
                        "self-update is channel-managed for '{}' installs; {}{}",
                        provenance.channel.as_str(),
                        recommended_action,
                        if parsed.force {
                            " (force override is only allowed for direct-script installs)"
                        } else {
                            ""
                        }
                    ))
                };

                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.update.check",
                        json!({
                            "checked": status.checked,
                            "checked_at": checked_at,
                            "channel": provenance.channel.as_str(),
                            "update_policy": provenance.update_policy.as_str(),
                            "force": parsed.force,
                            "can_self_update": can_self_update,
                            "recommended_action": recommended_action,
                            "current_version": current_version,
                            "update_available": status.update_available,
                            "latest_version": status.latest_version,
                            "published_at": status.published_at,
                            "source": status.source,
                            "reason": status.reason
                        }),
                    );
                } else {
                    println!("Self-update check completed.");
                    println!("  checked_at: {checked_at}");
                    println!("  channel: {}", provenance.channel.as_str());
                    println!("  update_policy: {}", provenance.update_policy.as_str());
                    println!("  force: {}", parsed.force);
                    println!("  can_self_update: {}", can_self_update);
                    println!("  recommended_action: {}", recommended_action);
                    println!(
                        "  update_available: {}",
                        status
                            .update_available
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    );
                    println!(
                        "  latest_version: {}",
                        status.latest_version.unwrap_or_else(|| "-".to_string())
                    );
                    if let Some(reason) = status.reason {
                        println!("  reason: {reason}");
                    }
                }
                return Ok(());
            }

            if options.execution_mode == ExecutionMode::Detach {
                return Err(
                    "self update does not support --detach for direct binary replacement"
                        .to_string(),
                );
            }

            if is_running_as_root() && !env_flag_enabled(SELF_UPDATE_ALLOW_ROOT_ENV) {
                let reason = format!(
                    "self update blocked while running as root; set {SELF_UPDATE_ALLOW_ROOT_ENV}=1 for explicit override"
                );
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.update",
                        json!({
                            "accepted": false,
                            "channel": provenance.channel.as_str(),
                            "update_policy": provenance.update_policy.as_str(),
                            "force": parsed.force,
                            "can_self_update": false,
                            "recommended_action": recommended_action,
                            "current_version": current_version,
                            "latest_version": serde_json::Value::Null,
                            "reason": reason
                        }),
                    );
                }
                return Err(mark_json_error_emitted(format!(
                    "self update unavailable: running as root is blocked by default (set {SELF_UPDATE_ALLOW_ROOT_ENV}=1 to override)"
                )));
            }

            if provenance.channel == InstallChannel::AppBundleShim {
                let reason =
                    "app-bundled CLI cannot self-update; update Helm GUI through its channel"
                        .to_string();
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.update",
                        json!({
                            "accepted": false,
                            "channel": provenance.channel.as_str(),
                            "update_policy": provenance.update_policy.as_str(),
                            "force": parsed.force,
                            "can_self_update": false,
                            "recommended_action": recommended_action,
                            "current_version": current_version,
                            "latest_version": serde_json::Value::Null,
                            "reason": reason
                        }),
                    );
                }
                return Err(mark_json_error_emitted(
                    "self update unavailable: app-bundled CLI is channel-managed",
                ));
            }

            if provenance.update_policy == UpdatePolicy::Managed {
                let reason = "self-update blocked by managed policy".to_string();
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.update",
                        json!({
                            "accepted": false,
                            "channel": provenance.channel.as_str(),
                            "update_policy": provenance.update_policy.as_str(),
                            "force": parsed.force,
                            "can_self_update": false,
                            "recommended_action": recommended_action,
                            "current_version": current_version,
                            "latest_version": serde_json::Value::Null,
                            "reason": reason
                        }),
                    );
                }
                return Err(mark_json_error_emitted(
                    "self update unavailable: managed policy denies direct self-update",
                ));
            }

            if !can_self_update && !force_direct_override(parsed.force) {
                let reason = format!(
                    "self-update is channel-managed for '{}' installs; {}{}",
                    provenance.channel.as_str(),
                    recommended_action,
                    if parsed.force {
                        " (force override is only allowed for direct-script installs)"
                    } else {
                        ""
                    }
                );
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.update",
                        json!({
                            "accepted": false,
                            "channel": provenance.channel.as_str(),
                            "update_policy": provenance.update_policy.as_str(),
                            "force": parsed.force,
                            "can_self_update": can_self_update,
                            "recommended_action": recommended_action,
                            "current_version": current_version,
                            "latest_version": serde_json::Value::Null,
                            "reason": reason
                        }),
                    );
                }
                return Err(mark_json_error_emitted(
                    "self update unavailable: installation is channel-managed",
                ));
            }

            let applied = match direct_update_apply(&current_version, &provenance.executable_path) {
                Ok(applied) => applied,
                Err(error) => {
                    if options.json {
                        emit_self_update_apply_error_json(
                            &provenance,
                            &parsed,
                            can_self_update,
                            &recommended_action,
                            &current_version,
                            &error,
                        );
                        return Err(mark_json_error_emitted(error.to_string()));
                    }
                    return Err(error.to_string());
                }
            };
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.update",
                    json!({
                        "accepted": true,
                        "updated": applied.updated,
                        "channel": provenance.channel.as_str(),
                        "update_policy": if parsed.force { "self" } else { provenance.update_policy.as_str() },
                        "force": parsed.force,
                        "can_self_update": true,
                        "recommended_action": recommended_action,
                        "current_version": current_version,
                        "latest_version": applied.latest_version,
                        "published_at": applied.published_at,
                        "source": applied.source
                    }),
                );
            } else {
                if parsed.force {
                    println!(
                        "Self-update force mode enabled; proceeding with direct update endpoint."
                    );
                }
                if applied.updated {
                    println!("Self-update completed.");
                } else {
                    println!("Already up to date.");
                }
                println!("  channel: {}", provenance.channel.as_str());
                println!("  source: {}", applied.source);
                println!(
                    "  latest_version: {}",
                    applied.latest_version.unwrap_or_else(|| "-".to_string())
                );
            }
            Ok(())
        }
        "uninstall" => {
            if options.execution_mode == ExecutionMode::Detach {
                return Err("self uninstall does not support --detach".to_string());
            }
            if command_args.len() > 1 {
                return Err("self uninstall does not take additional arguments".to_string());
            }

            if provenance.update_policy == UpdatePolicy::Managed {
                let reason = "self-uninstall blocked by managed policy".to_string();
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.uninstall",
                        json!({
                            "accepted": false,
                            "removed": false,
                            "channel": provenance.channel.as_str(),
                            "update_policy": provenance.update_policy.as_str(),
                            "can_self_uninstall": false,
                            "recommended_action": uninstall_recommended_action,
                            "current_version": current_version,
                            "executable_path": provenance.executable_path.to_string_lossy().to_string(),
                            "marker_path": provenance.marker_path.to_string_lossy().to_string(),
                            "shim_path": serde_json::Value::Null,
                            "removed_executable": false,
                            "removed_shim": false,
                            "removed_marker": false,
                            "warning": serde_json::Value::Null,
                            "reason": reason
                        }),
                    );
                }
                return Err(mark_json_error_emitted(
                    "self uninstall unavailable: managed policy denies direct uninstall",
                ));
            }

            if !can_self_uninstall {
                let reason = format!(
                    "self-uninstall is channel-managed for '{}' installs; {}",
                    provenance.channel.as_str(),
                    uninstall_recommended_action
                );
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.uninstall",
                        json!({
                            "accepted": false,
                            "removed": false,
                            "channel": provenance.channel.as_str(),
                            "update_policy": provenance.update_policy.as_str(),
                            "can_self_uninstall": false,
                            "recommended_action": uninstall_recommended_action,
                            "current_version": current_version,
                            "executable_path": provenance.executable_path.to_string_lossy().to_string(),
                            "marker_path": provenance.marker_path.to_string_lossy().to_string(),
                            "shim_path": serde_json::Value::Null,
                            "removed_executable": false,
                            "removed_shim": false,
                            "removed_marker": false,
                            "warning": serde_json::Value::Null,
                            "reason": reason
                        }),
                    );
                }
                return Err(mark_json_error_emitted(
                    "self uninstall unavailable: installation is channel-managed",
                ));
            }

            let mut removed_executable = false;
            let mut removed_shim = false;
            let mut removed_marker = false;
            let mut marker_warning: Option<String> = None;
            let mut shim_path: Option<String> = None;

            let uninstall_result = match provenance.channel {
                InstallChannel::DirectScript => {
                    removed_executable =
                        remove_regular_file(&provenance.executable_path, "CLI executable")?;
                    let (marker_removed, warning) = remove_install_marker_if_channel(
                        &provenance.marker_path,
                        InstallChannel::DirectScript,
                    )?;
                    removed_marker = marker_removed;
                    marker_warning = warning;
                    Ok(())
                }
                InstallChannel::AppBundleShim => {
                    let path = default_helm_cli_shim_path()?;
                    shim_path = Some(path.to_string_lossy().to_string());
                    removed_shim = remove_managed_app_bundle_shim(path.as_path())?;
                    let (marker_removed, warning) = remove_install_marker_if_channel(
                        &provenance.marker_path,
                        InstallChannel::AppBundleShim,
                    )?;
                    removed_marker = marker_removed;
                    marker_warning = warning;
                    Ok(())
                }
                _ => Err("self uninstall unsupported for this install channel".to_string()),
            };
            if let Err(error) = uninstall_result {
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.uninstall",
                        json!({
                            "accepted": false,
                            "removed": false,
                            "channel": provenance.channel.as_str(),
                            "update_policy": provenance.update_policy.as_str(),
                            "can_self_uninstall": can_self_uninstall,
                            "recommended_action": uninstall_recommended_action,
                            "current_version": current_version,
                            "executable_path": provenance.executable_path.to_string_lossy().to_string(),
                            "marker_path": provenance.marker_path.to_string_lossy().to_string(),
                            "shim_path": shim_path,
                            "removed_executable": removed_executable,
                            "removed_shim": removed_shim,
                            "removed_marker": removed_marker,
                            "warning": marker_warning,
                            "reason": error
                        }),
                    );
                    return Err(mark_json_error_emitted(error));
                }
                return Err(error);
            }

            let removed = removed_executable || removed_shim;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.uninstall",
                    json!({
                        "accepted": true,
                        "removed": removed,
                        "channel": provenance.channel.as_str(),
                        "update_policy": provenance.update_policy.as_str(),
                        "can_self_uninstall": can_self_uninstall,
                        "recommended_action": uninstall_recommended_action,
                        "current_version": current_version,
                        "executable_path": provenance.executable_path.to_string_lossy().to_string(),
                        "marker_path": provenance.marker_path.to_string_lossy().to_string(),
                        "shim_path": shim_path,
                        "removed_executable": removed_executable,
                        "removed_shim": removed_shim,
                        "removed_marker": removed_marker,
                        "warning": marker_warning,
                        "reason": serde_json::Value::Null
                    }),
                );
            } else {
                if removed {
                    println!("Helm CLI uninstall completed.");
                } else {
                    println!("Helm CLI uninstall completed; no managed CLI shim was present.");
                }
                println!("  channel: {}", provenance.channel.as_str());
                println!("  update_policy: {}", provenance.update_policy.as_str());
                if removed_executable {
                    println!(
                        "  removed_executable: {}",
                        provenance.executable_path.display()
                    );
                }
                if let Some(shim_path) = shim_path.as_deref() {
                    println!("  shim_path: {shim_path}");
                    println!("  removed_shim: {}", removed_shim);
                }
                println!("  removed_marker: {}", removed_marker);
                if let Some(warning) = marker_warning {
                    println!("  warning: {warning}");
                }
            }
            Ok(())
        }
        _ => Err(format!(
            "unsupported self subcommand '{}'; currently supported: status, check, update, uninstall, auto-check",
            command_args[0]
        )),
    }
}

fn cmd_self_auto_check(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let emit_status = || -> Result<(), String> {
        let enabled = store
            .auto_check_for_updates()
            .map_err(|error| format!("failed to read auto-check setting: {error}"))?;
        let frequency_minutes = store
            .auto_check_frequency_minutes()
            .map_err(|error| format!("failed to read auto-check frequency: {error}"))?;
        let last_checked_unix = store
            .auto_check_last_checked_unix()
            .map_err(|error| format!("failed to read auto-check last-run timestamp: {error}"))?;
        if options.json {
            emit_json_payload(
                "helm.cli.v1.self.auto_check.status",
                json!({
                    "enabled": enabled,
                    "frequency_minutes": frequency_minutes,
                    "last_checked_unix": last_checked_unix
                }),
            );
        } else {
            println!("Self auto-check status");
            println!("  enabled: {}", enabled);
            println!("  frequency_minutes: {}", frequency_minutes);
            println!(
                "  last_checked_unix: {}",
                last_checked_unix
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "null".to_string())
            );
        }
        Ok(())
    };

    if command_args.is_empty() || is_help_token(&command_args[0]) {
        return emit_status();
    }

    match command_args[0].as_str() {
        "status" => {
            if command_args.len() != 1 {
                return Err("self auto-check status does not take additional arguments".to_string());
            }
            emit_status()
        }
        "enable" => {
            if command_args.len() != 1 {
                return Err("self auto-check enable does not take additional arguments".to_string());
            }
            store
                .set_auto_check_for_updates(true)
                .map_err(|error| format!("failed to set auto-check setting: {error}"))?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.auto_check.set",
                    json!({
                        "enabled": true
                    }),
                );
            } else {
                println!("Self auto-check is now enabled.");
            }
            Ok(())
        }
        "disable" => {
            if command_args.len() != 1 {
                return Err(
                    "self auto-check disable does not take additional arguments".to_string()
                );
            }
            store
                .set_auto_check_for_updates(false)
                .map_err(|error| format!("failed to set auto-check setting: {error}"))?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.auto_check.set",
                    json!({
                        "enabled": false
                    }),
                );
            } else {
                println!("Self auto-check is now disabled.");
            }
            Ok(())
        }
        "frequency" => {
            if command_args.len() != 2 {
                return Err("self auto-check frequency requires <minutes>".to_string());
            }
            let minutes = parse_positive_u32(&command_args[1], "auto_check_frequency_minutes")?;
            store
                .set_auto_check_frequency_minutes(minutes)
                .map_err(|error| format!("failed to set auto-check frequency: {error}"))?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.auto_check.frequency",
                    json!({
                        "frequency_minutes": minutes
                    }),
                );
            } else {
                println!("Self auto-check frequency set to {} minutes.", minutes);
            }
            Ok(())
        }
        other => Err(format!(
            "unsupported self auto-check subcommand '{}'; currently supported: status, enable, disable, frequency",
            other
        )),
    }
}

fn cmd_diagnostics(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() || is_help_token(&command_args[0]) {
        if options.json {
            emit_help_json_payload(Some(Command::Diagnostics), &[], true);
        } else {
            print_diagnostics_help();
        }
        return Ok(());
    }

    match command_args[0].as_str() {
        "summary" => cmd_diagnostics_summary(store, options),
        "task" => cmd_diagnostics_task(store, options, &command_args[1..]),
        "manager" => cmd_diagnostics_manager(store, options, &command_args[1..]),
        "provenance" => cmd_diagnostics_provenance(options),
        "export" => cmd_diagnostics_export(store, options, &command_args[1..]),
        _ => Err(format!(
            "unsupported diagnostics subcommand '{}'; currently supported: summary, task, manager, provenance, export",
            command_args[0]
        )),
    }
}

fn cmd_doctor(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return cmd_doctor_scan(store, options);
    }
    if is_help_token(&command_args[0]) {
        if options.json {
            emit_help_json_payload(Some(Command::Doctor), &[], true);
        } else {
            print_doctor_help();
        }
        return Ok(());
    }
    match command_args[0].as_str() {
        "scan" => cmd_doctor_scan(store, options),
        "repair" => cmd_doctor_repair(store, options, &command_args[1..]),
        other => Err(format!(
            "unsupported doctor subcommand '{}'; currently supported: scan, repair",
            other
        )),
    }
}

fn cmd_doctor_scan(store: &SqliteStore, options: GlobalOptions) -> Result<(), String> {
    let installed_packages = store
        .list_installed()
        .map_err(|error| format!("failed to list installed packages for doctor scan: {error}"))?;
    let install_instances = store
        .list_install_instances(None)
        .map_err(|error| format!("failed to list install instances for doctor scan: {error}"))?;
    let mut instances_by_manager: HashMap<ManagerId, Vec<ManagerInstallInstance>> = HashMap::new();
    for instance in install_instances {
        instances_by_manager
            .entry(instance.manager)
            .or_default()
            .push(instance);
    }
    let report = helm_core::doctor::scan_package_state_report(
        ManagerId::ALL,
        &instances_by_manager,
        installed_packages.as_slice(),
    );

    if options.json {
        emit_json_payload("helm.cli.v1.doctor.scan", json!({ "report": report }));
        return Ok(());
    }

    println!("Doctor Health: {}", report.health.as_str());
    println!(
        "Findings: {} (warnings: {}, errors: {})",
        report.summary.total_findings, report.summary.warnings, report.summary.errors
    );
    if report.findings.is_empty() {
        println!("No findings detected.");
        return Ok(());
    }
    println!();
    for finding in report.findings {
        println!(
            "- [{}] {} ({})",
            finding.severity.as_str(),
            finding.summary,
            finding.issue_code
        );
        if let Some(source) = finding.source_manager_id {
            println!("  source_manager: {source}");
        }
        if let Some(package) = finding.package_name {
            println!("  package: {package}");
        }
        println!("  fingerprint: {}", finding.fingerprint);
    }
    Ok(())
}

fn cmd_doctor_repair(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() || is_help_token(&command_args[0]) {
        if options.json {
            emit_help_json_payload(Some(Command::Doctor), &["repair".to_string()], true);
        } else {
            print_doctor_repair_help();
        }
        return Ok(());
    }

    match command_args[0].as_str() {
        "plan" => {
            if command_args.len() != 5 {
                return Err(
                    "doctor repair plan requires: <manager-id> <source-manager-id> <package-name> <issue-code>"
                        .to_string(),
                );
            }
            let manager = parse_manager_id(&command_args[1])?;
            let source_manager = parse_manager_id(&command_args[2])?;
            let package_name = command_args[3].trim();
            let issue_code = command_args[4].trim();
            let plan = helm_core::repair::plan_for_issue(
                manager,
                source_manager,
                package_name,
                issue_code,
            )
            .ok_or_else(|| {
                format!(
                    "no repair plan available for manager='{}' source='{}' package='{}' issue='{}'",
                    manager.as_str(),
                    source_manager.as_str(),
                    package_name,
                    issue_code
                )
            })?;

            if options.json {
                emit_json_payload("helm.cli.v1.doctor.repair.plan", json!({ "plan": plan }));
                return Ok(());
            }

            println!(
                "Repair plan for {} / {} / {}:",
                manager.as_str(),
                source_manager.as_str(),
                package_name
            );
            println!(
                "  fingerprint: {}\n  knowledge: {} ({})",
                plan.fingerprint, plan.knowledge_source, plan.knowledge_version
            );
            for option in plan.options {
                println!(
                    "  - {} [{}]{}",
                    option.option_id,
                    option.action.as_str(),
                    if option.recommended {
                        " (recommended)"
                    } else {
                        ""
                    }
                );
                println!("    {}", option.description);
            }
            Ok(())
        }
        "apply" => {
            if command_args.len() != 6 {
                return Err(
                    "doctor repair apply requires: <manager-id> <source-manager-id> <package-name> <issue-code> <option-id>"
                        .to_string(),
                );
            }
            let manager = parse_manager_id(&command_args[1])?;
            let source_manager = parse_manager_id(&command_args[2])?;
            let package_name = command_args[3].trim().to_string();
            let issue_code = command_args[4].trim();
            let option_id = command_args[5].trim();
            let plan = helm_core::repair::plan_for_issue(
                manager,
                source_manager,
                package_name.as_str(),
                issue_code,
            )
            .ok_or_else(|| {
                format!(
                    "no repair plan available for manager='{}' source='{}' package='{}' issue='{}'",
                    manager.as_str(),
                    source_manager.as_str(),
                    package_name,
                    issue_code
                )
            })?;
            let option = helm_core::repair::resolve_option(&plan, option_id)
                .ok_or_else(|| format!("unknown repair option '{}'", option_id))?;

            let store_handle = Arc::new(SqliteStore::new(store.database_path().to_path_buf()));
            store_handle
                .migrate_to_latest()
                .map_err(|error| format!("failed to initialize store for repair apply: {error}"))?;

            match option.action {
                helm_core::repair::RepairAction::ReinstallManagerViaHomebrew => {
                    let args = vec![
                        manager.as_str().to_string(),
                        "--method".to_string(),
                        "homebrew".to_string(),
                    ];
                    cmd_managers_mutation(store_handle, options, "install", args.as_slice())
                }
                helm_core::repair::RepairAction::RemoveStalePackageEntry => {
                    let args = vec![
                        format!("{}@{}", package_name, source_manager.as_str()),
                        "--yes".to_string(),
                    ];
                    cmd_packages_mutation(store_handle, options, "uninstall", args.as_slice())
                }
                helm_core::repair::RepairAction::ApplyPostInstallSetupDefaults => {
                    let manager_instances = store_handle
                        .list_install_instances(Some(manager))
                        .map_err(|error| {
                            format!(
                                "failed to list manager install instances for repair apply: {error}"
                            )
                        })?;
                    let automation_result =
                        helm_core::post_install_setup::apply_recommended_post_install_setup(
                            manager,
                            Some(manager_instances.as_slice()),
                        )
                        .map_err(|error| {
                            format!(
                                "failed to apply recommended post-install setup for '{}': {error}",
                                manager.as_str()
                            )
                        })?;
                    if options.json {
                        emit_json_payload(
                            "helm.cli.v1.doctor.repair.apply_post_install_setup",
                            json!({
                                "manager": manager.as_str(),
                                "changed": automation_result.changed,
                                "rc_file": automation_result.rc_file.display().to_string(),
                                "summary": automation_result.summary
                            }),
                        );
                    } else {
                        println!(
                            "Applied post-install setup for {}: {} ({})",
                            manager.as_str(),
                            automation_result.summary,
                            automation_result.rc_file.display()
                        );
                    }
                    cmd_managers_detect(store_handle, options, &[manager.as_str().to_string()])
                }
            }
        }
        other => Err(format!(
            "unsupported doctor repair subcommand '{}'; currently supported: plan, apply",
            other
        )),
    }
}

fn cmd_completion(options: GlobalOptions, command_args: &[String]) -> Result<(), String> {
    if command_args.is_empty() || is_help_token(&command_args[0]) {
        if options.json {
            emit_help_json_payload(Some(Command::Completion), &[], true);
        } else {
            print_completion_help();
        }
        return Ok(());
    }

    if command_args.len() != 1 {
        return Err(
            "completion requires exactly one shell argument: bash, zsh, or fish".to_string(),
        );
    }

    match command_args[0].as_str() {
        "bash" => {
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.completion",
                    json!({
                        "shell": "bash",
                        "script": BASH_COMPLETION_SCRIPT
                    }),
                );
            } else {
                print!("{BASH_COMPLETION_SCRIPT}");
            }
            Ok(())
        }
        "zsh" => {
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.completion",
                    json!({
                        "shell": "zsh",
                        "script": ZSH_COMPLETION_SCRIPT
                    }),
                );
            } else {
                print!("{ZSH_COMPLETION_SCRIPT}");
            }
            Ok(())
        }
        "fish" => {
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.completion",
                    json!({
                        "shell": "fish",
                        "script": FISH_COMPLETION_SCRIPT
                    }),
                );
            } else {
                print!("{FISH_COMPLETION_SCRIPT}");
            }
            Ok(())
        }
        other => Err(format!(
            "unsupported completion shell '{}' (expected: bash, zsh, fish)",
            other
        )),
    }
}

fn cmd_internal_coordinator(
    store: Arc<SqliteStore>,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return Err("internal coordinator requires a subcommand".to_string());
    }

    match command_args[0].as_str() {
        "serve" => {
            let socket_path = parse_internal_coordinator_socket_path(&command_args[1..])?;
            run_coordinator_server(store, socket_path)
        }
        other => Err(format!(
            "unsupported internal coordinator subcommand '{}'",
            other
        )),
    }
}

fn parse_internal_coordinator_socket_path(command_args: &[String]) -> Result<PathBuf, String> {
    let transport = coordinator_transport::parse_internal_coordinator_state_dir_arg(command_args)?
        .map(coordinator_transport::FileIpcCoordinatorTransport::from_state_dir);
    match transport {
        Some(transport) => Ok(transport.state_dir().to_path_buf()),
        None => coordinator_socket_path(),
    }
}

fn coordinator_socket_path() -> Result<PathBuf, String> {
    let db_path = database_path()?;
    let transport = coordinator_transport::FileIpcCoordinatorTransport::for_database_path(
        Path::new(db_path.as_str()),
    );
    Ok(transport.state_dir().to_path_buf())
}

fn coordinator_file_transport(
    state_dir: &std::path::Path,
) -> coordinator_transport::FileIpcCoordinatorTransport {
    coordinator_transport::FileIpcCoordinatorTransport::from_state_dir(state_dir.to_path_buf())
}

fn coordinator_ready_file(state_dir: &std::path::Path) -> PathBuf {
    coordinator_file_transport(state_dir).ready_file()
}

fn coordinator_requests_dir(state_dir: &std::path::Path) -> PathBuf {
    coordinator_file_transport(state_dir).requests_dir()
}

fn coordinator_responses_dir(state_dir: &std::path::Path) -> PathBuf {
    coordinator_file_transport(state_dir).responses_dir()
}

fn coordinator_request_file(state_dir: &std::path::Path, request_id: &str) -> PathBuf {
    coordinator_file_transport(state_dir).request_file(request_id)
}

fn coordinator_response_file(state_dir: &std::path::Path, request_id: &str) -> PathBuf {
    coordinator_file_transport(state_dir).response_file(request_id)
}

const COORDINATOR_BOOTSTRAP_LOCK_WAIT_TIMEOUT_MS: u64 =
    coordinator_transport::COORDINATOR_BOOTSTRAP_LOCK_WAIT_TIMEOUT_MS;
const COORDINATOR_DAEMON_READY_TIMEOUT_MS: u64 =
    coordinator_transport::COORDINATOR_DAEMON_READY_TIMEOUT_MS;

fn coordinator_bootstrap_lock_file(state_dir: &std::path::Path) -> PathBuf {
    coordinator_file_transport(state_dir).bootstrap_lock_file()
}

struct CoordinatorBootstrapLockGuard {
    lock_file: PathBuf,
}

impl Drop for CoordinatorBootstrapLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.lock_file.as_path());
    }
}

fn try_clear_stale_coordinator_bootstrap_lock(lock_file: &std::path::Path) -> Result<bool, String> {
    coordinator_transport::try_clear_stale_coordinator_bootstrap_lock(lock_file)
}

fn acquire_coordinator_bootstrap_lock(
    state_dir: &std::path::Path,
) -> Result<CoordinatorBootstrapLockGuard, String> {
    std::fs::create_dir_all(state_dir).map_err(|error| {
        format!(
            "failed to create coordinator bootstrap state directory '{}': {error}",
            state_dir.display()
        )
    })?;
    let lock_file = coordinator_bootstrap_lock_file(state_dir);
    let started = Instant::now();

    loop {
        let open_result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_file.as_path());

        match open_result {
            Ok(mut file) => {
                file.write_all(std::process::id().to_string().as_bytes())
                    .map_err(|error| {
                        format!(
                            "failed to write coordinator bootstrap lock '{}': {error}",
                            lock_file.display()
                        )
                    })?;
                file.sync_all().map_err(|error| {
                    format!(
                        "failed to flush coordinator bootstrap lock '{}': {error}",
                        lock_file.display()
                    )
                })?;
                return Ok(CoordinatorBootstrapLockGuard { lock_file });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if try_clear_stale_coordinator_bootstrap_lock(lock_file.as_path())? {
                    continue;
                }
                if started.elapsed()
                    >= Duration::from_millis(COORDINATOR_BOOTSTRAP_LOCK_WAIT_TIMEOUT_MS)
                {
                    return Err(format!(
                        "timed out waiting for coordinator bootstrap lock '{}'",
                        lock_file.display()
                    ));
                }
                thread::sleep(coordinator_bootstrap_lock_poll_interval(started.elapsed()));
            }
            Err(error) => {
                return Err(format!(
                    "failed to create coordinator bootstrap lock '{}': {error}",
                    lock_file.display()
                ));
            }
        }
    }
}

fn next_coordinator_request_id() -> String {
    let counter = COORDINATOR_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{counter}", std::process::id())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &std::path::Path) -> Result<(), String> {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!(
            "failed to set private directory permissions on '{}': {error}",
            path.display()
        )
    })
}

fn ensure_private_directory(path: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| {
        format!(
            "failed to create coordinator directory '{}': {error}",
            path.display()
        )
    })?;
    #[cfg(unix)]
    {
        set_private_directory_permissions(path)?;
    }
    Ok(())
}

fn write_private_json_temp_file(path: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    #[cfg(unix)]
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|error| {
                format!(
                    "failed to create private temp json file '{}': {error}",
                    path.display()
                )
            })?;
        file.write_all(bytes).map_err(|error| {
            format!(
                "failed to write private temp json file '{}': {error}",
                path.display()
            )
        })?;
        file.sync_all().map_err(|error| {
            format!(
                "failed to flush private temp json file '{}': {error}",
                path.display()
            )
        })?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes).map_err(|error| {
            format!(
                "failed to write temp json file '{}': {error}",
                path.display()
            )
        })
    }
}

fn write_json_file<T: Serialize>(path: &std::path::Path, value: &T) -> Result<(), String> {
    let rendered = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode json file '{}': {error}", path.display()))?;
    let temp_name = format!(
        "{}.tmp-{}-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("payload"),
        std::process::id(),
        COORDINATOR_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let temp_path = path.with_file_name(temp_name);
    write_private_json_temp_file(&temp_path, rendered.as_slice())?;
    std::fs::rename(&temp_path, path).map_err(|error| {
        format!(
            "failed to move temp json file '{}' into '{}': {error}",
            temp_path.display(),
            path.display()
        )
    })?;
    Ok(())
}

fn read_json_file<T: DeserializeOwned>(path: &std::path::Path) -> Result<T, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read json file '{}': {error}", path.display()))?;
    serde_json::from_str::<T>(&content)
        .map_err(|error| format!("failed to decode json file '{}': {error}", path.display()))
}

fn file_modified_unix_seconds(path: &std::path::Path) -> Option<i64> {
    coordinator_transport::file_modified_unix_seconds(path)
}

fn process_is_alive(pid: u32) -> bool {
    coordinator_transport::process_is_alive(pid)
}

fn coordinator_process_looks_owned(pid: u32, state_dir: &std::path::Path) -> bool {
    coordinator_transport::coordinator_process_looks_owned(pid, state_dir)
}

fn terminate_coordinator_process_if_owned(pid: u32, state_dir: &std::path::Path) {
    if !coordinator_process_looks_owned(pid, state_dir) {
        return;
    }

    let _ = std::process::Command::new("/bin/kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status();
    for _ in 0..20 {
        if !process_is_alive(pid) {
            return;
        }
        thread::sleep(Duration::from_millis(25));
    }
    let _ = std::process::Command::new("/bin/kill")
        .arg("-KILL")
        .arg(pid.to_string())
        .status();
}

fn inspect_coordinator_state_health(state_dir: &std::path::Path) -> CoordinatorStateHealth {
    let ready_path = coordinator_ready_file(state_dir);
    if !ready_path.exists() {
        return CoordinatorStateHealth::default();
    }

    let mut health = CoordinatorStateHealth {
        ready_file_present: true,
        last_heartbeat_unix: file_modified_unix_seconds(ready_path.as_path()),
        ..CoordinatorStateHealth::default()
    };

    match read_json_file::<CoordinatorReadyState>(ready_path.as_path()) {
        Ok(ready) => {
            health.pid = Some(ready.pid);
            health.pid_alive = Some(process_is_alive(ready.pid));
            health.executable_path = ready.executable_path.clone();
            health.executable_exists = ready
                .executable_path
                .as_ref()
                .map(|value| std::path::Path::new(value).exists());
            health.last_heartbeat_unix = Some(ready.heartbeat_unix);

            if health.pid_alive == Some(false) {
                health.stale_reasons.push("pid_not_alive".to_string());
            }
            if health.executable_exists == Some(false) {
                health.stale_reasons.push("executable_missing".to_string());
            }
        }
        Err(_) => {
            health
                .stale_reasons
                .push("ready_file_decode_failed".to_string());
        }
    }
    health
}

fn recover_stale_coordinator_state(
    state_dir: &std::path::Path,
    health: &CoordinatorStateHealth,
) -> Result<bool, String> {
    if !health.is_stale() {
        return Ok(false);
    }

    if let (Some(pid), Some(true)) = (health.pid, health.pid_alive) {
        terminate_coordinator_process_if_owned(pid, state_dir);
    }
    reset_coordinator_state_dir(state_dir)?;
    Ok(true)
}

fn write_coordinator_ready_state(
    state_dir: &std::path::Path,
    started_at: i64,
) -> Result<(), String> {
    let ready = CoordinatorReadyState {
        pid: std::process::id(),
        started_at,
        heartbeat_unix: json_generated_at_unix(),
        executable_path: env::current_exe()
            .ok()
            .map(|path| path.to_string_lossy().to_string()),
    };
    write_json_file(coordinator_ready_file(state_dir).as_path(), &ready)
}

fn is_coordinator_timeout_error(error: &str) -> bool {
    coordinator_transport::is_coordinator_timeout_error(error)
}

fn should_launch_coordinator_on_demand(
    start_if_needed: bool,
    launched_for_recovery: bool,
    error: &str,
) -> bool {
    coordinator_transport::should_launch_coordinator_on_demand(
        start_if_needed,
        launched_for_recovery,
        error,
    )
}

fn reset_coordinator_state_dir(state_dir: &std::path::Path) -> Result<(), String> {
    if state_dir.exists() {
        std::fs::remove_dir_all(state_dir).map_err(|error| {
            format!(
                "failed to reset coordinator state directory '{}': {error}",
                state_dir.display()
            )
        })?;
    }
    ensure_private_directory(state_dir)?;
    ensure_private_directory(coordinator_requests_dir(state_dir).as_path())?;
    ensure_private_directory(coordinator_responses_dir(state_dir).as_path())?;
    Ok(())
}

fn spawn_coordinator_daemon(socket_path: &std::path::Path) -> Result<(), String> {
    verbose_log(format!(
        "spawning coordinator daemon with state dir '{}'",
        socket_path.display()
    ));
    reset_coordinator_state_dir(socket_path)?;

    let executable = env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    let mut command = std::process::Command::new(executable);
    command
        .arg("__coordinator__")
        .arg("serve")
        .arg("--state-dir")
        .arg(socket_path.as_os_str())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    if let Ok(path) = database_path() {
        command.env("HELM_DB_PATH", path);
    }
    if verbose_enabled() {
        command.env("HELM_CLI_VERBOSE", "1");
    }

    command
        .spawn()
        .map_err(|error| format!("failed to spawn coordinator daemon: {error}"))?;

    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(COORDINATOR_DAEMON_READY_TIMEOUT_MS) {
        if let Ok(response) = send_coordinator_request_once(socket_path, &CoordinatorRequest::Ping)
            && response.ok
        {
            return Ok(());
        }
        thread::sleep(coordinator_startup_poll_interval(started.elapsed()));
    }

    Err("coordinator daemon did not become ready in time".to_string())
}

fn ensure_coordinator_daemon_running(socket_path: &std::path::Path) -> Result<(), String> {
    let _lock = acquire_coordinator_bootstrap_lock(socket_path)?;
    if let Ok(response) = send_coordinator_request_once(socket_path, &CoordinatorRequest::Ping)
        && response.ok
    {
        return Ok(());
    }
    spawn_coordinator_daemon(socket_path)
}

fn coordinator_send_request_external(
    request: &CoordinatorRequest,
    start_if_needed: bool,
) -> Result<CoordinatorResponse, String> {
    let socket_path = coordinator_socket_path()?;
    verbose_log(format!(
        "sending coordinator request kind='{}' start_if_needed={} state_dir='{}'",
        coordinator_request_kind(request),
        start_if_needed,
        socket_path.display()
    ));
    let initial_health = inspect_coordinator_state_health(socket_path.as_path());
    if initial_health.is_stale() {
        verbose_log(format!(
            "detected stale coordinator state in '{}': {}",
            socket_path.display(),
            initial_health.stale_reasons.join(", ")
        ));
        recover_stale_coordinator_state(socket_path.as_path(), &initial_health)?;
    }

    match send_coordinator_request_once(socket_path.as_path(), request) {
        Ok(response) => {
            verbose_log(format!(
                "coordinator response kind='{}' ok={} task_id={:?} job_id={:?}",
                coordinator_request_kind(request),
                response.ok,
                response.task_id,
                response.job_id
            ));
            Ok(response)
        }
        Err(error) => {
            let mut effective_error = error;
            let mut launched_for_recovery = false;

            if is_coordinator_timeout_error(effective_error.as_str()) {
                let timeout_health = inspect_coordinator_state_health(socket_path.as_path());
                if timeout_health.is_stale() {
                    verbose_log(format!(
                        "coordinator request timeout with stale state in '{}': {}",
                        socket_path.display(),
                        timeout_health.stale_reasons.join(", ")
                    ));
                    recover_stale_coordinator_state(socket_path.as_path(), &timeout_health)?;
                    if start_if_needed {
                        ensure_coordinator_daemon_running(socket_path.as_path())?;
                        launched_for_recovery = true;
                    }
                    match send_coordinator_request_once(socket_path.as_path(), request) {
                        Ok(response) => {
                            verbose_log(format!(
                                "coordinator response after stale-timeout recovery kind='{}' ok={} task_id={:?} job_id={:?}",
                                coordinator_request_kind(request),
                                response.ok,
                                response.task_id,
                                response.job_id
                            ));
                            return Ok(response);
                        }
                        Err(retry_error) => {
                            effective_error = retry_error;
                        }
                    }
                }
            }

            if !should_launch_coordinator_on_demand(
                start_if_needed,
                launched_for_recovery,
                effective_error.as_str(),
            ) {
                return Err(effective_error);
            }
            verbose_log(format!(
                "coordinator request kind='{}' failed before startup: {}; attempting launch-on-demand",
                coordinator_request_kind(request),
                effective_error
            ));
            ensure_coordinator_daemon_running(socket_path.as_path())?;
            let response = send_coordinator_request_once(socket_path.as_path(), request)?;
            verbose_log(format!(
                "coordinator response after launch kind='{}' ok={} task_id={:?} job_id={:?}",
                coordinator_request_kind(request),
                response.ok,
                response.task_id,
                response.job_id
            ));
            Ok(response)
        }
    }
}

fn coordinator_send_request_local(
    store: &SqliteStore,
    request: &CoordinatorRequest,
) -> Result<CoordinatorResponse, String> {
    let local_store = Arc::new(SqliteStore::new(store.database_path().to_path_buf()));
    local_store
        .migrate_to_latest()
        .map_err(|error| format!("failed to initialize local coordinator store: {error}"))?;
    let runtime = build_adapter_runtime(local_store.clone())?;
    Ok(handle_coordinator_request(
        &runtime,
        local_store.as_ref(),
        request.clone(),
    ))
}

fn send_coordinator_request_once(
    socket_path: &std::path::Path,
    request: &CoordinatorRequest,
) -> Result<CoordinatorResponse, String> {
    let ready_file = coordinator_ready_file(socket_path);
    if !ready_file.exists() {
        return Err(format!(
            "failed to connect to coordinator at '{}': coordinator not ready",
            socket_path.display()
        ));
    }

    let request_id = next_coordinator_request_id();
    let request_file = coordinator_request_file(socket_path, &request_id);
    let response_file = coordinator_response_file(socket_path, &request_id);

    write_json_file(request_file.as_path(), request)?;

    let timeout = coordinator_request_timeout();
    let started = Instant::now();
    while started.elapsed() < timeout {
        if response_file.exists() {
            let response = read_json_file::<CoordinatorResponse>(response_file.as_path())?;
            let _ = std::fs::remove_file(response_file.as_path());
            return Ok(response);
        }
        thread::sleep(coordinator_response_poll_interval(started.elapsed()));
    }

    let _ = std::fs::remove_file(request_file.as_path());
    Err(format!(
        "timed out waiting for coordinator response in '{}'",
        socket_path.display()
    ))
}

fn coordinator_submit_request(
    store: &SqliteStore,
    manager: ManagerId,
    request: CoordinatorSubmitRequest,
    execution_mode: ExecutionMode,
) -> Result<CoordinatorResponse, String> {
    let wait = execution_mode == ExecutionMode::Wait;
    let coordinator_request = CoordinatorRequest::Submit {
        manager_id: manager.as_str().to_string(),
        request,
        wait,
    };
    let transport = coordinator_transport_for_submit(execution_mode);
    let response = match transport {
        CoordinatorClientTransport::LocalInProcess => {
            coordinator_send_request_local(store, &coordinator_request)?
        }
        CoordinatorClientTransport::ExternalFileIpc => {
            coordinator_send_request_external(&coordinator_request, true)?
        }
    };

    coordinator_response_or_error(response, "coordinator submit request failed")
}

fn coordinator_cancel_task(task_id: u64) -> Result<(), String> {
    let response = match coordinator_transport_for_cancel() {
        CoordinatorClientTransport::LocalInProcess => {
            let store = open_store()?;
            coordinator_send_request_local(store.as_ref(), &CoordinatorRequest::Cancel { task_id })?
        }
        CoordinatorClientTransport::ExternalFileIpc => {
            coordinator_send_request_external(&CoordinatorRequest::Cancel { task_id }, false)?
        }
    };
    coordinator_response_or_error(response, &format!("failed to cancel task '{}'", task_id))
        .map(|_| ())
}

fn coordinator_start_workflow(
    store: &SqliteStore,
    workflow: CoordinatorWorkflowRequest,
    execution_mode: ExecutionMode,
) -> Result<CoordinatorResponse, String> {
    let coordinator_request = CoordinatorRequest::StartWorkflow { workflow };
    let response = match coordinator_transport_for_workflow(execution_mode) {
        CoordinatorClientTransport::LocalInProcess => {
            coordinator_send_request_local(store, &coordinator_request)?
        }
        CoordinatorClientTransport::ExternalFileIpc => {
            coordinator_send_request_external(&coordinator_request, true)?
        }
    };
    coordinator_response_or_error(response, "coordinator workflow request failed")
}

fn coordinator_response_or_error(
    response: CoordinatorResponse,
    fallback_error: &str,
) -> Result<CoordinatorResponse, String> {
    if response.ok {
        return Ok(response);
    }
    let message = response.error.unwrap_or_else(|| fallback_error.to_string());
    if let Some(exit_code) = response.exit_code {
        return Err(mark_exit_code(message, exit_code));
    }
    Err(message)
}

fn run_coordinator_server(store: Arc<SqliteStore>, socket_path: PathBuf) -> Result<(), String> {
    verbose_log(format!(
        "coordinator server starting with state dir '{}'",
        socket_path.display()
    ));
    if socket_path.exists() {
        if let Ok(response) =
            send_coordinator_request_once(socket_path.as_path(), &CoordinatorRequest::Ping)
            && response.ok
        {
            verbose_log("coordinator already active; serve call returning");
            return Ok(());
        }
        reset_coordinator_state_dir(socket_path.as_path())?;
    } else {
        std::fs::create_dir_all(socket_path.as_path()).map_err(|error| {
            format!(
                "failed to create coordinator state directory '{}': {error}",
                socket_path.display()
            )
        })?;
        std::fs::create_dir_all(coordinator_requests_dir(socket_path.as_path()).as_path())
            .map_err(|error| {
                format!(
                    "failed to create coordinator requests directory '{}': {error}",
                    coordinator_requests_dir(socket_path.as_path()).display()
                )
            })?;
        std::fs::create_dir_all(coordinator_responses_dir(socket_path.as_path()).as_path())
            .map_err(|error| {
                format!(
                    "failed to create coordinator responses directory '{}': {error}",
                    coordinator_responses_dir(socket_path.as_path()).display()
                )
            })?;
    }

    let requests_dir = coordinator_requests_dir(socket_path.as_path());
    let runtime = Arc::new(build_adapter_runtime(store.clone())?);
    let started_at_unix = json_generated_at_unix();
    write_coordinator_ready_state(socket_path.as_path(), started_at_unix)?;
    verbose_log("coordinator ready and processing requests");
    let mut next_auto_check_tick = Instant::now();
    let mut next_ready_heartbeat_tick = Instant::now() + Duration::from_secs(2);
    let mut empty_poll_iterations = 0u32;
    loop {
        if Instant::now() >= next_ready_heartbeat_tick {
            if let Err(error) =
                write_coordinator_ready_state(socket_path.as_path(), started_at_unix)
            {
                verbose_log(format!("coordinator heartbeat write failed: {}", error));
            }
            next_ready_heartbeat_tick = Instant::now() + Duration::from_secs(2);
        }

        if Instant::now() >= next_auto_check_tick {
            if let Err(error) = run_due_auto_check(store.as_ref()) {
                verbose_log(format!("auto-check tick failed: {}", error));
            }
            next_auto_check_tick = Instant::now() + Duration::from_secs(30);
        }

        let mut entries: Vec<_> = std::fs::read_dir(requests_dir.as_path())
            .map_err(|error| {
                format!(
                    "failed to read coordinator requests directory '{}': {error}",
                    requests_dir.display()
                )
            })?
            .flatten()
            .collect();
        entries.sort_by_key(|entry| entry.file_name());

        if entries.is_empty() {
            empty_poll_iterations = empty_poll_iterations.saturating_add(1);
            thread::sleep(coordinator_server_idle_poll_interval(empty_poll_iterations));
            continue;
        }
        empty_poll_iterations = 0;

        for entry in entries {
            let request_path = entry.path();
            if !request_path.is_file() {
                continue;
            }

            let request_id = request_path
                .file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.to_string())
                .unwrap_or_else(next_coordinator_request_id);
            let response_path = coordinator_response_file(socket_path.as_path(), &request_id);

            let response = match read_json_file::<CoordinatorRequest>(request_path.as_path()) {
                Ok(request) => {
                    verbose_log(format!(
                        "coordinator handling request kind='{}'",
                        coordinator_request_kind(&request)
                    ));
                    handle_coordinator_request(runtime.as_ref(), store.as_ref(), request)
                }
                Err(error) => CoordinatorResponse {
                    ok: false,
                    task_id: None,
                    job_id: None,
                    payload: None,
                    exit_code: Some(1),
                    error: Some(error),
                },
            };

            let _ = write_json_file(response_path.as_path(), &response);
            let _ = std::fs::remove_file(request_path.as_path());
        }
    }
}

fn handle_coordinator_request(
    runtime: &AdapterRuntime,
    store: &SqliteStore,
    request: CoordinatorRequest,
) -> CoordinatorResponse {
    match request {
        CoordinatorRequest::Ping => CoordinatorResponse {
            ok: true,
            task_id: None,
            job_id: None,
            payload: None,
            exit_code: None,
            error: None,
        },
        CoordinatorRequest::Cancel { task_id } => {
            let tokio_runtime = match cli_tokio_runtime() {
                Ok(runtime) => runtime,
                Err(error) => {
                    return CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id),
                        job_id: None,
                        payload: None,
                        exit_code: Some(1),
                        error: Some(error),
                    };
                }
            };
            match tokio_runtime
                .block_on(runtime.cancel(TaskId(task_id), CancellationMode::Immediate))
            {
                Ok(()) => CoordinatorResponse {
                    ok: true,
                    task_id: Some(task_id),
                    job_id: None,
                    payload: None,
                    exit_code: None,
                    error: None,
                },
                Err(error) => CoordinatorResponse {
                    ok: false,
                    task_id: Some(task_id),
                    job_id: None,
                    payload: None,
                    exit_code: Some(1),
                    error: Some(format_core_error(error)),
                },
            }
        }
        CoordinatorRequest::Submit {
            manager_id,
            request,
            wait,
        } => {
            let manager = match parse_manager_id(&manager_id) {
                Ok(manager) => manager,
                Err(error) => {
                    return CoordinatorResponse {
                        ok: false,
                        task_id: None,
                        job_id: None,
                        payload: None,
                        exit_code: Some(1),
                        error: Some(error),
                    };
                }
            };
            let adapter_request = coordinator_submit_request_to_adapter(manager, request);
            if let Err(error) = sync_manager_executable_overrides(store) {
                return CoordinatorResponse {
                    ok: false,
                    task_id: None,
                    job_id: None,
                    payload: None,
                    exit_code: Some(1),
                    error: Some(error),
                };
            }
            let tokio_runtime = match cli_tokio_runtime() {
                Ok(runtime) => runtime,
                Err(error) => {
                    return CoordinatorResponse {
                        ok: false,
                        task_id: None,
                        job_id: None,
                        payload: None,
                        exit_code: Some(1),
                        error: Some(error),
                    };
                }
            };

            let submitted = tokio_runtime.block_on(runtime.submit(manager, adapter_request));
            let task_id = match submitted {
                Ok(task_id) => task_id,
                Err(error) => {
                    return CoordinatorResponse {
                        ok: false,
                        task_id: None,
                        job_id: None,
                        payload: None,
                        exit_code: Some(1),
                        error: Some(format_core_error(error)),
                    };
                }
            };

            if !wait {
                return CoordinatorResponse {
                    ok: true,
                    task_id: Some(task_id.0),
                    job_id: None,
                    payload: None,
                    exit_code: None,
                    error: None,
                };
            }

            let snapshot = tokio_runtime.block_on(runtime.wait_for_terminal(task_id, None));
            match snapshot {
                Ok(snapshot) => match snapshot.terminal_state {
                    Some(AdapterTaskTerminalState::Succeeded(response)) => CoordinatorResponse {
                        ok: true,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: Some(adapter_response_to_coordinator_payload(response)),
                        exit_code: None,
                        error: None,
                    },
                    Some(AdapterTaskTerminalState::Failed(error)) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        exit_code: Some(2),
                        error: Some(format_core_error(error)),
                    },
                    Some(AdapterTaskTerminalState::Cancelled(Some(error))) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        exit_code: Some(4),
                        error: Some(format_core_error(error)),
                    },
                    Some(AdapterTaskTerminalState::Cancelled(None)) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        exit_code: Some(4),
                        error: Some(format!("task {} was cancelled", task_id.0)),
                    },
                    None => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        exit_code: Some(2),
                        error: Some(format!(
                            "task {} reached terminal state without outcome payload",
                            task_id.0
                        )),
                    },
                },
                Err(error) => CoordinatorResponse {
                    ok: false,
                    task_id: Some(task_id.0),
                    job_id: None,
                    payload: None,
                    exit_code: Some(1),
                    error: Some(format_core_error(error)),
                },
            }
        }
        CoordinatorRequest::StartWorkflow { workflow } => {
            let job_id = next_coordinator_request_id();
            let runtime_clone = runtime.clone();
            let store_clone = Arc::new(SqliteStore::new(store.database_path().to_path_buf()));
            if let Err(error) = store_clone.migrate_to_latest() {
                return CoordinatorResponse {
                    ok: false,
                    task_id: None,
                    job_id: Some(job_id),
                    payload: None,
                    exit_code: Some(1),
                    error: Some(format!(
                        "failed to initialize coordinator workflow store: {error}"
                    )),
                };
            }

            thread::spawn(move || {
                let _ = run_coordinator_workflow(runtime_clone, store_clone, workflow);
            });

            CoordinatorResponse {
                ok: true,
                task_id: None,
                job_id: Some(job_id),
                payload: None,
                exit_code: None,
                error: None,
            }
        }
    }
}

fn run_coordinator_workflow(
    runtime: AdapterRuntime,
    store: Arc<SqliteStore>,
    workflow: CoordinatorWorkflowRequest,
) -> Result<(), String> {
    let workflow_kind = coordinator_workflow_kind(&workflow).to_string();
    verbose_log(format!("coordinator workflow started: {}", workflow_kind));
    let tokio_runtime = cli_tokio_runtime()?;
    let result = match workflow {
        CoordinatorWorkflowRequest::RefreshAll => {
            let rows = tokio_runtime.block_on(refresh_all_no_timeout(&runtime));
            let failures = rows.iter().filter(|row| !row.success).count();
            if let Some(error) = manager_operation_failure_error("refresh", failures) {
                return Err(error);
            }
            Ok(())
        }
        CoordinatorWorkflowRequest::RefreshManager { manager_id } => {
            let manager = parse_manager_id(&manager_id)?;
            tokio_runtime.block_on(refresh_single_manager(&runtime, manager))
        }
        CoordinatorWorkflowRequest::DetectAll => {
            let rows = tokio_runtime.block_on(detect_all_no_timeout(&runtime));
            let failures = rows.iter().filter(|row| !row.success).count();
            if let Some(error) = manager_operation_failure_error("detection", failures) {
                return Err(error);
            }
            Ok(())
        }
        CoordinatorWorkflowRequest::UpdatesRun {
            include_pinned,
            allow_os_updates,
            manager_id,
        } => {
            let manager_filter = manager_id.as_deref().map(parse_manager_id).transpose()?;
            let steps = collect_upgrade_execution_steps(
                store.as_ref(),
                &runtime,
                include_pinned,
                allow_os_updates,
                manager_filter,
            )?;
            let failures = count_upgrade_step_failures(&steps, |step| {
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: step.manager,
                        name: upgrade_request_name(step),
                    }),
                });
                tokio_runtime
                    .block_on(submit_request_wait(&runtime, step.manager, request))
                    .map(|_| ())
            });
            if let Some(error) = manager_operation_failure_error("upgrade", failures) {
                return Err(error);
            }
            Ok(())
        }
    };
    verbose_log(format!(
        "coordinator workflow finished: {} => {}",
        workflow_kind,
        if result.is_ok() { "ok" } else { "error" }
    ));
    result
}

fn coordinator_submit_request_to_adapter(
    manager: ManagerId,
    request: CoordinatorSubmitRequest,
) -> AdapterRequest {
    match request {
        CoordinatorSubmitRequest::Detect => AdapterRequest::Detect(DetectRequest),
        CoordinatorSubmitRequest::Search { query } => AdapterRequest::Search(SearchRequest {
            query: SearchQuery {
                text: query,
                issued_at: SystemTime::now(),
            },
        }),
        CoordinatorSubmitRequest::RustupAddComponent {
            toolchain,
            component,
        } => AdapterRequest::RustupAddComponent(RustupAddComponentRequest {
            toolchain,
            component,
        }),
        CoordinatorSubmitRequest::RustupRemoveComponent {
            toolchain,
            component,
        } => AdapterRequest::RustupRemoveComponent(RustupRemoveComponentRequest {
            toolchain,
            component,
        }),
        CoordinatorSubmitRequest::RustupAddTarget { toolchain, target } => {
            AdapterRequest::RustupAddTarget(RustupAddTargetRequest { toolchain, target })
        }
        CoordinatorSubmitRequest::RustupRemoveTarget { toolchain, target } => {
            AdapterRequest::RustupRemoveTarget(RustupRemoveTargetRequest { toolchain, target })
        }
        CoordinatorSubmitRequest::RustupSetDefaultToolchain { toolchain } => {
            AdapterRequest::RustupSetDefaultToolchain(RustupSetDefaultToolchainRequest {
                toolchain,
            })
        }
        CoordinatorSubmitRequest::RustupSetOverride { toolchain, path } => {
            AdapterRequest::RustupSetOverride(RustupSetOverrideRequest {
                toolchain,
                path: PathBuf::from(path),
            })
        }
        CoordinatorSubmitRequest::RustupUnsetOverride { toolchain, path } => {
            AdapterRequest::RustupUnsetOverride(RustupUnsetOverrideRequest {
                toolchain,
                path: PathBuf::from(path),
            })
        }
        CoordinatorSubmitRequest::RustupSetProfile { profile } => {
            AdapterRequest::RustupSetProfile(RustupSetProfileRequest { profile })
        }
        CoordinatorSubmitRequest::Install {
            package_name,
            version,
        } => AdapterRequest::Install(InstallRequest {
            package: PackageRef {
                manager,
                name: package_name,
            },
            version,
        }),
        CoordinatorSubmitRequest::Uninstall { package_name } => {
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager,
                    name: package_name,
                },
            })
        }
        CoordinatorSubmitRequest::Upgrade { package_name } => {
            AdapterRequest::Upgrade(UpgradeRequest {
                package: package_name.map(|name| PackageRef { manager, name }),
            })
        }
        CoordinatorSubmitRequest::Pin {
            package_name,
            version,
        } => AdapterRequest::Pin(PinRequest {
            package: PackageRef {
                manager,
                name: package_name,
            },
            version,
        }),
        CoordinatorSubmitRequest::Unpin { package_name } => AdapterRequest::Unpin(UnpinRequest {
            package: PackageRef {
                manager,
                name: package_name,
            },
        }),
    }
}

fn adapter_request_to_coordinator_submit(
    request: AdapterRequest,
) -> Result<CoordinatorSubmitRequest, String> {
    match request {
        AdapterRequest::Detect(_) => Ok(CoordinatorSubmitRequest::Detect),
        AdapterRequest::Search(search) => Ok(CoordinatorSubmitRequest::Search {
            query: search.query.text,
        }),
        AdapterRequest::RustupAddComponent(request) => {
            Ok(CoordinatorSubmitRequest::RustupAddComponent {
                toolchain: request.toolchain,
                component: request.component,
            })
        }
        AdapterRequest::RustupRemoveComponent(request) => {
            Ok(CoordinatorSubmitRequest::RustupRemoveComponent {
                toolchain: request.toolchain,
                component: request.component,
            })
        }
        AdapterRequest::RustupAddTarget(request) => Ok(CoordinatorSubmitRequest::RustupAddTarget {
            toolchain: request.toolchain,
            target: request.target,
        }),
        AdapterRequest::RustupRemoveTarget(request) => {
            Ok(CoordinatorSubmitRequest::RustupRemoveTarget {
                toolchain: request.toolchain,
                target: request.target,
            })
        }
        AdapterRequest::RustupSetDefaultToolchain(request) => {
            Ok(CoordinatorSubmitRequest::RustupSetDefaultToolchain {
                toolchain: request.toolchain,
            })
        }
        AdapterRequest::RustupSetOverride(request) => {
            Ok(CoordinatorSubmitRequest::RustupSetOverride {
                toolchain: request.toolchain,
                path: request.path.to_string_lossy().to_string(),
            })
        }
        AdapterRequest::RustupUnsetOverride(request) => {
            Ok(CoordinatorSubmitRequest::RustupUnsetOverride {
                toolchain: request.toolchain,
                path: request.path.to_string_lossy().to_string(),
            })
        }
        AdapterRequest::RustupSetProfile(request) => {
            Ok(CoordinatorSubmitRequest::RustupSetProfile {
                profile: request.profile,
            })
        }
        AdapterRequest::Install(install) => Ok(CoordinatorSubmitRequest::Install {
            package_name: install.package.name,
            version: install.version,
        }),
        AdapterRequest::Uninstall(uninstall) => Ok(CoordinatorSubmitRequest::Uninstall {
            package_name: uninstall.package.name,
        }),
        AdapterRequest::Upgrade(upgrade) => Ok(CoordinatorSubmitRequest::Upgrade {
            package_name: upgrade.package.map(|package| package.name),
        }),
        AdapterRequest::Pin(pin) => Ok(CoordinatorSubmitRequest::Pin {
            package_name: pin.package.name,
            version: pin.version,
        }),
        AdapterRequest::Unpin(unpin) => Ok(CoordinatorSubmitRequest::Unpin {
            package_name: unpin.package.name,
        }),
        unsupported => Err(format!(
            "coordinator submit request does not support adapter action '{:?}'",
            unsupported.action()
        )),
    }
}

fn adapter_response_to_coordinator_payload(response: AdapterResponse) -> CoordinatorPayload {
    match response {
        AdapterResponse::Detection(info) => CoordinatorPayload::Detection {
            installed: info.installed,
            version: info.version,
            executable_path: info
                .executable_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
        },
        AdapterResponse::Refreshed => CoordinatorPayload::Refreshed,
        AdapterResponse::SnapshotSync {
            installed: _,
            outdated: _,
        } => CoordinatorPayload::Refreshed,
        AdapterResponse::InstalledPackages(packages) => CoordinatorPayload::InstalledPackages {
            count: packages.len(),
        },
        AdapterResponse::OutdatedPackages(packages) => CoordinatorPayload::OutdatedPackages {
            count: packages.len(),
        },
        AdapterResponse::SearchResults(results) => CoordinatorPayload::SearchResults {
            count: results.len(),
        },
        AdapterResponse::Mutation(mutation) => CoordinatorPayload::Mutation {
            manager_id: mutation.package.manager.as_str().to_string(),
            package_name: mutation.package.name,
            action: format!("{:?}", mutation.action).to_lowercase(),
            before_version: mutation.before_version,
            after_version: mutation.after_version,
        },
    }
}

fn cmd_diagnostics_summary(store: &SqliteStore, options: GlobalOptions) -> Result<(), String> {
    let summary = build_diagnostics_summary(store)?;
    if options.json {
        emit_json_payload(
            "helm.cli.v1.diagnostics.summary",
            json!({ "summary": summary }),
        );
        return Ok(());
    }

    println!("Diagnostics Summary");
    println!("  installed_packages: {}", summary.installed_packages);
    println!("  updatable_packages: {}", summary.updatable_packages);
    println!("  enabled_managers: {}", summary.enabled_managers);
    println!(
        "  detected_enabled_managers: {}",
        summary.detected_enabled_managers
    );
    println!("  queued_tasks: {}", summary.queued_tasks);
    println!("  running_tasks: {}", summary.running_tasks);
    println!("  completed_tasks: {}", summary.completed_tasks);
    println!("  failed_tasks: {}", summary.failed_tasks);
    println!("  cancelled_tasks: {}", summary.cancelled_tasks);
    if summary.failed_task_ids.is_empty() {
        println!("  failed_task_ids: -");
    } else {
        println!(
            "  failed_task_ids: {}",
            summary
                .failed_task_ids
                .iter()
                .map(|task_id| task_id.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if summary.undetected_enabled_managers.is_empty() {
        println!("  undetected_enabled_managers: -");
    } else {
        println!(
            "  undetected_enabled_managers: {}",
            summary.undetected_enabled_managers.join(", ")
        );
    }
    if summary.failure_classes.is_empty() {
        println!("  failure_classes: -");
    } else {
        println!("  failure_classes:");
        for (class, count) in &summary.failure_classes {
            if let Some(hint) = summary.failure_class_hints.get(class) {
                println!("    {class}: {count} ({hint})");
            } else {
                println!("    {class}: {count}");
            }
        }
    }
    println!("  coordinator:");
    println!("    state_dir: {}", summary.coordinator.state_dir);
    println!(
        "    ready_file_present: {}",
        summary.coordinator.ready_file_present
    );
    println!(
        "    pid: {}",
        summary
            .coordinator
            .pid
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "    pid_alive: {}",
        summary
            .coordinator
            .pid_alive
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "    executable_path: {}",
        summary
            .coordinator
            .executable_path
            .as_deref()
            .unwrap_or("-")
    );
    println!(
        "    executable_exists: {}",
        summary
            .coordinator
            .executable_exists
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "    last_heartbeat_unix: {}",
        summary
            .coordinator
            .last_heartbeat_unix
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    if summary.coordinator.stale_reasons.is_empty() {
        println!("    stale_reasons: -");
    } else {
        println!(
            "    stale_reasons: {}",
            summary.coordinator.stale_reasons.join(", ")
        );
    }
    Ok(())
}

fn cmd_diagnostics_task(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let task_id = parse_task_id_argument(command_args, "diagnostics task")?;
    let recent_tasks = store
        .list_recent_tasks(TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list recent tasks: {error}"))?;
    let task = recent_tasks
        .into_iter()
        .find(|row| row.id.0 == task_id)
        .ok_or_else(|| format!("task '{}' not found in recent task window", task_id))?;
    let task_status = task.status;
    let task_view = task_to_cli_task(task);

    let logs = store
        .list_task_logs(TaskId(task_id), TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list task logs: {error}"))?
        .into_iter()
        .map(task_log_to_cli_record)
        .collect::<Vec<_>>();

    let output = helm_core::execution::task_output(TaskId(task_id));
    let output_payload = task_output_to_cli_diagnostics(output.as_ref());
    let diagnostics_error = build_task_diagnostics_error(task_status, &logs, output.as_ref());

    if options.json {
        emit_json_payload(
            "helm.cli.v1.diagnostics.task",
            json!({
                "task": task_view,
                "logs": logs,
                "output": output_payload,
                "error": diagnostics_error
            }),
        );
        return Ok(());
    }

    println!("Task Diagnostics #{}", task_view.id);
    println!("  manager: {}", task_view.manager);
    println!("  task_type: {}", task_view.task_type);
    println!("  status: {}", task_view.status);
    println!("  created_at_unix: {}", task_view.created_at_unix);
    println!("  log_entries: {}", logs.len());
    if let Some(last_log) = logs.first() {
        println!(
            "  last_log: [{}] [{}] {}",
            last_log.created_at_unix, last_log.level, last_log.message
        );
    } else {
        println!("  last_log: -");
    }
    if let Some(error) = diagnostics_error {
        println!("  error_code: {}", error.code);
        println!("  error_message: {}", error.message);
        println!("  error_hint: {}", error.hint.as_deref().unwrap_or("-"));
    } else {
        println!("  error_code: -");
        println!("  error_message: -");
        println!("  error_hint: -");
    }
    if output_payload.available {
        println!("  output_available: true");
        println!(
            "  command: {}",
            output_payload.command.as_deref().unwrap_or("-")
        );
        println!("  cwd: {}", output_payload.cwd.as_deref().unwrap_or("-"));
        println!(
            "  program_path: {}",
            output_payload.program_path.as_deref().unwrap_or("-")
        );
        println!(
            "  path_snippet: {}",
            output_payload.path_snippet.as_deref().unwrap_or("-")
        );
        println!(
            "  started_at_unix_ms: {}",
            output_payload
                .started_at_unix_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
        println!(
            "  finished_at_unix_ms: {}",
            output_payload
                .finished_at_unix_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
        println!(
            "  duration_ms: {}",
            output_payload
                .duration_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
        println!(
            "  exit_code: {}",
            output_payload
                .exit_code
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string())
        );
        println!(
            "  termination_reason: {}",
            output_payload.termination_reason.as_deref().unwrap_or("-")
        );
        println!(
            "  output_error_code: {}",
            output_payload.error_code.as_deref().unwrap_or("-")
        );
        println!(
            "  output_error_message: {}",
            output_payload.error_message.as_deref().unwrap_or("-")
        );
    } else {
        println!("  output_available: false");
    }
    Ok(())
}

fn cmd_diagnostics_manager(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return Err("diagnostics manager requires a manager id".to_string());
    }
    let manager_id = parse_manager_id(&command_args[0])?;
    let manager_id_str = manager_id.as_str().to_string();
    let manager = list_managers(store)?
        .into_iter()
        .find(|row| row.manager_id == manager_id_str)
        .ok_or_else(|| format!("manager '{}' not found", manager_id.as_str()))?;

    let recent_tasks = store
        .list_recent_tasks(TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list recent tasks: {error}"))?
        .into_iter()
        .filter(|task| task.manager == manager_id)
        .map(task_to_cli_task)
        .collect::<Vec<_>>();

    let mut recent_logs = Vec::new();
    for task in recent_tasks.iter().take(5) {
        let mut logs = store
            .list_task_logs(TaskId(task.id), 64)
            .map_err(|error| format!("failed to list logs for task '{}': {error}", task.id))?
            .into_iter()
            .map(task_log_to_cli_record)
            .collect::<Vec<_>>();
        recent_logs.append(&mut logs);
    }
    recent_logs.sort_by_key(|record| record.created_at_unix);
    recent_logs.reverse();
    if recent_logs.len() > 200 {
        recent_logs.truncate(200);
    }

    if options.json {
        emit_json_payload(
            "helm.cli.v1.diagnostics.manager",
            json!({
                "manager": manager,
                "recent_tasks": recent_tasks,
                "recent_logs": recent_logs
            }),
        );
        return Ok(());
    }

    println!("Manager Diagnostics: {}", manager.manager_id);
    println!("  display_name: {}", manager.display_name);
    println!("  authority: {}", manager.authority);
    println!("  enabled: {}", manager.enabled);
    println!("  detected: {}", manager.detected);
    println!("  version: {}", manager.version.as_deref().unwrap_or("-"));
    println!(
        "  executable_path: {}",
        manager.executable_path.as_deref().unwrap_or("-")
    );
    println!("  recent_tasks: {}", recent_tasks.len());
    println!("  recent_logs: {}", recent_logs.len());
    if let Some(last_error) = recent_logs.iter().find(|record| record.level == "error") {
        println!("  last_error: {}", last_error.message);
    } else {
        println!("  last_error: -");
    }
    Ok(())
}

fn cmd_diagnostics_provenance(options: GlobalOptions) -> Result<(), String> {
    let executable = env::current_exe()
        .map_err(|error| format!("failed to resolve current executable path: {error}"))?;
    let provenance = detect_install_provenance(&executable);
    let can_self_update = provenance_can_self_update(provenance.update_policy);
    let recommended_action = provenance_recommended_action(provenance.channel);

    if options.json {
        emit_json_payload(
            "helm.cli.v1.diagnostics.provenance",
            json!({
                "channel": provenance.channel.as_str(),
                "artifact": provenance.artifact,
                "installed_at": provenance.installed_at,
                "update_policy": provenance.update_policy.as_str(),
                "version": provenance.version,
                "source": provenance.source.as_str(),
                "marker_path": provenance.marker_path.to_string_lossy().to_string(),
                "executable_path": provenance.executable_path.to_string_lossy().to_string(),
                "can_self_update": can_self_update,
                "recommended_action": recommended_action
            }),
        );
        return Ok(());
    }

    println!("Installation Provenance");
    println!("  channel: {}", provenance.channel.as_str());
    println!("  artifact: {}", provenance.artifact);
    println!(
        "  installed_at: {}",
        provenance.installed_at.as_deref().unwrap_or("-")
    );
    println!("  update_policy: {}", provenance.update_policy.as_str());
    println!(
        "  version: {}",
        provenance.version.as_deref().unwrap_or("-")
    );
    println!("  source: {}", provenance.source.as_str());
    println!(
        "  marker_path: {}",
        provenance.marker_path.to_string_lossy()
    );
    println!(
        "  executable_path: {}",
        provenance.executable_path.to_string_lossy()
    );
    println!("  can_self_update: {can_self_update}");
    println!("  recommended_action: {recommended_action}");
    Ok(())
}

fn cmd_diagnostics_export(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    let export_path = parse_diagnostics_export_path(command_args)?;
    let summary = build_diagnostics_summary(store)?;
    let managers = list_managers(store)?;
    let tasks = store
        .list_recent_tasks(TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list recent tasks: {error}"))?
        .into_iter()
        .map(task_to_cli_task)
        .collect::<Vec<_>>();

    let failed_task_logs = tasks
        .iter()
        .filter(|task| task.status == "failed")
        .take(25)
        .map(|task| {
            let logs = store
                .list_task_logs(TaskId(task.id), 128)
                .map_err(|error| format!("failed to list task logs: {error}"))?
                .into_iter()
                .map(task_log_to_cli_record)
                .collect::<Vec<_>>();
            Ok(json!({
                "taskId": task.id,
                "logs": logs
            }))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let data = json!({
        "summary": summary,
        "managers": managers,
        "tasks": tasks,
        "failedTaskLogs": failed_task_logs
    });

    let payload = json!({
        "schema": "helm.cli.v1.diagnostics.export",
        "schema_version": JSON_SCHEMA_VERSION,
        "generated_at": json_generated_at_unix(),
        "data": data
    });

    if let Some(path) = export_path {
        let rendered = serde_json::to_string_pretty(&payload)
            .map_err(|error| format!("failed to serialize diagnostics export payload: {error}"))?;
        std::fs::write(&path, rendered).map_err(|error| {
            format!("failed to write diagnostics export to '{}': {error}", path)
        })?;
        if options.json {
            emit_json_payload(
                "helm.cli.v1.diagnostics.export.write",
                json!({
                    "path": path,
                    "written": true
                }),
            );
        } else {
            println!("Diagnostics export written to '{}'.", path);
        }
        return Ok(());
    }

    if options.json {
        emit_json_payload("helm.cli.v1.diagnostics.export", data);
        return Ok(());
    }

    let rendered = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("failed to serialize diagnostics export payload: {error}"))?;
    println!("{rendered}");
    Ok(())
}

fn parse_diagnostics_export_path(command_args: &[String]) -> Result<Option<String>, String> {
    let mut path: Option<String> = None;
    let mut index = 0usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--path" => {
                if index + 1 >= command_args.len() {
                    return Err("diagnostics export --path requires a value".to_string());
                }
                path = Some(command_args[index + 1].clone());
                index += 2;
            }
            other => {
                return Err(format!(
                    "unsupported diagnostics export argument '{}' (supported: --path <file>)",
                    other
                ));
            }
        }
    }
    Ok(path)
}

fn task_output_to_cli_diagnostics(output: Option<&TaskOutputRecord>) -> CliTaskDiagnosticsOutput {
    CliTaskDiagnosticsOutput {
        available: output.is_some(),
        command: output.and_then(|entry| entry.command.clone()),
        cwd: output.and_then(|entry| entry.cwd.clone()),
        program_path: output.and_then(|entry| entry.program_path.clone()),
        path_snippet: output.and_then(|entry| entry.path_snippet.clone()),
        started_at_unix_ms: output.and_then(|entry| entry.started_at_unix_ms),
        finished_at_unix_ms: output.and_then(|entry| entry.finished_at_unix_ms),
        duration_ms: output.and_then(|entry| entry.duration_ms),
        exit_code: output.and_then(|entry| entry.exit_code),
        termination_reason: output.and_then(|entry| entry.termination_reason.clone()),
        error_code: output.and_then(|entry| entry.error_code.clone()),
        error_message: output.and_then(|entry| entry.error_message.clone()),
        stdout: output.and_then(|entry| entry.stdout.clone()),
        stderr: output.and_then(|entry| entry.stderr.clone()),
    }
}

fn build_task_diagnostics_error(
    status: TaskStatus,
    logs: &[CliTaskLogRecord],
    output: Option<&TaskOutputRecord>,
) -> Option<CliTaskDiagnosticsError> {
    if !matches!(status, TaskStatus::Failed | TaskStatus::Cancelled) {
        return None;
    }

    if let Some(output) = output
        && let (Some(code), Some(message)) = (&output.error_code, &output.error_message)
        && !code.trim().is_empty()
        && !message.trim().is_empty()
    {
        let normalized_code = code.trim().to_string();
        return Some(CliTaskDiagnosticsError {
            hint: failure_class_hint_string(normalized_code.as_str()),
            code: normalized_code,
            message: message.clone(),
        });
    }

    if let Some(parsed) = parse_terminal_error_from_logs(status, logs, output) {
        return Some(parsed);
    }

    let fallback_message = output
        .and_then(|entry| entry.error_message.clone())
        .or_else(|| {
            output
                .and_then(|entry| entry.stderr.as_deref())
                .and_then(last_non_empty_line)
                .map(str::to_string)
        })
        .or_else(|| {
            logs.iter()
                .find(|record| matches!(record.level.as_str(), "error" | "warn"))
                .map(|record| record.message.clone())
        })
        .unwrap_or_else(|| match status {
            TaskStatus::Failed => "task failed without persisted diagnostics".to_string(),
            TaskStatus::Cancelled => "task cancelled without persisted diagnostics".to_string(),
            _ => "task error".to_string(),
        });
    let code = classify_failure_class(output, Some(fallback_message.as_str())).to_string();
    Some(CliTaskDiagnosticsError {
        hint: failure_class_hint_string(code.as_str()),
        code,
        message: fallback_message,
    })
}

fn parse_terminal_error_from_logs(
    status: TaskStatus,
    logs: &[CliTaskLogRecord],
    output: Option<&TaskOutputRecord>,
) -> Option<CliTaskDiagnosticsError> {
    let expected_prefix = match status {
        TaskStatus::Failed => "task failed",
        TaskStatus::Cancelled => "task cancelled",
        _ => return None,
    };

    for record in logs {
        if !matches!(record.level.as_str(), "error" | "warn") {
            continue;
        }
        let message = record.message.trim();
        if !message.starts_with(expected_prefix) {
            continue;
        }

        if let Some(parsed) = parse_structured_terminal_error_message(message, expected_prefix) {
            return Some(parsed);
        }

        if let Some(unstructured) = message
            .strip_prefix(expected_prefix)
            .and_then(|suffix| suffix.strip_prefix(':'))
            .map(str::trim)
            .filter(|detail| !detail.is_empty())
        {
            let code = classify_failure_class(output, Some(unstructured)).to_string();
            return Some(CliTaskDiagnosticsError {
                hint: failure_class_hint_string(code.as_str()),
                code,
                message: unstructured.to_string(),
            });
        }
    }
    None
}

fn parse_structured_terminal_error_message(
    message: &str,
    prefix: &str,
) -> Option<CliTaskDiagnosticsError> {
    let remainder = message.strip_prefix(prefix)?.trim_start();
    let bracketed = remainder.strip_prefix('[')?;
    let bracket_end = bracketed.find(']')?;
    let code = bracketed[..bracket_end].trim();
    if code.is_empty() {
        return None;
    }
    let detail = bracketed[bracket_end + 1..]
        .trim_start()
        .strip_prefix(':')
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(CliTaskDiagnosticsError {
        hint: failure_class_hint_string(code),
        code: code.to_string(),
        message: detail.to_string(),
    })
}

fn last_non_empty_line(value: &str) -> Option<&str> {
    value.lines().rev().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn classify_failure_class(
    output: Option<&TaskOutputRecord>,
    message: Option<&str>,
) -> &'static str {
    cli_errors::classify_failure_class(output, message)
}

fn failure_class_hint(code: &str) -> Option<&'static str> {
    cli_errors::failure_class_hint(code)
}

fn failure_class_hint_string(code: &str) -> Option<String> {
    cli_errors::failure_class_hint_string(code)
}

fn diagnose_failure_class_for_task(store: &SqliteStore, task_id: TaskId) -> Result<String, String> {
    let logs = store
        .list_task_logs(task_id, 128)
        .map_err(|error| format!("failed to list task logs for task '{}': {error}", task_id.0))?
        .into_iter()
        .map(task_log_to_cli_record)
        .collect::<Vec<_>>();
    let output = helm_core::execution::task_output(task_id);
    let message = logs
        .iter()
        .find(|record| {
            matches!(record.level.as_str(), "error" | "warn")
                || record.status.as_deref() == Some("failed")
        })
        .map(|record| record.message.as_str())
        .or_else(|| {
            output
                .as_ref()
                .and_then(|entry| entry.error_message.as_deref())
        })
        .or_else(|| {
            output
                .as_ref()
                .and_then(|entry| entry.stderr.as_deref())
                .and_then(last_non_empty_line)
        });

    Ok(classify_failure_class(output.as_ref(), message).to_string())
}

fn build_coordinator_health_summary() -> CliCoordinatorHealthSummary {
    match coordinator_socket_path() {
        Ok(state_dir) => {
            let health = inspect_coordinator_state_health(state_dir.as_path());
            CliCoordinatorHealthSummary {
                state_dir: state_dir.to_string_lossy().to_string(),
                ready_file_present: health.ready_file_present,
                pid: health.pid,
                pid_alive: health.pid_alive,
                executable_path: health.executable_path,
                executable_exists: health.executable_exists,
                last_heartbeat_unix: health.last_heartbeat_unix,
                stale_reasons: health.stale_reasons,
            }
        }
        Err(error) => CliCoordinatorHealthSummary {
            state_dir: format!("<unavailable: {error}>"),
            ready_file_present: false,
            pid: None,
            pid_alive: None,
            executable_path: None,
            executable_exists: None,
            last_heartbeat_unix: None,
            stale_reasons: vec!["state_dir_unavailable".to_string()],
        },
    }
}

fn build_diagnostics_summary(store: &SqliteStore) -> Result<CliDiagnosticsSummary, String> {
    let installed = store
        .list_installed()
        .map_err(|error| format!("failed to list installed packages: {error}"))?;
    let outdated = store
        .list_outdated()
        .map_err(|error| format!("failed to list outdated packages: {error}"))?;
    let tasks = store
        .list_recent_tasks(TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list recent tasks: {error}"))?;
    let managers = list_managers(store)?;

    let mut queued_tasks = 0usize;
    let mut running_tasks = 0usize;
    let mut completed_tasks = 0usize;
    let mut failed_tasks = 0usize;
    let mut cancelled_tasks = 0usize;
    let mut failed_task_ids = Vec::new();
    let mut failure_classes: BTreeMap<String, usize> = BTreeMap::new();
    let mut failure_class_hints: BTreeMap<String, String> = BTreeMap::new();
    for task in tasks {
        match task.status {
            TaskStatus::Queued => queued_tasks = queued_tasks.saturating_add(1),
            TaskStatus::Running => running_tasks = running_tasks.saturating_add(1),
            TaskStatus::Completed => completed_tasks = completed_tasks.saturating_add(1),
            TaskStatus::Failed => {
                failed_tasks = failed_tasks.saturating_add(1);
                failed_task_ids.push(task.id.0);
                let class = diagnose_failure_class_for_task(store, task.id)?;
                if let Some(hint) = failure_class_hint(class.as_str()) {
                    failure_class_hints
                        .entry(class.clone())
                        .or_insert_with(|| hint.to_string());
                }
                let entry = failure_classes.entry(class).or_insert(0);
                *entry = entry.saturating_add(1);
            }
            TaskStatus::Cancelled => cancelled_tasks = cancelled_tasks.saturating_add(1),
        }
    }

    let enabled_managers = managers.iter().filter(|manager| manager.enabled).count();
    let detected_enabled_managers = managers
        .iter()
        .filter(|manager| manager.enabled && manager.detected)
        .count();
    let undetected_enabled_managers = managers
        .iter()
        .filter(|manager| manager.enabled && !manager.detected)
        .map(|manager| manager.manager_id.clone())
        .collect::<Vec<_>>();

    Ok(CliDiagnosticsSummary {
        installed_packages: installed.len(),
        updatable_packages: outdated.len(),
        enabled_managers,
        detected_enabled_managers,
        queued_tasks,
        running_tasks,
        completed_tasks,
        failed_tasks,
        cancelled_tasks,
        failed_task_ids,
        undetected_enabled_managers,
        failure_classes,
        failure_class_hints,
        coordinator: build_coordinator_health_summary(),
    })
}

fn cmd_settings_list(store: &SqliteStore, options: GlobalOptions) -> Result<(), String> {
    let safe_mode = store
        .safe_mode()
        .map_err(|error| format!("failed to read safe_mode: {error}"))?;
    let homebrew_policy = store
        .homebrew_keg_policy()
        .map_err(|error| format!("failed to read homebrew_keg_policy: {error}"))?;
    let homebrew_auto_cleanup = matches!(homebrew_policy, HomebrewKegPolicy::Cleanup);
    let db_path = database_path()?;
    let auto_check_for_updates = store
        .auto_check_for_updates()
        .map_err(|error| format!("failed to read auto_check_for_updates: {error}"))?;
    let auto_check_frequency_minutes = store
        .auto_check_frequency_minutes()
        .map_err(|error| format!("failed to read auto_check_frequency_minutes: {error}"))?;
    let auto_check_last_checked_unix = store
        .auto_check_last_checked_unix()
        .map_err(|error| format!("failed to read auto_check_last_checked_unix: {error}"))?;
    let cli_onboarding_completed = store
        .cli_onboarding_completed()
        .map_err(|error| format!("failed to read cli_onboarding_completed: {error}"))?;
    let cli_accepted_license_terms_version = store
        .cli_accepted_license_terms_version()
        .map_err(|error| format!("failed to read cli_accepted_license_terms_version: {error}"))?;
    if options.json {
        emit_json_payload(
            "helm.cli.v1.settings.list",
            json!({
                "safe_mode": safe_mode,
                "homebrew_keg_auto_cleanup": homebrew_auto_cleanup,
                "database_path": db_path,
                "auto_check_for_updates": auto_check_for_updates,
                "auto_check_frequency_minutes": auto_check_frequency_minutes,
                "auto_check_last_checked_unix": auto_check_last_checked_unix,
                "cli_onboarding_completed": cli_onboarding_completed,
                "cli_accepted_license_terms_version": cli_accepted_license_terms_version
            }),
        );
        return Ok(());
    }

    println!("Settings");
    println!("  safe_mode: {safe_mode}");
    println!("  homebrew_keg_auto_cleanup: {homebrew_auto_cleanup}");
    println!("  database_path: {db_path}");
    println!("  auto_check_for_updates: {auto_check_for_updates}");
    println!("  auto_check_frequency_minutes: {auto_check_frequency_minutes}");
    println!(
        "  auto_check_last_checked_unix: {}",
        auto_check_last_checked_unix
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string())
    );
    println!("  cli_onboarding_completed: {cli_onboarding_completed}");
    println!(
        "  cli_accepted_license_terms_version: {}",
        cli_accepted_license_terms_version.unwrap_or_else(|| "null".to_string())
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct ParsedTaskLogOptions {
    task_id: u64,
    limit: usize,
    level_filter: Option<String>,
    status_filter: Option<String>,
    poll_ms: Option<u64>,
    timeout_ms: Option<u64>,
}

fn parse_task_id_argument(command_args: &[String], command_name: &str) -> Result<u64, String> {
    if command_args.is_empty() {
        return Err(format!("{command_name} requires a task id"));
    }
    command_args[0]
        .parse::<u64>()
        .map_err(|_| format!("invalid task id '{}'", command_args[0]))
}

fn parse_tasks_log_options(
    command_args: &[String],
    command_name: &str,
) -> Result<ParsedTaskLogOptions, String> {
    let task_id = parse_task_id_argument(command_args, &format!("tasks {command_name}"))?;
    let mut limit: usize = 200;
    let mut level_filter: Option<String> = None;
    let mut status_filter: Option<String> = None;
    let mut poll_ms: Option<u64> = None;
    let mut timeout_ms: Option<u64> = None;

    let mut index = 1usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--limit" => {
                if index + 1 >= command_args.len() {
                    return Err(format!("tasks {command_name} --limit requires a value"));
                }
                limit = command_args[index + 1]
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value '{}'", command_args[index + 1]))?;
                index += 2;
            }
            "--level" => {
                if index + 1 >= command_args.len() {
                    return Err(format!("tasks {command_name} --level requires a value"));
                }
                let normalized = command_args[index + 1].to_ascii_lowercase();
                if !matches!(normalized.as_str(), "info" | "warn" | "error") {
                    return Err(format!(
                        "unsupported log level '{}' (expected: info, warn, error)",
                        command_args[index + 1]
                    ));
                }
                level_filter = Some(normalized);
                index += 2;
            }
            "--status" => {
                if index + 1 >= command_args.len() {
                    return Err(format!("tasks {command_name} --status requires a value"));
                }
                let normalized = command_args[index + 1].to_ascii_lowercase();
                if !matches!(
                    normalized.as_str(),
                    "queued" | "running" | "completed" | "failed" | "cancelled"
                ) {
                    return Err(format!(
                        "unsupported task status '{}' (expected: queued, running, completed, failed, cancelled)",
                        command_args[index + 1]
                    ));
                }
                status_filter = Some(normalized);
                index += 2;
            }
            "--poll-ms" => {
                if command_name != "follow" {
                    return Err("--poll-ms is only supported by 'tasks follow'".to_string());
                }
                if index + 1 >= command_args.len() {
                    return Err("tasks follow --poll-ms requires a value".to_string());
                }
                let value = command_args[index + 1].parse::<u64>().map_err(|_| {
                    format!("invalid --poll-ms value '{}'", command_args[index + 1])
                })?;
                if value == 0 {
                    return Err("--poll-ms must be greater than 0".to_string());
                }
                poll_ms = Some(value);
                index += 2;
            }
            "--timeout-ms" => {
                if command_name != "follow" {
                    return Err("--timeout-ms is only supported by 'tasks follow'".to_string());
                }
                if index + 1 >= command_args.len() {
                    return Err("tasks follow --timeout-ms requires a value".to_string());
                }
                let value = command_args[index + 1].parse::<u64>().map_err(|_| {
                    format!("invalid --timeout-ms value '{}'", command_args[index + 1])
                })?;
                if value == 0 {
                    return Err("--timeout-ms must be greater than 0".to_string());
                }
                timeout_ms = Some(value);
                index += 2;
            }
            other => {
                return Err(format!(
                    "unsupported tasks {command_name} argument '{other}'"
                ));
            }
        }
    }

    Ok(ParsedTaskLogOptions {
        task_id,
        limit,
        level_filter,
        status_filter,
        poll_ms,
        timeout_ms,
    })
}

fn task_status_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Failed => "failed",
    }
}

fn task_log_level_str(level: TaskLogLevel) -> &'static str {
    match level {
        TaskLogLevel::Info => "info",
        TaskLogLevel::Warn => "warn",
        TaskLogLevel::Error => "error",
    }
}

fn find_task_status(store: &SqliteStore, task_id: u64) -> Result<Option<TaskStatus>, String> {
    let tasks = store
        .list_recent_tasks(TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list recent tasks: {error}"))?;
    Ok(tasks
        .into_iter()
        .find(|task| task.id.0 == task_id)
        .map(|task| task.status))
}

fn cancel_inflight_tasks_for_manager(
    store: &SqliteStore,
    manager: ManagerId,
) -> (Vec<u64>, Vec<String>) {
    let tasks = match store.list_recent_tasks(TASK_FETCH_LIMIT) {
        Ok(tasks) => tasks,
        Err(error) => {
            return (
                Vec::new(),
                vec![format!(
                    "failed to list recent tasks for cancellation: {error}"
                )],
            );
        }
    };

    let inflight_task_ids = tasks
        .into_iter()
        .filter(|task| {
            task.manager == manager
                && matches!(task.status, TaskStatus::Queued | TaskStatus::Running)
        })
        .map(|task| task.id.0)
        .collect::<Vec<_>>();
    if inflight_task_ids.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let mut cancelled_task_ids = Vec::new();
    let mut cancellation_errors = Vec::new();
    for task_id in inflight_task_ids {
        match coordinator_cancel_task(task_id) {
            Ok(()) => cancelled_task_ids.push(task_id),
            Err(error) => {
                cancellation_errors.push(format!("task {}: {}", task_id, error));
            }
        }
    }

    (cancelled_task_ids, cancellation_errors)
}

fn task_log_to_cli_record(record: helm_core::models::TaskLogRecord) -> CliTaskLogRecord {
    let created_at_unix = record
        .created_at
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    CliTaskLogRecord {
        id: record.id,
        task_id: record.task_id.0,
        manager: record.manager.as_str().to_string(),
        task_type: format!("{:?}", record.task_type).to_lowercase(),
        status: record
            .status
            .map(|value| task_status_str(value).to_string()),
        level: task_log_level_str(record.level).to_string(),
        message: record.message,
        created_at_unix,
    }
}

fn require_nonempty_packages_rustup_arg(raw: &str, label: &str) -> Result<String, String> {
    let normalized = raw.trim();
    if normalized.is_empty() {
        return Err(format!("packages rustup {label} cannot be empty"));
    }
    Ok(normalized.to_string())
}

fn require_absolute_packages_rustup_path(raw: &str, label: &str) -> Result<String, String> {
    let normalized = require_nonempty_packages_rustup_arg(raw, label)?;
    if !Path::new(normalized.as_str()).is_absolute() {
        return Err(format!(
            "packages rustup {label} requires an absolute path (received '{}')",
            normalized
        ));
    }
    Ok(normalized)
}

fn parse_packages_rustup_args(command_args: &[String]) -> Result<RustupPackagesCommand, String> {
    if command_args.is_empty() {
        return Err(
            "packages rustup requires a subcommand: show, component, target, default, override, profile"
                .to_string(),
        );
    }

    match command_args[0].as_str() {
        "show" => {
            if command_args.len() != 2 {
                return Err("packages rustup show requires <toolchain>".to_string());
            }
            Ok(RustupPackagesCommand::Show {
                toolchain: require_nonempty_packages_rustup_arg(&command_args[1], "toolchain")?,
            })
        }
        "component" => {
            if command_args.len() != 4 {
                return Err(
                    "packages rustup component requires <add|remove> <toolchain> <component>"
                        .to_string(),
                );
            }
            let toolchain = require_nonempty_packages_rustup_arg(&command_args[2], "toolchain")?;
            let component = require_nonempty_packages_rustup_arg(&command_args[3], "component")?;
            match command_args[1].as_str() {
                "add" => Ok(RustupPackagesCommand::ComponentAdd {
                    toolchain,
                    component,
                }),
                "remove" => Ok(RustupPackagesCommand::ComponentRemove {
                    toolchain,
                    component,
                }),
                other => Err(format!(
                    "unsupported packages rustup component action '{}'; supported: add, remove",
                    other
                )),
            }
        }
        "target" => {
            if command_args.len() != 4 {
                return Err(
                    "packages rustup target requires <add|remove> <toolchain> <target>".to_string(),
                );
            }
            let toolchain = require_nonempty_packages_rustup_arg(&command_args[2], "toolchain")?;
            let target = require_nonempty_packages_rustup_arg(&command_args[3], "target")?;
            match command_args[1].as_str() {
                "add" => Ok(RustupPackagesCommand::TargetAdd { toolchain, target }),
                "remove" => Ok(RustupPackagesCommand::TargetRemove { toolchain, target }),
                other => Err(format!(
                    "unsupported packages rustup target action '{}'; supported: add, remove",
                    other
                )),
            }
        }
        "default" => {
            if command_args.len() != 2 {
                return Err("packages rustup default requires <toolchain>".to_string());
            }
            Ok(RustupPackagesCommand::Default {
                toolchain: require_nonempty_packages_rustup_arg(&command_args[1], "toolchain")?,
            })
        }
        "override" => {
            if command_args.len() != 4 {
                return Err(
                    "packages rustup override requires <set|unset> <toolchain> <absolute-path>"
                        .to_string(),
                );
            }
            let toolchain = require_nonempty_packages_rustup_arg(&command_args[2], "toolchain")?;
            let path = require_absolute_packages_rustup_path(&command_args[3], "path")?;
            match command_args[1].as_str() {
                "set" => Ok(RustupPackagesCommand::OverrideSet { toolchain, path }),
                "unset" => Ok(RustupPackagesCommand::OverrideUnset { toolchain, path }),
                other => Err(format!(
                    "unsupported packages rustup override action '{}'; supported: set, unset",
                    other
                )),
            }
        }
        "profile" => {
            if command_args.len() != 3 {
                return Err("packages rustup profile requires set <profile>".to_string());
            }
            match command_args[1].as_str() {
                "set" => Ok(RustupPackagesCommand::ProfileSet {
                    profile: require_nonempty_packages_rustup_arg(&command_args[2], "profile")?,
                }),
                other => Err(format!(
                    "unsupported packages rustup profile action '{}'; supported: set",
                    other
                )),
            }
        }
        other => Err(format!(
            "unsupported packages rustup subcommand '{}'; supported: show, component, target, default, override, profile",
            other
        )),
    }
}

#[derive(Debug, Clone)]
struct ParsedPackageShowArgs {
    package_name: String,
    manager: Option<ManagerId>,
    coordinate_hint: Option<(String, String)>,
}

#[derive(Debug, Clone)]
struct ParsedPackageMutationArgs {
    package_name: String,
    manager: ManagerId,
    version: Option<String>,
    preview: bool,
    yes: bool,
}

fn parse_package_show_args(command_args: &[String]) -> Result<ParsedPackageShowArgs, String> {
    if command_args.is_empty() {
        return Err("packages show requires a package name".to_string());
    }

    let (package_name, mut selector_manager) = parse_package_selector(&command_args[0])?;
    let mut index = 1usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--manager" => {
                if index + 1 >= command_args.len() {
                    return Err("packages show --manager requires a manager id".to_string());
                }
                let manager = parse_manager_id(&command_args[index + 1])?;
                if let Some(existing) = selector_manager
                    && existing != manager
                {
                    return Err(format!(
                        "conflicting manager selectors '{}' and '{}'",
                        existing.as_str(),
                        manager.as_str()
                    ));
                }
                selector_manager = Some(manager);
                index += 2;
            }
            other => {
                return Err(format!("unsupported packages show argument '{other}'"));
            }
        }
    }

    let coordinate_hint = package_coordinate_hint(package_name.as_str());

    Ok(ParsedPackageShowArgs {
        package_name,
        manager: selector_manager,
        coordinate_hint,
    })
}

fn parse_package_mutation_args(
    subcommand: &str,
    command_args: &[String],
    allow_version: bool,
) -> Result<ParsedPackageMutationArgs, String> {
    if command_args.is_empty() {
        return Err("package mutation requires a package name".to_string());
    }

    let (mut package_name, mut selector_manager) = parse_package_selector(&command_args[0])?;
    let mut manager: Option<ManagerId> = selector_manager.take();
    let mut version: Option<String> = None;
    let uninstall_command = subcommand == "uninstall";
    let mut preview = false;
    let mut yes = false;

    let mut index = 1usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--manager" => {
                if index + 1 >= command_args.len() {
                    return Err("package mutation --manager requires a manager id".to_string());
                }
                let value = parse_manager_id(&command_args[index + 1])?;
                if let Some(existing) = manager
                    && existing != value
                {
                    return Err(format!(
                        "conflicting manager selectors '{}' and '{}'",
                        existing.as_str(),
                        value.as_str()
                    ));
                }
                manager = Some(value);
                index += 2;
            }
            "--version" => {
                if !allow_version {
                    return Err("this package command does not support --version".to_string());
                }
                if index + 1 >= command_args.len() {
                    return Err("package mutation --version requires a value".to_string());
                }
                let value = command_args[index + 1].trim().to_string();
                if value.is_empty() {
                    return Err("package mutation --version cannot be empty".to_string());
                }
                version = Some(value);
                index += 2;
            }
            "--preview" if uninstall_command => {
                preview = true;
                index += 1;
            }
            "--yes" if uninstall_command => {
                yes = true;
                index += 1;
            }
            other => {
                return Err(format!("unsupported package mutation argument '{other}'"));
            }
        }
    }

    let manager = manager
        .ok_or_else(|| "package mutation requires --manager <id> or name@manager".to_string())?;

    if allow_version
        && let Some((coordinate_package_name, coordinate_version)) =
            package_coordinate_hint_for_manager(package_name.as_str(), manager)
    {
        if let Some(explicit_version) = version.as_ref()
            && explicit_version != &coordinate_version
        {
            return Err(format!(
                "conflicting version selectors '{}' and '{}'; remove one or make them match",
                coordinate_version, explicit_version
            ));
        }
        package_name = coordinate_package_name;
        version = Some(coordinate_version);
    }

    Ok(ParsedPackageMutationArgs {
        package_name,
        manager,
        version,
        preview,
        yes,
    })
}

fn manager_supports_package_coordinate_versions(manager: ManagerId) -> bool {
    matches!(manager, ManagerId::Asdf | ManagerId::Mise)
}

fn package_coordinate_hint(raw: &str) -> Option<(String, String)> {
    let coordinate = PackageCoordinate::parse(raw)?;
    let selector = coordinate.version_selector?;
    if coordinate.package_name == raw {
        return None;
    }
    Some((coordinate.package_name, selector.raw))
}

fn package_coordinate_hint_for_manager(raw: &str, manager: ManagerId) -> Option<(String, String)> {
    if !manager_supports_package_coordinate_versions(manager) {
        return None;
    }
    package_coordinate_hint(raw)
}

fn parse_package_selector(raw: &str) -> Result<(String, Option<ManagerId>), String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("package name cannot be empty".to_string());
    }

    if let Some((name, manager_raw)) = trimmed.rsplit_once('@')
        && !name.trim().is_empty()
        && !manager_raw.trim().is_empty()
    {
        let manager_raw = manager_raw.trim();
        if let Ok(manager) = parse_manager_id(manager_raw) {
            return Ok((name.trim().to_string(), Some(manager)));
        }
    }

    Ok((trimmed.to_string(), None))
}

fn parse_homebrew_keg_policy_package_selector(raw: &str) -> Result<PackageRef, String> {
    let (package_name, selected_manager) = parse_package_selector(raw)?;
    let manager = selected_manager.unwrap_or(ManagerId::HomebrewFormula);
    if manager != ManagerId::HomebrewFormula {
        return Err(format!(
            "packages keg-policy only supports manager '{}' (received '{}')",
            ManagerId::HomebrewFormula.as_str(),
            manager.as_str()
        ));
    }
    Ok(PackageRef {
        manager,
        name: package_name,
    })
}

fn parse_homebrew_keg_policy_arg(raw: &str) -> Result<Option<HomebrewKegPolicy>, String> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "keep" => Ok(Some(HomebrewKegPolicy::Keep)),
        "cleanup" => Ok(Some(HomebrewKegPolicy::Cleanup)),
        "default" | "inherit" => Ok(None),
        other => Err(format!(
            "unsupported keg policy '{}' (expected: keep, cleanup, default)",
            other
        )),
    }
}

fn parse_manager_target(
    command_args: &[String],
    command_name: &str,
) -> Result<ManagerTarget, String> {
    if command_args.is_empty() {
        return Ok(ManagerTarget::All);
    }

    if command_args.len() == 1 {
        if command_args[0] == "--all" {
            return Ok(ManagerTarget::All);
        }
        return Ok(ManagerTarget::One(parse_manager_id(&command_args[0])?));
    }

    if command_args.len() == 2 && command_args[0] == "--manager" {
        return Ok(ManagerTarget::One(parse_manager_id(&command_args[1])?));
    }

    Err(format!(
        "invalid arguments for '{}'; expected '--all', '--manager <id>', or '<id>'",
        command_name
    ))
}

fn cli_tokio_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    if let Some(runtime) = CLI_TOKIO_RUNTIME.get() {
        return Ok(runtime);
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("failed to initialize tokio runtime: {error}"))?;

    let _ = CLI_TOKIO_RUNTIME.set(runtime);
    CLI_TOKIO_RUNTIME
        .get()
        .ok_or_else(|| "failed to initialize tokio runtime".to_string())
}

fn build_adapter_runtime(store: Arc<SqliteStore>) -> Result<AdapterRuntime, String> {
    sync_manager_executable_overrides(store.as_ref())?;

    let executor = Arc::new(TokioProcessExecutor);
    let adapters: Vec<Arc<dyn ManagerAdapter>> = vec![
        Arc::new(HomebrewAdapter::new(ProcessHomebrewSource::new(
            executor.clone(),
        ))),
        Arc::new(HomebrewCaskAdapter::new(ProcessHomebrewCaskSource::new(
            executor.clone(),
        ))),
        Arc::new(MiseAdapter::new(ProcessMiseSource::new(executor.clone()))),
        Arc::new(AsdfAdapter::new(ProcessAsdfSource::new(executor.clone()))),
        Arc::new(NpmAdapter::new(ProcessNpmSource::new(executor.clone()))),
        Arc::new(PnpmAdapter::new(ProcessPnpmSource::new(executor.clone()))),
        Arc::new(YarnAdapter::new(ProcessYarnSource::new(executor.clone()))),
        Arc::new(CargoAdapter::new(ProcessCargoSource::new(executor.clone()))),
        Arc::new(CargoBinstallAdapter::new(ProcessCargoBinstallSource::new(
            executor.clone(),
        ))),
        Arc::new(PipAdapter::new(ProcessPipSource::new(executor.clone()))),
        Arc::new(PipxAdapter::new(ProcessPipxSource::new(executor.clone()))),
        Arc::new(PoetryAdapter::new(ProcessPoetrySource::new(
            executor.clone(),
        ))),
        Arc::new(RubyGemsAdapter::new(ProcessRubyGemsSource::new(
            executor.clone(),
        ))),
        Arc::new(BundlerAdapter::new(ProcessBundlerSource::new(
            executor.clone(),
        ))),
        Arc::new(RustupAdapter::new(ProcessRustupSource::new(
            executor.clone(),
        ))),
        Arc::new(SoftwareUpdateAdapter::new(
            ProcessSoftwareUpdateSource::new(executor.clone()),
        )),
        Arc::new(MacPortsAdapter::new(ProcessMacPortsSource::new(
            executor.clone(),
        ))),
        Arc::new(NixDarwinAdapter::new(ProcessNixDarwinSource::new(
            executor.clone(),
        ))),
        Arc::new(MasAdapter::new(ProcessMasSource::new(executor.clone()))),
        Arc::new(DockerDesktopAdapter::new(ProcessDockerDesktopSource::new(
            executor.clone(),
        ))),
        Arc::new(PodmanAdapter::new(ProcessPodmanSource::new(
            executor.clone(),
        ))),
        Arc::new(ColimaAdapter::new(ProcessColimaSource::new(
            executor.clone(),
        ))),
        Arc::new(SparkleAdapter::new(ProcessSparkleSource::new(
            executor.clone(),
        ))),
        Arc::new(SetappAdapter::new(ProcessSetappSource::new(
            executor.clone(),
        ))),
        Arc::new(ParallelsDesktopAdapter::new(
            ProcessParallelsDesktopSource::new(executor.clone()),
        )),
        Arc::new(XcodeCommandLineToolsAdapter::new(
            ProcessXcodeCommandLineToolsSource::new(executor.clone()),
        )),
        Arc::new(Rosetta2Adapter::new(ProcessRosetta2Source::new(
            executor.clone(),
        ))),
        Arc::new(FirmwareUpdatesAdapter::new(
            ProcessFirmwareUpdatesSource::new(executor),
        )),
    ];

    AdapterRuntime::with_all_stores(adapters, store.clone(), store.clone(), store.clone(), store)
        .map_err(format_core_error)
}

fn sync_manager_executable_overrides(store: &SqliteStore) -> Result<(), String> {
    let detections: HashMap<ManagerId, DetectionInfo> = store
        .list_detections()
        .map_err(|error| format!("failed to list manager detections: {error}"))?
        .into_iter()
        .collect();

    let preferences: HashMap<ManagerId, helm_core::persistence::ManagerPreference> = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?
        .into_iter()
        .map(|preference| (preference.manager, preference))
        .collect();

    let mut executable_overrides: HashMap<ManagerId, PathBuf> = HashMap::new();
    let mut timeout_profiles: HashMap<ManagerId, ManagerTimeoutProfile> = HashMap::new();
    for manager in ManagerId::ALL {
        let preferred = preferences
            .get(&manager)
            .and_then(|preference| normalize_nonempty(preference.selected_executable_path.clone()));
        let detected = detections.get(&manager).and_then(|info| {
            info.executable_path
                .as_ref()
                .map(|path| path.to_string_lossy().to_string())
                .and_then(|path| normalize_nonempty(Some(path)))
        });
        let selected = preferred.or(detected);
        if let Some(path) = selected {
            executable_overrides.insert(manager, PathBuf::from(path));
        }

        let hard_timeout = preferences
            .get(&manager)
            .and_then(|preference| preference.timeout_hard_seconds)
            .filter(|value| *value > 0)
            .map(Duration::from_secs);
        let idle_timeout = preferences
            .get(&manager)
            .and_then(|preference| preference.timeout_idle_seconds)
            .filter(|value| *value > 0)
            .map(Duration::from_secs);
        let profile = ManagerTimeoutProfile {
            hard_timeout,
            idle_timeout,
        };
        if profile.hard_timeout.is_some() || profile.idle_timeout.is_some() {
            timeout_profiles.insert(manager, profile);
        }
    }
    replace_manager_execution_preferences(executable_overrides, timeout_profiles);
    Ok(())
}

async fn submit_request_wait(
    runtime: &AdapterRuntime,
    manager: ManagerId,
    request: AdapterRequest,
) -> Result<(TaskId, helm_core::adapters::AdapterResponse), String> {
    let task_id = runtime
        .submit(manager, request)
        .await
        .map_err(format_core_error)?;
    let snapshot = runtime
        .wait_for_terminal(task_id, None)
        .await
        .map_err(format_core_error)?;

    match snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(response)) => Ok((task_id, response)),
        Some(AdapterTaskTerminalState::Failed(error)) => {
            Err(mark_exit_code(format_core_error(error), 2))
        }
        Some(AdapterTaskTerminalState::Cancelled(Some(error))) => {
            Err(mark_exit_code(format_core_error(error), 4))
        }
        Some(AdapterTaskTerminalState::Cancelled(None)) => Err(mark_exit_code(
            format!("task {} was cancelled", task_id.0),
            4,
        )),
        None => Err(mark_exit_code(
            format!(
                "task {} reached terminal state without outcome payload",
                task_id.0
            ),
            2,
        )),
    }
}

async fn detect_all_no_timeout(runtime: &AdapterRuntime) -> Vec<CliManagerResult> {
    let adapters = runtime.adapter_list();
    let adapter_refs: Vec<&dyn ManagerAdapter> =
        adapters.iter().map(|adapter| adapter.as_ref()).collect();
    let phases = helm_core::orchestration::authority_order::detection_phases(&adapter_refs);
    let mut rows = Vec::new();

    for phase in phases {
        for manager in phase {
            if !runtime.is_manager_enabled(manager)
                || !runtime.supports_capability(manager, Capability::Detect)
            {
                rows.push(CliManagerResult {
                    manager_id: manager.as_str().to_string(),
                    success: true,
                    error: None,
                });
                continue;
            }

            let result =
                submit_request_wait(runtime, manager, AdapterRequest::Detect(DetectRequest))
                    .await
                    .map(|_| ());
            rows.push(CliManagerResult {
                manager_id: manager.as_str().to_string(),
                success: result.is_ok(),
                error: result.err(),
            });
        }
    }

    rows
}

async fn refresh_all_no_timeout(runtime: &AdapterRuntime) -> Vec<CliManagerResult> {
    let adapters = runtime.adapter_list();
    let adapter_refs: Vec<&dyn ManagerAdapter> =
        adapters.iter().map(|adapter| adapter.as_ref()).collect();
    let phases = helm_core::orchestration::authority_order::authority_phases(&adapter_refs);
    let mut rows = Vec::new();

    for phase in phases {
        for manager in phase {
            if !runtime.is_manager_enabled(manager) {
                rows.push(CliManagerResult {
                    manager_id: manager.as_str().to_string(),
                    success: true,
                    error: None,
                });
                continue;
            }

            let result = refresh_single_manager(runtime, manager).await;
            rows.push(CliManagerResult {
                manager_id: manager.as_str().to_string(),
                success: result.is_ok(),
                error: result.err(),
            });
        }
    }

    rows
}

fn emit_manager_results(
    options: GlobalOptions,
    schema: &str,
    heading: &str,
    rows: Vec<CliManagerResult>,
) -> usize {
    let failures = rows.iter().filter(|row| !row.success).count();

    if options.json {
        emit_json_payload(schema, json!(rows));
        return failures;
    }

    if rows.is_empty() {
        println!("{heading}: no managers matched.");
        return failures;
    }

    println!("{heading}");
    for row in rows {
        if row.success {
            println!("  {}: ok", row.manager_id);
        } else {
            println!(
                "  {}: failed ({})",
                row.manager_id,
                row.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }
    }
    failures
}

fn emit_detection_result(
    options: GlobalOptions,
    task_id: TaskId,
    manager: ManagerId,
    info: &DetectionInfo,
) {
    let executable_path = info
        .executable_path
        .as_ref()
        .map(|path| path.to_string_lossy().into_owned());

    if options.json {
        emit_json_payload(
            "helm.cli.v1.managers.detect.wait",
            json!({
                "task_id": task_id.0,
                "manager_id": manager.as_str(),
                "installed": info.installed,
                "version": info.version,
                "executable_path": executable_path
            }),
        );
        return;
    }

    println!("Detection complete for '{}'", manager.as_str());
    println!("  task_id: {}", task_id.0);
    println!("  installed: {}", info.installed);
    println!(
        "  version: {}",
        info.version.as_deref().unwrap_or("unknown-version")
    );
    println!(
        "  executable_path: {}",
        executable_path.as_deref().unwrap_or("-")
    );
}

fn format_core_error(error: helm_core::models::CoreError) -> String {
    let manager = error
        .manager
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!(
        "{} (manager={}, task={:?}, action={:?}, kind={:?})",
        error.message, manager, error.task, error.action, error.kind
    )
}

fn normalize_nonempty(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn json_generated_at_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn emit_json_payload(schema: &str, data: serde_json::Value) {
    let payloads =
        build_json_payload_lines(schema, data, ndjson_enabled(), json_generated_at_unix());
    for payload in payloads {
        println!("{payload}");
    }
}

fn build_json_payload_lines(
    schema: &str,
    data: serde_json::Value,
    ndjson_mode: bool,
    generated_at: i64,
) -> Vec<serde_json::Value> {
    json_output::build_json_payload_lines(
        schema,
        data,
        ndjson_mode,
        generated_at,
        JSON_SCHEMA_VERSION,
    )
}

async fn refresh_single_manager(
    runtime: &AdapterRuntime,
    manager: ManagerId,
) -> Result<(), String> {
    if !runtime.has_manager(manager) {
        return Err(format!(
            "manager '{}' is not registered in runtime",
            manager.as_str()
        ));
    }

    let mut ran_any_action = false;
    let mut detected_installed = None;

    if runtime.supports_capability(manager, Capability::Detect) {
        let (_, response) =
            submit_request_wait(runtime, manager, AdapterRequest::Detect(DetectRequest)).await?;
        match response {
            helm_core::adapters::AdapterResponse::Detection(info) => {
                detected_installed = Some(info.installed);
                if !info.installed {
                    return Ok(());
                }
            }
            _ => {
                return Err(format!(
                    "manager '{}' detect action returned unexpected payload",
                    manager.as_str()
                ));
            }
        }
        ran_any_action = true;
    }

    if detected_installed != Some(false)
        && runtime.supports_capability(manager, Capability::ListInstalled)
    {
        let _ = submit_request_wait(
            runtime,
            manager,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await?;
        ran_any_action = true;
    }

    if detected_installed != Some(false)
        && runtime.supports_capability(manager, Capability::ListOutdated)
    {
        let _ = submit_request_wait(
            runtime,
            manager,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await?;
        ran_any_action = true;
    }

    if !ran_any_action {
        return Err(format!(
            "manager '{}' has no detection or refresh capabilities",
            manager.as_str()
        ));
    }

    Ok(())
}

fn manager_enabled_map(store: &SqliteStore) -> Result<HashMap<ManagerId, bool>, String> {
    let detections: HashMap<ManagerId, DetectionInfo> = store
        .list_detections()
        .map_err(|error| format!("failed to list manager detections: {error}"))?
        .into_iter()
        .collect();
    let preferences = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?;
    let mut preference_map: HashMap<ManagerId, helm_core::persistence::ManagerPreference> =
        HashMap::new();
    for preference in preferences {
        preference_map.insert(preference.manager, preference);
    }

    let mut map: HashMap<ManagerId, bool> = HashMap::new();
    for manager in ManagerId::ALL {
        let configured_enabled = preference_map
            .get(&manager)
            .map(|preference| preference.enabled)
            .unwrap_or_else(|| default_enabled_for_manager(manager));
        let selected_path =
            resolved_manager_selected_executable_path(manager, &detections, &preference_map);
        let eligibility = manager_enablement_eligibility(
            manager,
            selected_path.as_deref().map(std::path::Path::new),
        );
        map.insert(manager, configured_enabled && eligibility.is_eligible);
    }
    Ok(map)
}

fn active_install_instances_by_manager(
    store: &SqliteStore,
) -> Result<HashMap<ManagerId, ManagerInstallInstance>, String> {
    let mut grouped: HashMap<ManagerId, Vec<ManagerInstallInstance>> = HashMap::new();
    for instance in store
        .list_install_instances(None)
        .map_err(|error| format!("failed to list manager install instances: {error}"))?
    {
        grouped.entry(instance.manager).or_default().push(instance);
    }

    let mut active = HashMap::new();
    for (manager, mut instances) in grouped {
        instances.sort_by(|left, right| {
            if left.is_active != right.is_active {
                return right.is_active.cmp(&left.is_active);
            }
            left.instance_id.cmp(&right.instance_id)
        });
        if let Some(instance) = instances.into_iter().next() {
            active.insert(manager, instance);
        }
    }

    Ok(active)
}

fn enabled_dependents_for_manager(
    store: &SqliteStore,
    enabled_map: &HashMap<ManagerId, bool>,
    manager: ManagerId,
) -> Result<Vec<ManagerId>, String> {
    let active_instances = active_install_instances_by_manager(store)?;
    let mut dependents = Vec::new();
    for candidate in ManagerId::ALL {
        if candidate == manager || !enabled_map.get(&candidate).copied().unwrap_or(true) {
            continue;
        }
        let Some(instance) = active_instances.get(&candidate) else {
            continue;
        };
        if provenance_dependency_manager(candidate, instance.provenance) == Some(manager) {
            dependents.push(candidate);
        }
    }
    dependents.sort_by_key(|id| id.as_str());
    Ok(dependents)
}

fn apply_manager_enablement_self_heal(store: &SqliteStore) -> Result<(), String> {
    let detections: HashMap<ManagerId, DetectionInfo> = store
        .list_detections()
        .map_err(|error| format!("failed to list manager detections: {error}"))?
        .into_iter()
        .collect();
    let mut preferences: HashMap<ManagerId, helm_core::persistence::ManagerPreference> = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?
        .into_iter()
        .map(|preference| (preference.manager, preference))
        .collect();

    for manager in ManagerId::ALL {
        let configured_enabled = preferences
            .get(&manager)
            .map(|preference| preference.enabled)
            .unwrap_or_else(|| default_enabled_for_manager(manager));
        if !configured_enabled {
            continue;
        }

        let selected_path =
            resolved_manager_selected_executable_path(manager, &detections, &preferences);
        let eligibility =
            manager_enablement_eligibility(manager, selected_path.as_deref().map(Path::new));
        if eligibility.is_eligible {
            continue;
        }

        store
            .set_manager_enabled(manager, false)
            .map_err(|error| format!("failed to auto-disable '{}': {error}", manager.as_str()))?;
        if let Some(preference) = preferences.get_mut(&manager) {
            preference.enabled = false;
        }

        verbose_log(format!(
            "manager policy self-heal: auto-disabled '{}' (reason_code={}, executable_path='{}')",
            manager.as_str(),
            eligibility.reason_code.unwrap_or("manager.ineligible"),
            selected_path.as_deref().unwrap_or("<none>")
        ));
    }

    Ok(())
}

fn default_enabled_for_manager(manager: ManagerId) -> bool {
    !matches!(
        manager,
        ManagerId::Asdf | ManagerId::MacPorts | ManagerId::NixDarwin
    )
}

fn list_installed_for_enabled(
    store: &SqliteStore,
    enabled_map: &HashMap<ManagerId, bool>,
) -> Result<Vec<InstalledPackage>, String> {
    let packages = store
        .list_installed()
        .map_err(|error| format!("failed to list installed packages: {error}"))?;
    Ok(packages
        .into_iter()
        .filter(|package| {
            enabled_map
                .get(&package.package.manager)
                .copied()
                .unwrap_or(true)
        })
        .collect())
}

fn list_outdated_for_enabled(
    store: &SqliteStore,
    enabled_map: &HashMap<ManagerId, bool>,
) -> Result<Vec<OutdatedPackage>, String> {
    let packages = store
        .list_outdated()
        .map_err(|error| format!("failed to list outdated packages: {error}"))?;
    Ok(packages
        .into_iter()
        .filter(|package| {
            enabled_map
                .get(&package.package.manager)
                .copied()
                .unwrap_or(true)
        })
        .collect())
}

fn package_manager_preference_key(package_name: &str, version: Option<&str>) -> String {
    let normalized_name = package_name.trim().to_ascii_lowercase();
    if normalized_name.is_empty() {
        return String::new();
    }

    let normalized_version = version.map(str::trim).filter(|value| !value.is_empty());
    let Some(normalized_version) = normalized_version else {
        return normalized_name;
    };

    let coordinate_raw = format!("{}@{}", package_name.trim(), normalized_version);
    let qualifier_key = PackageCoordinate::parse(coordinate_raw.as_str())
        .and_then(|coordinate| coordinate.version_selector)
        .map(|selector| selector.qualifier_atoms())
        .filter(|atoms| !atoms.is_empty())
        .map(|atoms| atoms.join("-").trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    if let Some(qualifier_key) = qualifier_key {
        format!("{}@{}", normalized_name, qualifier_key)
    } else {
        normalized_name
    }
}

fn search_local_for_enabled(
    store: &SqliteStore,
    enabled_map: &HashMap<ManagerId, bool>,
    query: &str,
) -> Result<Vec<CachedSearchResult>, String> {
    let results = store
        .query_local(query, 500)
        .map_err(|error| format!("failed to query local search cache: {error}"))?;
    Ok(results
        .into_iter()
        .filter(|result| {
            manager_participates_in_package_search(result.result.package.manager)
                && manager_participates_in_package_search(result.source_manager)
                && enabled_map
                    .get(&result.result.package.manager)
                    .copied()
                    .unwrap_or(true)
                && enabled_map
                    .get(&result.source_manager)
                    .copied()
                    .unwrap_or(true)
        })
        .collect())
}

fn merge_search_results(
    local_results: Vec<CachedSearchResult>,
    remote_results: Vec<CachedSearchResult>,
) -> Vec<CachedSearchResult> {
    let mut merged: HashMap<(ManagerId, String), CachedSearchResult> = HashMap::new();
    for row in local_results {
        let key = (row.result.package.manager, row.result.package.name.clone());
        merged.entry(key).or_insert(row);
    }
    for row in remote_results {
        let key = (row.result.package.manager, row.result.package.name.clone());
        merged.insert(key, row);
    }
    let mut rows = merged.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        let name_cmp = left
            .result
            .package
            .name
            .to_ascii_lowercase()
            .cmp(&right.result.package.name.to_ascii_lowercase());
        if name_cmp != std::cmp::Ordering::Equal {
            return name_cmp;
        }
        left.result
            .package
            .manager
            .as_str()
            .cmp(right.result.package.manager.as_str())
    });
    rows
}

fn search_remote_for_enabled(
    store: Arc<SqliteStore>,
    query: &str,
    manager_filter: Option<ManagerId>,
    enabled_map: &HashMap<ManagerId, bool>,
) -> Result<(Vec<CachedSearchResult>, Vec<String>), String> {
    if manager_filter.is_some_and(|manager| !manager_participates_in_package_search(manager)) {
        return Ok((Vec::new(), Vec::new()));
    }

    let managers = list_managers(store.as_ref())?;
    let mut target_managers = managers
        .into_iter()
        .filter_map(|row| {
            let manager_id = row.manager_id.parse::<ManagerId>().ok()?;
            if !manager_participates_in_package_search(manager_id) {
                return None;
            }
            if manager_filter.is_some() && manager_filter != Some(manager_id) {
                return None;
            }
            if !row.enabled || !row.detected || !row.supports_remote_search {
                return None;
            }
            if !enabled_map.get(&manager_id).copied().unwrap_or(true) {
                return None;
            }
            Some(manager_id)
        })
        .collect::<Vec<_>>();
    target_managers.sort_by(|left, right| left.as_str().cmp(right.as_str()));

    if target_managers.is_empty() {
        if let Some(manager) = manager_filter {
            return Err(format!(
                "manager '{}' is not available for remote search (enabled + detected + search-capable required)",
                manager.as_str()
            ));
        }
        return Ok((Vec::new(), Vec::new()));
    }

    sync_manager_executable_overrides(store.as_ref())?;
    let runtime = build_adapter_runtime(store.clone())?;
    let tokio_runtime = cli_tokio_runtime()?;
    let mut remote_results = Vec::new();
    let mut remote_errors = Vec::new();
    for manager in target_managers {
        let request = AdapterRequest::Search(SearchRequest {
            query: SearchQuery {
                text: query.to_string(),
                issued_at: SystemTime::now(),
            },
        });
        match tokio_runtime.block_on(submit_request_wait(&runtime, manager, request)) {
            Ok((_task_id, AdapterResponse::SearchResults(results))) => {
                remote_results.extend(results);
            }
            Ok((_task_id, _)) => {
                remote_errors.push(format!(
                    "{} returned unexpected response for remote search",
                    manager.as_str()
                ));
            }
            Err(error) => {
                remote_errors.push(format!("{}: {}", manager.as_str(), error));
            }
        }
    }

    Ok((remote_results, remote_errors))
}

fn manager_participates_in_package_search(manager: ManagerId) -> bool {
    helm_core::registry::manager_participates_in_package_search(manager)
}

fn list_tasks_for_enabled(
    store: &SqliteStore,
    enabled_map: &HashMap<ManagerId, bool>,
) -> Result<Vec<CliTaskRecord>, String> {
    let tasks = store
        .list_recent_tasks(TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list tasks: {error}"))?;
    Ok(tasks
        .into_iter()
        .filter(|task| enabled_map.get(&task.manager).copied().unwrap_or(true))
        .map(task_to_cli_task)
        .collect())
}

fn task_to_cli_task(task: TaskRecord) -> CliTaskRecord {
    let status = match task.status {
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Completed => "completed",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Failed => "failed",
    };

    let created_at_unix = task
        .created_at
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);

    CliTaskRecord {
        id: task.id.0,
        manager: task.manager.as_str().to_string(),
        task_type: format!("{:?}", task.task_type).to_lowercase(),
        status: status.to_string(),
        created_at_unix,
    }
}

fn manager_authority(manager: ManagerId) -> Option<ManagerAuthority> {
    registry::manager(manager).map(|descriptor| descriptor.authority)
}

fn manager_authority_base(authority: ManagerAuthority) -> usize {
    match authority {
        ManagerAuthority::Authoritative => 0,
        ManagerAuthority::Standard => 1,
        ManagerAuthority::Guarded => 2,
        ManagerAuthority::DetectionOnly => 3,
    }
}

fn manager_default_local_rank(manager: ManagerId) -> usize {
    let order: &[ManagerId] = match manager_authority(manager) {
        Some(ManagerAuthority::Authoritative) => {
            &[ManagerId::Mise, ManagerId::Rustup, ManagerId::Asdf]
        }
        Some(ManagerAuthority::Standard) => &[
            ManagerId::Mas,
            ManagerId::HomebrewCask,
            ManagerId::Npm,
            ManagerId::Pnpm,
            ManagerId::Yarn,
            ManagerId::Pipx,
            ManagerId::Pip,
            ManagerId::Poetry,
            ManagerId::Cargo,
            ManagerId::CargoBinstall,
            ManagerId::RubyGems,
            ManagerId::Bundler,
            ManagerId::DockerDesktop,
            ManagerId::Colima,
            ManagerId::Podman,
            ManagerId::Sparkle,
            ManagerId::Setapp,
            ManagerId::ParallelsDesktop,
        ],
        Some(ManagerAuthority::Guarded) => &[
            ManagerId::HomebrewFormula,
            ManagerId::MacPorts,
            ManagerId::NixDarwin,
            ManagerId::XcodeCommandLineTools,
            ManagerId::Rosetta2,
            ManagerId::FirmwareUpdates,
            ManagerId::SoftwareUpdate,
        ],
        Some(ManagerAuthority::DetectionOnly) => &[],
        None => &[],
    };

    order
        .iter()
        .position(|id| *id == manager)
        .unwrap_or(usize::MAX / 2)
}

fn manager_effective_local_rank(
    manager: ManagerId,
    overrides: &HashMap<ManagerId, usize>,
) -> usize {
    overrides
        .get(&manager)
        .copied()
        .unwrap_or_else(|| manager_default_local_rank(manager))
}

fn manager_effective_global_rank(
    manager: ManagerId,
    overrides: &HashMap<ManagerId, usize>,
) -> usize {
    let authority = manager_authority(manager).unwrap_or(ManagerAuthority::Standard);
    manager_authority_base(authority) * 10_000 + manager_effective_local_rank(manager, overrides)
}

fn load_manager_priority_overrides(
    store: &SqliteStore,
) -> Result<HashMap<ManagerId, usize>, String> {
    let Some(raw) = store
        .manager_priority_overrides_json()
        .map_err(|error| format!("failed to read manager priority overrides: {error}"))?
    else {
        return Ok(HashMap::new());
    };

    let parsed_value = serde_json::from_str::<serde_json::Value>(&raw)
        .map_err(|error| format!("failed to parse manager priority overrides json: {error}"))?;
    let Some(object) = parsed_value.as_object() else {
        return Ok(HashMap::new());
    };

    let mut overrides = HashMap::new();
    for (manager_raw, rank_value) in object {
        let Ok(manager) = manager_raw.parse::<ManagerId>() else {
            continue;
        };
        let Some(rank_u64) = rank_value.as_u64() else {
            continue;
        };
        overrides.insert(manager, rank_u64 as usize);
    }

    Ok(overrides)
}

fn save_manager_priority_overrides(
    store: &SqliteStore,
    overrides: &HashMap<ManagerId, usize>,
) -> Result<(), String> {
    let mut serialized: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for (manager, rank) in overrides {
        serialized.insert(manager.as_str().to_string(), *rank);
    }
    let encoded = serde_json::to_string(&serialized)
        .map_err(|error| format!("failed to serialize manager priority overrides: {error}"))?;
    store
        .set_manager_priority_overrides_json(Some(encoded.as_str()))
        .map_err(|error| format!("failed to persist manager priority overrides: {error}"))
}

fn manager_priority_entries(store: &SqliteStore) -> Result<Vec<CliManagerPriorityEntry>, String> {
    let overrides = load_manager_priority_overrides(store)?;
    let managers = list_managers(store)?;
    let mut rows = Vec::with_capacity(managers.len());

    for manager in managers {
        let manager_id = manager
            .manager_id
            .parse::<ManagerId>()
            .map_err(|_| format!("unknown manager id '{}'", manager.manager_id))?;
        rows.push(CliManagerPriorityEntry {
            manager_id: manager.manager_id,
            display_name: manager.display_name,
            authority: manager.authority,
            rank: manager_effective_local_rank(manager_id, &overrides),
            default_rank: manager_default_local_rank(manager_id),
            overridden: overrides.contains_key(&manager_id),
            detected: manager.detected,
            enabled: manager.enabled,
        });
    }

    rows.sort_by(|left, right| {
        if left.authority != right.authority {
            return left.authority.cmp(&right.authority);
        }
        if left.rank != right.rank {
            return left.rank.cmp(&right.rank);
        }
        left.manager_id.cmp(&right.manager_id)
    });

    Ok(rows)
}

fn parse_manager_priority_set_args(command_args: &[String]) -> Result<(ManagerId, usize), String> {
    if command_args.len() < 3 {
        return Err("managers priority set requires <manager-id> --rank <n>".to_string());
    }

    let manager = parse_manager_id(&command_args[0])?;
    let mut rank: Option<usize> = None;
    let mut index = 1usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--rank" => {
                if index + 1 >= command_args.len() {
                    return Err("managers priority set --rank requires a value".to_string());
                }
                if rank.is_some() {
                    return Err("managers priority set --rank specified multiple times".to_string());
                }
                let raw = &command_args[index + 1];
                let parsed = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid rank '{}' (expected positive integer)", raw))?;
                if parsed == 0 {
                    return Err("rank must be >= 1".to_string());
                }
                rank = Some(parsed - 1);
                index += 2;
            }
            other => {
                return Err(format!(
                    "unsupported managers priority set argument '{}' (supported: --rank <n>)",
                    other
                ));
            }
        }
    }

    let rank = rank.ok_or_else(|| "managers priority set requires --rank <n>".to_string())?;
    Ok((manager, rank))
}

fn set_manager_priority_rank(
    store: &SqliteStore,
    manager: ManagerId,
    rank: usize,
) -> Result<usize, String> {
    let authority = manager_authority(manager).ok_or_else(|| {
        format!(
            "manager '{}' could not be resolved for priority updates",
            manager.as_str()
        )
    })?;

    let mut overrides = load_manager_priority_overrides(store)?;
    let mut authority_managers = registry::managers()
        .iter()
        .filter(|descriptor| descriptor.authority == authority)
        .map(|descriptor| descriptor.id)
        .collect::<Vec<_>>();

    authority_managers.sort_by(|left, right| {
        let left_rank = manager_effective_local_rank(*left, &overrides);
        let right_rank = manager_effective_local_rank(*right, &overrides);
        if left_rank != right_rank {
            return left_rank.cmp(&right_rank);
        }
        left.as_str().cmp(right.as_str())
    });

    authority_managers.retain(|candidate| *candidate != manager);
    let insert_index = rank.min(authority_managers.len());
    authority_managers.insert(insert_index, manager);

    for (index, manager_id) in authority_managers.into_iter().enumerate() {
        overrides.insert(manager_id, index);
    }

    save_manager_priority_overrides(store, &overrides)?;
    Ok(insert_index)
}

fn list_managers(store: &SqliteStore) -> Result<Vec<CliManagerStatus>, String> {
    let detections = store
        .list_detections()
        .map_err(|error| format!("failed to list detections: {error}"))?;
    let preferences = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?;
    let install_instances = store
        .list_install_instances(None)
        .map_err(|error| format!("failed to list manager install instances: {error}"))?;

    let detection_map: HashMap<ManagerId, helm_core::models::DetectionInfo> =
        detections.into_iter().collect();
    let preference_map: HashMap<ManagerId, helm_core::persistence::ManagerPreference> = preferences
        .into_iter()
        .map(|preference| (preference.manager, preference))
        .collect();
    let mut install_instance_map: HashMap<ManagerId, Vec<ManagerInstallInstance>> = HashMap::new();
    for instance in install_instances {
        let instance = apply_manager_automation_policy(&instance);
        install_instance_map
            .entry(instance.manager)
            .or_default()
            .push(instance);
    }
    let mut multi_instance_ack_fingerprints: HashMap<ManagerId, Option<String>> = HashMap::new();
    for manager in ManagerId::ALL {
        let fingerprint = store
            .manager_multi_instance_ack_fingerprint(manager)
            .map_err(|error| {
                format!(
                    "failed to read manager multi-instance acknowledgement fingerprint: {error}"
                )
            })?;
        multi_instance_ack_fingerprints.insert(manager, normalize_nonempty(fingerprint));
    }
    for instances in install_instance_map.values_mut() {
        instances.sort_by(|left, right| {
            right
                .is_active
                .cmp(&left.is_active)
                .then_with(|| left.instance_id.cmp(&right.instance_id))
        });
    }

    let mut rows = Vec::with_capacity(registry::managers().len());
    for descriptor in registry::managers() {
        let detection = detection_map.get(&descriptor.id);
        let preference = preference_map.get(&descriptor.id);
        let manager_install_instances = install_instance_map.get(&descriptor.id);
        let active_instance = manager_install_instances
            .and_then(|instances| instances.iter().find(|instance| instance.is_active))
            .or_else(|| manager_install_instances.and_then(|instances| instances.first()));
        let acknowledged_fingerprint = multi_instance_ack_fingerprints
            .get(&descriptor.id)
            .and_then(|value| value.as_deref());
        let (multi_instance_state, multi_instance_fingerprint, multi_instance_acknowledged) =
            resolve_multi_instance_state(
                manager_install_instances.into_iter().flat_map(|instances| {
                    instances
                        .iter()
                        .map(|instance| instance.instance_id.as_str())
                }),
                acknowledged_fingerprint,
            );
        let active_executable_path = detection.and_then(|info| info.executable_path.as_deref());
        let executable_paths = if detection.map(|info| info.installed).unwrap_or(false) {
            collect_manager_executable_paths(descriptor.id, active_executable_path)
        } else {
            Vec::new()
        };
        let default_executable_path =
            default_manager_executable_path(descriptor.id, &executable_paths);
        let configured_enabled = preference
            .map(|preference| preference.enabled)
            .unwrap_or_else(|| default_enabled_for_manager(descriptor.id));
        let selected_executable_path = resolved_manager_selected_executable_path(
            descriptor.id,
            &detection_map,
            &preference_map,
        );
        let selected_executable_differs_from_default = selected_executable_differs_from_default(
            default_executable_path.as_deref(),
            selected_executable_path.as_deref(),
        );
        let executable_path_diagnostic = manager_executable_path_diagnostic(
            default_executable_path.as_deref(),
            selected_executable_path.as_deref(),
        )
        .to_string();
        let eligibility = manager_enablement_eligibility(
            descriptor.id,
            selected_executable_path
                .as_deref()
                .map(std::path::Path::new),
        );
        let enabled = configured_enabled && eligibility.is_eligible;

        rows.push(CliManagerStatus {
            manager_id: descriptor.id.as_str().to_string(),
            display_name: descriptor.display_name.to_string(),
            authority: match descriptor.authority {
                ManagerAuthority::Authoritative => "authoritative",
                ManagerAuthority::Standard => "standard",
                ManagerAuthority::Guarded => "guarded",
                ManagerAuthority::DetectionOnly => "detection_only",
            }
            .to_string(),
            detected: detection.map(|info| info.installed).unwrap_or(false),
            version: detection.and_then(|info| info.version.clone()),
            executable_path: detection
                .and_then(|info| info.executable_path.as_ref())
                .map(|path| path.to_string_lossy().into_owned()),
            enabled,
            is_implemented: true,
            is_optional: matches!(
                descriptor.id,
                ManagerId::Asdf | ManagerId::MacPorts | ManagerId::NixDarwin
            ),
            is_detection_only: matches!(descriptor.authority, ManagerAuthority::DetectionOnly),
            supports_remote_search: descriptor.capabilities.contains(&Capability::Search),
            supports_package_install: descriptor.capabilities.contains(&Capability::Install),
            supports_package_uninstall: descriptor.capabilities.contains(&Capability::Uninstall),
            supports_package_upgrade: descriptor.capabilities.contains(&Capability::Upgrade),
            selected_executable_path,
            selected_executable_differs_from_default,
            executable_path_diagnostic,
            selected_install_method: preference.and_then(|preference| {
                preference
                    .selected_install_method
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
            }),
            is_eligible: eligibility.is_eligible,
            ineligible_reason_code: eligibility.reason_code.map(str::to_string),
            ineligible_reason_message: eligibility.reason_message.map(str::to_string),
            install_instance_count: install_instance_map
                .get(&descriptor.id)
                .map(|instances| instances.len())
                .unwrap_or(0),
            multi_instance_state: multi_instance_state.as_str().to_string(),
            multi_instance_acknowledged,
            multi_instance_fingerprint,
            active_provenance: active_instance
                .map(|instance| instance.provenance.as_str().to_string()),
            active_confidence: active_instance.map(|instance| instance.confidence),
            active_decision_margin: active_instance.and_then(|instance| instance.decision_margin),
            active_automation_level: active_instance
                .map(|instance| instance.automation_level.as_str().to_string()),
            active_uninstall_strategy: active_instance
                .map(|instance| instance.uninstall_strategy.as_str().to_string()),
            active_update_strategy: active_instance
                .map(|instance| instance.update_strategy.as_str().to_string()),
            active_remediation_strategy: active_instance
                .map(|instance| instance.remediation_strategy.as_str().to_string()),
            active_explanation_primary: active_instance
                .and_then(|instance| normalize_nonempty(instance.explanation_primary.clone())),
            active_explanation_secondary: active_instance
                .and_then(|instance| normalize_nonempty(instance.explanation_secondary.clone())),
            competing_provenance: active_instance.and_then(|instance| {
                instance
                    .competing_provenance
                    .map(|value| value.as_str().to_string())
            }),
            competing_confidence: active_instance
                .and_then(|instance| instance.competing_confidence),
        });
    }

    let overrides = load_manager_priority_overrides(store)?;
    rows.sort_by(|left, right| {
        if left.detected != right.detected {
            return right.detected.cmp(&left.detected);
        }

        let left_manager = left.manager_id.parse::<ManagerId>().ok();
        let right_manager = right.manager_id.parse::<ManagerId>().ok();
        let left_rank = left_manager
            .map(|id| manager_effective_global_rank(id, &overrides))
            .unwrap_or(usize::MAX / 2);
        let right_rank = right_manager
            .map(|id| manager_effective_global_rank(id, &overrides))
            .unwrap_or(usize::MAX / 2);
        if left_rank != right_rank {
            return left_rank.cmp(&right_rank);
        }

        let display_cmp = left
            .display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase());
        if display_cmp != std::cmp::Ordering::Equal {
            return display_cmp;
        }
        left.manager_id.cmp(&right.manager_id)
    });
    Ok(rows)
}

fn list_manager_install_instances(
    store: &SqliteStore,
    manager: Option<ManagerId>,
) -> Result<Vec<CliManagerInstallInstance>, String> {
    let rows = store
        .list_install_instances(manager)
        .map_err(|error| format!("failed to list manager install instances: {error}"))?;

    let mut result = rows
        .into_iter()
        .map(|instance| {
            let instance = apply_manager_automation_policy(&instance);
            CliManagerInstallInstance {
                manager_id: instance.manager.as_str().to_string(),
                instance_id: instance.instance_id,
                identity_kind: instance.identity_kind.as_str().to_string(),
                identity_value: instance.identity_value,
                display_path: instance.display_path.to_string_lossy().to_string(),
                canonical_path: instance
                    .canonical_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                alias_paths: instance
                    .alias_paths
                    .iter()
                    .map(|path| path.to_string_lossy().to_string())
                    .collect(),
                is_active: instance.is_active,
                version: instance.version,
                provenance: instance.provenance.as_str().to_string(),
                confidence: instance.confidence,
                decision_margin: instance.decision_margin,
                automation_level: instance.automation_level.as_str().to_string(),
                uninstall_strategy: instance.uninstall_strategy.as_str().to_string(),
                update_strategy: instance.update_strategy.as_str().to_string(),
                remediation_strategy: instance.remediation_strategy.as_str().to_string(),
                explanation_primary: instance.explanation_primary,
                explanation_secondary: instance.explanation_secondary,
                competing_provenance: instance
                    .competing_provenance
                    .map(|value| value.as_str().to_string()),
                competing_confidence: instance.competing_confidence,
            }
        })
        .collect::<Vec<_>>();

    result.sort_by(|left, right| {
        left.manager_id
            .cmp(&right.manager_id)
            .then_with(|| right.is_active.cmp(&left.is_active))
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    Ok(result)
}

fn set_manager_active_install_instance(
    store: &SqliteStore,
    manager: ManagerId,
    instance_id: &str,
) -> Result<String, String> {
    let target = normalize_nonempty(Some(instance_id.to_string()))
        .ok_or_else(|| "instance id must not be empty".to_string())?;
    let mut instances = store
        .list_install_instances(Some(manager))
        .map_err(|error| format!("failed to list manager install instances: {error}"))?;
    if instances.is_empty() {
        return Err(format!(
            "manager '{}' has no detected install instances",
            manager.as_str()
        ));
    }
    let Some(selected_index) = instances
        .iter()
        .position(|instance| instance.instance_id.trim() == target.as_str())
    else {
        return Err(format!(
            "manager '{}' install instance '{}' not found",
            manager.as_str(),
            target
        ));
    };

    let selected_path = instances[selected_index]
        .display_path
        .to_string_lossy()
        .to_string();
    for (index, instance) in instances.iter_mut().enumerate() {
        instance.is_active = index == selected_index;
    }

    store
        .replace_install_instances(manager, &instances)
        .map_err(|error| {
            format!("failed to persist manager install instance selection: {error}")
        })?;
    store
        .set_manager_selected_executable_path(manager, Some(selected_path.as_str()))
        .map_err(|error| format!("failed to set selected executable path: {error}"))?;
    store
        .set_manager_multi_instance_ack_fingerprint(manager, None)
        .map_err(|error| format!("failed to clear multi-instance acknowledgement: {error}"))?;

    Ok(format!(
        "Manager '{}' active install instance set to '{}' ({})",
        manager.as_str(),
        target,
        selected_path
    ))
}

fn acknowledge_manager_multi_instance_state(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<String, String> {
    let instances = store
        .list_install_instances(Some(manager))
        .map_err(|error| format!("failed to list manager install instances: {error}"))?;
    let Some(fingerprint) = install_instance_fingerprint(&instances) else {
        return Err(format!(
            "manager '{}' does not currently have multiple install instances to acknowledge",
            manager.as_str()
        ));
    };
    store
        .set_manager_multi_instance_ack_fingerprint(manager, Some(fingerprint.as_str()))
        .map_err(|error| format!("failed to persist multi-instance acknowledgement: {error}"))?;
    Ok(format!(
        "Manager '{}' multi-instance state acknowledged.",
        manager.as_str()
    ))
}

fn clear_manager_multi_instance_ack(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<String, String> {
    store
        .set_manager_multi_instance_ack_fingerprint(manager, None)
        .map_err(|error| format!("failed to clear multi-instance acknowledgement: {error}"))?;
    Ok(format!(
        "Manager '{}' multi-instance acknowledgement cleared.",
        manager.as_str()
    ))
}

fn manager_executable_status(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<CliManagerExecutableStatus, String> {
    let detections: HashMap<ManagerId, DetectionInfo> = store
        .list_detections()
        .map_err(|error| format!("failed to list manager detections: {error}"))?
        .into_iter()
        .collect();
    let preferences: HashMap<ManagerId, helm_core::persistence::ManagerPreference> = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?
        .into_iter()
        .map(|preference| (preference.manager, preference))
        .collect();

    let active_executable_path = detections.get(&manager).and_then(|detection| {
        detection
            .executable_path
            .as_deref()
            .and_then(normalize_path_string)
    });
    let executable_paths = collect_manager_executable_paths(
        manager,
        detections
            .get(&manager)
            .and_then(|d| d.executable_path.as_deref()),
    );
    let default_executable_path = default_manager_executable_path(manager, &executable_paths);
    let selected_executable_path =
        resolved_manager_selected_executable_path(manager, &detections, &preferences);
    let selected_executable_differs_from_default = selected_executable_differs_from_default(
        default_executable_path.as_deref(),
        selected_executable_path.as_deref(),
    );
    let executable_path_diagnostic = manager_executable_path_diagnostic(
        default_executable_path.as_deref(),
        selected_executable_path.as_deref(),
    )
    .to_string();

    Ok(CliManagerExecutableStatus {
        manager_id: manager.as_str().to_string(),
        active_executable_path,
        executable_paths,
        default_executable_path,
        selected_executable_path,
        selected_executable_differs_from_default,
        executable_path_diagnostic,
    })
}

fn resolved_manager_selected_executable_path(
    manager: ManagerId,
    detections: &HashMap<ManagerId, DetectionInfo>,
    preferences: &HashMap<ManagerId, helm_core::persistence::ManagerPreference>,
) -> Option<String> {
    let executable_paths = collect_manager_executable_paths(
        manager,
        detections
            .get(&manager)
            .and_then(|d| d.executable_path.as_deref()),
    );
    let default_executable_path = default_manager_executable_path(manager, &executable_paths);
    let preferred_executable_path = preferences
        .get(&manager)
        .and_then(|preference| normalize_nonempty(preference.selected_executable_path.clone()));
    resolve_selected_executable_path(preferred_executable_path, default_executable_path)
}

fn manager_enablement_eligibility_for_store(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<helm_core::manager_policy::ManagerEnablementEligibility, String> {
    let detections: HashMap<ManagerId, DetectionInfo> = store
        .list_detections()
        .map_err(|error| format!("failed to list manager detections: {error}"))?
        .into_iter()
        .collect();
    let preferences: HashMap<ManagerId, helm_core::persistence::ManagerPreference> = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?
        .into_iter()
        .map(|preference| (preference.manager, preference))
        .collect();
    let selected_path =
        resolved_manager_selected_executable_path(manager, &detections, &preferences);
    Ok(manager_enablement_eligibility(
        manager,
        selected_path.as_deref().map(std::path::Path::new),
    ))
}

fn manager_install_methods_status(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<CliManagerInstallMethodsStatus, String> {
    let preferences: HashMap<ManagerId, helm_core::persistence::ManagerPreference> = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?
        .into_iter()
        .map(|preference| (preference.manager, preference))
        .collect();
    let context = manager_install_method_policy_context();
    let selected_install_method = normalize_install_method(
        manager,
        preferences
            .get(&manager)
            .and_then(|preference| preference.selected_install_method.clone()),
    )
    .filter(|method| install_method_allowed_by_policy(manager, method, context));
    let install_methods = manager_install_method_candidates(manager)
        .into_iter()
        .filter(|method| install_method_allowed_by_policy(manager, method, context))
        .map(|method| method.to_string())
        .collect::<Vec<_>>();

    Ok(CliManagerInstallMethodsStatus {
        manager_id: manager.as_str().to_string(),
        install_methods,
        selected_install_method,
    })
}

fn parse_selected_executable_arg(raw: &str) -> Result<Option<String>, String> {
    if matches!(raw, "path-default" | "default" | "auto") {
        return Ok(None);
    }

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("selected executable path cannot be empty".to_string());
    }

    let path = PathBuf::from(trimmed);
    let normalized = path.canonicalize().unwrap_or_else(|_| path.clone());
    if !normalized.is_file() {
        return Err(format!(
            "selected executable path '{}' is not a file",
            trimmed
        ));
    }

    Ok(normalize_path_string(normalized.as_path()))
}

fn parse_selected_install_method_arg(
    manager: ManagerId,
    raw: &str,
) -> Result<Option<String>, String> {
    if matches!(raw, "default" | "auto") {
        return Ok(None);
    }

    let candidates = manager_install_method_candidates(manager);
    if candidates.contains(&raw) {
        return Ok(Some(raw.to_string()));
    }

    Err(format!(
        "unsupported install method '{}' for manager '{}' (supported: {})",
        raw,
        manager.as_str(),
        candidates.join(", ")
    ))
}

fn manager_executable_candidates(id: ManagerId) -> &'static [&'static str] {
    match id {
        ManagerId::HomebrewFormula | ManagerId::HomebrewCask => {
            &["brew", "/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        }
        ManagerId::Asdf => &["asdf"],
        ManagerId::Mise => &["mise"],
        ManagerId::Rustup => &["rustup"],
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
        ManagerId::XcodeCommandLineTools => &["xcode-select"],
        ManagerId::SoftwareUpdate => &["/usr/sbin/softwareupdate"],
        _ => &[],
    }
}

fn manager_install_method_candidates(id: ManagerId) -> Vec<&'static str> {
    helm_core::manager_lifecycle::manager_supported_install_methods(id)
}

fn normalize_path_string(path: &std::path::Path) -> Option<String> {
    let rendered = path.to_string_lossy().trim().to_string();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

fn manager_additional_bin_roots() -> Vec<std::path::PathBuf> {
    let mut roots = vec![
        std::path::PathBuf::from("/opt/homebrew/bin"),
        std::path::PathBuf::from("/usr/local/bin"),
        std::path::PathBuf::from("/opt/local/bin"),
    ];

    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        roots.push(home.join(".local/bin"));
        roots.push(home.join(".cargo/bin"));
        roots.push(home.join(".asdf/bin"));
        roots.push(home.join(".asdf/shims"));
    }

    roots
}

fn manager_versioned_install_roots(id: ManagerId) -> Vec<std::path::PathBuf> {
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
        roots.push(std::path::PathBuf::from("/opt/homebrew/Cellar"));
        roots.push(std::path::PathBuf::from("/usr/local/Cellar"));
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
    ) && let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from)
    {
        roots.push(home.join(".asdf/installs"));
        roots.push(home.join(".local/share/mise/installs"));
    }

    roots
}

fn push_discovered_path(
    candidate_path: &std::path::Path,
    discovered: &mut Vec<String>,
    seen: &mut std::collections::HashSet<String>,
) {
    if candidate_path.is_file()
        && let Some(rendered) = normalize_path_string(candidate_path)
        && seen.insert(rendered.clone())
    {
        discovered.push(rendered);
    }
}

fn discover_executable_paths(id: ManagerId, candidates: &[&str]) -> Vec<String> {
    let mut discovered = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let additional_bin_roots = manager_additional_bin_roots();
    let versioned_roots = manager_versioned_install_roots(id);

    let path_dirs: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .as_deref()
        .map(std::env::split_paths)
        .map(|iter| iter.collect::<Vec<_>>())
        .unwrap_or_default();

    for candidate in candidates {
        if candidate.contains('/') {
            let absolute = std::path::PathBuf::from(candidate);
            push_discovered_path(absolute.as_path(), &mut discovered, &mut seen);
            continue;
        }

        for path_dir in &path_dirs {
            let full = path_dir.join(candidate);
            push_discovered_path(full.as_path(), &mut discovered, &mut seen);
        }

        for path_dir in &additional_bin_roots {
            let full = path_dir.join(candidate);
            push_discovered_path(full.as_path(), &mut discovered, &mut seen);
        }

        for root in &versioned_roots {
            let Ok(tool_dirs) = std::fs::read_dir(root) else {
                continue;
            };
            for tool_dir in tool_dirs.flatten() {
                let tool_path = tool_dir.path();
                if !tool_path.is_dir() {
                    continue;
                }

                let Ok(version_dirs) = std::fs::read_dir(&tool_path) else {
                    continue;
                };
                for version_dir in version_dirs.flatten() {
                    let version_path = version_dir.path();
                    if !version_path.is_dir() {
                        continue;
                    }
                    let full = version_path.join("bin").join(candidate);
                    push_discovered_path(full.as_path(), &mut discovered, &mut seen);
                }
            }
        }
    }

    discovered
}

fn cached_discovered_executable_paths(id: ManagerId, candidates: &[&str]) -> Vec<String> {
    let cache = EXECUTABLE_DISCOVERY_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock()
        && let Some(cached) = guard.get(&id)
    {
        return cached.clone();
    }

    let discovered = discover_executable_paths(id, candidates);

    if let Ok(mut guard) = cache.lock() {
        guard.insert(id, discovered.clone());
    }

    discovered
}

fn collect_manager_executable_paths(
    id: ManagerId,
    active_path: Option<&std::path::Path>,
) -> Vec<String> {
    let mut resolved = Vec::new();
    let mut seen = std::collections::HashSet::new();

    if let Some(active_path) = active_path
        && let Some(rendered) = normalize_path_string(active_path)
    {
        seen.insert(rendered.clone());
        resolved.push(rendered);
    }

    for discovered in cached_discovered_executable_paths(id, manager_executable_candidates(id)) {
        if seen.insert(discovered.clone()) {
            resolved.push(discovered);
        }
    }

    resolved
}

fn default_manager_executable_path(id: ManagerId, executable_paths: &[String]) -> Option<String> {
    if let Some(first) = executable_paths.first() {
        return Some(first.clone());
    }
    cached_discovered_executable_paths(id, manager_executable_candidates(id))
        .into_iter()
        .next()
}

fn resolve_selected_executable_path(
    preferred: Option<String>,
    default_path: Option<String>,
) -> Option<String> {
    if let Some(preferred) = preferred
        && std::path::Path::new(preferred.as_str()).is_file()
    {
        return Some(preferred);
    }
    default_path
}

fn selected_executable_differs_from_default(
    default_path: Option<&str>,
    selected_path: Option<&str>,
) -> bool {
    matches!(
        (default_path, selected_path),
        (Some(default), Some(selected)) if default != selected
    )
}

fn manager_executable_path_diagnostic(
    default_path: Option<&str>,
    selected_path: Option<&str>,
) -> &'static str {
    match (default_path, selected_path) {
        (Some(default), Some(selected)) if default == selected => "aligned",
        (Some(_), Some(_)) => "diverged",
        (None, Some(_)) => "selected_only",
        (Some(_), None) => "default_only",
        (None, None) => "unresolved",
    }
}

fn normalize_install_method(id: ManagerId, method: Option<String>) -> Option<String> {
    let method = normalize_nonempty(method)?;
    if manager_install_method_candidates(id).contains(&method.as_str()) {
        return Some(method);
    }
    None
}

fn manager_selected_install_method(store: &SqliteStore, manager: ManagerId) -> Option<String> {
    let preferences = store.list_manager_preferences().ok()?;
    let preference = preferences
        .into_iter()
        .find(|preference| preference.manager == manager)?;
    normalize_install_method(manager, preference.selected_install_method)
}

fn encode_homebrew_upgrade_target(package_name: &str, cleanup_old_kegs: bool) -> String {
    if cleanup_old_kegs {
        format!("{package_name}@@helm.cleanup")
    } else {
        package_name.to_string()
    }
}

fn effective_homebrew_keg_policy(store: &SqliteStore, package_name: &str) -> HomebrewKegPolicy {
    let package = PackageRef {
        manager: ManagerId::HomebrewFormula,
        name: package_name.to_string(),
    };

    if let Ok(Some(policy)) = store.package_keg_policy(&package) {
        return policy;
    }

    store
        .homebrew_keg_policy()
        .unwrap_or(HomebrewKegPolicy::Keep)
}

fn homebrew_dependency_available(store: &SqliteStore) -> bool {
    let mut detected_path: Option<PathBuf> = None;
    if let Ok(detections) = store.list_detections()
        && let Some((_, detection)) = detections
            .iter()
            .find(|(manager, _)| *manager == ManagerId::HomebrewFormula)
    {
        if detection.installed {
            return true;
        }
        detected_path = detection.executable_path.clone();
    }

    !collect_manager_executable_paths(ManagerId::HomebrewFormula, detected_path.as_deref())
        .is_empty()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagerInstallMethodPolicyTag {
    Allowed,
    ManagedRestricted,
    Blocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ManagerInstallMethodPolicyContext {
    managed_environment: bool,
    allow_restricted_methods: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ManagerAutomationPolicyContext {
    mode: ManagedAutomationPolicyMode,
}

fn manager_install_method_policy_context() -> ManagerInstallMethodPolicyContext {
    *MANAGER_INSTALL_METHOD_POLICY_CONTEXT.get_or_init(|| {
        let managed_override = env_flag_enabled(MANAGED_INSTALL_METHOD_POLICY_ENV);
        let allow_restricted_methods =
            env_flag_enabled(MANAGED_INSTALL_METHOD_POLICY_ALLOW_RESTRICTED_ENV);
        let managed_from_provenance = env::current_exe()
            .ok()
            .map(|path| detect_install_provenance(&path))
            .map(|provenance| {
                provenance.channel == InstallChannel::Managed
                    || provenance.update_policy == UpdatePolicy::Managed
            })
            .unwrap_or(false);

        ManagerInstallMethodPolicyContext {
            managed_environment: managed_override || managed_from_provenance,
            allow_restricted_methods,
        }
    })
}

fn parse_managed_automation_policy_mode(raw: &str) -> Option<ManagedAutomationPolicyMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "automatic" | "auto" | "none" | "off" => Some(ManagedAutomationPolicyMode::Automatic),
        "needs_confirmation" | "confirm" | "confirmation" => {
            Some(ManagedAutomationPolicyMode::NeedsConfirmation)
        }
        "read_only" | "readonly" => Some(ManagedAutomationPolicyMode::ReadOnly),
        _ => None,
    }
}

fn manager_automation_policy_context() -> ManagerAutomationPolicyContext {
    *MANAGER_AUTOMATION_POLICY_CONTEXT.get_or_init(|| {
        let explicit_mode = env::var(MANAGED_AUTOMATION_POLICY_ENV)
            .ok()
            .and_then(|value| parse_managed_automation_policy_mode(value.as_str()));
        let mode = explicit_mode.unwrap_or_else(|| {
            if manager_install_method_policy_context().managed_environment {
                ManagedAutomationPolicyMode::NeedsConfirmation
            } else {
                ManagedAutomationPolicyMode::Automatic
            }
        });
        ManagerAutomationPolicyContext { mode }
    })
}

fn apply_manager_automation_policy(instance: &ManagerInstallInstance) -> ManagerInstallInstance {
    apply_managed_automation_policy(instance, manager_automation_policy_context().mode)
}

fn manager_install_method_policy_tag(
    manager: ManagerId,
    method: &str,
) -> ManagerInstallMethodPolicyTag {
    match registry::manager_install_method_spec(manager, method).map(|spec| spec.policy_tag) {
        Some(registry::InstallMethodPolicyTag::Allowed) => ManagerInstallMethodPolicyTag::Allowed,
        Some(registry::InstallMethodPolicyTag::ManagedRestricted) => {
            ManagerInstallMethodPolicyTag::ManagedRestricted
        }
        Some(registry::InstallMethodPolicyTag::BlockedByPolicy) | None => {
            ManagerInstallMethodPolicyTag::Blocked
        }
    }
}

fn install_method_allowed_by_policy(
    manager: ManagerId,
    method: &str,
    context: ManagerInstallMethodPolicyContext,
) -> bool {
    match manager_install_method_policy_tag(manager, method) {
        ManagerInstallMethodPolicyTag::Allowed => true,
        ManagerInstallMethodPolicyTag::ManagedRestricted => {
            !context.managed_environment || context.allow_restricted_methods
        }
        ManagerInstallMethodPolicyTag::Blocked => false,
    }
}

fn manager_helm_supported_install_methods(id: ManagerId) -> Vec<&'static str> {
    helm_core::manager_lifecycle::manager_supported_install_methods(id)
}

fn manager_supported_install_methods_for_install(
    manager: ManagerId,
) -> Result<Vec<&'static str>, String> {
    let planner_supported = manager_helm_supported_install_methods(manager);
    let context = manager_install_method_policy_context();
    let supported = planner_supported
        .iter()
        .copied()
        .filter(|method| install_method_allowed_by_policy(manager, method, context))
        .collect::<Vec<_>>();
    if !supported.is_empty() {
        return Ok(supported);
    }
    if !planner_supported.is_empty() {
        return Err(format!(
            "manager '{}' install methods are currently blocked by managed policy",
            manager.as_str()
        ));
    }
    Ok(Vec::new())
}

fn manager_install_method_allowed_for_selection(manager: ManagerId, method: &str) -> bool {
    install_method_allowed_by_policy(manager, method, manager_install_method_policy_context())
}

fn can_prompt_install_method_selection(options: &GlobalOptions) -> bool {
    !options.json && can_run_interactive_onboarding()
}

fn prompt_install_method_choice(
    manager: ManagerId,
    supported_methods: &[&str],
) -> Result<String, String> {
    println!("Select install method for manager '{}':", manager.as_str());
    for (index, method) in supported_methods.iter().enumerate() {
        println!("  {}) {}", index + 1, method);
    }

    loop {
        let input = prompt_line(&format!(
            "Select method [1-{}] (default 1): ",
            supported_methods.len()
        ))?;
        let selected = if input.is_empty() {
            "1".to_string()
        } else {
            input
        };
        if let Ok(index) = selected.parse::<usize>()
            && (1..=supported_methods.len()).contains(&index)
        {
            return Ok(supported_methods[index - 1].to_string());
        }
        eprintln!(
            "Please select one of: {}",
            (1..=supported_methods.len())
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

fn resolve_install_method_override_decision(
    manager: ManagerId,
    explicit_override: Option<String>,
    persisted_selected: Option<String>,
    supported_methods: &[&str],
    can_prompt_interactive: bool,
) -> Result<Option<String>, String> {
    if let Some(explicit_override) = explicit_override {
        return Ok(Some(explicit_override));
    }

    let persisted_supported =
        persisted_selected.filter(|method| supported_methods.contains(&method.as_str()));
    if let Some(persisted_supported) = persisted_supported {
        return Ok(Some(persisted_supported));
    }

    if supported_methods.is_empty() {
        return Ok(None);
    }
    if supported_methods.len() == 1 {
        return Ok(Some(supported_methods[0].to_string()));
    }
    if can_prompt_interactive {
        return prompt_install_method_choice(manager, supported_methods).map(Some);
    }

    Err(format!(
        "managers install '{}' requires --method <method-id> in non-interactive mode (supported: {})",
        manager.as_str(),
        supported_methods.join(", ")
    ))
}

fn resolve_install_method_override_for_install(
    store: &SqliteStore,
    manager: ManagerId,
    options: &GlobalOptions,
    explicit_override: Option<String>,
) -> Result<Option<String>, String> {
    let supported_methods = manager_supported_install_methods_for_install(manager)?;
    if let Some(method) = explicit_override.as_deref()
        && !supported_methods.contains(&method)
    {
        return Err(format!(
            "managers install '{}' does not allow --method '{}' in the current policy context (supported: {})",
            manager.as_str(),
            method,
            supported_methods.join(", ")
        ));
    }
    resolve_install_method_override_decision(
        manager,
        explicit_override,
        manager_selected_install_method(store, manager),
        supported_methods.as_slice(),
        can_prompt_install_method_selection(options),
    )
}

fn resolve_install_method_override_for_tui(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<Option<String>, String> {
    let supported_methods = manager_supported_install_methods_for_install(manager)?;
    resolve_install_method_override_decision(
        manager,
        None,
        manager_selected_install_method(store, manager),
        supported_methods.as_slice(),
        false,
    )
    .map_err(|_| {
        format!(
            "manager '{}' install method is ambiguous; select one first with 'm' in Managers",
            manager.as_str()
        )
    })
}

fn build_manager_mutation_request(
    store: &SqliteStore,
    manager: ManagerId,
    subcommand: &str,
    install_method_override: Option<String>,
) -> Result<(ManagerId, AdapterRequest), String> {
    build_manager_mutation_request_with_options(
        store,
        manager,
        subcommand,
        install_method_override,
        helm_core::manager_lifecycle::ManagerInstallOptions::default(),
    )
}

fn build_manager_mutation_request_with_options(
    store: &SqliteStore,
    manager: ManagerId,
    subcommand: &str,
    install_method_override: Option<String>,
    install_options: helm_core::manager_lifecycle::ManagerInstallOptions,
) -> Result<(ManagerId, AdapterRequest), String> {
    let selected_method = normalize_install_method(manager, install_method_override)
        .or_else(|| manager_selected_install_method(store, manager));
    let (target_manager, request) = match subcommand {
        "install" => {
            let install_plan = helm_core::manager_lifecycle::plan_manager_install(
                manager,
                selected_method.as_deref(),
                &install_options,
            )
            .map_err(|error| {
                manager_install_plan_error_message(manager, selected_method.as_deref(), error)
            })?;
            (install_plan.target_manager, install_plan.request)
        }
        "update" => {
            let active_instance = active_manager_install_instance(store, manager)?;
            let update_plan = helm_core::manager_lifecycle::plan_manager_update(
                manager,
                active_instance.as_ref(),
            )
            .map_err(|error| manager_update_plan_error_message(manager, error))?;

            let request = match &update_plan.target {
                helm_core::manager_lifecycle::ManagerUpdateTarget::ManagerSelf => {
                    helm_core::manager_lifecycle::build_update_request(&update_plan, None)
                }
                helm_core::manager_lifecycle::ManagerUpdateTarget::HomebrewFormula {
                    formula_name,
                } => {
                    let policy = effective_homebrew_keg_policy(store, formula_name);
                    let cleanup_old_kegs = policy == HomebrewKegPolicy::Cleanup;
                    let target_name =
                        encode_homebrew_upgrade_target(formula_name, cleanup_old_kegs);
                    helm_core::manager_lifecycle::build_update_request(
                        &update_plan,
                        Some(target_name),
                    )
                }
            }
            .ok_or_else(|| {
                format!(
                    "manager '{}' update strategy resolution returned an unsupported strategy",
                    manager.as_str(),
                )
            })?;

            (update_plan.target_manager, request)
        }
        "uninstall" => match manager {
            ManagerId::Rustup => (
                ManagerId::Rustup,
                AdapterRequest::Uninstall(UninstallRequest {
                    package: PackageRef {
                        manager: ManagerId::Rustup,
                        name: "__self__".to_string(),
                    },
                }),
            ),
            _ => {
                if let Some(formula_name) = manager_homebrew_formula_name(manager) {
                    (
                        ManagerId::HomebrewFormula,
                        AdapterRequest::Uninstall(UninstallRequest {
                            package: PackageRef {
                                manager: ManagerId::HomebrewFormula,
                                name: formula_name.to_string(),
                            },
                        }),
                    )
                } else {
                    return Err(format!(
                        "manager '{}' does not currently support uninstall",
                        manager.as_str()
                    ));
                }
            }
        },
        _ => {
            return Err(format!(
                "unsupported managers mutation command '{}'",
                subcommand
            ));
        }
    };

    if target_manager == ManagerId::HomebrewFormula && !homebrew_dependency_available(store) {
        return Err(
            "homebrew is required for this manager operation but was not detected on this system"
                .to_string(),
        );
    }

    Ok((target_manager, request))
}

fn manager_install_plan_error_message(
    manager: ManagerId,
    selected_method: Option<&str>,
    error: helm_core::manager_lifecycle::ManagerInstallPlanError,
) -> String {
    match error {
        helm_core::manager_lifecycle::ManagerInstallPlanError::UnsupportedManager => {
            format!(
                "manager '{}' does not currently support install",
                manager.as_str()
            )
        }
        helm_core::manager_lifecycle::ManagerInstallPlanError::UnsupportedMethod => {
            format!(
                "manager '{}' install is unsupported for selected method '{}'",
                manager.as_str(),
                selected_method.unwrap_or("unknown")
            )
        }
        helm_core::manager_lifecycle::ManagerInstallPlanError::InvalidRustupBinaryPath => {
            "rustup install source 'existingBinaryPath' requires a non-empty absolute binary path"
                .to_string()
        }
        helm_core::manager_lifecycle::ManagerInstallPlanError::InvalidMiseBinaryPath => {
            "mise install source 'existingBinaryPath' requires a non-empty absolute binary path"
                .to_string()
        }
    }
}

fn manager_update_plan_error_message(
    manager: ManagerId,
    error: helm_core::manager_lifecycle::ManagerUpdatePlanError,
) -> String {
    match error {
        helm_core::manager_lifecycle::ManagerUpdatePlanError::UnsupportedManager => {
            format!(
                "manager '{}' does not currently support update",
                manager.as_str()
            )
        }
        helm_core::manager_lifecycle::ManagerUpdatePlanError::ReadOnly => {
            format!(
                "manager '{}' update is blocked because active provenance is read-only",
                manager.as_str(),
            )
        }
        helm_core::manager_lifecycle::ManagerUpdatePlanError::AmbiguousProvenance => {
            format!(
                "manager '{}' update provenance is ambiguous; inspect with `helm managers instances {}` and choose an explicit path before updating.",
                manager.as_str(),
                manager.as_str(),
            )
        }
        helm_core::manager_lifecycle::ManagerUpdatePlanError::FormulaUnresolved => {
            format!(
                "manager '{}' update provenance resolved to homebrew but formula ownership could not be determined from active install instance; inspect with `helm managers instances {}` and refresh detection before retrying.",
                manager.as_str(),
                manager.as_str(),
            )
        }
    }
}

fn parse_rustup_install_source_arg(
    raw: &str,
) -> Result<helm_core::manager_lifecycle::RustupInstallSource, String> {
    match raw.trim() {
        "officialDownload" | "official-download" | "official_download" => {
            Ok(helm_core::manager_lifecycle::RustupInstallSource::OfficialDownload)
        }
        "existingBinaryPath" | "existing-binary-path" | "existing_binary_path" => {
            Ok(helm_core::manager_lifecycle::RustupInstallSource::ExistingBinaryPath)
        }
        other => Err(format!(
            "unsupported rustup install source '{}'; supported: officialDownload, existingBinaryPath",
            other
        )),
    }
}

fn parse_mise_install_source_arg(
    raw: &str,
) -> Result<helm_core::manager_lifecycle::MiseInstallSource, String> {
    match raw.trim() {
        "officialDownload" | "official-download" | "official_download" => {
            Ok(helm_core::manager_lifecycle::MiseInstallSource::OfficialDownload)
        }
        "existingBinaryPath" | "existing-binary-path" | "existing_binary_path" => {
            Ok(helm_core::manager_lifecycle::MiseInstallSource::ExistingBinaryPath)
        }
        other => Err(format!(
            "unsupported mise install source '{}'; supported: officialDownload, existingBinaryPath",
            other
        )),
    }
}

fn parse_mise_cleanup_mode_arg(
    raw: &str,
) -> Result<helm_core::manager_lifecycle::MiseUninstallCleanupMode, String> {
    match raw.trim() {
        "managerOnly" | "manager-only" | "manager_only" => {
            Ok(helm_core::manager_lifecycle::MiseUninstallCleanupMode::ManagerOnly)
        }
        "fullCleanup" | "full-cleanup" | "full_cleanup" => {
            Ok(helm_core::manager_lifecycle::MiseUninstallCleanupMode::FullCleanup)
        }
        other => Err(format!(
            "unsupported mise cleanup mode '{}'; supported: managerOnly, fullCleanup",
            other
        )),
    }
}

fn parse_mise_config_removal_arg(
    raw: &str,
) -> Result<helm_core::manager_lifecycle::MiseUninstallConfigRemoval, String> {
    match raw.trim() {
        "keepConfig" | "keep-config" | "keep_config" => {
            Ok(helm_core::manager_lifecycle::MiseUninstallConfigRemoval::KeepConfig)
        }
        "removeConfig" | "remove-config" | "remove_config" => {
            Ok(helm_core::manager_lifecycle::MiseUninstallConfigRemoval::RemoveConfig)
        }
        other => Err(format!(
            "unsupported mise config removal mode '{}'; supported: keepConfig, removeConfig",
            other
        )),
    }
}

fn parse_homebrew_cleanup_mode_arg(
    raw: &str,
) -> Result<helm_core::manager_lifecycle::HomebrewUninstallCleanupMode, String> {
    match raw.trim() {
        "managerOnly" | "manager-only" | "manager_only" => {
            Ok(helm_core::manager_lifecycle::HomebrewUninstallCleanupMode::ManagerOnly)
        }
        "fullCleanup" | "full-cleanup" | "full_cleanup" => {
            Ok(helm_core::manager_lifecycle::HomebrewUninstallCleanupMode::FullCleanup)
        }
        other => Err(format!(
            "unsupported homebrew cleanup mode '{}'; supported: managerOnly, fullCleanup",
            other
        )),
    }
}

fn parse_manager_mutation_args(
    subcommand: &str,
    command_args: &[String],
) -> Result<ParsedManagerMutationArgs, String> {
    let mut manager: Option<ManagerId> = None;
    let mut preview = false;
    let mut yes = false;
    let mut allow_unknown_provenance = false;
    let mut install_method_raw: Option<String> = None;
    let mut rustup_install_source: Option<helm_core::manager_lifecycle::RustupInstallSource> = None;
    let mut rustup_binary_path: Option<String> = None;
    let mut mise_install_source: Option<helm_core::manager_lifecycle::MiseInstallSource> = None;
    let mut mise_binary_path: Option<String> = None;
    let mut homebrew_cleanup_mode: Option<
        helm_core::manager_lifecycle::HomebrewUninstallCleanupMode,
    > = None;
    let mut mise_cleanup_mode: Option<helm_core::manager_lifecycle::MiseUninstallCleanupMode> =
        None;
    let mut mise_config_removal: Option<helm_core::manager_lifecycle::MiseUninstallConfigRemoval> =
        None;

    let uninstall_command = subcommand == "uninstall";
    let install_command = subcommand == "install";
    let mut index = 0usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--preview" if uninstall_command => {
                preview = true;
                index += 1;
            }
            "--yes" if uninstall_command => {
                yes = true;
                index += 1;
            }
            "--allow-unknown-provenance" if uninstall_command => {
                allow_unknown_provenance = true;
                index += 1;
            }
            "--homebrew-cleanup-mode" if uninstall_command => {
                if index + 1 >= command_args.len() {
                    return Err(
                        "managers uninstall --homebrew-cleanup-mode requires a mode value"
                            .to_string(),
                    );
                }
                if homebrew_cleanup_mode.is_some() {
                    return Err(
                        "managers uninstall --homebrew-cleanup-mode specified multiple times"
                            .to_string(),
                    );
                }
                homebrew_cleanup_mode = Some(parse_homebrew_cleanup_mode_arg(
                    command_args[index + 1].as_str(),
                )?);
                index += 2;
            }
            "--mise-cleanup-mode" if uninstall_command => {
                if index + 1 >= command_args.len() {
                    return Err(
                        "managers uninstall --mise-cleanup-mode requires a mode value".to_string(),
                    );
                }
                if mise_cleanup_mode.is_some() {
                    return Err(
                        "managers uninstall --mise-cleanup-mode specified multiple times"
                            .to_string(),
                    );
                }
                mise_cleanup_mode = Some(parse_mise_cleanup_mode_arg(
                    command_args[index + 1].as_str(),
                )?);
                index += 2;
            }
            "--mise-config-removal" if uninstall_command => {
                if index + 1 >= command_args.len() {
                    return Err(
                        "managers uninstall --mise-config-removal requires a mode value"
                            .to_string(),
                    );
                }
                if mise_config_removal.is_some() {
                    return Err(
                        "managers uninstall --mise-config-removal specified multiple times"
                            .to_string(),
                    );
                }
                mise_config_removal = Some(parse_mise_config_removal_arg(
                    command_args[index + 1].as_str(),
                )?);
                index += 2;
            }
            "--method" if install_command => {
                if index + 1 >= command_args.len() {
                    return Err("managers install --method requires a method id".to_string());
                }
                if install_method_raw.is_some() {
                    return Err("managers install --method specified multiple times".to_string());
                }
                install_method_raw = Some(command_args[index + 1].clone());
                index += 2;
            }
            "--rustup-install-source" if install_command => {
                if index + 1 >= command_args.len() {
                    return Err(
                        "managers install --rustup-install-source requires a source value"
                            .to_string(),
                    );
                }
                if rustup_install_source.is_some() {
                    return Err(
                        "managers install --rustup-install-source specified multiple times"
                            .to_string(),
                    );
                }
                rustup_install_source = Some(parse_rustup_install_source_arg(
                    command_args[index + 1].as_str(),
                )?);
                index += 2;
            }
            "--rustup-binary-path" if install_command => {
                if index + 1 >= command_args.len() {
                    return Err(
                        "managers install --rustup-binary-path requires a file path".to_string()
                    );
                }
                if rustup_binary_path.is_some() {
                    return Err(
                        "managers install --rustup-binary-path specified multiple times"
                            .to_string(),
                    );
                }
                rustup_binary_path = Some(command_args[index + 1].clone());
                index += 2;
            }
            "--mise-install-source" if install_command => {
                if index + 1 >= command_args.len() {
                    return Err(
                        "managers install --mise-install-source requires a source value"
                            .to_string(),
                    );
                }
                if mise_install_source.is_some() {
                    return Err(
                        "managers install --mise-install-source specified multiple times"
                            .to_string(),
                    );
                }
                mise_install_source = Some(parse_mise_install_source_arg(
                    command_args[index + 1].as_str(),
                )?);
                index += 2;
            }
            "--mise-binary-path" if install_command => {
                if index + 1 >= command_args.len() {
                    return Err(
                        "managers install --mise-binary-path requires a file path".to_string()
                    );
                }
                if mise_binary_path.is_some() {
                    return Err(
                        "managers install --mise-binary-path specified multiple times".to_string(),
                    );
                }
                mise_binary_path = Some(command_args[index + 1].clone());
                index += 2;
            }
            flag if flag.starts_with("--") => {
                if uninstall_command {
                    return Err(format!(
                        "unsupported managers uninstall argument '{}'; supported: <manager-id>, --preview, --yes, --allow-unknown-provenance, --homebrew-cleanup-mode <managerOnly|fullCleanup>, --mise-cleanup-mode <managerOnly|fullCleanup>, --mise-config-removal <keepConfig|removeConfig>",
                        flag
                    ));
                }
                if install_command {
                    return Err(format!(
                        "unsupported managers install argument '{}'; supported: <manager-id>, --method <method-id>, --rustup-install-source <officialDownload|existingBinaryPath>, --rustup-binary-path <path>, --mise-install-source <officialDownload|existingBinaryPath>, --mise-binary-path <path>",
                        flag
                    ));
                }
                return Err(format!(
                    "unsupported managers {} argument '{}'; expected exactly one <manager-id>",
                    subcommand, flag
                ));
            }
            value => {
                if manager.is_some() {
                    return Err(format!(
                        "managers {} requires exactly one manager id",
                        subcommand
                    ));
                }
                manager = Some(parse_manager_id(value)?);
                index += 1;
            }
        };
    }

    let manager = manager
        .ok_or_else(|| format!("managers {} requires exactly one manager id", subcommand))?;
    let install_method_override = install_method_raw
        .map(|raw| parse_selected_install_method_arg(manager, raw.as_str()))
        .transpose()?
        .flatten();
    let install_options = if install_command {
        if manager != ManagerId::Rustup
            && (rustup_install_source.is_some() || rustup_binary_path.is_some())
        {
            return Err(
                "managers install rustup install-source flags are only supported for manager 'rustup'"
                    .to_string(),
            );
        }
        if manager != ManagerId::Mise
            && (mise_install_source.is_some() || mise_binary_path.is_some())
        {
            return Err(
                "managers install mise install-source flags are only supported for manager 'mise'"
                    .to_string(),
            );
        }

        let mut source = rustup_install_source;
        if rustup_binary_path.is_some() {
            match source {
                Some(helm_core::manager_lifecycle::RustupInstallSource::OfficialDownload) => {
                    return Err(
                        "managers install --rustup-binary-path is incompatible with --rustup-install-source officialDownload"
                            .to_string(),
                    );
                }
                Some(helm_core::manager_lifecycle::RustupInstallSource::ExistingBinaryPath) => {}
                None => {
                    source =
                        Some(helm_core::manager_lifecycle::RustupInstallSource::ExistingBinaryPath);
                }
            }
        }

        let mut mise_source = mise_install_source;
        if mise_binary_path.is_some() {
            match mise_source {
                Some(helm_core::manager_lifecycle::MiseInstallSource::OfficialDownload) => {
                    return Err(
                        "managers install --mise-binary-path is incompatible with --mise-install-source officialDownload"
                            .to_string(),
                    );
                }
                Some(helm_core::manager_lifecycle::MiseInstallSource::ExistingBinaryPath) => {}
                None => {
                    mise_source =
                        Some(helm_core::manager_lifecycle::MiseInstallSource::ExistingBinaryPath);
                }
            }
        }

        if manager == ManagerId::Rustup {
            helm_core::manager_lifecycle::ManagerInstallOptions {
                rustup_install_source: source,
                rustup_binary_path,
                ..helm_core::manager_lifecycle::ManagerInstallOptions::default()
            }
        } else if manager == ManagerId::Mise {
            helm_core::manager_lifecycle::ManagerInstallOptions {
                mise_install_source: mise_source,
                mise_binary_path,
                ..helm_core::manager_lifecycle::ManagerInstallOptions::default()
            }
        } else {
            helm_core::manager_lifecycle::ManagerInstallOptions::default()
        }
    } else {
        helm_core::manager_lifecycle::ManagerInstallOptions::default()
    };

    if uninstall_command
        && manager != ManagerId::Mise
        && (mise_cleanup_mode.is_some() || mise_config_removal.is_some())
    {
        return Err(
            "managers uninstall mise cleanup flags are only supported for manager 'mise'"
                .to_string(),
        );
    }
    let uninstall_options = if uninstall_command && manager == ManagerId::Mise {
        helm_core::manager_lifecycle::ManagerUninstallOptions {
            homebrew_cleanup_mode,
            mise_cleanup_mode,
            mise_config_removal,
            remove_helm_managed_shell_setup: None,
        }
    } else if uninstall_command {
        helm_core::manager_lifecycle::ManagerUninstallOptions {
            homebrew_cleanup_mode,
            ..helm_core::manager_lifecycle::ManagerUninstallOptions::default()
        }
    } else {
        helm_core::manager_lifecycle::ManagerUninstallOptions::default()
    };

    Ok(ParsedManagerMutationArgs {
        manager,
        preview,
        yes,
        allow_unknown_provenance,
        install_method_override,
        install_options,
        uninstall_options,
    })
}

#[cfg(test)]
fn build_manager_uninstall_plan(
    store: &SqliteStore,
    manager: ManagerId,
    allow_unknown_provenance: bool,
    preview_only: bool,
) -> Result<ManagerUninstallPlan, String> {
    build_manager_uninstall_plan_with_options(
        store,
        manager,
        allow_unknown_provenance,
        preview_only,
        helm_core::manager_lifecycle::ManagerUninstallOptions::default(),
    )
}

fn build_manager_uninstall_plan_with_options(
    store: &SqliteStore,
    manager: ManagerId,
    allow_unknown_provenance: bool,
    preview_only: bool,
    uninstall_options: helm_core::manager_lifecycle::ManagerUninstallOptions,
) -> Result<ManagerUninstallPlan, String> {
    let active_instance = active_manager_install_instance(store, manager)?;
    match helm_core::manager_lifecycle::plan_manager_uninstall_route_with_options(
        manager,
        active_instance.as_ref(),
        allow_unknown_provenance,
        preview_only,
        &uninstall_options,
    ) {
        Ok(route) => build_provenance_manager_uninstall_plan(
            store,
            manager,
            active_instance,
            preview_only,
            route,
        ),
        Err(helm_core::manager_lifecycle::ManagerUninstallRouteError::UnsupportedManager) => {
            // Remaining managers stay intentionally gated here until their uninstall strategy
            // can be proven safe and deterministic for provenance-first routing.
            let (target_manager, request) =
                build_manager_mutation_request(store, manager, "uninstall", None)?;
            let strategy = active_instance
                .as_ref()
                .map(|instance| instance.uninstall_strategy)
                .unwrap_or(StrategyKind::InteractivePrompt);
            let preview = build_manager_uninstall_preview(
                store,
                ManagerUninstallPreviewContext {
                    requested_manager: manager,
                    target_manager,
                    request: &request,
                    strategy,
                    active_instance: active_instance.as_ref(),
                    unknown_override_required: false,
                    used_unknown_override: false,
                    legacy_fallback_used: true,
                },
                DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
            );

            if preview.read_only_blocked && !preview_only {
                return Err(
                    "manager uninstall is blocked because active provenance is read-only"
                        .to_string(),
                );
            }
            if target_manager == ManagerId::HomebrewFormula
                && !homebrew_dependency_available(store)
                && !preview_only
            {
                return Err(
                    "homebrew is required for this manager operation but was not detected on this system"
                        .to_string(),
                );
            }

            Ok(ManagerUninstallPlan {
                target_manager,
                request,
                preview,
            })
        }
        Err(error) => Err(manager_uninstall_route_error_message(manager, error)),
    }
}

fn build_provenance_manager_uninstall_plan(
    store: &SqliteStore,
    manager: ManagerId,
    active_instance: Option<ManagerInstallInstance>,
    preview_only: bool,
    route: helm_core::manager_lifecycle::ManagerUninstallRoutePlan,
) -> Result<ManagerUninstallPlan, String> {
    let preview = build_manager_uninstall_preview(
        store,
        ManagerUninstallPreviewContext {
            requested_manager: manager,
            target_manager: route.target_manager,
            request: &route.request,
            strategy: route.strategy,
            active_instance: active_instance.as_ref(),
            unknown_override_required: route.unknown_override_required,
            used_unknown_override: route.used_unknown_override,
            legacy_fallback_used: false,
        },
        DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
    );

    if preview.read_only_blocked && !preview_only {
        return Err(format!(
            "manager '{}' uninstall is blocked because active provenance is read-only",
            manager.as_str(),
        ));
    }
    if preview.unknown_override_required && !route.used_unknown_override && !preview_only {
        return Err(format!(
            "manager '{}' uninstall provenance is ambiguous; rerun with --preview to inspect blast radius, then pass --allow-unknown-provenance --yes to continue.",
            manager.as_str(),
        ));
    }
    if route.target_manager == ManagerId::HomebrewFormula
        && !homebrew_dependency_available(store)
        && !preview_only
    {
        return Err(
            "homebrew is required for this manager operation but was not detected on this system"
                .to_string(),
        );
    }

    Ok(ManagerUninstallPlan {
        target_manager: route.target_manager,
        request: route.request,
        preview,
    })
}

fn manager_uninstall_route_error_message(
    manager: ManagerId,
    error: helm_core::manager_lifecycle::ManagerUninstallRouteError,
) -> String {
    match error {
        helm_core::manager_lifecycle::ManagerUninstallRouteError::UnsupportedManager => {
            format!(
                "manager '{}' does not currently support uninstall",
                manager.as_str()
            )
        }
        helm_core::manager_lifecycle::ManagerUninstallRouteError::AmbiguousProvenance => {
            format!(
                "manager '{}' uninstall provenance is ambiguous; rerun with --preview to inspect blast radius, then pass --allow-unknown-provenance --yes to continue.",
                manager.as_str(),
            )
        }
        helm_core::manager_lifecycle::ManagerUninstallRouteError::FormulaUnresolved => {
            format!(
                "manager '{}' uninstall provenance resolved to homebrew but formula ownership could not be determined from active install instance; inspect with `helm managers instances {}` and refresh detection before retrying.",
                manager.as_str(),
                manager.as_str(),
            )
        }
        helm_core::manager_lifecycle::ManagerUninstallRouteError::InvalidOptions => {
            format!(
                "manager '{}' uninstall options are invalid for the selected strategy; review mise cleanup/config flags and retry.",
                manager.as_str()
            )
        }
    }
}

fn manager_homebrew_formula_name(manager: ManagerId) -> Option<&'static str> {
    helm_core::manager_lifecycle::manager_homebrew_formula_name(manager)
}

#[cfg(test)]
fn homebrew_formula_name_from_path(path: &Path) -> Option<String> {
    helm_core::manager_lifecycle::homebrew_formula_name_from_path(path)
}

fn active_manager_install_instance(
    store: &SqliteStore,
    manager: ManagerId,
) -> Result<Option<ManagerInstallInstance>, String> {
    let mut instances = store
        .list_install_instances(Some(manager))
        .map_err(|error| format!("failed to list manager install instances: {error}"))?;
    instances.sort_by(|left, right| {
        right
            .is_active
            .cmp(&left.is_active)
            .then_with(|| left.instance_id.cmp(&right.instance_id))
    });
    Ok(instances
        .into_iter()
        .next()
        .map(|instance| apply_manager_automation_policy(&instance)))
}

fn build_package_uninstall_preview_for_package(
    store: &SqliteStore,
    package: &PackageRef,
) -> Result<PackageUninstallPreview, String> {
    let active_instance = active_manager_install_instance(store, package.manager)?;
    let runtime_state = package_runtime_state_from_snapshot(store, package)?;
    let rustup_override_paths = rustup_override_paths_for_preview(package, runtime_state.as_ref());
    Ok(build_package_uninstall_preview(
        PackageUninstallPreviewContext {
            package,
            active_instance: active_instance.as_ref(),
            package_runtime_state: runtime_state.as_ref(),
            rustup_override_paths: rustup_override_paths.as_slice(),
        },
        DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD,
    ))
}

fn package_runtime_state_from_snapshot(
    store: &SqliteStore,
    package: &PackageRef,
) -> Result<Option<PackageRuntimeState>, String> {
    if let Some(state) = store
        .list_installed()
        .map_err(|error| format!("failed to list installed packages: {error}"))?
        .into_iter()
        .find(|row| row.package == *package)
        .map(|row| row.runtime_state)
    {
        return Ok(Some(state));
    }

    Ok(store
        .list_outdated()
        .map_err(|error| format!("failed to list outdated packages: {error}"))?
        .into_iter()
        .find(|row| row.package == *package)
        .map(|row| row.runtime_state))
}

fn rustup_override_paths_for_preview(
    package: &PackageRef,
    runtime_state: Option<&PackageRuntimeState>,
) -> Vec<String> {
    if package.manager != ManagerId::Rustup
        || !runtime_state.is_some_and(|state| state.has_override)
    {
        return Vec::new();
    }

    load_rustup_toolchain_detail_for_cli(package.name.as_str(), "preview package uninstall")
        .map(|detail| detail.override_paths)
        .unwrap_or_default()
}

fn parse_manager_id(raw: &str) -> Result<ManagerId, String> {
    raw.parse::<ManagerId>().map_err(|_| {
        format!("unknown manager id '{raw}'. Run 'helm managers list' to see supported ids.")
    })
}

fn read_setting(store: &SqliteStore, key: &str) -> Result<String, String> {
    match key {
        "safe_mode" => store
            .safe_mode()
            .map(|value| value.to_string())
            .map_err(|error| format!("failed to read setting '{key}': {error}")),
        "homebrew_keg_auto_cleanup" => store
            .homebrew_keg_policy()
            .map(|policy| matches!(policy, HomebrewKegPolicy::Cleanup).to_string())
            .map_err(|error| format!("failed to read setting '{key}': {error}")),
        "auto_check_for_updates" => store
            .auto_check_for_updates()
            .map(|value| value.to_string())
            .map_err(|error| format!("failed to read setting '{key}': {error}")),
        "auto_check_frequency_minutes" => store
            .auto_check_frequency_minutes()
            .map(|value| value.to_string())
            .map_err(|error| format!("failed to read setting '{key}': {error}")),
        "auto_check_last_checked_unix" => store
            .auto_check_last_checked_unix()
            .map(|value| {
                value
                    .map(|unix| unix.to_string())
                    .unwrap_or_else(|| "null".to_string())
            })
            .map_err(|error| format!("failed to read setting '{key}': {error}")),
        "cli_onboarding_completed" => store
            .cli_onboarding_completed()
            .map(|value| value.to_string())
            .map_err(|error| format!("failed to read setting '{key}': {error}")),
        "cli_accepted_license_terms_version" => store
            .cli_accepted_license_terms_version()
            .map(|value| value.unwrap_or_else(|| "null".to_string()))
            .map_err(|error| format!("failed to read setting '{key}': {error}")),
        _ => Err(format!("unsupported setting key '{key}'")),
    }
}

fn write_setting(store: &SqliteStore, key: &str, value: &str) -> Result<(), String> {
    match key {
        "safe_mode" => {
            let enabled = parse_bool(value)?;
            store
                .set_safe_mode(enabled)
                .map_err(|error| format!("failed to set setting '{key}': {error}"))
        }
        "homebrew_keg_auto_cleanup" => {
            let enabled = parse_bool(value)?;
            let policy = if enabled {
                HomebrewKegPolicy::Cleanup
            } else {
                HomebrewKegPolicy::Keep
            };
            store
                .set_homebrew_keg_policy(policy)
                .map_err(|error| format!("failed to set setting '{key}': {error}"))
        }
        "auto_check_for_updates" => store
            .set_auto_check_for_updates(parse_bool(value)?)
            .map_err(|error| format!("failed to set setting '{key}': {error}")),
        "auto_check_frequency_minutes" => store
            .set_auto_check_frequency_minutes(parse_positive_u32(value, key)?)
            .map_err(|error| format!("failed to set setting '{key}': {error}")),
        _ => Err(format!("unsupported setting key '{key}'")),
    }
}

fn reset_setting(store: &SqliteStore, key: &str) -> Result<(), String> {
    match key {
        "safe_mode" => store
            .set_safe_mode(false)
            .map_err(|error| format!("failed to reset setting '{key}': {error}")),
        "homebrew_keg_auto_cleanup" => store
            .set_homebrew_keg_policy(HomebrewKegPolicy::Keep)
            .map_err(|error| format!("failed to reset setting '{key}': {error}")),
        "auto_check_for_updates" => store
            .set_auto_check_for_updates(false)
            .map_err(|error| format!("failed to reset setting '{key}': {error}")),
        "auto_check_frequency_minutes" => store
            .set_auto_check_frequency_minutes(1_440)
            .map_err(|error| format!("failed to reset setting '{key}': {error}")),
        _ => Err(format!("unsupported setting key '{key}'")),
    }
}

#[derive(Debug, Clone, Copy)]
struct ParsedUpdatesRunPreviewArgs {
    include_pinned: bool,
    allow_os_updates: bool,
    manager_filter: Option<ManagerId>,
    yes: bool,
}

#[derive(Debug, Clone, Copy)]
struct ParsedListLimitArgs {
    limit: Option<usize>,
}

#[derive(Debug, Clone)]
struct ParsedTasksListArgs {
    limit: Option<usize>,
    status_filter: Option<String>,
}

fn parse_list_limit_args(
    command_args: &[String],
    command_name: &str,
) -> Result<ParsedListLimitArgs, String> {
    let mut limit: Option<usize> = None;
    let mut index = 0usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--limit" => {
                if index + 1 >= command_args.len() {
                    return Err(format!("{command_name} --limit requires a value"));
                }
                if limit.is_some() {
                    return Err(format!("{command_name} --limit specified multiple times"));
                }
                let value = command_args[index + 1]
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value '{}'", command_args[index + 1]))?;
                if value == 0 {
                    return Err("--limit must be greater than 0".to_string());
                }
                limit = Some(value);
                index += 2;
            }
            other => {
                return Err(format!(
                    "{command_name} does not support argument '{}'",
                    other
                ));
            }
        }
    }

    Ok(ParsedListLimitArgs { limit })
}

fn parse_tasks_list_args(command_args: &[String]) -> Result<ParsedTasksListArgs, String> {
    let mut limit: Option<usize> = None;
    let mut status_filter: Option<String> = None;
    let mut index = 0usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--limit" => {
                if index + 1 >= command_args.len() {
                    return Err("tasks list --limit requires a value".to_string());
                }
                if limit.is_some() {
                    return Err("tasks list --limit specified multiple times".to_string());
                }
                let value = command_args[index + 1]
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value '{}'", command_args[index + 1]))?;
                if value == 0 {
                    return Err("--limit must be greater than 0".to_string());
                }
                limit = Some(value);
                index += 2;
            }
            "--status" => {
                if index + 1 >= command_args.len() {
                    return Err("tasks list --status requires a value".to_string());
                }
                if status_filter.is_some() {
                    return Err("tasks list --status specified multiple times".to_string());
                }
                let normalized = command_args[index + 1].to_ascii_lowercase();
                if !matches!(
                    normalized.as_str(),
                    "queued" | "running" | "completed" | "failed" | "cancelled"
                ) {
                    return Err(format!(
                        "unsupported task status '{}' (expected: queued, running, completed, failed, cancelled)",
                        command_args[index + 1]
                    ));
                }
                status_filter = Some(normalized);
                index += 2;
            }
            other => {
                return Err(format!(
                    "unsupported tasks list argument '{}'; supported: --limit <n>, --status <state>",
                    other
                ));
            }
        }
    }

    Ok(ParsedTasksListArgs {
        limit,
        status_filter,
    })
}

#[derive(Debug, Clone, Copy)]
struct ParsedUpdatesListArgs {
    manager_filter: Option<ManagerId>,
    limit: Option<usize>,
}

#[derive(Debug, Clone)]
struct ParsedSearchArgs {
    query: String,
    manager_filter: Option<ManagerId>,
    local_only: bool,
    remote_only: bool,
    limit: Option<usize>,
}

fn parse_updates_list_args(command_args: &[String]) -> Result<ParsedUpdatesListArgs, String> {
    let mut manager_filter: Option<ManagerId> = None;
    let mut limit: Option<usize> = None;
    let mut index = 0usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--manager" => {
                if index + 1 >= command_args.len() {
                    return Err("updates list --manager requires a manager id".to_string());
                }
                if manager_filter.is_some() {
                    return Err("updates list --manager specified multiple times".to_string());
                }
                manager_filter = Some(parse_manager_id(&command_args[index + 1])?);
                index += 2;
            }
            "--limit" => {
                if index + 1 >= command_args.len() {
                    return Err("updates list --limit requires a value".to_string());
                }
                if limit.is_some() {
                    return Err("updates list --limit specified multiple times".to_string());
                }
                let value = command_args[index + 1]
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value '{}'", command_args[index + 1]))?;
                if value == 0 {
                    return Err("--limit must be greater than 0".to_string());
                }
                limit = Some(value);
                index += 2;
            }
            other => {
                return Err(format!(
                    "unsupported updates list argument '{}'; supported: --manager <id>, --limit <n>",
                    other
                ));
            }
        }
    }

    Ok(ParsedUpdatesListArgs {
        manager_filter,
        limit,
    })
}

fn parse_search_args(command_args: &[String]) -> Result<ParsedSearchArgs, String> {
    let mut query: Option<String> = None;
    let mut manager_filter: Option<ManagerId> = None;
    let mut local_only = false;
    let mut remote_only = false;
    let mut limit: Option<usize> = None;

    let mut index = 0usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--manager" => {
                if index + 1 >= command_args.len() {
                    return Err("search --manager requires a manager id".to_string());
                }
                if manager_filter.is_some() {
                    return Err("search --manager specified multiple times".to_string());
                }
                manager_filter = Some(parse_manager_id(&command_args[index + 1])?);
                index += 2;
            }
            "--local" => {
                local_only = true;
                index += 1;
            }
            "--remote" => {
                remote_only = true;
                index += 1;
            }
            "--limit" => {
                if index + 1 >= command_args.len() {
                    return Err("search --limit requires a value".to_string());
                }
                if limit.is_some() {
                    return Err("search --limit specified multiple times".to_string());
                }
                let value = command_args[index + 1]
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --limit value '{}'", command_args[index + 1]))?;
                if value == 0 {
                    return Err("--limit must be greater than 0".to_string());
                }
                limit = Some(value);
                index += 2;
            }
            value => {
                if query.is_some() {
                    return Err(format!(
                        "search accepts exactly one query argument (unexpected '{}')",
                        value
                    ));
                }
                query = Some(value.to_string());
                index += 1;
            }
        }
    }

    if local_only && remote_only {
        return Err("search flags --local and --remote are mutually exclusive".to_string());
    }
    let query = query.ok_or_else(|| "search requires a query argument".to_string())?;

    Ok(ParsedSearchArgs {
        query,
        manager_filter,
        local_only,
        remote_only,
        limit,
    })
}

fn parse_updates_run_preview_args(
    command_args: &[String],
    allow_yes: bool,
) -> Result<ParsedUpdatesRunPreviewArgs, String> {
    let mut include_pinned = false;
    let mut allow_os_updates = false;
    let mut manager_filter: Option<ManagerId> = None;
    let mut yes = false;

    let mut index = 0usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--include-pinned" => {
                include_pinned = true;
                index += 1;
            }
            "--allow-os-updates" => {
                allow_os_updates = true;
                index += 1;
            }
            "--manager" => {
                if index + 1 >= command_args.len() {
                    return Err("updates --manager requires a manager id".to_string());
                }
                if manager_filter.is_some() {
                    return Err("updates --manager specified multiple times".to_string());
                }
                manager_filter = Some(parse_manager_id(&command_args[index + 1])?);
                index += 2;
            }
            "--yes" if allow_yes => {
                yes = true;
                index += 1;
            }
            "--yes" => {
                return Err("--yes is only valid for 'updates run'".to_string());
            }
            other => {
                return Err(format!(
                    "unsupported updates argument '{}'; supported: --include-pinned, --allow-os-updates, --manager <id>{}",
                    other,
                    if allow_yes { ", --yes" } else { "" }
                ));
            }
        }
    }

    Ok(ParsedUpdatesRunPreviewArgs {
        include_pinned,
        allow_os_updates,
        manager_filter,
        yes,
    })
}

fn manager_authority_key(id: ManagerId) -> &'static str {
    match registry::manager(id).map(|descriptor| descriptor.authority) {
        Some(ManagerAuthority::Authoritative) => "authoritative",
        Some(ManagerAuthority::Standard) => "standard",
        Some(ManagerAuthority::Guarded) => "guarded",
        Some(ManagerAuthority::DetectionOnly) => "detection_only",
        None => "standard",
    }
}

fn upgrade_plan_step_id(manager: ManagerId, package_name: &str) -> String {
    format!("{}:{}", manager.as_str(), package_name)
}

fn upgrade_request_name(step: &UpgradeExecutionStep) -> String {
    if step.manager == ManagerId::HomebrewFormula && step.cleanup_old_kegs {
        encode_homebrew_upgrade_target(&step.package_name, true)
    } else {
        step.package_name.clone()
    }
}

fn manager_operation_failure_error(operation: &str, failures: usize) -> Option<String> {
    if failures == 0 {
        return None;
    }
    let exit_code = if failures > 1 { 3 } else { 2 };
    Some(mark_exit_code(
        format!("{failures} manager {operation} operations failed"),
        exit_code,
    ))
}

fn count_upgrade_step_failures<F>(steps: &[UpgradeExecutionStep], mut run_step: F) -> usize
where
    F: FnMut(&UpgradeExecutionStep) -> Result<(), String>,
{
    let mut failures = 0usize;
    for step in steps {
        if let Err(error) = run_step(step) {
            failures += 1;
            verbose_log(format!(
                "upgrade step failed manager='{}' package='{}': {}",
                step.manager.as_str(),
                step.package_name,
                error
            ));
        }
    }
    failures
}

fn serialize_upgrade_plan_steps(steps: &[UpgradeExecutionStep]) -> Vec<CliUpgradePlanStep> {
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| CliUpgradePlanStep {
            step_id: upgrade_plan_step_id(step.manager, &step.package_name),
            order_index: index as u64,
            manager_id: step.manager.as_str().to_string(),
            authority: manager_authority_key(step.manager).to_string(),
            action: "upgrade".to_string(),
            package_name: step.package_name.clone(),
            pinned: step.pinned,
            restart_required: step.restart_required,
            cleanup_old_kegs: step.cleanup_old_kegs,
        })
        .collect()
}

fn collect_upgrade_execution_steps(
    store: &SqliteStore,
    runtime: &AdapterRuntime,
    include_pinned: bool,
    allow_os_updates: bool,
    manager_filter: Option<ManagerId>,
) -> Result<Vec<UpgradeExecutionStep>, String> {
    let enabled_map = manager_enabled_map(store)?;
    let outdated = list_outdated_for_enabled(store, &enabled_map)?;
    let pinned_keys: std::collections::HashSet<String> = store
        .list_pins()
        .map_err(|error| format!("failed to list pin records: {error}"))?
        .into_iter()
        .map(|pin| format!("{}:{}", pin.package.manager.as_str(), pin.package.name))
        .collect();

    let mut manager_steps: HashMap<ManagerId, Vec<UpgradeExecutionStep>> = HashMap::new();
    let mut seen_steps: std::collections::HashSet<String> = std::collections::HashSet::new();

    for package in outdated {
        let manager = package.package.manager;
        if manager_filter.is_some() && manager_filter != Some(manager) {
            continue;
        }
        if !runtime.has_manager(manager)
            || !runtime.supports_capability(manager, Capability::Upgrade)
        {
            continue;
        }

        let package_key = format!("{}:{}", manager.as_str(), package.package.name);
        if !include_pinned && (package.pinned || pinned_keys.contains(&package_key)) {
            continue;
        }

        let step_name = if manager == ManagerId::SoftwareUpdate {
            if !allow_os_updates || runtime.is_safe_mode() {
                continue;
            }
            "__confirm_os_updates__".to_string()
        } else {
            package.package.name.clone()
        };

        let dedupe_key = format!("{}:{}", manager.as_str(), step_name);
        if !seen_steps.insert(dedupe_key) {
            continue;
        }

        let cleanup_old_kegs = manager == ManagerId::HomebrewFormula
            && effective_homebrew_keg_policy(store, &package.package.name)
                == HomebrewKegPolicy::Cleanup;
        manager_steps
            .entry(manager)
            .or_default()
            .push(UpgradeExecutionStep {
                manager,
                package_name: step_name,
                cleanup_old_kegs,
                pinned: package.pinned || pinned_keys.contains(&package_key),
                restart_required: package.restart_required,
            });
    }

    for steps in manager_steps.values_mut() {
        steps.sort_by(|left, right| left.package_name.cmp(&right.package_name));
    }

    let adapters = runtime.adapter_list();
    let adapter_refs: Vec<&dyn ManagerAdapter> =
        adapters.iter().map(|adapter| adapter.as_ref()).collect();
    let phases = helm_core::orchestration::authority_order::authority_phases(&adapter_refs);
    let overrides = load_manager_priority_overrides(store)?;
    let mut ordered_steps: Vec<UpgradeExecutionStep> = Vec::new();

    for phase in phases {
        let mut phase_managers = phase;
        phase_managers.sort_by(|left, right| {
            let left_rank = manager_effective_local_rank(*left, &overrides);
            let right_rank = manager_effective_local_rank(*right, &overrides);
            if left_rank != right_rank {
                return left_rank.cmp(&right_rank);
            }
            left.as_str().cmp(right.as_str())
        });
        for manager in phase_managers {
            if let Some(steps) = manager_steps.remove(&manager) {
                ordered_steps.extend(steps);
            }
        }
    }

    if !manager_steps.is_empty() {
        let mut managers = manager_steps.keys().copied().collect::<Vec<_>>();
        managers.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        for manager in managers {
            if let Some(steps) = manager_steps.remove(&manager) {
                ordered_steps.extend(steps);
            }
        }
    }

    Ok(ordered_steps)
}

fn parse_bool(raw: &str) -> Result<bool, String> {
    if raw.eq_ignore_ascii_case("true")
        || raw.eq_ignore_ascii_case("1")
        || raw.eq_ignore_ascii_case("yes")
        || raw.eq_ignore_ascii_case("on")
    {
        return Ok(true);
    }
    if raw.eq_ignore_ascii_case("false")
        || raw.eq_ignore_ascii_case("0")
        || raw.eq_ignore_ascii_case("no")
        || raw.eq_ignore_ascii_case("off")
    {
        return Ok(false);
    }
    Err(format!(
        "invalid boolean value '{raw}' (expected true/false, 1/0, yes/no, on/off)"
    ))
}

fn parse_positive_u32(raw: &str, key: &str) -> Result<u32, String> {
    let value = raw
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("invalid integer value '{}' for '{}'", raw, key))?;
    if value == 0 {
        return Err(format!("'{}' must be greater than 0", key));
    }
    Ok(value)
}

fn is_help_token(raw: &str) -> bool {
    matches!(raw, "help" | "-h" | "--help")
}

fn print_help_topic(command_args: &[String]) -> bool {
    let Some(topic) = command_args.first() else {
        return false;
    };
    let Some(command) = parse_top_level_command(topic.as_str()) else {
        return false;
    };
    print_command_help_topic(command, &command_args[1..])
}

fn print_command_help_topic(command: Command, path: &[String]) -> bool {
    match command {
        Command::Tui => {
            if !path.is_empty() {
                return false;
            }
            print_tui_help();
            true
        }
        Command::Status => {
            if !path.is_empty() {
                return false;
            }
            print_status_help();
            true
        }
        Command::Refresh => {
            if !path.is_empty() {
                return false;
            }
            print_refresh_help();
            true
        }
        Command::Search => {
            if !path.is_empty() {
                return false;
            }
            print_search_help();
            true
        }
        Command::Ls => {
            if !path.is_empty() {
                return false;
            }
            print_packages_list_help();
            true
        }
        Command::Packages => print_packages_help_topic(path),
        Command::Updates => print_updates_help_topic(path),
        Command::Tasks => print_tasks_help_topic(path),
        Command::Managers => print_managers_help_topic(path),
        Command::Settings => print_settings_help_topic(path),
        Command::Diagnostics => print_diagnostics_help_topic(path),
        Command::Doctor => print_doctor_help_topic(path),
        Command::Onboarding => print_onboarding_help_topic(path),
        Command::SelfCmd => print_self_help_topic(path),
        Command::Completion => print_completion_help_topic(path),
        Command::InternalCoordinator | Command::Help | Command::Version => false,
    }
}

fn print_tui_help() {
    println!("USAGE:");
    println!("  helm");
    println!();
    println!("DESCRIPTION:");
    println!("  Launch the interactive Helm terminal UI (TUI) when stdout is a TTY.");
    println!("  In non-TTY contexts, 'helm' without arguments prints help instead.");
}

fn print_packages_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_packages_help();
        return true;
    }
    if path.len() == 1 {
        return match path[0].as_str() {
            "list" => {
                print_packages_list_help();
                true
            }
            "search" => {
                print_packages_search_help();
                true
            }
            "show" => {
                print_packages_show_help();
                true
            }
            "install" => {
                print_packages_install_help();
                true
            }
            "uninstall" => {
                print_packages_uninstall_help();
                true
            }
            "upgrade" => {
                print_packages_upgrade_help();
                true
            }
            "pin" => {
                print_packages_pin_help();
                true
            }
            "unpin" => {
                print_packages_unpin_help();
                true
            }
            "rustup" => {
                print_packages_rustup_help();
                true
            }
            "keg-policy" => {
                print_packages_keg_policy_help();
                true
            }
            _ => false,
        };
    }

    if path.len() == 2 && path[0] == "rustup" {
        return match path[1].as_str() {
            "show" => {
                print_packages_rustup_show_help();
                true
            }
            "component" => {
                print_packages_rustup_component_help();
                true
            }
            "target" => {
                print_packages_rustup_target_help();
                true
            }
            "default" => {
                print_packages_rustup_default_help();
                true
            }
            "override" => {
                print_packages_rustup_override_help();
                true
            }
            "profile" => {
                print_packages_rustup_profile_help();
                true
            }
            _ => false,
        };
    }

    if path.len() == 3 && path[0] == "rustup" {
        return match (path[1].as_str(), path[2].as_str()) {
            ("component", "add") | ("component", "remove") => {
                print_packages_rustup_component_help();
                true
            }
            ("target", "add") | ("target", "remove") => {
                print_packages_rustup_target_help();
                true
            }
            ("override", "set") | ("override", "unset") => {
                print_packages_rustup_override_help();
                true
            }
            ("profile", "set") => {
                print_packages_rustup_profile_help();
                true
            }
            _ => false,
        };
    }

    if path.len() == 2 && path[0] == "keg-policy" {
        return match path[1].as_str() {
            "list" => {
                print_packages_keg_policy_list_help();
                true
            }
            "get" => {
                print_packages_keg_policy_get_help();
                true
            }
            "set" => {
                print_packages_keg_policy_set_help();
                true
            }
            "reset" => {
                print_packages_keg_policy_reset_help();
                true
            }
            _ => false,
        };
    }

    false
}

fn print_updates_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_updates_help();
        return true;
    }
    if path.len() != 1 {
        return false;
    }
    match path[0].as_str() {
        "list" => {
            print_updates_list_help();
            true
        }
        "summary" => {
            print_updates_summary_help();
            true
        }
        "preview" => {
            print_updates_preview_help();
            true
        }
        "run" => {
            print_updates_run_help();
            true
        }
        _ => false,
    }
}

fn print_tasks_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_tasks_help();
        return true;
    }
    if path.len() != 1 {
        return false;
    }
    match path[0].as_str() {
        "list" => {
            print_tasks_list_help();
            true
        }
        "show" => {
            print_tasks_show_help();
            true
        }
        "logs" => {
            print_tasks_logs_help();
            true
        }
        "output" => {
            print_tasks_output_help();
            true
        }
        "follow" => {
            print_tasks_follow_help();
            true
        }
        "cancel" => {
            print_tasks_cancel_help();
            true
        }
        _ => false,
    }
}

fn print_managers_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_managers_help();
        return true;
    }

    if path.len() == 1 {
        match path[0].as_str() {
            "list" => {
                print_managers_list_help();
                return true;
            }
            "show" => {
                print_managers_show_help();
                return true;
            }
            "detect" => {
                print_managers_detect_help();
                return true;
            }
            "enable" => {
                print_managers_enable_help();
                return true;
            }
            "disable" => {
                print_managers_disable_help();
                return true;
            }
            "install" => {
                print_managers_install_help();
                return true;
            }
            "update" => {
                print_managers_update_help();
                return true;
            }
            "uninstall" => {
                print_managers_uninstall_help();
                return true;
            }
            "executables" => {
                print_managers_executables_help();
                return true;
            }
            "install-methods" => {
                print_managers_install_methods_help();
                return true;
            }
            "instances" => {
                print_managers_instances_help();
                return true;
            }
            "priority" => {
                print_managers_priority_help();
                return true;
            }
            _ => {}
        }
    }

    if path.len() == 2 {
        match (path[0].as_str(), path[1].as_str()) {
            ("executables", "list") => {
                print_managers_executables_list_help();
                return true;
            }
            ("executables", "set") => {
                print_managers_executables_set_help();
                return true;
            }
            ("install-methods", "list") => {
                print_managers_install_methods_list_help();
                return true;
            }
            ("install-methods", "set") => {
                print_managers_install_methods_set_help();
                return true;
            }
            ("instances", "ack") => {
                print_managers_instances_ack_help();
                return true;
            }
            ("instances", "clear-ack") => {
                print_managers_instances_clear_ack_help();
                return true;
            }
            ("priority", "list") => {
                print_managers_priority_list_help();
                return true;
            }
            ("priority", "set") => {
                print_managers_priority_set_help();
                return true;
            }
            ("priority", "reset") => {
                print_managers_priority_reset_help();
                return true;
            }
            _ => {}
        }
    }

    if path.len() == 3 && (path[0].as_str(), path[1].as_str()) == ("instances", "set-active") {
        print_managers_instances_set_active_help();
        return true;
    }

    false
}

fn print_settings_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_settings_help();
        return true;
    }
    if path.len() != 1 {
        return false;
    }
    match path[0].as_str() {
        "list" => {
            print_settings_list_help();
            true
        }
        "get" => {
            print_settings_get_help();
            true
        }
        "set" => {
            print_settings_set_help();
            true
        }
        "reset" => {
            print_settings_reset_help();
            true
        }
        _ => false,
    }
}

fn print_onboarding_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_onboarding_help();
        return true;
    }
    if path.len() != 1 {
        return false;
    }
    match path[0].as_str() {
        "status" => {
            print_onboarding_status_help();
            true
        }
        "run" => {
            print_onboarding_run_help();
            true
        }
        "reset" => {
            print_onboarding_reset_help();
            true
        }
        _ => false,
    }
}

fn print_self_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_self_help();
        return true;
    }
    if path.len() == 1 {
        match path[0].as_str() {
            "status" => {
                print_self_status_help();
                return true;
            }
            "check" => {
                print_self_check_help();
                return true;
            }
            "update" => {
                print_self_update_help();
                return true;
            }
            "uninstall" => {
                print_self_uninstall_help();
                return true;
            }
            "auto-check" => {
                print_self_auto_check_help();
                return true;
            }
            _ => {}
        }
    }

    if path.len() == 2 && path[0] == "auto-check" {
        match path[1].as_str() {
            "status" => {
                print_self_auto_check_status_help();
                return true;
            }
            "enable" => {
                print_self_auto_check_enable_help();
                return true;
            }
            "disable" => {
                print_self_auto_check_disable_help();
                return true;
            }
            "frequency" => {
                print_self_auto_check_frequency_help();
                return true;
            }
            _ => {}
        }
    }

    false
}

fn print_diagnostics_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_diagnostics_help();
        return true;
    }
    if path.len() != 1 {
        return false;
    }
    match path[0].as_str() {
        "summary" => {
            print_diagnostics_summary_help();
            true
        }
        "task" => {
            print_diagnostics_task_help();
            true
        }
        "manager" => {
            print_diagnostics_manager_help();
            true
        }
        "provenance" => {
            print_diagnostics_provenance_help();
            true
        }
        "export" => {
            print_diagnostics_export_help();
            true
        }
        _ => false,
    }
}

fn print_completion_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_completion_help();
        return true;
    }
    false
}

fn print_doctor_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_doctor_help();
        return true;
    }
    if path.len() == 1 {
        match path[0].as_str() {
            "scan" => {
                print_doctor_scan_help();
                return true;
            }
            "repair" => {
                print_doctor_repair_help();
                return true;
            }
            _ => {}
        }
    }
    if path.len() == 2 && path[0] == "repair" {
        match path[1].as_str() {
            "plan" => {
                print_doctor_repair_plan_help();
                return true;
            }
            "apply" => {
                print_doctor_repair_apply_help();
                return true;
            }
            _ => {}
        }
    }
    false
}

fn print_help() {
    println!("Helm CLI");
    println!("Copyright (c) 2026 Jason Cavinder");
    println!();
    println!("NOTE:");
    println!("  helm (no arguments) launches the interactive TUI when stdout is a TTY.");
    println!("  In non-TTY contexts, helm (no arguments) prints this help.");
    println!();
    println!("USAGE:");
    println!("  helm");
    println!(
        "  helm [--json|--ndjson] [-v|--verbose|-q|--quiet] [--no-color] [--locale <id>] [--timeout <seconds>] [--wait|--detach] <command> [subcommand]"
    );
    println!("  helm -V | --version");
    println!("  helm help");
    println!("  helm <command> help");
    println!("  helm help <command> [subcommand]");
    println!();
    println!("COMMANDS:");
    println!("  status                 Show overall snapshot summary");
    println!("  refresh                Run detection + refresh pipeline");
    println!("  search <query>         Progressive package search (local + remote)");
    println!("  ls                     List installed packages (alias)");
    println!("  packages [list|search|show|install|uninstall|upgrade|pin|unpin|rustup|keg-policy]");
    println!("                         Package listing/search/details and mutations");
    println!("  updates [list|summary|preview|run]");
    println!("                         List/summarize/preview/run package upgrades");
    println!("  tasks [list|show|logs|output|follow|cancel]");
    println!("                         Inspect task state/logs/output and cancellation status");
    println!(
        "  managers [list|show|detect|enable|disable|install|update|uninstall|executables|install-methods|instances|priority]"
    );
    println!("                         Manager status, enablement, and selection controls");
    println!("  settings [list|get|set|reset]");
    println!("                         Read and update selected settings");
    println!("  diagnostics [summary|task|manager|provenance|export]");
    println!("                         Read diagnostics snapshots and export support data");
    println!("  doctor [scan|repair]");
    println!("                         Doctor scans and repair workflows");
    println!("  onboarding [status|run|reset]");
    println!("                         Inspect/run/reset CLI first-run onboarding state");
    println!("  self [status|check|update|uninstall|auto-check]");
    println!("                         Helm self-update/uninstall namespace");
    println!("  completion [bash|zsh|fish]");
    println!("                         Generate shell completion scripts");
    println!("  help                   Show this help");
    println!();
    println!("GLOBAL FLAGS:");
    println!("  --json                 Emit JSON output");
    println!("  --ndjson               Emit newline-delimited JSON output");
    println!("  -v, --verbose          Emit verbose diagnostics to stderr");
    println!("  -q, --quiet            Suppress non-error output in supported commands");
    println!("  --no-color             Disable ANSI color output (human mode)");
    println!("  --locale <id>          Override locale identifier for this invocation");
    println!("  --timeout <seconds>    Override coordinator request timeout");
    println!("  --wait                 Wait for task completion (default)");
    println!("  --detach               Return after task submission");
    println!("  --accept-license       Auto-accept CLI license terms during first-run onboarding");
    println!("  --accept-defaults      Auto-apply default CLI onboarding settings");
    println!("  -V, --version          Show version");
}

fn print_status_help() {
    println!("USAGE:");
    println!("  helm status");
    println!();
    println!("DESCRIPTION:");
    println!(
        "  Show an aggregate snapshot: installed/updatable packages, tasks, and manager counts."
    );
}

fn print_refresh_help() {
    println!("USAGE:");
    println!("  helm refresh [--all|--manager <id>|<id>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Run detection + refresh actions for one manager or all enabled managers.");
}

fn print_search_help() {
    println!("USAGE:");
    println!("  helm search <query> [--manager <id>] [--local|--remote] [--limit <n>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Progressive package search: local cache plus remote manager fan-out.");
    println!("  Use --local for cache-only or --remote for remote-only.");
}

fn print_packages_help() {
    println!("USAGE:");
    println!("  helm packages <subcommand> [args]");
    println!("  helm ls [--limit <n>]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  list [--limit <n>]");
    println!("  search <query> [--manager <id>] [--local|--remote] [--limit <n>]");
    println!("  show <name> [--manager <id>]");
    println!("  install <name|name@manager> --manager <id> [--version <v>]");
    println!("  uninstall <name|name@manager> --manager <id> [--preview] [--yes]");
    println!("  upgrade <name|name@manager> --manager <id>");
    println!("  pin <name|name@manager> --manager <id> [--version <v>]");
    println!("  unpin <name|name@manager> --manager <id>");
    println!("  rustup <show|component|target|default|override|profile> ...");
    println!("  keg-policy <list|get|set|reset> ...");
    println!();
    println!("DESCRIPTION:");
    println!("  List/search/show and mutate package state for manager-scoped package references.");
}

fn print_packages_list_help() {
    println!("USAGE:");
    println!("  helm packages list [--limit <n>]");
    println!("  helm ls [--limit <n>]");
    println!();
    println!("DESCRIPTION:");
    println!("  List installed packages from the local snapshot.");
    println!("  Use --limit to cap returned rows.");
}

fn print_packages_search_help() {
    println!("USAGE:");
    println!("  helm packages search <query> [--manager <id>] [--local|--remote] [--limit <n>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Progressive package search: local cache plus remote manager fan-out.");
}

fn print_packages_show_help() {
    println!("USAGE:");
    println!("  helm packages show <name> [--manager <id>]");
    println!("  helm packages show <name@manager>");
    println!();
    println!("DESCRIPTION:");
    println!("  Show manager-scoped package details with ambiguity protection.");
    println!(
        "  For asdf/mise package coordinates, <name@selector> is accepted when manager is explicit."
    );
}

fn print_packages_install_help() {
    println!("USAGE:");
    println!("  helm packages install <name|name@manager> --manager <id> [--version <v>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Install a package via the selected manager.");
    println!("  For asdf/mise, <name@selector> can be used instead of --version <selector>.");
}

fn print_packages_uninstall_help() {
    println!("USAGE:");
    println!("  helm packages uninstall <name|name@manager> --manager <id> [--preview] [--yes]");
    println!();
    println!("DESCRIPTION:");
    println!(
        "  Uninstall a package via the selected manager. Use --preview to inspect blast radius."
    );
}

fn print_packages_upgrade_help() {
    println!("USAGE:");
    println!("  helm packages upgrade <name|name@manager> --manager <id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Upgrade a package via the selected manager.");
}

fn print_packages_pin_help() {
    println!("USAGE:");
    println!("  helm packages pin <name|name@manager> --manager <id> [--version <v>]");
    println!();
    println!("DESCRIPTION:");
    println!(
        "  Pin a package version (native manager pinning when available, otherwise virtual pin)."
    );
    println!("  For asdf/mise, <name@selector> can be used instead of --version <selector>.");
}

fn print_packages_unpin_help() {
    println!("USAGE:");
    println!("  helm packages unpin <name|name@manager> --manager <id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Remove pin state for a package.");
}

fn print_packages_rustup_help() {
    println!("USAGE:");
    println!("  helm packages rustup show <toolchain>");
    println!("  helm packages rustup component <add|remove> <toolchain> <component>");
    println!("  helm packages rustup target <add|remove> <toolchain> <target>");
    println!("  helm packages rustup default <toolchain>");
    println!("  helm packages rustup override <set|unset> <toolchain> <absolute-path>");
    println!("  helm packages rustup profile set <minimal|default|complete>");
    println!();
    println!("DESCRIPTION:");
    println!(
        "  Inspect and configure rustup-managed toolchains, components, targets, overrides, and profile state."
    );
}

fn print_packages_rustup_show_help() {
    println!("USAGE:");
    println!("  helm packages rustup show <toolchain>");
    println!();
    println!("DESCRIPTION:");
    println!(
        "  Show rustup-specific detail for one installed toolchain, including runtime state, profile, overrides, components, and targets."
    );
}

fn print_packages_rustup_component_help() {
    println!("USAGE:");
    println!("  helm packages rustup component add <toolchain> <component>");
    println!("  helm packages rustup component remove <toolchain> <component>");
    println!();
    println!("DESCRIPTION:");
    println!("  Add or remove one rustup component for a specific toolchain.");
}

fn print_packages_rustup_target_help() {
    println!("USAGE:");
    println!("  helm packages rustup target add <toolchain> <target>");
    println!("  helm packages rustup target remove <toolchain> <target>");
    println!();
    println!("DESCRIPTION:");
    println!("  Add or remove one compilation target for a specific rustup toolchain.");
}

fn print_packages_rustup_default_help() {
    println!("USAGE:");
    println!("  helm packages rustup default <toolchain>");
    println!();
    println!("DESCRIPTION:");
    println!("  Set the rustup default toolchain.");
}

fn print_packages_rustup_override_help() {
    println!("USAGE:");
    println!("  helm packages rustup override set <toolchain> <absolute-path>");
    println!("  helm packages rustup override unset <toolchain> <absolute-path>");
    println!();
    println!("DESCRIPTION:");
    println!("  Set or clear a rustup directory override for one absolute filesystem path.");
}

fn print_packages_rustup_profile_help() {
    println!("USAGE:");
    println!("  helm packages rustup profile set <minimal|default|complete>");
    println!();
    println!("DESCRIPTION:");
    println!("  Change the rustup installation profile used for future component defaults.");
}

fn print_packages_keg_policy_help() {
    println!("USAGE:");
    println!("  helm packages keg-policy list");
    println!("  helm packages keg-policy get <name|name@homebrew_formula>");
    println!("  helm packages keg-policy set <name|name@homebrew_formula> <keep|cleanup|default>");
    println!("  helm packages keg-policy reset <name|name@homebrew_formula>");
    println!();
    println!("DESCRIPTION:");
    println!("  Manage per-package Homebrew keg cleanup overrides.");
}

fn print_packages_keg_policy_list_help() {
    println!("USAGE:");
    println!("  helm packages keg-policy list");
    println!();
    println!("DESCRIPTION:");
    println!("  List global default Homebrew keg policy and package-specific overrides.");
}

fn print_packages_keg_policy_get_help() {
    println!("USAGE:");
    println!("  helm packages keg-policy get <name|name@homebrew_formula>");
    println!();
    println!("DESCRIPTION:");
    println!("  Show default, override, and effective Homebrew keg policy for one package.");
}

fn print_packages_keg_policy_set_help() {
    println!("USAGE:");
    println!("  helm packages keg-policy set <name|name@homebrew_formula> <keep|cleanup|default>");
    println!();
    println!("DESCRIPTION:");
    println!("  Set or clear package-specific Homebrew keg policy override.");
}

fn print_packages_keg_policy_reset_help() {
    println!("USAGE:");
    println!("  helm packages keg-policy reset <name|name@homebrew_formula>");
    println!();
    println!("DESCRIPTION:");
    println!("  Clear package-specific Homebrew keg policy override and inherit default.");
}

fn print_updates_help() {
    println!("USAGE:");
    println!("  helm updates list [--manager <id>] [--limit <n>]");
    println!("  helm updates summary");
    println!("  helm updates preview [--include-pinned] [--allow-os-updates] [--manager <id>]");
    println!("  helm updates run --yes [--include-pinned] [--allow-os-updates] [--manager <id>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Inspect and execute bulk upgrade plans from the cached outdated snapshot.");
}

fn print_updates_list_help() {
    println!("USAGE:");
    println!("  helm updates list [--manager <id>] [--limit <n>]");
    println!();
    println!("DESCRIPTION:");
    println!("  List available package updates from the current snapshot.");
    println!("  Use --limit to cap returned rows.");
}

fn print_updates_summary_help() {
    println!("USAGE:");
    println!("  helm updates summary");
    println!();
    println!("DESCRIPTION:");
    println!("  Show aggregate update counts and manager breakdown.");
}

fn print_updates_preview_help() {
    println!("USAGE:");
    println!("  helm updates preview [--include-pinned] [--allow-os-updates] [--manager <id>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Build and display ordered upgrade steps without executing.");
}

fn print_updates_run_help() {
    println!("USAGE:");
    println!("  helm updates run --yes [--include-pinned] [--allow-os-updates] [--manager <id>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Execute the ordered upgrade plan derived from the current snapshot.");
}

fn print_tasks_help() {
    println!("USAGE:");
    println!(
        "  helm tasks list [--limit <n>] [--status queued|running|completed|failed|cancelled]"
    );
    println!("  helm tasks show <task-id>");
    println!(
        "  helm tasks logs <task-id> [--limit <n>] [--level info|warn|error] [--status queued|running|completed|failed|cancelled]"
    );
    println!("  helm tasks output <task-id>");
    println!("  helm tasks follow <task-id> [--poll-ms <ms>] [--timeout-ms <ms>]");
    println!("  helm tasks cancel <task-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Inspect task state/logs/output and follow lifecycle logs.");
    println!(
        "  tasks follow is text-stream only; --json/--ndjson are not supported (exit code 1)."
    );
    println!("  Task cancellation routes through the shared CLI coordinator.");
}

fn print_tasks_list_help() {
    println!("USAGE:");
    println!(
        "  helm tasks list [--limit <n>] [--status queued|running|completed|failed|cancelled]"
    );
    println!();
    println!("DESCRIPTION:");
    println!("  List recent tasks for enabled managers.");
    println!("  Use --limit to cap returned rows and --status to filter by lifecycle state.");
}

fn print_tasks_show_help() {
    println!("USAGE:");
    println!("  helm tasks show <task-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Show summary fields for a single task.");
}

fn print_tasks_logs_help() {
    println!("USAGE:");
    println!(
        "  helm tasks logs <task-id> [--limit <n>] [--level info|warn|error] [--status queued|running|completed|failed|cancelled]"
    );
    println!();
    println!("DESCRIPTION:");
    println!("  Read persisted lifecycle logs for a task.");
}

fn print_tasks_output_help() {
    println!("USAGE:");
    println!("  helm tasks output <task-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Read in-process captured stdout/stderr when available.");
}

fn print_tasks_follow_help() {
    println!("USAGE:");
    println!("  helm tasks follow <task-id> [--poll-ms <ms>] [--timeout-ms <ms>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Poll and stream persisted lifecycle logs until terminal status or timeout.");
    println!("  --json/--ndjson are not supported for tasks follow and return exit code 1.");
}

fn print_tasks_cancel_help() {
    println!("USAGE:");
    println!("  helm tasks cancel <task-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Cancel a task through the shared CLI coordinator.");
}

fn print_managers_help() {
    println!("USAGE:");
    println!("  helm managers <subcommand> [args]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  list");
    println!("  show <manager-id>");
    println!("  detect [--all|--manager <id>|<id>]");
    println!("  enable <manager-id>");
    println!("  disable <manager-id>");
    println!("  install <manager-id>");
    println!("  update <manager-id>");
    println!("  uninstall <manager-id>");
    println!("  executables list <manager-id>");
    println!("  executables set <manager-id> <path|path-default>");
    println!("  install-methods list <manager-id>");
    println!("  install-methods set <manager-id> <method-id|default>");
    println!("  instances [<manager-id>]");
    println!("  instances ack <manager-id>");
    println!("  instances clear-ack <manager-id>");
    println!("  instances set-active <manager-id> <instance-id>");
    println!("  priority list");
    println!("  priority set <manager-id> --rank <n>");
    println!("  priority reset");
    println!();
    println!("DESCRIPTION:");
    println!(
        "  Manage manager enablement, lifecycle actions, executable/install-method selection, and priority ordering."
    );
}

fn print_managers_list_help() {
    println!("USAGE:");
    println!("  helm managers list");
    println!();
    println!("DESCRIPTION:");
    println!("  List manager status and capability metadata.");
}

fn print_managers_show_help() {
    println!("USAGE:");
    println!("  helm managers show <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Show status details for one manager.");
}

fn print_managers_detect_help() {
    println!("USAGE:");
    println!("  helm managers detect [--all|--manager <id>|<id>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Run manager detection for one manager or all enabled managers.");
}

fn print_managers_enable_help() {
    println!("USAGE:");
    println!("  helm managers enable <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Enable a manager in Helm preferences.");
}

fn print_managers_disable_help() {
    println!("USAGE:");
    println!("  helm managers disable <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Disable a manager in Helm preferences.");
}

fn print_managers_install_help() {
    println!("USAGE:");
    println!(
        "  helm managers install <manager-id> [--method <method-id>] [--rustup-install-source <officialDownload|existingBinaryPath>] [--rustup-binary-path <path>] [--mise-install-source <officialDownload|existingBinaryPath>] [--mise-binary-path <path>]"
    );
    println!();
    println!("DESCRIPTION:");
    println!("  Install a supported manager.");
    println!("  Use --method for a one-off method override without changing saved preferences.");
    println!(
        "  If method choice is ambiguous, interactive TTY prompts; non-interactive mode requires --method."
    );
    println!(
        "  rustup-only: --rustup-install-source selects official download vs existing binary path. --rustup-binary-path implies existingBinaryPath when source is omitted."
    );
    println!(
        "  mise-only: --mise-install-source selects script installer source mode. --mise-binary-path implies existingBinaryPath when source is omitted."
    );
}

fn print_managers_update_help() {
    println!("USAGE:");
    println!("  helm managers update <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Update a supported manager via manager-specific routing.");
}

fn print_managers_uninstall_help() {
    println!("USAGE:");
    println!(
        "  helm managers uninstall <manager-id> [--preview] [--yes] [--allow-unknown-provenance] [--homebrew-cleanup-mode <managerOnly|fullCleanup>] [--mise-cleanup-mode <managerOnly|fullCleanup>] [--mise-config-removal <keepConfig|removeConfig>]"
    );
    println!();
    println!("DESCRIPTION:");
    println!(
        "  Uninstall a supported manager via detected provenance strategy with blast-radius preview."
    );
    println!("  homebrew-routed managers: --homebrew-cleanup-mode defaults to managerOnly.");
    println!("  mise-only: --mise-cleanup-mode defaults to managerOnly.");
    println!("  mise-only: fullCleanup requires --mise-config-removal keepConfig|removeConfig.");
}

fn print_managers_executables_help() {
    println!("USAGE:");
    println!("  helm managers executables list <manager-id>");
    println!("  helm managers executables set <manager-id> <path|path-default>");
    println!();
    println!("DESCRIPTION:");
    println!("  Inspect or select executable resolution for a manager.");
}

fn print_managers_executables_list_help() {
    println!("USAGE:");
    println!("  helm managers executables list <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  List active/default/discovered executable paths for a manager.");
}

fn print_managers_executables_set_help() {
    println!("USAGE:");
    println!("  helm managers executables set <manager-id> <path|path-default>");
    println!();
    println!("DESCRIPTION:");
    println!("  Persist executable override (or reset to default selection).");
}

fn print_managers_install_methods_help() {
    println!("USAGE:");
    println!("  helm managers install-methods list <manager-id>");
    println!("  helm managers install-methods set <manager-id> <method-id|default>");
    println!();
    println!("DESCRIPTION:");
    println!("  Inspect or select preferred install method for a manager.");
}

fn print_managers_install_methods_list_help() {
    println!("USAGE:");
    println!("  helm managers install-methods list <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  List available install methods and current selection.");
}

fn print_managers_install_methods_set_help() {
    println!("USAGE:");
    println!("  helm managers install-methods set <manager-id> <method-id|default>");
    println!();
    println!("DESCRIPTION:");
    println!("  Persist install-method preference (or reset to default behavior).");
}

fn print_managers_instances_help() {
    println!("USAGE:");
    println!("  helm managers instances [<manager-id>]");
    println!("  helm managers instances ack <manager-id>");
    println!("  helm managers instances clear-ack <manager-id>");
    println!("  helm managers instances set-active <manager-id> <instance-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Show detected install instances with provenance, confidence, and explainability.");
    println!("  Use ack/clear-ack to manage multi-instance acknowledgement state.");
    println!("  Use set-active to select which detected install instance Helm manages.");
}

fn print_managers_instances_ack_help() {
    println!("USAGE:");
    println!("  helm managers instances ack <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Acknowledge an intentional multi-install manager state.");
}

fn print_managers_instances_clear_ack_help() {
    println!("USAGE:");
    println!("  helm managers instances clear-ack <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Clear the stored multi-install acknowledgement for a manager.");
}

fn print_managers_instances_set_active_help() {
    println!("USAGE:");
    println!("  helm managers instances set-active <manager-id> <instance-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Set the active managed install instance and clear any prior acknowledgement.");
}

fn print_managers_priority_help() {
    println!("USAGE:");
    println!("  helm managers priority list");
    println!("  helm managers priority set <manager-id> --rank <n>");
    println!("  helm managers priority reset");
    println!();
    println!("DESCRIPTION:");
    println!("  Inspect and manage manager priority ordering within authority groups.");
}

fn print_managers_priority_list_help() {
    println!("USAGE:");
    println!("  helm managers priority list");
    println!();
    println!("DESCRIPTION:");
    println!("  Show effective manager ordering (detected managers first, by authority and rank).");
}

fn print_managers_priority_set_help() {
    println!("USAGE:");
    println!("  helm managers priority set <manager-id> --rank <n>");
    println!();
    println!("DESCRIPTION:");
    println!("  Move manager to the provided 1-based rank within its authority group.");
}

fn print_managers_priority_reset_help() {
    println!("USAGE:");
    println!("  helm managers priority reset");
    println!();
    println!("DESCRIPTION:");
    println!("  Clear priority overrides and restore default ordering.");
}

fn print_settings_help() {
    println!("USAGE:");
    println!("  helm settings list");
    println!("  helm settings get <key>");
    println!("  helm settings set <key> <value>");
    println!("  helm settings reset <key>");
    println!();
    println!("DESCRIPTION:");
    println!("  Read and mutate CLI-visible settings.");
    println!(
        "  Implemented keys: safe_mode, homebrew_keg_auto_cleanup, auto_check_for_updates, auto_check_frequency_minutes, auto_check_last_checked_unix (read-only), cli_onboarding_completed (read-only), cli_accepted_license_terms_version (read-only)."
    );
}

fn print_settings_list_help() {
    println!("USAGE:");
    println!("  helm settings list");
    println!();
    println!("DESCRIPTION:");
    println!("  List supported settings and current values.");
}

fn print_settings_get_help() {
    println!("USAGE:");
    println!("  helm settings get <key>");
    println!();
    println!("DESCRIPTION:");
    println!("  Read one setting value.");
}

fn print_settings_set_help() {
    println!("USAGE:");
    println!("  helm settings set <key> <value>");
    println!();
    println!("DESCRIPTION:");
    println!("  Set one supported setting.");
}

fn print_settings_reset_help() {
    println!("USAGE:");
    println!("  helm settings reset <key>");
    println!();
    println!("DESCRIPTION:");
    println!("  Reset one supported setting to default.");
}

fn print_onboarding_help() {
    println!("USAGE:");
    println!("  helm onboarding status");
    println!("  helm onboarding run");
    println!("  helm onboarding reset");
    println!();
    println!("DESCRIPTION:");
    println!("  Inspect, run, or reset CLI first-run onboarding state.");
    println!("  Use --accept-license and --accept-defaults for script-friendly onboarding.");
}

fn print_onboarding_status_help() {
    println!("USAGE:");
    println!("  helm onboarding status");
    println!();
    println!("DESCRIPTION:");
    println!("  Show CLI onboarding completion and accepted license terms version.");
}

fn print_onboarding_run_help() {
    println!("USAGE:");
    println!("  helm onboarding run [--accept-license] [--accept-defaults]");
    println!();
    println!("DESCRIPTION:");
    println!("  Complete CLI onboarding if needed.");
    println!(
        "  In non-interactive/scripted environments, pass both --accept-license and --accept-defaults."
    );
}

fn print_onboarding_reset_help() {
    println!("USAGE:");
    println!("  helm onboarding reset");
    println!();
    println!("DESCRIPTION:");
    println!("  Clear CLI onboarding and accepted license state.");
}

fn print_diagnostics_help() {
    println!("USAGE:");
    println!("  helm diagnostics summary");
    println!("  helm diagnostics task <task-id>");
    println!("  helm diagnostics manager <manager-id>");
    println!("  helm diagnostics provenance");
    println!("  helm diagnostics export [--path <file>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Inspect diagnostics snapshots and export support payloads.");
}

fn print_doctor_help() {
    println!("USAGE:");
    println!("  helm doctor");
    println!("  helm doctor scan");
    println!("  helm doctor repair <plan|apply> ...");
    println!("  helm doctor <scan|repair> [args]");
    println!();
    println!("DESCRIPTION:");
    println!("  Local health scanning and targeted repair workflows.");
    println!("  Without subcommands, doctor defaults to scan.");
    println!("  Use 'helm diagnostics ...' for diagnostics namespaces.");
}

fn print_doctor_scan_help() {
    println!("USAGE:");
    println!("  helm doctor scan");
    println!();
    println!("DESCRIPTION:");
    println!("  Run local doctor detectors and print a health report.");
}

fn print_doctor_repair_help() {
    println!("USAGE:");
    println!(
        "  helm doctor repair plan <manager-id> <source-manager-id> <package-name> <issue-code>"
    );
    println!(
        "  helm doctor repair apply <manager-id> <source-manager-id> <package-name> <issue-code> <option-id>"
    );
    println!();
    println!("DESCRIPTION:");
    println!("  Plan or apply a known repair for a doctor finding fingerprint.");
}

fn print_doctor_repair_plan_help() {
    println!("USAGE:");
    println!(
        "  helm doctor repair plan <manager-id> <source-manager-id> <package-name> <issue-code>"
    );
    println!();
    println!("DESCRIPTION:");
    println!("  Show available repair options for a specific issue.");
}

fn print_doctor_repair_apply_help() {
    println!("USAGE:");
    println!(
        "  helm doctor repair apply <manager-id> <source-manager-id> <package-name> <issue-code> <option-id>"
    );
    println!();
    println!("DESCRIPTION:");
    println!("  Apply a specific repair option and queue the resulting task.");
}

fn print_diagnostics_summary_help() {
    println!("USAGE:");
    println!("  helm diagnostics summary");
    println!();
    println!("DESCRIPTION:");
    println!("  Show aggregate diagnostics counters for managers, tasks, and packages.");
}

fn print_diagnostics_task_help() {
    println!("USAGE:");
    println!("  helm diagnostics task <task-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Show task details with lifecycle logs and in-process output (when available).");
}

fn print_diagnostics_manager_help() {
    println!("USAGE:");
    println!("  helm diagnostics manager <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Show manager status with recent manager-scoped tasks and logs.");
}

fn print_diagnostics_provenance_help() {
    println!("USAGE:");
    println!("  helm diagnostics provenance");
    println!();
    println!("DESCRIPTION:");
    println!(
        "  Show CLI install channel provenance, update policy, and recommended update action."
    );
}

fn print_diagnostics_export_help() {
    println!("USAGE:");
    println!("  helm diagnostics export [--path <file>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Build structured diagnostics payload and print or write it to a file.");
}

fn print_self_help() {
    println!("USAGE:");
    println!("  helm self status");
    println!("  helm self check");
    println!("  helm self update [--check] [--force]");
    println!("  helm self uninstall");
    println!("  helm self auto-check status");
    println!("  helm self auto-check enable");
    println!("  helm self auto-check disable");
    println!("  helm self auto-check frequency <minutes>");
    println!();
    println!("DESCRIPTION:");
    println!("  Self-update namespace for Helm.");
    println!("  Self-update behavior is driven by install provenance and update policy.");
    println!("  Channel-managed installs print recommended channel upgrade commands.");
}

fn print_self_status_help() {
    println!("USAGE:");
    println!("  helm self status");
    println!();
    println!("DESCRIPTION:");
    println!("  Show Helm self-update status snapshot.");
}

fn print_self_check_help() {
    println!("USAGE:");
    println!("  helm self check");
    println!();
    println!("DESCRIPTION:");
    println!("  Refresh self-update status for supported install methods.");
}

fn print_self_update_help() {
    println!("USAGE:");
    println!("  helm self update [--check] [--force]");
    println!();
    println!("DESCRIPTION:");
    println!("  Apply direct CLI self-update when provenance policy allows it.");
    println!("  --check performs a non-mutating availability check.");
    println!("  --force is only honored for direct-script installs.");
}

fn print_self_uninstall_help() {
    println!("USAGE:");
    println!("  helm self uninstall");
    println!();
    println!("DESCRIPTION:");
    println!("  Remove Helm CLI when install provenance allows direct uninstall.");
    println!("  direct-script installs remove the active executable and matching marker.");
    println!("  app-bundle-shim installs remove the managed ~/.local/bin/helm shim and marker.");
    println!("  channel-managed installs print the required channel uninstall action.");
}

fn print_self_auto_check_help() {
    println!("USAGE:");
    println!("  helm self auto-check status");
    println!("  helm self auto-check enable");
    println!("  helm self auto-check disable");
    println!("  helm self auto-check frequency <minutes>");
    println!();
    println!("DESCRIPTION:");
    println!("  Inspect or toggle automatic update checking.");
}

fn print_self_auto_check_status_help() {
    println!("USAGE:");
    println!("  helm self auto-check status");
    println!("  helm self auto-check");
    println!();
    println!("DESCRIPTION:");
    println!("  Show whether automatic update checking is enabled.");
}

fn print_self_auto_check_enable_help() {
    println!("USAGE:");
    println!("  helm self auto-check enable");
    println!();
    println!("DESCRIPTION:");
    println!("  Enable automatic update checking.");
}

fn print_self_auto_check_disable_help() {
    println!("USAGE:");
    println!("  helm self auto-check disable");
    println!();
    println!("DESCRIPTION:");
    println!("  Disable automatic update checking.");
}

fn print_self_auto_check_frequency_help() {
    println!("USAGE:");
    println!("  helm self auto-check frequency <minutes>");
    println!();
    println!("DESCRIPTION:");
    println!("  Set auto-check cadence in minutes.");
}

fn print_completion_help() {
    println!("USAGE:");
    println!("  helm completion <bash|zsh|fish>");
    println!();
    println!("DESCRIPTION:");
    println!("  Print shell completion script to stdout for the selected shell.");
}

#[cfg(test)]
mod tests {
    use super::{
        CLI_LICENSE_TERMS_VERSION, Command, CoordinatorClientTransport, ExecutionMode,
        GlobalOptions, HomebrewKegPolicy, InstallChannel, ManagerId, RustupPackagesCommand,
        SelfUpdateErrorKind, UpdatePolicy, UpgradeExecutionStep,
        acquire_coordinator_bootstrap_lock, apply_manager_enablement_self_heal,
        build_json_payload_lines, classify_failure_class, cmd_updates_run,
        command_help_topic_exists, coordinator_transport_for_cancel,
        coordinator_transport_for_submit, coordinator_transport_for_workflow,
        count_upgrade_step_failures, ensure_cli_onboarding_completed, exit_code_for_error,
        failure_class_hint, list_managers, manager_operation_failure_error, mark_exit_code,
        parse_args, parse_args_with_tty, parse_homebrew_keg_policy_arg, parse_manager_id,
        parse_manager_mutation_args, parse_package_mutation_args, parse_package_selector,
        parse_package_show_args, parse_packages_rustup_args, parse_search_args,
        parse_structured_terminal_error_message, parse_updates_run_preview_args,
        provenance_can_self_update, raw_args_request_json, raw_args_request_ndjson,
        read_update_bytes_with_limit, remove_install_marker_if_channel, resolve_redirect_url,
        resolve_update_redirect_target, selected_executable_differs_from_default,
        self_uninstall_recommended_action, should_launch_coordinator_on_demand,
        strip_exit_code_marker, upgrade_request_name,
    };
    use helm_core::execution::TaskOutputRecord;
    use helm_core::models::{
        AutomationLevel, DetectionInfo, InstallInstanceIdentityKind, InstallProvenance,
        ManagerInstallInstance, StrategyKind,
    };
    use helm_core::persistence::DetectionStore;
    use helm_core::sqlite::SqliteStore;
    use serde_json::json;
    use std::fs;
    use std::io::Cursor;
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    const COORDINATOR_TRANSPORT_INVARIANTS_DOC: &str =
        "../../../../docs/architecture/CLI_COORDINATOR_TRANSPORT_INVARIANTS.md";

    fn temp_file_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("helm-cli-self-uninstall-{name}-{nanos}.json"))
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("helm-cli-self-heal-{name}-{nanos}.sqlite3"))
    }

    fn seed_homebrew_detected(store: &SqliteStore) {
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
    }

    #[cfg(unix)]
    fn unix_mode(path: &Path) -> u32 {
        fs::metadata(path)
            .expect("expected path metadata")
            .permissions()
            .mode()
            & 0o777
    }

    #[cfg(unix)]
    fn unix_uid(path: &Path) -> u32 {
        fs::metadata(path).expect("expected path metadata").uid()
    }

    #[test]
    fn exit_code_mapping_defaults_to_runtime_error_without_marker() {
        assert_eq!(exit_code_for_error("refresh failed"), 1);
        assert_eq!(exit_code_for_error("task 42 was cancelled"), 1);
    }

    #[test]
    fn exit_code_mapping_uses_explicit_marker() {
        assert_eq!(
            exit_code_for_error(mark_exit_code("task failed", 2).as_str()),
            2
        );
        assert_eq!(
            exit_code_for_error(mark_exit_code("partial failure", 3).as_str()),
            3
        );
        assert_eq!(
            exit_code_for_error(mark_exit_code("cancelled", 4).as_str()),
            4
        );
    }

    #[test]
    fn exit_code_marker_round_trip() {
        let marked = mark_exit_code("refresh failed", 3);
        let (code, message) = strip_exit_code_marker(marked.as_str());
        assert_eq!(code, Some(3));
        assert_eq!(message, "refresh failed");
    }

    #[test]
    fn invalid_exit_code_marker_falls_back_to_unmarked_error() {
        let (code, message) = strip_exit_code_marker("__HELM_EXIT_CODE__:abc:bad");
        assert_eq!(code, None);
        assert_eq!(message, "__HELM_EXIT_CODE__:abc:bad");
    }

    #[test]
    fn manager_operation_failure_error_returns_none_when_no_failures() {
        assert_eq!(manager_operation_failure_error("upgrade", 0), None);
    }

    #[test]
    fn manager_operation_failure_error_marks_single_and_multiple_failures() {
        let single = manager_operation_failure_error("upgrade", 1).expect("single failure marker");
        let (single_code, single_message) = strip_exit_code_marker(single.as_str());
        assert_eq!(single_code, Some(2));
        assert_eq!(single_message, "1 manager upgrade operations failed");

        let multiple =
            manager_operation_failure_error("upgrade", 2).expect("multiple failure marker");
        let (multiple_code, multiple_message) = strip_exit_code_marker(multiple.as_str());
        assert_eq!(multiple_code, Some(3));
        assert_eq!(multiple_message, "2 manager upgrade operations failed");
    }

    #[test]
    fn selected_executable_differs_from_default_reports_alignment_and_divergence() {
        assert!(!selected_executable_differs_from_default(
            Some("/opt/homebrew/bin/npm"),
            Some("/opt/homebrew/bin/npm")
        ));
        assert!(selected_executable_differs_from_default(
            Some("/opt/homebrew/bin/npm"),
            Some("/Users/test/.local/bin/npm")
        ));
        assert!(!selected_executable_differs_from_default(
            None,
            Some("/tmp/npm")
        ));
        assert!(!selected_executable_differs_from_default(
            Some("/tmp/npm"),
            None
        ));
    }

    #[test]
    fn manager_executable_path_diagnostic_reports_expected_states() {
        assert_eq!(
            super::manager_executable_path_diagnostic(
                Some("/opt/homebrew/bin/npm"),
                Some("/opt/homebrew/bin/npm")
            ),
            "aligned"
        );
        assert_eq!(
            super::manager_executable_path_diagnostic(
                Some("/opt/homebrew/bin/npm"),
                Some("/Users/test/.local/bin/npm")
            ),
            "diverged"
        );
        assert_eq!(
            super::manager_executable_path_diagnostic(None, Some("/tmp/npm")),
            "selected_only"
        );
        assert_eq!(
            super::manager_executable_path_diagnostic(Some("/tmp/npm"), None),
            "default_only"
        );
    }

    #[test]
    fn count_upgrade_step_failures_counts_errors_without_short_circuiting() {
        let steps = vec![
            UpgradeExecutionStep {
                manager: ManagerId::Npm,
                package_name: "first".to_string(),
                cleanup_old_kegs: false,
                pinned: false,
                restart_required: false,
            },
            UpgradeExecutionStep {
                manager: ManagerId::Pnpm,
                package_name: "second".to_string(),
                cleanup_old_kegs: false,
                pinned: false,
                restart_required: false,
            },
            UpgradeExecutionStep {
                manager: ManagerId::Yarn,
                package_name: "third".to_string(),
                cleanup_old_kegs: false,
                pinned: false,
                restart_required: false,
            },
        ];

        let mut seen = Vec::new();
        let failures = count_upgrade_step_failures(&steps, |step| {
            seen.push(step.package_name.clone());
            if step.package_name == "second" || step.package_name == "third" {
                Err("simulated failure".to_string())
            } else {
                Ok(())
            }
        });

        assert_eq!(
            seen,
            vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string()
            ]
        );
        assert_eq!(failures, 2);
    }

    #[test]
    fn upgrade_request_name_encodes_homebrew_cleanup_targets() {
        let homebrew_step = UpgradeExecutionStep {
            manager: ManagerId::HomebrewFormula,
            package_name: "wget".to_string(),
            cleanup_old_kegs: true,
            pinned: false,
            restart_required: false,
        };
        let npm_step = UpgradeExecutionStep {
            manager: ManagerId::Npm,
            package_name: "eslint".to_string(),
            cleanup_old_kegs: true,
            pinned: false,
            restart_required: false,
        };

        assert_eq!(
            upgrade_request_name(&homebrew_step),
            "wget@@helm.cleanup".to_string()
        );
        assert_eq!(upgrade_request_name(&npm_step), "eslint".to_string());
    }

    #[test]
    fn coordinator_launch_on_demand_is_disabled_for_timeout_errors() {
        assert!(!should_launch_coordinator_on_demand(
            true,
            false,
            "timed out waiting for coordinator response in '/tmp/helm'"
        ));
        assert!(!should_launch_coordinator_on_demand(
            false,
            false,
            "failed to connect to coordinator"
        ));
        assert!(!should_launch_coordinator_on_demand(
            true,
            true,
            "failed to connect to coordinator"
        ));
        assert!(should_launch_coordinator_on_demand(
            true,
            false,
            "failed to connect to coordinator at '/tmp/helm': coordinator not ready"
        ));
    }

    #[test]
    fn coordinator_transport_mode_selection_matches_execution_contract() {
        assert_eq!(
            coordinator_transport_for_submit(ExecutionMode::Wait),
            CoordinatorClientTransport::LocalInProcess
        );
        assert_eq!(
            coordinator_transport_for_submit(ExecutionMode::Detach),
            CoordinatorClientTransport::ExternalFileIpc
        );
        assert_eq!(
            coordinator_transport_for_workflow(ExecutionMode::Wait),
            CoordinatorClientTransport::LocalInProcess
        );
        assert_eq!(
            coordinator_transport_for_workflow(ExecutionMode::Detach),
            CoordinatorClientTransport::ExternalFileIpc
        );
        assert_eq!(
            coordinator_transport_for_cancel(),
            CoordinatorClientTransport::ExternalFileIpc
        );
    }

    #[test]
    fn coordinator_bootstrap_lock_serializes_parallel_acquisition() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let state_dir =
            std::env::temp_dir().join(format!("helm-cli-coordinator-bootstrap-lock-{nanos}"));
        std::fs::create_dir_all(state_dir.as_path())
            .expect("failed to create coordinator bootstrap test state dir");

        let first = acquire_coordinator_bootstrap_lock(state_dir.as_path())
            .expect("first bootstrap lock acquisition should succeed");
        let (lock_tx, lock_rx) = std::sync::mpsc::channel();
        let state_dir_for_thread = state_dir.clone();

        let handle = std::thread::spawn(move || {
            let second = acquire_coordinator_bootstrap_lock(state_dir_for_thread.as_path())
                .expect("second bootstrap lock acquisition should succeed");
            lock_tx
                .send(())
                .expect("bootstrap-lock notification send should succeed");
            drop(second);
        });

        std::thread::sleep(Duration::from_millis(150));
        assert!(
            matches!(
                lock_rx.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ),
            "second lock should still be blocked while first is held"
        );

        drop(first);
        lock_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second lock should acquire after first is released");
        handle
            .join()
            .expect("bootstrap lock worker should complete");

        let _ = std::fs::remove_dir_all(state_dir);
    }

    #[test]
    fn coordinator_transport_invariants_doc_is_present() {
        let doc_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(COORDINATOR_TRANSPORT_INVARIANTS_DOC);
        assert!(
            doc_path.exists(),
            "coordinator transport invariants doc missing at {}",
            doc_path.display()
        );
    }

    #[test]
    fn parse_args_supports_new_global_flag_surface() {
        let (options, command, args) = parse_args(vec![
            "--ndjson".to_string(),
            "-q".to_string(),
            "--locale".to_string(),
            "en-US".to_string(),
            "--timeout=45".to_string(),
            "--detach".to_string(),
            "status".to_string(),
        ])
        .expect("args should parse");
        assert!(options.json);
        assert!(options.ndjson);
        assert!(options.quiet);
        assert_eq!(options.locale.as_deref(), Some("en-US"));
        assert_eq!(options.timeout_seconds, Some(45));
        assert_eq!(options.execution_mode, ExecutionMode::Detach);
        assert_eq!(command, Command::Status);
        assert!(args.is_empty());
    }

    #[test]
    fn parse_args_supports_combined_short_flags_for_verbose_version() {
        let (options, command, args) =
            parse_args(vec!["-vV".to_string()]).expect("combined short flags should parse");
        assert!(options.verbose);
        assert!(!options.quiet);
        assert_eq!(command, Command::Version);
        assert!(args.is_empty());
    }

    #[test]
    fn parse_args_rejects_unknown_combined_short_flag() {
        let error = parse_args(vec!["-vZ".to_string(), "status".to_string()])
            .expect_err("unknown combined short flag should fail");
        assert!(error.contains("unknown short flag"));
        assert!(error.contains("-Z"));
    }

    #[test]
    fn parse_args_unknown_command_includes_help_hint() {
        let error = parse_args(vec!["not-a-command".to_string()])
            .expect_err("unknown command should fail with hint");
        assert!(error.contains("unknown command"));
        assert!(error.contains("helm help"));
    }

    #[test]
    fn parse_args_keeps_non_flag_hyphenated_command_args() {
        let (_, command, args) = parse_args(vec!["search".to_string(), "-foo".to_string()])
            .expect("search query args should parse");
        assert_eq!(command, Command::Search);
        assert_eq!(args, vec!["-foo".to_string()]);
    }

    #[test]
    fn package_search_includes_rustup_manager() {
        assert!(super::manager_participates_in_package_search(
            ManagerId::Rustup
        ));
        assert!(super::manager_participates_in_package_search(
            ManagerId::HomebrewFormula
        ));
    }

    #[test]
    fn parse_args_rejects_verbose_and_quiet_together() {
        let error = parse_args(vec![
            "--verbose".to_string(),
            "--quiet".to_string(),
            "status".to_string(),
        ])
        .expect_err("conflicting verbosity flags must fail");
        assert!(error.contains("--verbose"));
        assert!(error.contains("--quiet"));
    }

    #[test]
    fn classify_failure_class_detects_cwd_missing_pattern() {
        let class = classify_failure_class(
            None,
            Some("Error: The current working directory must exist to run brew."),
        );
        assert_eq!(class, "cwd_missing");
    }

    #[test]
    fn coordinator_response_poll_interval_backoff_is_bounded() {
        assert_eq!(
            super::coordinator_response_poll_interval(Duration::from_millis(50)),
            Duration::from_millis(10)
        );
        assert_eq!(
            super::coordinator_response_poll_interval(Duration::from_millis(500)),
            Duration::from_millis(25)
        );
        assert_eq!(
            super::coordinator_response_poll_interval(Duration::from_millis(2_000)),
            Duration::from_millis(100)
        );
        assert_eq!(
            super::coordinator_response_poll_interval(Duration::from_millis(8_000)),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn coordinator_server_idle_poll_interval_backoff_is_bounded() {
        assert_eq!(
            super::coordinator_server_idle_poll_interval(1),
            Duration::from_millis(25)
        );
        assert_eq!(
            super::coordinator_server_idle_poll_interval(20),
            Duration::from_millis(100)
        );
        assert_eq!(
            super::coordinator_server_idle_poll_interval(50),
            Duration::from_millis(250)
        );
    }

    #[test]
    fn coordinator_bootstrap_lock_poll_interval_backoff_is_bounded() {
        assert_eq!(
            super::coordinator_bootstrap_lock_poll_interval(Duration::from_millis(100)),
            Duration::from_millis(25)
        );
        assert_eq!(
            super::coordinator_bootstrap_lock_poll_interval(Duration::from_millis(1_000)),
            Duration::from_millis(50)
        );
        assert_eq!(
            super::coordinator_bootstrap_lock_poll_interval(Duration::from_millis(4_000)),
            Duration::from_millis(100)
        );
    }

    #[test]
    fn coordinator_startup_poll_interval_backoff_is_bounded() {
        assert_eq!(
            super::coordinator_startup_poll_interval(Duration::from_millis(100)),
            Duration::from_millis(10)
        );
        assert_eq!(
            super::coordinator_startup_poll_interval(Duration::from_millis(1_000)),
            Duration::from_millis(25)
        );
        assert_eq!(
            super::coordinator_startup_poll_interval(Duration::from_millis(4_000)),
            Duration::from_millis(50)
        );
    }

    #[test]
    fn classify_failure_class_detects_network_offline_pattern() {
        let class = classify_failure_class(
            None,
            Some("failed to connect: network is unreachable for host registry.npmjs.org"),
        );
        assert_eq!(class, "network_offline");
    }

    #[test]
    fn classify_failure_class_detects_check_internet_connection_pattern() {
        let class = classify_failure_class(
            None,
            Some("request failed: check your internet connection and try again"),
        );
        assert_eq!(class, "network_offline");
    }

    #[test]
    fn classify_failure_class_detects_network_proxy_pattern() {
        let class = classify_failure_class(
            None,
            Some("proxy authentication required (HTTP 407) while reaching mirror"),
        );
        assert_eq!(class, "network_proxy");
    }

    #[test]
    fn classify_failure_class_detects_network_captive_portal_pattern() {
        let class = classify_failure_class(
            None,
            Some("network authentication required (HTTP 511): captive portal login"),
        );
        assert_eq!(class, "network_captive_portal");
    }

    #[test]
    fn failure_class_hint_provides_actionable_network_guidance() {
        assert_eq!(
            failure_class_hint("network_dns"),
            Some("Check DNS resolution and retry the operation.")
        );
        assert_eq!(
            failure_class_hint("network_proxy"),
            Some("Check proxy configuration and credentials, then retry.")
        );
        assert_eq!(
            failure_class_hint("network_captive_portal"),
            Some("Complete captive-portal sign-in in a browser, then retry.")
        );
    }

    #[test]
    fn classify_failure_class_prefers_timeout_reason_from_output() {
        let output = TaskOutputRecord {
            termination_reason: Some("timeout".to_string()),
            ..TaskOutputRecord::default()
        };
        let class = classify_failure_class(Some(&output), Some("some unrelated failure"));
        assert_eq!(class, "timeout");
    }

    #[test]
    fn classify_failure_class_detects_hard_timeout_error_code() {
        let output = TaskOutputRecord {
            error_code: Some("hard_timeout".to_string()),
            ..TaskOutputRecord::default()
        };
        let class = classify_failure_class(Some(&output), Some("process reached hard timeout"));
        assert_eq!(class, "hard_timeout");
    }

    #[test]
    fn classify_failure_class_detects_idle_timeout_error_code() {
        let output = TaskOutputRecord {
            error_code: Some("idle_timeout".to_string()),
            ..TaskOutputRecord::default()
        };
        let class = classify_failure_class(Some(&output), Some("process produced no output"));
        assert_eq!(class, "idle_timeout");
    }

    #[test]
    fn parse_structured_terminal_error_message_extracts_error_details() {
        let parsed = parse_structured_terminal_error_message(
            "task failed [timeout]: process timed out after 60000ms",
            "task failed",
        )
        .expect("structured failed-task log should parse");
        assert_eq!(parsed.code, "timeout");
        assert_eq!(parsed.message, "process timed out after 60000ms");
    }

    #[test]
    fn inspect_coordinator_state_health_marks_dead_pid_as_stale() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let state_dir = std::env::temp_dir().join(format!("helm-cli-coordinator-health-{nanos}"));
        std::fs::create_dir_all(&state_dir).expect("failed to create coordinator state dir");
        let ready_path = super::coordinator_ready_file(state_dir.as_path());
        let ready = super::CoordinatorReadyState {
            pid: 999_999,
            started_at: 1,
            heartbeat_unix: 2,
            executable_path: Some("/tmp/nonexistent-helm".to_string()),
        };
        super::write_json_file(ready_path.as_path(), &ready).expect("failed to write ready file");

        let health = super::inspect_coordinator_state_health(state_dir.as_path());
        assert!(health.ready_file_present);
        assert_eq!(health.pid, Some(999_999));
        assert_eq!(health.pid_alive, Some(false));
        assert_eq!(health.executable_exists, Some(false));
        assert!(
            health.stale_reasons.contains(&"pid_not_alive".to_string()),
            "expected pid_not_alive stale reason"
        );
        assert!(
            health
                .stale_reasons
                .contains(&"executable_missing".to_string()),
            "expected executable_missing stale reason"
        );

        let _ = std::fs::remove_dir_all(state_dir);
    }

    #[cfg(unix)]
    #[test]
    fn coordinator_ipc_paths_use_private_modes_and_consistent_ownership() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let state_dir = std::env::temp_dir().join(format!("helm-cli-coordinator-perms-{nanos}"));
        super::reset_coordinator_state_dir(state_dir.as_path())
            .expect("reset coordinator state dir should succeed");

        let requests_dir = super::coordinator_requests_dir(state_dir.as_path());
        let responses_dir = super::coordinator_responses_dir(state_dir.as_path());
        assert_eq!(unix_mode(state_dir.as_path()), 0o700);
        assert_eq!(unix_mode(requests_dir.as_path()), 0o700);
        assert_eq!(unix_mode(responses_dir.as_path()), 0o700);

        let owner_uid = unix_uid(state_dir.as_path());
        assert_eq!(unix_uid(requests_dir.as_path()), owner_uid);
        assert_eq!(unix_uid(responses_dir.as_path()), owner_uid);

        let request_file = super::coordinator_request_file(state_dir.as_path(), "perm-check");
        let response_file = super::coordinator_response_file(state_dir.as_path(), "perm-check");
        super::write_json_file(request_file.as_path(), &json!({ "kind": "ping" }))
            .expect("request file write should succeed");
        super::write_json_file(response_file.as_path(), &json!({ "ok": true }))
            .expect("response file write should succeed");
        super::write_coordinator_ready_state(state_dir.as_path(), 123)
            .expect("ready file write should succeed");
        let ready_file = super::coordinator_ready_file(state_dir.as_path());

        assert_eq!(unix_mode(request_file.as_path()), 0o600);
        assert_eq!(unix_mode(response_file.as_path()), 0o600);
        assert_eq!(unix_mode(ready_file.as_path()), 0o600);
        assert_eq!(unix_uid(request_file.as_path()), owner_uid);
        assert_eq!(unix_uid(response_file.as_path()), owner_uid);
        assert_eq!(unix_uid(ready_file.as_path()), owner_uid);

        let _ = fs::remove_dir_all(state_dir);
    }

    #[test]
    fn parse_args_rejects_wait_and_detach_together() {
        let error = parse_args(vec![
            "--wait".to_string(),
            "--detach".to_string(),
            "status".to_string(),
        ])
        .expect_err("conflicting execution mode flags must fail");
        assert!(error.contains("--wait"));
        assert!(error.contains("--detach"));
    }

    #[test]
    fn parse_args_maps_doctor_alias() {
        let (_, command, args) = parse_args(vec!["doctor".to_string(), "provenance".to_string()])
            .expect("doctor alias should parse");
        assert_eq!(command, Command::Doctor);
        assert_eq!(args, vec!["provenance".to_string()]);
    }

    #[test]
    fn parse_args_supports_onboarding_command_with_accept_flags() {
        let (options, command, args) = parse_args(vec![
            "--accept-license".to_string(),
            "--accept-defaults".to_string(),
            "onboarding".to_string(),
            "status".to_string(),
        ])
        .expect("onboarding args should parse");
        assert!(options.accept_license);
        assert!(options.accept_defaults);
        assert_eq!(command, Command::Onboarding);
        assert_eq!(args, vec!["status".to_string()]);
    }

    #[test]
    fn parse_args_no_args_launches_tui_when_stdout_is_tty() {
        let (_, command, args) =
            parse_args_with_tty(Vec::new(), true).expect("empty args should parse");
        assert_eq!(command, Command::Tui);
        assert!(args.is_empty());
    }

    #[test]
    fn parse_args_no_args_falls_back_to_help_when_stdout_is_not_tty() {
        let (_, command, args) =
            parse_args_with_tty(Vec::new(), false).expect("empty args should parse");
        assert_eq!(command, Command::Help);
        assert!(args.is_empty());
    }

    #[test]
    fn raw_args_json_detection_supports_json_and_ndjson_flags() {
        assert!(raw_args_request_json(&[
            "--json".to_string(),
            "status".to_string()
        ]));
        assert!(raw_args_request_json(&[
            "--ndjson".to_string(),
            "status".to_string()
        ]));
        assert!(!raw_args_request_json(&["status".to_string()]));
    }

    #[test]
    fn raw_args_ndjson_detection_only_matches_ndjson_flag() {
        assert!(raw_args_request_ndjson(&[
            "--ndjson".to_string(),
            "status".to_string()
        ]));
        assert!(!raw_args_request_ndjson(&[
            "--json".to_string(),
            "status".to_string()
        ]));
        assert!(!raw_args_request_ndjson(&["status".to_string()]));
    }

    #[test]
    fn build_json_payload_lines_splits_array_data_in_ndjson_mode() {
        let payloads =
            build_json_payload_lines("helm.cli.v1.test", json!([{"id": 1}, {"id": 2}]), true, 123);
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0]["schema"], "helm.cli.v1.test");
        assert_eq!(payloads[0]["generated_at"], 123);
        assert_eq!(payloads[0]["data"]["id"], 1);
        assert_eq!(payloads[1]["data"]["id"], 2);
    }

    #[test]
    fn build_json_payload_lines_keeps_single_record_for_empty_array_ndjson() {
        let payloads = build_json_payload_lines("helm.cli.v1.test", json!([]), true, 123);
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["data"], json!([]));
    }

    #[test]
    fn build_json_payload_lines_keeps_array_data_in_json_mode() {
        let payloads = build_json_payload_lines(
            "helm.cli.v1.test",
            json!([{"id": 1}, {"id": 2}]),
            false,
            123,
        );
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["data"], json!([{"id": 1}, {"id": 2}]));
    }

    #[test]
    fn build_json_payload_lines_preserves_nested_array_items_in_ndjson_mode() {
        let payloads = build_json_payload_lines(
            "helm.cli.v1.test",
            json!([[{"id": 1}], [{"id": 2}]]),
            true,
            123,
        );
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0]["data"], json!([{"id": 1}]));
        assert_eq!(payloads[1]["data"], json!([{"id": 2}]));
    }

    #[test]
    fn build_json_payload_lines_does_not_split_nested_arrays_inside_objects() {
        let payloads = build_json_payload_lines(
            "helm.cli.v1.test",
            json!({"items": [{"id": 1}, {"id": 2}]}),
            true,
            123,
        );
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["data"]["items"], json!([{"id": 1}, {"id": 2}]));
    }

    #[test]
    fn command_help_topic_validation_covers_nested_paths() {
        assert!(command_help_topic_exists(
            Command::Packages,
            &["list".to_string()]
        ));
        assert!(command_help_topic_exists(
            Command::Packages,
            &["keg-policy".to_string(), "set".to_string()]
        ));
        assert!(command_help_topic_exists(
            Command::Managers,
            &["executables".to_string(), "set".to_string()]
        ));
        assert!(command_help_topic_exists(
            Command::Managers,
            &["instances".to_string()]
        ));
        assert!(command_help_topic_exists(
            Command::Managers,
            &["instances".to_string(), "ack".to_string()]
        ));
        assert!(command_help_topic_exists(
            Command::Managers,
            &["instances".to_string(), "clear-ack".to_string()]
        ));
        assert!(command_help_topic_exists(
            Command::Managers,
            &[
                "instances".to_string(),
                "set-active".to_string(),
                "<instance-id>".to_string()
            ]
        ));
        assert!(command_help_topic_exists(
            Command::SelfCmd,
            &["auto-check".to_string(), "frequency".to_string()]
        ));
        assert!(command_help_topic_exists(
            Command::SelfCmd,
            &["uninstall".to_string()]
        ));
        assert!(command_help_topic_exists(
            Command::Onboarding,
            &["status".to_string()]
        ));
        assert!(!command_help_topic_exists(
            Command::Updates,
            &["unknown".to_string()]
        ));
    }

    #[test]
    fn list_managers_includes_active_install_instance_metadata() {
        let db_path = temp_db_path("manager-instance-metadata");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::Rustup,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/rustup")),
                    version: Some("1.28.2".to_string()),
                },
            )
            .expect("rustup detection should persist");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-homebrew".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.94,
                    decision_margin: Some(0.53),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some("path is in Homebrew Cellar".to_string()),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::RustupInit),
                    competing_confidence: Some(0.41),
                }],
            )
            .expect("rustup instance should persist");

        let managers = list_managers(&store).expect("manager list should load");
        let rustup = managers
            .into_iter()
            .find(|manager| manager.manager_id == "rustup")
            .expect("rustup manager row should exist");

        assert_eq!(rustup.install_instance_count, 1);
        assert_eq!(rustup.active_provenance.as_deref(), Some("homebrew"));
        assert_eq!(rustup.active_automation_level.as_deref(), Some("automatic"));
        assert_eq!(
            rustup.active_uninstall_strategy.as_deref(),
            Some("homebrew_formula")
        );
        assert_eq!(
            rustup.active_update_strategy.as_deref(),
            Some("homebrew_formula")
        );
        assert_eq!(rustup.competing_provenance.as_deref(), Some("rustup_init"));
        assert_eq!(rustup.active_decision_margin, Some(0.53));
        assert!(
            rustup.active_confidence.unwrap_or_default() >= 0.90,
            "expected confidence >= 0.90"
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn list_managers_reports_multi_instance_state_transitions() {
        let db_path = temp_db_path("manager-multi-instance-state");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let homebrew = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "rustup-homebrew".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup".to_string(),
            display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
            canonical_path: Some(PathBuf::from(
                "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
            )),
            alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
            is_active: false,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::Homebrew,
            confidence: 0.94,
            decision_margin: Some(0.53),
            automation_level: AutomationLevel::Automatic,
            uninstall_strategy: StrategyKind::HomebrewFormula,
            update_strategy: StrategyKind::HomebrewFormula,
            remediation_strategy: StrategyKind::ManualRemediation,
            explanation_primary: Some("path is in Homebrew Cellar".to_string()),
            explanation_secondary: None,
            competing_provenance: Some(InstallProvenance::RustupInit),
            competing_confidence: Some(0.41),
        };
        let user = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "rustup-user".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
            display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
            canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
            alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::RustupInit,
            confidence: 0.92,
            decision_margin: Some(0.30),
            automation_level: AutomationLevel::Automatic,
            uninstall_strategy: StrategyKind::RustupSelf,
            update_strategy: StrategyKind::RustupSelf,
            remediation_strategy: StrategyKind::RustupSelf,
            explanation_primary: Some("path is under CARGO_HOME/bin".to_string()),
            explanation_secondary: None,
            competing_provenance: Some(InstallProvenance::Homebrew),
            competing_confidence: Some(0.45),
        };

        store
            .replace_install_instances(ManagerId::Rustup, &[homebrew.clone(), user.clone()])
            .expect("rustup instances should persist");

        let managers = list_managers(&store).expect("manager list should load");
        let rustup = managers
            .into_iter()
            .find(|manager| manager.manager_id == "rustup")
            .expect("rustup manager row should exist");
        assert_eq!(rustup.multi_instance_state, "attention_needed");
        assert!(!rustup.multi_instance_acknowledged);
        let acknowledged_fingerprint = rustup
            .multi_instance_fingerprint
            .as_deref()
            .expect("fingerprint should be present")
            .to_string();

        store
            .set_manager_multi_instance_ack_fingerprint(
                ManagerId::Rustup,
                Some(acknowledged_fingerprint.as_str()),
            )
            .expect("manager acknowledgement should persist");

        let managers = list_managers(&store).expect("manager list should load");
        let rustup = managers
            .into_iter()
            .find(|manager| manager.manager_id == "rustup")
            .expect("rustup manager row should exist");
        assert_eq!(rustup.multi_instance_state, "acknowledged");
        assert!(rustup.multi_instance_acknowledged);
        assert_eq!(
            rustup.multi_instance_fingerprint.as_deref(),
            Some(acknowledged_fingerprint.as_str())
        );

        let mut extra = user;
        extra.instance_id = "rustup-extra".to_string();
        extra.identity_value = "/Users/test/.local/bin/rustup".to_string();
        extra.display_path = PathBuf::from("/Users/test/.local/bin/rustup");
        extra.canonical_path = Some(PathBuf::from("/Users/test/.local/bin/rustup"));
        extra.is_active = false;

        store
            .replace_install_instances(ManagerId::Rustup, &[homebrew, extra])
            .expect("updated rustup instances should persist");

        let managers = list_managers(&store).expect("manager list should load");
        let rustup = managers
            .into_iter()
            .find(|manager| manager.manager_id == "rustup")
            .expect("rustup manager row should exist");
        assert_eq!(rustup.multi_instance_state, "attention_needed");
        assert!(!rustup.multi_instance_acknowledged);
        assert_ne!(
            rustup.multi_instance_fingerprint.as_deref(),
            Some(acknowledged_fingerprint.as_str())
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn set_manager_active_install_instance_switches_active_and_clears_ack() {
        let db_path = temp_db_path("manager-set-active-install-instance");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let make_instance =
            |instance_id: &str, display_path: &str, active: bool| ManagerInstallInstance {
                manager: ManagerId::Rustup,
                instance_id: instance_id.to_string(),
                identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                identity_value: display_path.to_string(),
                display_path: PathBuf::from(display_path),
                canonical_path: Some(PathBuf::from(display_path)),
                alias_paths: vec![PathBuf::from(display_path)],
                is_active: active,
                version: Some("1.28.2".to_string()),
                provenance: InstallProvenance::RustupInit,
                confidence: 0.92,
                decision_margin: Some(0.30),
                automation_level: AutomationLevel::Automatic,
                uninstall_strategy: StrategyKind::RustupSelf,
                update_strategy: StrategyKind::RustupSelf,
                remediation_strategy: StrategyKind::RustupSelf,
                explanation_primary: Some("path is under CARGO_HOME/bin".to_string()),
                explanation_secondary: None,
                competing_provenance: Some(InstallProvenance::Homebrew),
                competing_confidence: Some(0.45),
            };

        let homebrew = make_instance("rustup-homebrew", "/opt/homebrew/bin/rustup", false);
        let user = make_instance("rustup-user", "/Users/test/.cargo/bin/rustup", true);
        store
            .replace_install_instances(ManagerId::Rustup, &[homebrew, user])
            .expect("rustup instances should persist");
        store
            .set_manager_multi_instance_ack_fingerprint(ManagerId::Rustup, Some("ack"))
            .expect("multi-instance ack should persist");

        let message = super::set_manager_active_install_instance(
            &store,
            ManagerId::Rustup,
            "rustup-homebrew",
        )
        .expect("active instance selection should succeed");
        assert!(message.contains("rustup-homebrew"));

        let rows = store
            .list_install_instances(Some(ManagerId::Rustup))
            .expect("manager instances should load");
        let active_ids = rows
            .iter()
            .filter(|instance| instance.is_active)
            .map(|instance| instance.instance_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(active_ids, vec!["rustup-homebrew"]);
        assert_eq!(
            store
                .manager_multi_instance_ack_fingerprint(ManagerId::Rustup)
                .expect("manager ack should load"),
            None
        );

        let selected_path = store
            .list_manager_preferences()
            .expect("manager preferences should load")
            .into_iter()
            .find(|preference| preference.manager == ManagerId::Rustup)
            .and_then(|preference| preference.selected_executable_path);
        assert_eq!(selected_path.as_deref(), Some("/opt/homebrew/bin/rustup"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn acknowledge_manager_multi_instance_state_requires_multiple_instances() {
        let db_path = temp_db_path("manager-multi-instance-ack");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let single = ManagerInstallInstance {
            manager: ManagerId::Rustup,
            instance_id: "rustup-user".to_string(),
            identity_kind: InstallInstanceIdentityKind::CanonicalPath,
            identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
            display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
            canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
            alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
            is_active: true,
            version: Some("1.28.2".to_string()),
            provenance: InstallProvenance::RustupInit,
            confidence: 0.92,
            decision_margin: Some(0.30),
            automation_level: AutomationLevel::Automatic,
            uninstall_strategy: StrategyKind::RustupSelf,
            update_strategy: StrategyKind::RustupSelf,
            remediation_strategy: StrategyKind::RustupSelf,
            explanation_primary: Some("path is under CARGO_HOME/bin".to_string()),
            explanation_secondary: None,
            competing_provenance: Some(InstallProvenance::Homebrew),
            competing_confidence: Some(0.45),
        };
        store
            .replace_install_instances(ManagerId::Rustup, std::slice::from_ref(&single))
            .expect("single rustup instance should persist");

        let error = super::acknowledge_manager_multi_instance_state(&store, ManagerId::Rustup)
            .expect_err("single-instance ack should fail");
        assert!(
            error.contains("does not currently have multiple install instances"),
            "unexpected error: {error}"
        );

        let mut first = single;
        first.is_active = true;
        let mut second = first.clone();
        second.instance_id = "rustup-homebrew".to_string();
        second.identity_value = "/opt/homebrew/bin/rustup".to_string();
        second.display_path = PathBuf::from("/opt/homebrew/bin/rustup");
        second.canonical_path = Some(PathBuf::from("/opt/homebrew/bin/rustup"));
        second.is_active = false;
        second.provenance = InstallProvenance::Homebrew;
        second.uninstall_strategy = StrategyKind::HomebrewFormula;
        second.update_strategy = StrategyKind::HomebrewFormula;
        store
            .replace_install_instances(ManagerId::Rustup, &[first, second])
            .expect("multi-instance rustup state should persist");

        super::acknowledge_manager_multi_instance_state(&store, ManagerId::Rustup)
            .expect("multi-instance ack should succeed");
        assert!(
            store
                .manager_multi_instance_ack_fingerprint(ManagerId::Rustup)
                .expect("manager ack should load")
                .is_some(),
            "expected persisted multi-instance acknowledgement fingerprint"
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn list_manager_install_instances_sorts_active_first_then_instance_id() {
        let db_path = temp_db_path("manager-instances-sort-active-first");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "z-inactive".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                        display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                        canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                        alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                        is_active: false,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::RustupInit,
                        confidence: 0.92,
                        decision_margin: Some(0.30),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::RustupSelf,
                        update_strategy: StrategyKind::RustupSelf,
                        remediation_strategy: StrategyKind::RustupSelf,
                        explanation_primary: Some(
                            "inactive path is under CARGO_HOME/bin".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.45),
                    },
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "a-active".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup".to_string(),
                        display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
                        canonical_path: Some(PathBuf::from(
                            "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
                        )),
                        alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
                        is_active: true,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.94,
                        decision_margin: Some(0.53),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some("path is in Homebrew Cellar".to_string()),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::RustupInit),
                        competing_confidence: Some(0.41),
                    },
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "b-inactive".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/Users/test/.local/bin/rustup".to_string(),
                        display_path: PathBuf::from("/Users/test/.local/bin/rustup"),
                        canonical_path: Some(PathBuf::from("/Users/test/.local/bin/rustup")),
                        alias_paths: vec![PathBuf::from("/Users/test/.local/bin/rustup")],
                        is_active: false,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::Unknown,
                        confidence: 0.44,
                        decision_margin: Some(0.05),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::InteractivePrompt,
                        update_strategy: StrategyKind::InteractivePrompt,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some(
                            "insufficient or conflicting rustup provenance evidence".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.40),
                    },
                ],
            )
            .expect("rustup instances should persist");

        let instances = super::list_manager_install_instances(&store, Some(ManagerId::Rustup))
            .expect("manager instances should load");
        assert_eq!(instances.len(), 3);
        assert_eq!(instances[0].instance_id, "a-active");
        assert!(instances[0].is_active);
        assert_eq!(instances[1].instance_id, "b-inactive");
        assert!(!instances[1].is_active);
        assert_eq!(instances[2].instance_id, "z-inactive");
        assert!(!instances[2].is_active);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn manager_list_summary_matches_instances_surface_when_no_active_instance_exists() {
        let db_path = temp_db_path("manager-list-summary-no-active-instance");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::Rustup,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/rustup")),
                    version: Some("1.28.2".to_string()),
                },
            )
            .expect("rustup detection should persist");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "z-inactive".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                        display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                        canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                        alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                        is_active: false,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::RustupInit,
                        confidence: 0.92,
                        decision_margin: Some(0.30),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::RustupSelf,
                        update_strategy: StrategyKind::RustupSelf,
                        remediation_strategy: StrategyKind::RustupSelf,
                        explanation_primary: Some(
                            "inactive path is under CARGO_HOME/bin".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.45),
                    },
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "a-inactive".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup".to_string(),
                        display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
                        canonical_path: Some(PathBuf::from(
                            "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
                        )),
                        alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
                        is_active: false,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.94,
                        decision_margin: Some(0.53),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some("path is in Homebrew Cellar".to_string()),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::RustupInit),
                        competing_confidence: Some(0.41),
                    },
                ],
            )
            .expect("rustup instances should persist");

        let managers = list_managers(&store).expect("manager list should load");
        let rustup = managers
            .into_iter()
            .find(|manager| manager.manager_id == "rustup")
            .expect("rustup manager row should exist");
        let instances = super::list_manager_install_instances(&store, Some(ManagerId::Rustup))
            .expect("manager instances should load");
        let selected = instances
            .iter()
            .find(|instance| instance.is_active)
            .or_else(|| instances.first())
            .expect("instance list should not be empty");

        assert_eq!(
            rustup.active_provenance.as_deref(),
            Some(selected.provenance.as_str())
        );
        assert_eq!(rustup.active_confidence, Some(selected.confidence));
        assert_eq!(rustup.active_decision_margin, selected.decision_margin);
        assert_eq!(
            rustup.active_automation_level.as_deref(),
            Some(selected.automation_level.as_str())
        );
        assert_eq!(
            rustup.active_uninstall_strategy.as_deref(),
            Some(selected.uninstall_strategy.as_str())
        );
        assert_eq!(
            rustup.active_update_strategy.as_deref(),
            Some(selected.update_strategy.as_str())
        );
        assert_eq!(
            rustup.active_remediation_strategy.as_deref(),
            Some(selected.remediation_strategy.as_str())
        );
        assert_eq!(
            rustup.active_explanation_primary.as_deref(),
            selected.explanation_primary.as_deref()
        );
        assert_eq!(
            rustup.active_explanation_secondary.as_deref(),
            selected.explanation_secondary.as_deref()
        );
        assert_eq!(
            rustup.competing_provenance.as_deref(),
            selected.competing_provenance.as_deref()
        );
        assert_eq!(rustup.competing_confidence, selected.competing_confidence);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn parse_search_args_supports_progressive_manager_and_limit() {
        let parsed = parse_search_args(&[
            "--manager".to_string(),
            "homebrew_formula".to_string(),
            "--limit".to_string(),
            "25".to_string(),
            "ripgrep".to_string(),
        ])
        .expect("search args should parse");
        assert_eq!(parsed.query, "ripgrep");
        assert_eq!(parsed.manager_filter, Some(ManagerId::HomebrewFormula));
        assert_eq!(parsed.limit, Some(25));
        assert!(!parsed.local_only);
        assert!(!parsed.remote_only);
    }

    #[test]
    fn parse_search_args_rejects_conflicting_modes() {
        let error = parse_search_args(&[
            "--local".to_string(),
            "--remote".to_string(),
            "ripgrep".to_string(),
        ])
        .expect_err("search mode conflict must fail");
        assert!(error.contains("--local"));
        assert!(error.contains("--remote"));
    }

    #[test]
    fn parse_updates_run_preview_args_supports_manager_filter() {
        let parsed = parse_updates_run_preview_args(
            &[
                "--include-pinned".to_string(),
                "--manager".to_string(),
                "homebrew_formula".to_string(),
            ],
            false,
        )
        .expect("updates preview args should parse");
        assert!(parsed.include_pinned);
        assert_eq!(parsed.manager_filter, Some(ManagerId::HomebrewFormula));
        assert!(!parsed.yes);
    }

    #[test]
    fn parse_updates_run_preview_args_rejects_duplicate_manager_filter() {
        let error = parse_updates_run_preview_args(
            &[
                "--manager".to_string(),
                "homebrew_formula".to_string(),
                "--manager".to_string(),
                "npm".to_string(),
            ],
            true,
        )
        .expect_err("duplicate --manager should fail");
        assert!(error.contains("specified multiple times"));
    }

    #[test]
    fn parse_manager_mutation_args_uninstall_accepts_preview_confirmation_flags() {
        let parsed = parse_manager_mutation_args(
            "uninstall",
            &[
                "--preview".to_string(),
                "rustup".to_string(),
                "--yes".to_string(),
                "--allow-unknown-provenance".to_string(),
            ],
        )
        .expect("uninstall args should parse");

        assert_eq!(parsed.manager, ManagerId::Rustup);
        assert!(parsed.preview);
        assert!(parsed.yes);
        assert!(parsed.allow_unknown_provenance);
        assert_eq!(parsed.install_method_override, None);
        assert_eq!(
            parsed.uninstall_options,
            helm_core::manager_lifecycle::ManagerUninstallOptions::default()
        );
    }

    #[test]
    fn parse_manager_mutation_args_uninstall_accepts_homebrew_cleanup_options() {
        let parsed = parse_manager_mutation_args(
            "uninstall",
            &[
                "rustup".to_string(),
                "--homebrew-cleanup-mode".to_string(),
                "fullCleanup".to_string(),
            ],
        )
        .expect("homebrew cleanup options should parse");

        assert_eq!(parsed.manager, ManagerId::Rustup);
        assert_eq!(
            parsed.uninstall_options.homebrew_cleanup_mode,
            Some(helm_core::manager_lifecycle::HomebrewUninstallCleanupMode::FullCleanup)
        );
    }

    #[test]
    fn parse_manager_mutation_args_uninstall_accepts_mise_cleanup_options() {
        let parsed = parse_manager_mutation_args(
            "uninstall",
            &[
                "mise".to_string(),
                "--preview".to_string(),
                "--mise-cleanup-mode".to_string(),
                "fullCleanup".to_string(),
                "--mise-config-removal".to_string(),
                "removeConfig".to_string(),
            ],
        )
        .expect("mise uninstall cleanup options should parse");

        assert_eq!(parsed.manager, ManagerId::Mise);
        assert_eq!(
            parsed.uninstall_options.mise_cleanup_mode,
            Some(helm_core::manager_lifecycle::MiseUninstallCleanupMode::FullCleanup)
        );
        assert_eq!(
            parsed.uninstall_options.mise_config_removal,
            Some(helm_core::manager_lifecycle::MiseUninstallConfigRemoval::RemoveConfig)
        );
    }

    #[test]
    fn parse_manager_mutation_args_uninstall_rejects_mise_cleanup_flags_for_non_mise_manager() {
        let error = parse_manager_mutation_args(
            "uninstall",
            &[
                "rustup".to_string(),
                "--mise-cleanup-mode".to_string(),
                "fullCleanup".to_string(),
            ],
        )
        .expect_err("non-mise managers should reject mise cleanup flags");
        assert!(error.contains("only supported for manager 'mise'"));
    }

    #[test]
    fn parse_manager_mutation_args_uninstall_rejects_invalid_mise_cleanup_mode() {
        let error = parse_manager_mutation_args(
            "uninstall",
            &[
                "mise".to_string(),
                "--mise-cleanup-mode".to_string(),
                "invalid".to_string(),
            ],
        )
        .expect_err("invalid mise cleanup mode should fail");
        assert!(error.contains("unsupported mise cleanup mode"));
    }

    #[test]
    fn parse_manager_mutation_args_uninstall_rejects_invalid_homebrew_cleanup_mode() {
        let error = parse_manager_mutation_args(
            "uninstall",
            &[
                "rustup".to_string(),
                "--homebrew-cleanup-mode".to_string(),
                "invalid".to_string(),
            ],
        )
        .expect_err("invalid homebrew cleanup mode should fail");
        assert!(error.contains("unsupported homebrew cleanup mode"));
    }

    #[test]
    fn parse_manager_mutation_args_uninstall_rejects_invalid_mise_config_removal_mode() {
        let error = parse_manager_mutation_args(
            "uninstall",
            &[
                "mise".to_string(),
                "--mise-config-removal".to_string(),
                "invalid".to_string(),
            ],
        )
        .expect_err("invalid mise config-removal mode should fail");
        assert!(error.contains("unsupported mise config removal mode"));
    }

    #[test]
    fn parse_package_mutation_args_uninstall_accepts_preview_confirmation_flags() {
        let parsed = parse_package_mutation_args(
            "uninstall",
            &[
                "git".to_string(),
                "--manager".to_string(),
                "homebrew_formula".to_string(),
                "--preview".to_string(),
                "--yes".to_string(),
            ],
            false,
        )
        .expect("package uninstall args should parse");

        assert_eq!(parsed.package_name, "git");
        assert_eq!(parsed.manager, ManagerId::HomebrewFormula);
        assert!(parsed.preview);
        assert!(parsed.yes);
        assert_eq!(parsed.version, None);
    }

    #[test]
    fn parse_package_mutation_args_upgrade_rejects_uninstall_only_flags() {
        let error = parse_package_mutation_args(
            "upgrade",
            &[
                "ripgrep".to_string(),
                "--manager".to_string(),
                "homebrew_formula".to_string(),
                "--preview".to_string(),
            ],
            false,
        )
        .expect_err("upgrade should reject uninstall-only preview flag");
        assert!(error.contains("unsupported package mutation argument '--preview'"));
    }

    #[test]
    fn parse_package_mutation_args_install_infers_coordinate_version_for_asdf() {
        let parsed = parse_package_mutation_args(
            "install",
            &[
                "python@mambaforge-24.11.0-1".to_string(),
                "--manager".to_string(),
                "asdf".to_string(),
            ],
            true,
        )
        .expect("asdf package coordinate should infer version");

        assert_eq!(parsed.package_name, "python");
        assert_eq!(parsed.manager, ManagerId::Asdf);
        assert_eq!(parsed.version.as_deref(), Some("mambaforge-24.11.0-1"));
    }

    #[test]
    fn parse_package_mutation_args_install_infers_coordinate_version_for_mise_with_manager_suffix()
    {
        let parsed = parse_package_mutation_args(
            "install",
            &["java@zulu-jre-javafx-8.92.0.21@mise".to_string()],
            true,
        )
        .expect("mise package coordinate should infer version");

        assert_eq!(parsed.package_name, "java");
        assert_eq!(parsed.manager, ManagerId::Mise);
        assert_eq!(parsed.version.as_deref(), Some("zulu-jre-javafx-8.92.0.21"));
    }

    #[test]
    fn parse_package_mutation_args_install_keeps_homebrew_formula_name_with_version_suffix() {
        let parsed = parse_package_mutation_args(
            "install",
            &[
                "python@3.12".to_string(),
                "--manager".to_string(),
                "homebrew_formula".to_string(),
            ],
            true,
        )
        .expect("homebrew formula names with @version should remain package names");

        assert_eq!(parsed.package_name, "python@3.12");
        assert_eq!(parsed.manager, ManagerId::HomebrewFormula);
        assert_eq!(parsed.version, None);
    }

    #[test]
    fn parse_package_mutation_args_install_rejects_conflicting_coordinate_and_version_flag() {
        let error = parse_package_mutation_args(
            "install",
            &[
                "python@mambaforge-24.11.0-1".to_string(),
                "--manager".to_string(),
                "asdf".to_string(),
                "--version".to_string(),
                "3.12.4".to_string(),
            ],
            true,
        )
        .expect_err("conflicting version selectors should fail");
        assert!(error.contains("conflicting version selectors"));
    }

    #[test]
    fn parse_package_show_args_preserves_raw_package_name_and_records_coordinate_hint() {
        let parsed = parse_package_show_args(&[
            "python@mambaforge-24.11.0-1".to_string(),
            "--manager".to_string(),
            "asdf".to_string(),
        ])
        .expect("show args should parse");

        assert_eq!(parsed.package_name, "python@mambaforge-24.11.0-1");
        assert_eq!(parsed.manager, Some(ManagerId::Asdf));
        assert_eq!(
            parsed.coordinate_hint,
            Some(("python".to_string(), "mambaforge-24.11.0-1".to_string()))
        );
    }

    #[test]
    fn parse_packages_rustup_args_supports_component_add() {
        let parsed = parse_packages_rustup_args(&[
            "component".to_string(),
            "add".to_string(),
            "stable-aarch64-apple-darwin".to_string(),
            "clippy".to_string(),
        ])
        .expect("rustup component add should parse");

        assert_eq!(
            parsed,
            RustupPackagesCommand::ComponentAdd {
                toolchain: "stable-aarch64-apple-darwin".to_string(),
                component: "clippy".to_string(),
            }
        );
    }

    #[test]
    fn parse_packages_rustup_args_rejects_relative_override_path() {
        let error = parse_packages_rustup_args(&[
            "override".to_string(),
            "set".to_string(),
            "stable-aarch64-apple-darwin".to_string(),
            "relative/path".to_string(),
        ])
        .expect_err("relative override path should fail");

        assert!(error.contains("requires an absolute path"));
    }

    #[test]
    fn command_help_topic_exists_for_packages_rustup_nested_topics() {
        assert!(command_help_topic_exists(
            Command::Packages,
            &[
                "rustup".to_string(),
                "component".to_string(),
                "add".to_string(),
            ],
        ));
        assert!(command_help_topic_exists(
            Command::Packages,
            &[
                "rustup".to_string(),
                "override".to_string(),
                "unset".to_string(),
            ],
        ));
    }

    #[test]
    fn parse_package_selector_treats_version_suffix_as_package_name() {
        let (name, manager) = parse_package_selector("python@mambaforge-24.11.0-1")
            .expect("version-like suffix should parse as package name");
        assert_eq!(name, "python@mambaforge-24.11.0-1");
        assert_eq!(manager, None);
    }

    #[test]
    fn parse_package_selector_treats_composite_suffix_as_package_name() {
        let (name, manager) = parse_package_selector("java@zulu-jre-javafx-8.92.0.21")
            .expect("composite suffix should parse as package name");
        assert_eq!(name, "java@zulu-jre-javafx-8.92.0.21");
        assert_eq!(manager, None);
    }

    #[test]
    fn parse_package_selector_still_supports_manager_suffix() {
        let (name, manager) = parse_package_selector("git@homebrew_formula")
            .expect("manager suffix should still parse");
        assert_eq!(name, "git");
        assert_eq!(manager, Some(ManagerId::HomebrewFormula));
    }

    #[test]
    fn parse_manager_mutation_args_install_accepts_method_override() {
        let parsed = parse_manager_mutation_args(
            "install",
            &[
                "mise".to_string(),
                "--method".to_string(),
                "homebrew".to_string(),
            ],
        )
        .expect("install args with method override should parse");

        assert_eq!(parsed.manager, ManagerId::Mise);
        assert_eq!(parsed.install_method_override.as_deref(), Some("homebrew"));
        assert!(!parsed.preview);
        assert!(!parsed.yes);
        assert!(!parsed.allow_unknown_provenance);
        assert_eq!(parsed.install_options.rustup_install_source, None);
        assert_eq!(parsed.install_options.rustup_binary_path, None);
        assert_eq!(parsed.install_options.mise_install_source, None);
        assert_eq!(parsed.install_options.mise_binary_path, None);
    }

    #[test]
    fn parse_manager_mutation_args_install_rejects_duplicate_method_override() {
        let error = parse_manager_mutation_args(
            "install",
            &[
                "mise".to_string(),
                "--method".to_string(),
                "homebrew".to_string(),
                "--method".to_string(),
                "scriptInstaller".to_string(),
            ],
        )
        .expect_err("duplicate --method should fail");
        assert!(error.contains("specified multiple times"));
    }

    #[test]
    fn parse_manager_mutation_args_uninstall_rejects_method_override() {
        let error = parse_manager_mutation_args(
            "uninstall",
            &[
                "rustup".to_string(),
                "--method".to_string(),
                "homebrew".to_string(),
            ],
        )
        .expect_err("uninstall should reject install-method override");
        assert!(error.contains("unsupported managers uninstall argument '--method'"));
    }

    #[test]
    fn parse_manager_mutation_args_install_accepts_rustup_source_options() {
        let parsed = parse_manager_mutation_args(
            "install",
            &[
                "rustup".to_string(),
                "--rustup-install-source".to_string(),
                "existingBinaryPath".to_string(),
                "--rustup-binary-path".to_string(),
                "/tmp/rustup-init".to_string(),
            ],
        )
        .expect("rustup install source options should parse");

        assert_eq!(parsed.manager, ManagerId::Rustup);
        assert_eq!(
            parsed.install_options.rustup_install_source,
            Some(helm_core::manager_lifecycle::RustupInstallSource::ExistingBinaryPath)
        );
        assert_eq!(
            parsed.install_options.rustup_binary_path.as_deref(),
            Some("/tmp/rustup-init")
        );
    }

    #[test]
    fn parse_manager_mutation_args_install_infers_existing_binary_source_from_path() {
        let parsed = parse_manager_mutation_args(
            "install",
            &[
                "rustup".to_string(),
                "--rustup-binary-path".to_string(),
                "/tmp/rustup-init".to_string(),
            ],
        )
        .expect("rustup binary path should imply existing-binary source");

        assert_eq!(
            parsed.install_options.rustup_install_source,
            Some(helm_core::manager_lifecycle::RustupInstallSource::ExistingBinaryPath)
        );
        assert_eq!(
            parsed.install_options.rustup_binary_path.as_deref(),
            Some("/tmp/rustup-init")
        );
    }

    #[test]
    fn parse_manager_mutation_args_install_rejects_rustup_source_flags_for_non_rustup_manager() {
        let error = parse_manager_mutation_args(
            "install",
            &[
                "mise".to_string(),
                "--rustup-install-source".to_string(),
                "officialDownload".to_string(),
            ],
        )
        .expect_err("non-rustup managers should reject rustup install-source flags");
        assert!(error.contains("only supported for manager 'rustup'"));
    }

    #[test]
    fn parse_manager_mutation_args_install_rejects_invalid_rustup_source_value() {
        let error = parse_manager_mutation_args(
            "install",
            &[
                "rustup".to_string(),
                "--rustup-install-source".to_string(),
                "invalid".to_string(),
            ],
        )
        .expect_err("invalid rustup source should fail");
        assert!(error.contains("unsupported rustup install source"));
    }

    #[test]
    fn parse_manager_mutation_args_install_accepts_mise_source_options() {
        let parsed = parse_manager_mutation_args(
            "install",
            &[
                "mise".to_string(),
                "--mise-install-source".to_string(),
                "existingBinaryPath".to_string(),
                "--mise-binary-path".to_string(),
                "/tmp/mise".to_string(),
            ],
        )
        .expect("mise install source options should parse");

        assert_eq!(parsed.manager, ManagerId::Mise);
        assert_eq!(
            parsed.install_options.mise_install_source,
            Some(helm_core::manager_lifecycle::MiseInstallSource::ExistingBinaryPath)
        );
        assert_eq!(
            parsed.install_options.mise_binary_path.as_deref(),
            Some("/tmp/mise")
        );
    }

    #[test]
    fn parse_manager_mutation_args_install_infers_mise_existing_binary_source_from_path() {
        let parsed = parse_manager_mutation_args(
            "install",
            &[
                "mise".to_string(),
                "--mise-binary-path".to_string(),
                "/tmp/mise".to_string(),
            ],
        )
        .expect("mise binary path should imply existing-binary source");

        assert_eq!(
            parsed.install_options.mise_install_source,
            Some(helm_core::manager_lifecycle::MiseInstallSource::ExistingBinaryPath)
        );
        assert_eq!(
            parsed.install_options.mise_binary_path.as_deref(),
            Some("/tmp/mise")
        );
    }

    #[test]
    fn parse_manager_mutation_args_install_rejects_mise_source_flags_for_non_mise_manager() {
        let error = parse_manager_mutation_args(
            "install",
            &[
                "rustup".to_string(),
                "--mise-install-source".to_string(),
                "officialDownload".to_string(),
            ],
        )
        .expect_err("non-mise managers should reject mise install-source flags");
        assert!(error.contains("only supported for manager 'mise'"));
    }

    #[test]
    fn parse_manager_mutation_args_install_rejects_invalid_mise_source_value() {
        let error = parse_manager_mutation_args(
            "install",
            &[
                "mise".to_string(),
                "--mise-install-source".to_string(),
                "invalid".to_string(),
            ],
        )
        .expect_err("invalid mise source should fail");
        assert!(error.contains("unsupported mise install source"));
    }

    #[test]
    fn install_method_resolution_requires_explicit_method_in_non_interactive_ambiguous_case() {
        let error = super::resolve_install_method_override_decision(
            ManagerId::Mise,
            None,
            None,
            &["homebrew", "scriptInstaller"],
            false,
        )
        .expect_err("non-interactive ambiguous install method should require --method");
        assert!(error.contains("--method"));
        assert!(error.contains("supported: homebrew, scriptInstaller"));
    }

    #[test]
    fn install_method_resolution_uses_explicit_override_without_prompt() {
        let resolved = super::resolve_install_method_override_decision(
            ManagerId::Mise,
            Some("homebrew".to_string()),
            None,
            &["homebrew", "scriptInstaller"],
            false,
        )
        .expect("explicit override should be accepted");
        assert_eq!(resolved.as_deref(), Some("homebrew"));
    }

    #[test]
    fn install_method_resolution_uses_persisted_selection_without_override() {
        let resolved = super::resolve_install_method_override_decision(
            ManagerId::Mise,
            None,
            Some("homebrew".to_string()),
            &["homebrew", "scriptInstaller"],
            false,
        )
        .expect("persisted selection should avoid explicit method requirement");
        assert_eq!(resolved.as_deref(), Some("homebrew"));
    }

    #[test]
    fn install_method_resolution_ignores_unsupported_persisted_selection() {
        let resolved = super::resolve_install_method_override_decision(
            ManagerId::Mise,
            None,
            Some("scriptInstaller".to_string()),
            &["homebrew"],
            false,
        )
        .expect("single supported method should be selected when persisted value is unsupported");
        assert_eq!(resolved.as_deref(), Some("homebrew"));
    }

    #[test]
    fn managed_install_method_policy_blocks_restricted_methods_by_default() {
        let context = super::ManagerInstallMethodPolicyContext {
            managed_environment: true,
            allow_restricted_methods: false,
        };
        assert!(!super::install_method_allowed_by_policy(
            ManagerId::Mas,
            "appStore",
            context
        ));
        assert!(super::install_method_allowed_by_policy(
            ManagerId::Mas,
            "homebrew",
            context
        ));
    }

    #[test]
    fn managed_install_method_policy_allows_restricted_methods_with_override() {
        let context = super::ManagerInstallMethodPolicyContext {
            managed_environment: true,
            allow_restricted_methods: true,
        };
        assert!(super::install_method_allowed_by_policy(
            ManagerId::Mise,
            "scriptInstaller",
            context
        ));
    }

    #[test]
    fn parse_managed_automation_policy_mode_accepts_aliases() {
        assert_eq!(
            super::parse_managed_automation_policy_mode("automatic"),
            Some(super::ManagedAutomationPolicyMode::Automatic)
        );
        assert_eq!(
            super::parse_managed_automation_policy_mode("confirm"),
            Some(super::ManagedAutomationPolicyMode::NeedsConfirmation)
        );
        assert_eq!(
            super::parse_managed_automation_policy_mode("read_only"),
            Some(super::ManagedAutomationPolicyMode::ReadOnly)
        );
    }

    #[test]
    fn parse_managed_automation_policy_mode_rejects_unknown_values() {
        assert_eq!(super::parse_managed_automation_policy_mode(""), None);
        assert_eq!(
            super::parse_managed_automation_policy_mode("definitely-not-a-mode"),
            None
        );
    }

    #[test]
    fn manager_install_methods_status_filters_blocked_not_manageable_method() {
        let db_path = temp_db_path("manager-install-method-status-blocked-filter");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let status = super::manager_install_methods_status(&store, ManagerId::Sparkle)
            .expect("sparkle install-method status should resolve");
        assert!(status.install_methods.is_empty());
        assert!(status.selected_install_method.is_none());

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn manager_install_method_selection_blocks_not_manageable_method() {
        assert!(!super::manager_install_method_allowed_for_selection(
            ManagerId::Sparkle,
            "notManageable"
        ));
    }

    #[test]
    fn homebrew_formula_name_map_includes_expanded_manager_set() {
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Asdf),
            Some("asdf")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Mise),
            Some("mise")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Mas),
            Some("mas")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Pnpm),
            Some("pnpm")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Yarn),
            Some("yarn")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Pipx),
            Some("pipx")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Poetry),
            Some("poetry")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::CargoBinstall),
            Some("cargo-binstall")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Podman),
            Some("podman")
        );
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::Colima),
            Some("colima")
        );
        assert_eq!(super::manager_homebrew_formula_name(ManagerId::Npm), None);
        assert_eq!(
            super::manager_homebrew_formula_name(ManagerId::DockerDesktop),
            None
        );
    }

    #[test]
    fn homebrew_formula_name_from_path_extracts_cellar_formula() {
        let formula = super::homebrew_formula_name_from_path(Path::new(
            "/opt/homebrew/Cellar/python@3.12/3.12.9_1/bin/pip3",
        ));
        assert_eq!(formula.as_deref(), Some("python@3.12"));
    }

    #[test]
    fn homebrew_formula_name_from_path_returns_none_without_cellar() {
        let formula = super::homebrew_formula_name_from_path(Path::new("/opt/homebrew/bin/npm"));
        assert_eq!(formula, None);
    }

    #[test]
    fn parse_manager_mutation_args_rejects_uninstall_flags_for_update() {
        let error =
            parse_manager_mutation_args("update", &["rustup".to_string(), "--yes".to_string()])
                .expect_err("update should reject uninstall-only flags");
        assert!(error.contains("unsupported managers update argument '--yes'"));
    }

    #[test]
    fn homebrew_update_request_prefers_provenance_strategy() {
        let db_path = temp_db_path("homebrew-update-provenance-strategy");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        seed_homebrew_detected(&store);
        store
            .replace_install_instances(
                ManagerId::HomebrewFormula,
                &[ManagerInstallInstance {
                    manager: ManagerId::HomebrewFormula,
                    instance_id: "homebrew-instance-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/brew/4.4.1/bin/brew".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/brew"),
                    canonical_path: Some(PathBuf::from("/opt/homebrew/Cellar/brew/4.4.1/bin/brew")),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/brew")],
                    is_active: true,
                    version: Some("4.4.1".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.92,
                    decision_margin: Some(0.34),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for brew".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("install instance should persist");

        let (target_manager, request) = super::build_manager_mutation_request(
            &store,
            ManagerId::HomebrewFormula,
            "update",
            None,
        )
        .expect("homebrew update request should build");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Upgrade(upgrade) => {
                let package = upgrade
                    .package
                    .expect("homebrew self update package should exist");
                assert_eq!(package.manager, ManagerId::HomebrewFormula);
                assert_eq!(package.name, "__self__");
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn npm_update_request_uses_homebrew_parent_formula_from_active_instance() {
        let db_path = temp_db_path("npm-update-homebrew-parent-formula");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Npm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Npm,
                    instance_id: "npm-instance-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/node/22.14.0/bin/npm".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/npm"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/node/22.14.0/bin/npm",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/npm")],
                    is_active: true,
                    version: Some("10.9.2".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.89,
                    decision_margin: Some(0.28),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for node".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("npm install instance should persist");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Npm, "update", None)
                .expect("npm update request should build");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Upgrade(upgrade) => {
                let package = upgrade.package.expect("homebrew node package should exist");
                assert_eq!(package.manager, ManagerId::HomebrewFormula);
                assert!(package.name.contains("node"));
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn dynamic_homebrew_parent_managers_update_request_resolves_parent_formula() {
        let cases = [
            (
                ManagerId::Pip,
                "/opt/homebrew/Cellar/python@3.12/3.12.9_1/bin/pip3",
                "/opt/homebrew/bin/pip3",
                "python@3.12",
            ),
            (
                ManagerId::RubyGems,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/gem",
                "/opt/homebrew/bin/gem",
                "ruby",
            ),
            (
                ManagerId::Bundler,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/bundle3.4",
                "/opt/homebrew/bin/bundle3.4",
                "ruby",
            ),
            (
                ManagerId::Cargo,
                "/opt/homebrew/Cellar/rust/1.86.0/bin/cargo",
                "/opt/homebrew/bin/cargo",
                "rust",
            ),
        ];

        for (manager, canonical_path, display_path, expected_formula) in cases {
            let db_path = temp_db_path(
                format!("{}-update-homebrew-parent-formula", manager.as_str()).as_str(),
            );
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-instance-update", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: canonical_path.to_string(),
                        display_path: PathBuf::from(display_path),
                        canonical_path: Some(PathBuf::from(canonical_path)),
                        alias_paths: vec![PathBuf::from(display_path)],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.89,
                        decision_margin: Some(0.28),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some(
                            "canonical path is inside Homebrew Cellar".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::SourceBuild),
                        competing_confidence: Some(0.35),
                    }],
                )
                .expect("manager install instance should persist");

            let (target_manager, request) =
                super::build_manager_mutation_request(&store, manager, "update", None)
                    .expect("dynamic manager update request should build");
            assert_eq!(target_manager, ManagerId::HomebrewFormula);
            match request {
                super::AdapterRequest::Upgrade(upgrade) => {
                    let package = upgrade
                        .package
                        .expect("homebrew parent formula package should exist");
                    assert_eq!(package.manager, ManagerId::HomebrewFormula);
                    assert!(
                        package.name.starts_with(expected_formula),
                        "expected '{}' prefix in package '{}'",
                        expected_formula,
                        package.name
                    );
                }
                other => panic!("unexpected request: {other:?}"),
            }

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn pip_update_request_blocks_read_only_provenance() {
        let db_path = temp_db_path("pip-update-read-only");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Pip,
                &[ManagerInstallInstance {
                    manager: ManagerId::Pip,
                    instance_id: "pip-read-only-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/usr/bin/pip3".to_string(),
                    display_path: PathBuf::from("/usr/bin/pip3"),
                    canonical_path: Some(PathBuf::from("/usr/bin/pip3")),
                    alias_paths: vec![PathBuf::from("/usr/bin/pip3")],
                    is_active: true,
                    version: Some("24.0".to_string()),
                    provenance: InstallProvenance::System,
                    confidence: 0.94,
                    decision_margin: Some(0.48),
                    automation_level: AutomationLevel::ReadOnly,
                    uninstall_strategy: StrategyKind::ReadOnly,
                    update_strategy: StrategyKind::ReadOnly,
                    remediation_strategy: StrategyKind::ReadOnly,
                    explanation_primary: Some(
                        "system pip installation is read-only in Helm".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.21),
                }],
            )
            .expect("pip install instance should persist");

        let error = super::build_manager_mutation_request(&store, ManagerId::Pip, "update", None)
            .expect_err("read-only update should be blocked");
        assert!(error.contains("read-only"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn dynamic_homebrew_parent_managers_update_request_blocks_read_only_provenance() {
        let cases = [
            (ManagerId::Npm, "npm"),
            (ManagerId::Pip, "pip3"),
            (ManagerId::RubyGems, "gem"),
            (ManagerId::Bundler, "bundle"),
            (ManagerId::Cargo, "cargo"),
        ];

        for (manager, executable_name) in cases {
            let db_path = temp_db_path(format!("{}-update-read-only", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-read-only-update", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!("/usr/bin/{}", executable_name),
                        display_path: PathBuf::from(format!("/usr/bin/{}", executable_name)),
                        canonical_path: Some(PathBuf::from(format!(
                            "/usr/bin/{}",
                            executable_name
                        ))),
                        alias_paths: vec![PathBuf::from(format!("/usr/bin/{}", executable_name))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::System,
                        confidence: 0.94,
                        decision_margin: Some(0.48),
                        automation_level: AutomationLevel::ReadOnly,
                        uninstall_strategy: StrategyKind::ReadOnly,
                        update_strategy: StrategyKind::ReadOnly,
                        remediation_strategy: StrategyKind::ReadOnly,
                        explanation_primary: Some(
                            "system installation is read-only in Helm".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.21),
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_mutation_request(&store, manager, "update", None)
                .expect_err("read-only update should be blocked");
            assert!(error.contains("read-only"));

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn dynamic_homebrew_parent_managers_update_request_errors_when_formula_unresolved() {
        let cases = [
            (
                ManagerId::Pip,
                "/opt/homebrew/bin/pip3",
                "/opt/homebrew/bin/pip3",
            ),
            (
                ManagerId::RubyGems,
                "/opt/homebrew/bin/gem",
                "/opt/homebrew/bin/gem",
            ),
            (
                ManagerId::Bundler,
                "/opt/homebrew/bin/bundle3.4",
                "/opt/homebrew/bin/bundle3.4",
            ),
            (
                ManagerId::Cargo,
                "/opt/homebrew/bin/cargo",
                "/opt/homebrew/bin/cargo",
            ),
        ];

        for (manager, canonical_path, display_path) in cases {
            let db_path = temp_db_path(
                format!("{}-update-homebrew-formula-unresolved", manager.as_str()).as_str(),
            );
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-unresolved-update", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: canonical_path.to_string(),
                        display_path: PathBuf::from(display_path),
                        canonical_path: Some(PathBuf::from(canonical_path)),
                        alias_paths: vec![PathBuf::from(display_path)],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.80,
                        decision_margin: Some(0.20),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some(
                            "path indicates Homebrew ownership but formula could not be resolved"
                                .to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: None,
                        competing_confidence: None,
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_mutation_request(&store, manager, "update", None)
                .expect_err("update should fail when formula ownership cannot be derived");
            assert!(error.contains("formula ownership could not be determined"));
            assert!(error.contains(&format!("helm managers instances {}", manager.as_str())));

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn dynamic_homebrew_parent_managers_update_request_blocks_ambiguous_provenance() {
        let db_path = temp_db_path("pip-update-ambiguous");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Pip,
                &[ManagerInstallInstance {
                    manager: ManagerId::Pip,
                    instance_id: "pip-ambiguous-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/python@3.12/3.12.9_1/bin/pip3"
                        .to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/pip3"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/python@3.12/3.12.9_1/bin/pip3",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/pip3")],
                    is_active: true,
                    version: Some("24.0".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.47,
                    decision_margin: Some(0.05),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting pip provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.42),
                }],
            )
            .expect("pip install instance should persist");

        let error = super::build_manager_mutation_request(&store, ManagerId::Pip, "update", None)
            .expect_err("ambiguous update should be blocked");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("helm managers instances pip"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn npm_update_request_errors_when_formula_ownership_is_unresolved() {
        let db_path = temp_db_path("npm-update-homebrew-formula-unresolved");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Npm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Npm,
                    instance_id: "npm-unresolved-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/bin/npm".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/npm"),
                    canonical_path: Some(PathBuf::from("/opt/homebrew/bin/npm")),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/npm")],
                    is_active: true,
                    version: Some("10.9.2".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.80,
                    decision_margin: Some(0.20),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "npm path indicates Homebrew ownership but formula could not be resolved"
                            .to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: None,
                    competing_confidence: None,
                }],
            )
            .expect("npm install instance should persist");

        let error = super::build_manager_mutation_request(&store, ManagerId::Npm, "update", None)
            .expect_err("npm update should fail when formula ownership cannot be derived");
        assert!(error.contains("formula ownership could not be determined"));
        assert!(error.contains("helm managers instances npm"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn homebrew_update_request_blocks_ambiguous_provenance() {
        let db_path = temp_db_path("homebrew-update-ambiguous");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::HomebrewFormula,
                &[ManagerInstallInstance {
                    manager: ManagerId::HomebrewFormula,
                    instance_id: "homebrew-instance-ambiguous".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/tools/brew".to_string(),
                    display_path: PathBuf::from("/Users/test/tools/brew"),
                    canonical_path: Some(PathBuf::from("/Users/test/tools/brew")),
                    alias_paths: vec![PathBuf::from("/Users/test/tools/brew")],
                    is_active: true,
                    version: Some("4.4.1".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.40,
                    decision_margin: Some(0.05),
                    automation_level: AutomationLevel::ReadOnly,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting homebrew provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.39),
                }],
            )
            .expect("install instance should persist");

        let error = super::build_manager_mutation_request(
            &store,
            ManagerId::HomebrewFormula,
            "update",
            None,
        )
        .expect_err("ambiguous homebrew update should be blocked");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("helm managers instances homebrew_formula"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn asdf_update_request_prefers_provenance_strategy_over_install_method() {
        let db_path = temp_db_path("asdf-update-provenance-strategy");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .set_manager_selected_install_method(ManagerId::Asdf, Some("scriptInstaller"))
            .expect("asdf install method preference should persist");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Asdf,
                &[ManagerInstallInstance {
                    manager: ManagerId::Asdf,
                    instance_id: "asdf-instance-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/asdf/0.15.0/bin/asdf".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/asdf"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/asdf/0.15.0/bin/asdf",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/asdf")],
                    is_active: true,
                    version: Some("0.15.0".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.91,
                    decision_margin: Some(0.32),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for asdf".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Asdf),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("install instance should persist");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Asdf, "update", None)
                .expect("asdf update request should build");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Upgrade(upgrade) => {
                let package = upgrade.package.expect("homebrew asdf package should exist");
                assert_eq!(package.manager, ManagerId::HomebrewFormula);
                assert!(package.name.contains("asdf"));
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn asdf_update_request_blocks_ambiguous_provenance() {
        let db_path = temp_db_path("asdf-update-ambiguous");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Asdf,
                &[ManagerInstallInstance {
                    manager: ManagerId::Asdf,
                    instance_id: "asdf-instance-ambiguous".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.asdf/bin/asdf".to_string(),
                    display_path: PathBuf::from("/Users/test/.asdf/bin/asdf"),
                    canonical_path: Some(PathBuf::from("/Users/test/.asdf/bin/asdf")),
                    alias_paths: vec![PathBuf::from("/Users/test/.asdf/bin/asdf")],
                    is_active: true,
                    version: Some("0.15.0".to_string()),
                    provenance: InstallProvenance::Asdf,
                    confidence: 0.92,
                    decision_margin: Some(0.30),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "asdf executable path indicates asdf-managed layout".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("install instance should persist");

        let error = super::build_manager_mutation_request(&store, ManagerId::Asdf, "update", None)
            .expect_err("ambiguous asdf update should be blocked");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("helm managers instances asdf"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn mise_update_request_prefers_provenance_strategy_over_install_method() {
        let db_path = temp_db_path("mise-update-provenance-strategy");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .set_manager_selected_install_method(ManagerId::Mise, Some("scriptInstaller"))
            .expect("mise install method preference should persist");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Mise,
                &[ManagerInstallInstance {
                    manager: ManagerId::Mise,
                    instance_id: "mise-instance-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/mise/2025.1.0/bin/mise".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/mise"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/mise/2025.1.0/bin/mise",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/mise")],
                    is_active: true,
                    version: Some("2025.1.0".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.91,
                    decision_margin: Some(0.32),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for mise".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.37),
                }],
            )
            .expect("install instance should persist");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Mise, "update", None)
                .expect("mise update request should build");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Upgrade(upgrade) => {
                let package = upgrade.package.expect("homebrew mise package should exist");
                assert_eq!(package.manager, ManagerId::HomebrewFormula);
                assert!(package.name.contains("mise"));
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn mise_update_request_blocks_ambiguous_provenance() {
        let db_path = temp_db_path("mise-update-ambiguous");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Mise,
                &[ManagerInstallInstance {
                    manager: ManagerId::Mise,
                    instance_id: "mise-instance-ambiguous".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/usr/local/bin/mise".to_string(),
                    display_path: PathBuf::from("/usr/local/bin/mise"),
                    canonical_path: Some(PathBuf::from("/usr/local/bin/mise")),
                    alias_paths: vec![PathBuf::from("/usr/local/bin/mise")],
                    is_active: true,
                    version: Some("2025.1.0".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.47,
                    decision_margin: Some(0.05),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting mise provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.43),
                }],
            )
            .expect("install instance should persist");

        let error = super::build_manager_mutation_request(&store, ManagerId::Mise, "update", None)
            .expect_err("ambiguous mise update should be blocked");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("helm managers instances mise"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn mas_update_request_prefers_provenance_strategy_over_install_method() {
        let db_path = temp_db_path("mas-update-provenance-strategy");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .set_manager_selected_install_method(ManagerId::Mas, Some("appStore"))
            .expect("mas install method preference should persist");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Mas,
                &[ManagerInstallInstance {
                    manager: ManagerId::Mas,
                    instance_id: "mas-instance-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/mas/1.8.8/bin/mas".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/mas"),
                    canonical_path: Some(PathBuf::from("/opt/homebrew/Cellar/mas/1.8.8/bin/mas")),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/mas")],
                    is_active: true,
                    version: Some("1.8.8".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.90,
                    decision_margin: Some(0.31),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for mas".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.38),
                }],
            )
            .expect("install instance should persist");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Mas, "update", None)
                .expect("mas update request should build");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Upgrade(upgrade) => {
                let package = upgrade.package.expect("homebrew mas package should exist");
                assert_eq!(package.manager, ManagerId::HomebrewFormula);
                assert!(package.name.contains("mas"));
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn pnpm_update_request_prefers_provenance_strategy() {
        let db_path = temp_db_path("pnpm-update-provenance-strategy");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Pnpm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Pnpm,
                    instance_id: "pnpm-instance-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/pnpm/9.15.0/bin/pnpm".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/pnpm"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/pnpm/9.15.0/bin/pnpm",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/pnpm")],
                    is_active: true,
                    version: Some("9.15.0".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.90,
                    decision_margin: Some(0.30),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for pnpm".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.36),
                }],
            )
            .expect("install instance should persist");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Pnpm, "update", None)
                .expect("pnpm update request should build");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Upgrade(upgrade) => {
                let package = upgrade.package.expect("homebrew pnpm package should exist");
                assert_eq!(package.manager, ManagerId::HomebrewFormula);
                assert!(package.name.contains("pnpm"));
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn pnpm_update_request_blocks_ambiguous_provenance() {
        let db_path = temp_db_path("pnpm-update-ambiguous");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Pnpm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Pnpm,
                    instance_id: "pnpm-instance-ambiguous".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.local/bin/pnpm".to_string(),
                    display_path: PathBuf::from("/Users/test/.local/bin/pnpm"),
                    canonical_path: Some(PathBuf::from("/Users/test/.local/bin/pnpm")),
                    alias_paths: vec![PathBuf::from("/Users/test/.local/bin/pnpm")],
                    is_active: true,
                    version: Some("9.15.0".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.49,
                    decision_margin: Some(0.05),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting pnpm provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.44),
                }],
            )
            .expect("install instance should persist");

        let error = super::build_manager_mutation_request(&store, ManagerId::Pnpm, "update", None)
            .expect_err("ambiguous pnpm update should be blocked");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("helm managers instances pnpm"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn one_to_one_homebrew_managers_update_request_prefers_provenance_strategy() {
        let cases = [
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, formula_name) in cases {
            let db_path =
                temp_db_path(format!("{}-update-provenance-strategy", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-instance-update", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ),
                        display_path: PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        )),
                        canonical_path: Some(PathBuf::from(format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ))),
                        alias_paths: vec![PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        ))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.90,
                        decision_margin: Some(0.30),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some(
                            "canonical path is inside Homebrew Cellar".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::SourceBuild),
                        competing_confidence: Some(0.35),
                    }],
                )
                .expect("manager install instance should persist");

            let (target_manager, request) =
                super::build_manager_mutation_request(&store, manager, "update", None)
                    .expect("update request should build");
            assert_eq!(target_manager, ManagerId::HomebrewFormula);
            match request {
                super::AdapterRequest::Upgrade(upgrade) => {
                    let package = upgrade.package.expect("homebrew package should exist");
                    assert_eq!(package.manager, ManagerId::HomebrewFormula);
                    assert!(
                        package.name.starts_with(formula_name),
                        "expected formula '{}' in package '{}'",
                        formula_name,
                        package.name
                    );
                }
                other => panic!("unexpected request: {other:?}"),
            }

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn one_to_one_homebrew_managers_update_request_blocks_ambiguous_provenance() {
        let cases = [
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, formula_name) in cases {
            let db_path = temp_db_path(format!("{}-update-ambiguous", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-ambiguous-update", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ),
                        display_path: PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        )),
                        canonical_path: Some(PathBuf::from(format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ))),
                        alias_paths: vec![PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        ))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Unknown,
                        confidence: 0.49,
                        decision_margin: Some(0.05),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::InteractivePrompt,
                        update_strategy: StrategyKind::InteractivePrompt,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some(
                            "insufficient or conflicting provenance evidence".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.44),
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_mutation_request(&store, manager, "update", None)
                .expect_err("ambiguous update should block");
            assert!(error.contains("ambiguous"));
            assert!(error.contains(&format!("helm managers instances {}", manager.as_str())));

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn one_to_one_homebrew_managers_update_request_blocks_read_only_provenance() {
        let cases = [
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, executable_name) in cases {
            let db_path = temp_db_path(format!("{}-update-read-only", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-read-only-update", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!("/usr/bin/{}", executable_name),
                        display_path: PathBuf::from(format!("/usr/bin/{}", executable_name)),
                        canonical_path: Some(PathBuf::from(format!(
                            "/usr/bin/{}",
                            executable_name
                        ))),
                        alias_paths: vec![PathBuf::from(format!("/usr/bin/{}", executable_name))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::System,
                        confidence: 0.95,
                        decision_margin: Some(0.50),
                        automation_level: AutomationLevel::ReadOnly,
                        uninstall_strategy: StrategyKind::ReadOnly,
                        update_strategy: StrategyKind::ReadOnly,
                        remediation_strategy: StrategyKind::ReadOnly,
                        explanation_primary: Some(
                            "system installation is read-only in Helm".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.20),
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_mutation_request(&store, manager, "update", None)
                .expect_err("read-only update should block");
            assert!(error.contains("read-only"));

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn rustup_update_request_prefers_provenance_strategy_over_install_method() {
        let db_path = temp_db_path("rustup-update-provenance-strategy");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .set_manager_selected_install_method(ManagerId::Rustup, Some("homebrew"))
            .expect("rustup install method preference should persist");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-instance-update".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::RustupInit,
                    confidence: 0.91,
                    decision_margin: Some(0.32),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::RustupSelf,
                    update_strategy: StrategyKind::RustupSelf,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some("rustup binary is under CARGO_HOME/bin".to_string()),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.45),
                }],
            )
            .expect("install instance should persist");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Rustup, "update", None)
                .expect("rustup update request should build");
        assert_eq!(target_manager, ManagerId::Rustup);
        match request {
            super::AdapterRequest::Upgrade(upgrade) => {
                let package = upgrade
                    .package
                    .expect("rustup self update package should exist");
                assert_eq!(package.manager, ManagerId::Rustup);
                assert_eq!(package.name, "__self__");
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_update_request_blocks_ambiguous_provenance() {
        let db_path = temp_db_path("rustup-update-ambiguous");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-instance-ambiguous".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.44,
                    decision_margin: Some(0.04),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting rustup provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::RustupInit),
                    competing_confidence: Some(0.40),
                }],
            )
            .expect("install instance should persist");

        let error =
            super::build_manager_mutation_request(&store, ManagerId::Rustup, "update", None)
                .expect_err("ambiguous rustup update should be blocked");
        assert!(error.contains("ambiguous"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_update_request_uses_active_instance_in_multi_install() {
        let db_path = temp_db_path("rustup-update-multi-install");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        seed_homebrew_detected(&store);
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "rustup-homebrew-active".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup".to_string(),
                        display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
                        canonical_path: Some(PathBuf::from(
                            "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
                        )),
                        alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
                        is_active: true,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.93,
                        decision_margin: Some(0.31),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some("active path is in Homebrew Cellar".to_string()),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::RustupInit),
                        competing_confidence: Some(0.41),
                    },
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "rustup-self-inactive".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                        display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                        canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                        alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                        is_active: false,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::RustupInit,
                        confidence: 0.92,
                        decision_margin: Some(0.30),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::RustupSelf,
                        update_strategy: StrategyKind::RustupSelf,
                        remediation_strategy: StrategyKind::RustupSelf,
                        explanation_primary: Some(
                            "inactive path is under CARGO_HOME/bin".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.45),
                    },
                ],
            )
            .expect("install instances should persist");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Rustup, "update", None)
                .expect("multi-install update request should build");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Upgrade(upgrade) => {
                let package = upgrade
                    .package
                    .expect("homebrew rustup package should exist");
                assert_eq!(package.manager, ManagerId::HomebrewFormula);
                assert!(package.name.contains("rustup"));
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn manager_install_request_prefers_cli_method_override_over_saved_preference() {
        let db_path = temp_db_path("manager-install-cli-method-override");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        seed_homebrew_detected(&store);
        store
            .set_manager_selected_install_method(ManagerId::Mise, Some("homebrew"))
            .expect("persisting mise install preference should succeed");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Mise, "install", None)
                .expect("saved method should drive install route");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(install.package.name, "mise");
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let (target_manager, request) = super::build_manager_mutation_request(
            &store,
            ManagerId::Mise,
            "install",
            Some("scriptInstaller".to_string()),
        )
        .expect("cli method override should supersede saved preference");
        assert_eq!(target_manager, ManagerId::Mise);
        match request {
            super::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Mise);
                assert_eq!(install.package.name, "__self__");
                assert_eq!(
                    install.version.as_deref(),
                    Some("scriptInstaller:officialDownload")
                );
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn asdf_install_request_defaults_to_script_installer_and_allows_homebrew_override() {
        let db_path = temp_db_path("asdf-install-method-routing");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        seed_homebrew_detected(&store);
        store
            .set_manager_selected_install_method(ManagerId::Asdf, Some("scriptInstaller"))
            .expect("persisting script installer preference should succeed");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Asdf, "install", None)
                .expect("saved script installer preference should resolve install request");
        assert_eq!(target_manager, ManagerId::Asdf);
        match request {
            super::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Asdf);
                assert_eq!(install.package.name, "__self__");
                assert_eq!(
                    install.version.as_deref(),
                    Some("scriptInstaller:officialDownload")
                );
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let (target_manager, request) = super::build_manager_mutation_request(
            &store,
            ManagerId::Asdf,
            "install",
            Some("homebrew".to_string()),
        )
        .expect("cli method override should recover asdf install route");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(install.package.name, "asdf");
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_install_request_defaults_to_rustup_installer_and_allows_homebrew_override() {
        let db_path = temp_db_path("rustup-install-method-routing");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        seed_homebrew_detected(&store);
        store
            .set_manager_selected_install_method(ManagerId::Rustup, Some("rustupInstaller"))
            .expect("persisting rustup preferred method should succeed");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Rustup, "install", None)
                .expect("saved rustup installer preference should resolve install request");
        assert_eq!(target_manager, ManagerId::Rustup);
        match request {
            super::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::Rustup);
                assert_eq!(install.package.name, "__self__");
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let (target_manager, request) = super::build_manager_mutation_request(
            &store,
            ManagerId::Rustup,
            "install",
            Some("homebrew".to_string()),
        )
        .expect("cli method override should recover rustup install route");
        assert_eq!(target_manager, ManagerId::HomebrewFormula);
        match request {
            super::AdapterRequest::Install(install) => {
                assert_eq!(install.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(install.package.name, "rustup");
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let (target_manager, request) = super::build_manager_mutation_request_with_options(
            &store,
            ManagerId::Rustup,
            "install",
            Some("rustupInstaller".to_string()),
            helm_core::manager_lifecycle::ManagerInstallOptions {
                rustup_install_source: Some(
                    helm_core::manager_lifecycle::RustupInstallSource::ExistingBinaryPath,
                ),
                rustup_binary_path: Some("/tmp/rustup-init".to_string()),
                ..helm_core::manager_lifecycle::ManagerInstallOptions::default()
            },
        )
        .expect("rustup existing-binary install source should map into install request");
        assert_eq!(target_manager, ManagerId::Rustup);
        match request {
            super::AdapterRequest::Install(install) => {
                assert_eq!(
                    install.version.as_deref(),
                    Some("existingBinaryPath:/tmp/rustup-init")
                );
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn resolve_install_method_override_for_tui_prefers_supported_method() {
        let db_path = temp_db_path("manager-install-method-tui-resolution");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .set_manager_selected_install_method(ManagerId::Mise, Some("scriptInstaller"))
            .expect("persisting mise method should succeed");

        let override_method =
            super::resolve_install_method_override_for_tui(&store, ManagerId::Mise)
                .expect("tui method resolution should succeed");
        assert_eq!(override_method.as_deref(), Some("scriptInstaller"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn manager_uninstall_plan_ignores_saved_install_method_preference() {
        let db_path = temp_db_path("manager-uninstall-ignores-install-method");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .set_manager_selected_install_method(ManagerId::Mise, Some("scriptInstaller"))
            .expect("persisting mise selected install method should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Mise,
                &[ManagerInstallInstance {
                    manager: ManagerId::Mise,
                    instance_id: "mise-homebrew-active-preference-ignored".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/mise/2026.2.8/bin/mise".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/mise"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/mise/2026.2.8/bin/mise",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/mise")],
                    is_active: true,
                    version: Some("2026.2.8".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.94,
                    decision_margin: Some(0.32),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for mise".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.22),
                }],
            )
            .expect("mise install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Mise, false, false)
            .expect("uninstall plan should not depend on saved install method");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(uninstall.package.name, "mise");
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn asdf_uninstall_plan_uses_active_homebrew_strategy() {
        let db_path = temp_db_path("asdf-uninstall-active-homebrew");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Asdf,
                &[ManagerInstallInstance {
                    manager: ManagerId::Asdf,
                    instance_id: "asdf-homebrew-active".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/asdf/0.15.0/bin/asdf".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/asdf"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/asdf/0.15.0/bin/asdf",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/asdf")],
                    is_active: true,
                    version: Some("0.15.0".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.91,
                    decision_margin: Some(0.33),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for asdf".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Asdf),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("asdf install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Asdf, false, false)
            .expect("asdf uninstall plan should build from active strategy");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(uninstall.package.name, "asdf");
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn asdf_uninstall_plan_blocks_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("asdf-uninstall-ambiguous-no-override");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Asdf,
                &[ManagerInstallInstance {
                    manager: ManagerId::Asdf,
                    instance_id: "asdf-ambiguous".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.asdf/bin/asdf".to_string(),
                    display_path: PathBuf::from("/Users/test/.asdf/bin/asdf"),
                    canonical_path: Some(PathBuf::from("/Users/test/.asdf/bin/asdf")),
                    alias_paths: vec![PathBuf::from("/Users/test/.asdf/bin/asdf")],
                    is_active: true,
                    version: Some("0.15.0".to_string()),
                    provenance: InstallProvenance::Asdf,
                    confidence: 0.92,
                    decision_margin: Some(0.30),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "asdf executable path indicates asdf-managed layout".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("asdf install instance should persist");

        let error = super::build_manager_uninstall_plan(&store, ManagerId::Asdf, false, false)
            .expect_err("ambiguous asdf uninstall strategy should block without override");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("allow-unknown-provenance"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn asdf_uninstall_preview_allows_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("asdf-uninstall-ambiguous-preview");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Asdf,
                &[ManagerInstallInstance {
                    manager: ManagerId::Asdf,
                    instance_id: "asdf-ambiguous-preview".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.asdf/bin/asdf".to_string(),
                    display_path: PathBuf::from("/Users/test/.asdf/bin/asdf"),
                    canonical_path: Some(PathBuf::from("/Users/test/.asdf/bin/asdf")),
                    alias_paths: vec![PathBuf::from("/Users/test/.asdf/bin/asdf")],
                    is_active: true,
                    version: Some("0.15.0".to_string()),
                    provenance: InstallProvenance::Asdf,
                    confidence: 0.92,
                    decision_margin: Some(0.30),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "asdf executable path indicates asdf-managed layout".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("asdf install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Asdf, false, true)
            .expect("preview path should allow ambiguous uninstall routing");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        assert!(plan.preview.unknown_override_required);
        assert!(!plan.preview.used_unknown_override);
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn asdf_uninstall_plan_allows_ambiguous_provenance_with_override() {
        let db_path = temp_db_path("asdf-uninstall-ambiguous-with-override");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Asdf,
                &[ManagerInstallInstance {
                    manager: ManagerId::Asdf,
                    instance_id: "asdf-ambiguous-override".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.asdf/bin/asdf".to_string(),
                    display_path: PathBuf::from("/Users/test/.asdf/bin/asdf"),
                    canonical_path: Some(PathBuf::from("/Users/test/.asdf/bin/asdf")),
                    alias_paths: vec![PathBuf::from("/Users/test/.asdf/bin/asdf")],
                    is_active: true,
                    version: Some("0.15.0".to_string()),
                    provenance: InstallProvenance::Asdf,
                    confidence: 0.92,
                    decision_margin: Some(0.30),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "asdf executable path indicates asdf-managed layout".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("asdf install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Asdf, true, false)
            .expect("override should allow ambiguous uninstall strategy planning");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        assert!(plan.preview.unknown_override_required);
        assert!(plan.preview.used_unknown_override);
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn asdf_uninstall_read_only_strategy_blocks_mutation_and_allows_preview() {
        let db_path = temp_db_path("asdf-uninstall-read-only");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Asdf,
                &[ManagerInstallInstance {
                    manager: ManagerId::Asdf,
                    instance_id: "asdf-read-only".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/usr/bin/asdf".to_string(),
                    display_path: PathBuf::from("/usr/bin/asdf"),
                    canonical_path: Some(PathBuf::from("/usr/bin/asdf")),
                    alias_paths: vec![PathBuf::from("/usr/bin/asdf")],
                    is_active: true,
                    version: Some("0.15.0".to_string()),
                    provenance: InstallProvenance::System,
                    confidence: 0.95,
                    decision_margin: Some(0.50),
                    automation_level: AutomationLevel::ReadOnly,
                    uninstall_strategy: StrategyKind::ReadOnly,
                    update_strategy: StrategyKind::ReadOnly,
                    remediation_strategy: StrategyKind::ReadOnly,
                    explanation_primary: Some(
                        "system asdf installation is read-only in Helm".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.20),
                }],
            )
            .expect("asdf install instance should persist");

        let error = super::build_manager_uninstall_plan(&store, ManagerId::Asdf, false, false)
            .expect_err("read-only uninstall should block mutation");
        assert!(error.contains("read-only"));

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Asdf, false, true)
            .expect("preview should remain available for read-only uninstall");
        assert_eq!(plan.target_manager, ManagerId::Asdf);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::Asdf);
                assert_eq!(uninstall.package.name, "__self__");
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert_eq!(plan.preview.strategy, "read_only");
        assert!(plan.preview.read_only_blocked);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn mise_uninstall_plan_uses_active_homebrew_strategy() {
        let db_path = temp_db_path("mise-uninstall-active-homebrew");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Mise,
                &[ManagerInstallInstance {
                    manager: ManagerId::Mise,
                    instance_id: "mise-homebrew-active".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/mise/2025.1.0/bin/mise".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/mise"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/mise/2025.1.0/bin/mise",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/mise")],
                    is_active: true,
                    version: Some("2025.1.0".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.92,
                    decision_margin: Some(0.34),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for mise".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.33),
                }],
            )
            .expect("mise install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Mise, false, false)
            .expect("mise uninstall plan should build from active strategy");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(uninstall.package.name, "mise");
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn mise_uninstall_plan_blocks_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("mise-uninstall-unknown-no-override");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Mise,
                &[ManagerInstallInstance {
                    manager: ManagerId::Mise,
                    instance_id: "mise-unknown".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/usr/local/bin/mise".to_string(),
                    display_path: PathBuf::from("/usr/local/bin/mise"),
                    canonical_path: Some(PathBuf::from("/usr/local/bin/mise")),
                    alias_paths: vec![PathBuf::from("/usr/local/bin/mise")],
                    is_active: true,
                    version: Some("2025.1.0".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.49,
                    decision_margin: Some(0.06),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting mise provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.44),
                }],
            )
            .expect("mise install instance should persist");

        let error = super::build_manager_uninstall_plan(&store, ManagerId::Mise, false, false)
            .expect_err("ambiguous uninstall strategy should block without override");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("allow-unknown-provenance"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn mise_uninstall_preview_allows_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("mise-uninstall-unknown-preview");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Mise,
                &[ManagerInstallInstance {
                    manager: ManagerId::Mise,
                    instance_id: "mise-unknown-preview".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/usr/local/bin/mise".to_string(),
                    display_path: PathBuf::from("/usr/local/bin/mise"),
                    canonical_path: Some(PathBuf::from("/usr/local/bin/mise")),
                    alias_paths: vec![PathBuf::from("/usr/local/bin/mise")],
                    is_active: true,
                    version: Some("2025.1.0".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.48,
                    decision_margin: Some(0.05),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting mise provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.43),
                }],
            )
            .expect("mise install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Mise, false, true)
            .expect("preview path should allow ambiguous uninstall routing");
        assert_eq!(plan.target_manager, ManagerId::Mise);
        assert!(plan.preview.unknown_provenance);
        assert!(plan.preview.unknown_override_required);
        assert!(!plan.preview.used_unknown_override);
        assert_eq!(plan.preview.strategy, "interactive_prompt");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn mas_uninstall_plan_uses_active_homebrew_strategy() {
        let db_path = temp_db_path("mas-uninstall-active-homebrew");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Mas,
                &[ManagerInstallInstance {
                    manager: ManagerId::Mas,
                    instance_id: "mas-homebrew-active".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/mas/1.8.8/bin/mas".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/mas"),
                    canonical_path: Some(PathBuf::from("/opt/homebrew/Cellar/mas/1.8.8/bin/mas")),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/mas")],
                    is_active: true,
                    version: Some("1.8.8".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.91,
                    decision_margin: Some(0.33),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for mas".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("mas install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Mas, false, false)
            .expect("mas uninstall plan should build from active strategy");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(uninstall.package.name, "mas");
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn podman_uninstall_plan_uses_active_homebrew_strategy() {
        let db_path = temp_db_path("podman-uninstall-active-homebrew");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Podman,
                &[ManagerInstallInstance {
                    manager: ManagerId::Podman,
                    instance_id: "podman-homebrew-active".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/podman/5.0.0/bin/podman".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/podman"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/podman/5.0.0/bin/podman",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/podman")],
                    is_active: true,
                    version: Some("5.0.0".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.91,
                    decision_margin: Some(0.34),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for podman".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.33),
                }],
            )
            .expect("podman install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Podman, false, false)
            .expect("podman uninstall plan should build from active strategy");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(uninstall.package.name, "podman");
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn podman_uninstall_plan_blocks_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("podman-uninstall-ambiguous-no-override");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Podman,
                &[ManagerInstallInstance {
                    manager: ManagerId::Podman,
                    instance_id: "podman-ambiguous".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.local/bin/podman".to_string(),
                    display_path: PathBuf::from("/Users/test/.local/bin/podman"),
                    canonical_path: Some(PathBuf::from("/Users/test/.local/bin/podman")),
                    alias_paths: vec![PathBuf::from("/Users/test/.local/bin/podman")],
                    is_active: true,
                    version: Some("5.0.0".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.48,
                    decision_margin: Some(0.06),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting podman provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.42),
                }],
            )
            .expect("podman install instance should persist");

        let error = super::build_manager_uninstall_plan(&store, ManagerId::Podman, false, false)
            .expect_err("ambiguous podman uninstall strategy should block without override");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("allow-unknown-provenance"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn one_to_one_homebrew_managers_uninstall_plan_uses_active_strategy() {
        let cases = [
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, formula_name) in cases {
            let db_path =
                temp_db_path(format!("{}-uninstall-active-homebrew", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-instance-uninstall", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ),
                        display_path: PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        )),
                        canonical_path: Some(PathBuf::from(format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ))),
                        alias_paths: vec![PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        ))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.90,
                        decision_margin: Some(0.30),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some(
                            "canonical path is inside Homebrew Cellar".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::SourceBuild),
                        competing_confidence: Some(0.35),
                    }],
                )
                .expect("manager install instance should persist");

            let plan = super::build_manager_uninstall_plan(&store, manager, false, false)
                .expect("uninstall plan should build");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            match plan.request {
                super::AdapterRequest::Uninstall(uninstall) => {
                    assert_eq!(uninstall.package.manager, ManagerId::HomebrewFormula);
                    assert_eq!(uninstall.package.name, formula_name);
                }
                other => panic!("unexpected request: {other:?}"),
            }
            assert_eq!(plan.preview.strategy, "homebrew_formula");
            assert!(!plan.preview.legacy_fallback_used);

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn one_to_one_homebrew_managers_uninstall_plan_blocks_ambiguous_without_override() {
        let cases = [
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, formula_name) in cases {
            let db_path = temp_db_path(
                format!("{}-uninstall-ambiguous-no-override", manager.as_str()).as_str(),
            );
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-ambiguous-uninstall", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ),
                        display_path: PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        )),
                        canonical_path: Some(PathBuf::from(format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ))),
                        alias_paths: vec![PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        ))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Unknown,
                        confidence: 0.49,
                        decision_margin: Some(0.05),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::InteractivePrompt,
                        update_strategy: StrategyKind::InteractivePrompt,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some(
                            "insufficient or conflicting provenance evidence".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.44),
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_uninstall_plan(&store, manager, false, false)
                .expect_err("ambiguous uninstall should block without override");
            assert!(error.contains("ambiguous"));
            assert!(error.contains("allow-unknown-provenance"));

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn one_to_one_homebrew_managers_uninstall_preview_allows_ambiguous_without_override() {
        let cases = [
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, formula_name) in cases {
            let db_path =
                temp_db_path(format!("{}-uninstall-ambiguous-preview", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-ambiguous-preview", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ),
                        display_path: PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        )),
                        canonical_path: Some(PathBuf::from(format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ))),
                        alias_paths: vec![PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        ))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Unknown,
                        confidence: 0.49,
                        decision_margin: Some(0.05),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::InteractivePrompt,
                        update_strategy: StrategyKind::InteractivePrompt,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some(
                            "insufficient or conflicting provenance evidence".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.44),
                    }],
                )
                .expect("manager install instance should persist");

            let plan = super::build_manager_uninstall_plan(&store, manager, false, true)
                .expect("preview should allow ambiguous uninstall routing");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            assert!(plan.preview.unknown_override_required);
            assert!(!plan.preview.used_unknown_override);
            assert_eq!(plan.preview.strategy, "homebrew_formula");
            assert!(!plan.preview.legacy_fallback_used);

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn one_to_one_homebrew_managers_uninstall_plan_allows_ambiguous_with_override() {
        let cases = [
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, formula_name) in cases {
            let db_path = temp_db_path(
                format!("{}-uninstall-ambiguous-with-override", manager.as_str()).as_str(),
            );
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-ambiguous-override", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ),
                        display_path: PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        )),
                        canonical_path: Some(PathBuf::from(format!(
                            "/opt/homebrew/Cellar/{}/1.0.0/bin/{}",
                            formula_name,
                            manager.as_str()
                        ))),
                        alias_paths: vec![PathBuf::from(format!(
                            "/opt/homebrew/bin/{}",
                            manager.as_str()
                        ))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Unknown,
                        confidence: 0.49,
                        decision_margin: Some(0.05),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::InteractivePrompt,
                        update_strategy: StrategyKind::InteractivePrompt,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some(
                            "insufficient or conflicting provenance evidence".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.44),
                    }],
                )
                .expect("manager install instance should persist");

            let plan = super::build_manager_uninstall_plan(&store, manager, true, false)
                .expect("override should allow ambiguous uninstall planning");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            assert!(plan.preview.unknown_override_required);
            assert!(plan.preview.used_unknown_override);
            assert_eq!(plan.preview.strategy, "homebrew_formula");
            assert!(!plan.preview.legacy_fallback_used);

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn one_to_one_homebrew_managers_uninstall_read_only_blocks_mutation_and_allows_preview() {
        let cases = [
            (ManagerId::Pnpm, "pnpm"),
            (ManagerId::Yarn, "yarn"),
            (ManagerId::Pipx, "pipx"),
            (ManagerId::Poetry, "poetry"),
            (ManagerId::CargoBinstall, "cargo-binstall"),
            (ManagerId::Podman, "podman"),
            (ManagerId::Colima, "colima"),
        ];

        for (manager, executable_name) in cases {
            let db_path =
                temp_db_path(format!("{}-uninstall-read-only", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-read-only-uninstall", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!("/usr/bin/{}", executable_name),
                        display_path: PathBuf::from(format!("/usr/bin/{}", executable_name)),
                        canonical_path: Some(PathBuf::from(format!(
                            "/usr/bin/{}",
                            executable_name
                        ))),
                        alias_paths: vec![PathBuf::from(format!("/usr/bin/{}", executable_name))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::System,
                        confidence: 0.95,
                        decision_margin: Some(0.50),
                        automation_level: AutomationLevel::ReadOnly,
                        uninstall_strategy: StrategyKind::ReadOnly,
                        update_strategy: StrategyKind::ReadOnly,
                        remediation_strategy: StrategyKind::ReadOnly,
                        explanation_primary: Some(
                            "system installation is read-only in Helm".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.20),
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_uninstall_plan(&store, manager, false, false)
                .expect_err("read-only uninstall should block mutation");
            assert!(error.contains("read-only"));

            let plan = super::build_manager_uninstall_plan(&store, manager, false, true)
                .expect("preview should remain available for read-only uninstall");
            assert_eq!(plan.target_manager, manager);
            match plan.request {
                super::AdapterRequest::Uninstall(uninstall) => {
                    assert_eq!(uninstall.package.manager, manager);
                    assert_eq!(uninstall.package.name, "__self__");
                }
                other => panic!("unexpected request: {other:?}"),
            }
            assert_eq!(plan.preview.strategy, "read_only");
            assert!(plan.preview.read_only_blocked);
            assert!(!plan.preview.legacy_fallback_used);

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn one_to_one_homebrew_manager_uninstall_read_only_blocks_mutation_and_allows_preview() {
        let db_path = temp_db_path("yarn-uninstall-read-only");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Yarn,
                &[ManagerInstallInstance {
                    manager: ManagerId::Yarn,
                    instance_id: "yarn-read-only".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/usr/bin/yarn".to_string(),
                    display_path: PathBuf::from("/usr/bin/yarn"),
                    canonical_path: Some(PathBuf::from("/usr/bin/yarn")),
                    alias_paths: vec![PathBuf::from("/usr/bin/yarn")],
                    is_active: true,
                    version: Some("1.22.22".to_string()),
                    provenance: InstallProvenance::System,
                    confidence: 0.95,
                    decision_margin: Some(0.50),
                    automation_level: AutomationLevel::ReadOnly,
                    uninstall_strategy: StrategyKind::ReadOnly,
                    update_strategy: StrategyKind::ReadOnly,
                    remediation_strategy: StrategyKind::ReadOnly,
                    explanation_primary: Some(
                        "system yarn installation is read-only in Helm".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.20),
                }],
            )
            .expect("yarn install instance should persist");

        let error = super::build_manager_uninstall_plan(&store, ManagerId::Yarn, false, false)
            .expect_err("read-only uninstall should block mutation");
        assert!(error.contains("read-only"));

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Yarn, false, true)
            .expect("preview should remain available for read-only uninstall");
        assert_eq!(plan.target_manager, ManagerId::Yarn);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::Yarn);
                assert_eq!(uninstall.package.name, "__self__");
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert_eq!(plan.preview.strategy, "read_only");
        assert!(plan.preview.read_only_blocked);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn npm_uninstall_plan_uses_homebrew_parent_formula_from_active_instance() {
        let db_path = temp_db_path("npm-uninstall-homebrew-parent-formula");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Npm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Npm,
                    instance_id: "npm-instance-uninstall".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/node/22.14.0/bin/npm".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/npm"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/node/22.14.0/bin/npm",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/npm")],
                    is_active: true,
                    version: Some("10.9.2".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.89,
                    decision_margin: Some(0.28),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "canonical path is inside Homebrew Cellar for node".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::SourceBuild),
                    competing_confidence: Some(0.35),
                }],
            )
            .expect("npm install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Npm, false, false)
            .expect("npm uninstall plan should build");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::HomebrewFormula);
                assert_eq!(uninstall.package.name, "node");
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn dynamic_homebrew_parent_managers_uninstall_plan_resolves_parent_formula() {
        let cases = [
            (
                ManagerId::Pip,
                "/opt/homebrew/Cellar/python@3.12/3.12.9_1/bin/pip3",
                "/opt/homebrew/bin/pip3",
                "python@3.12",
            ),
            (
                ManagerId::RubyGems,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/gem",
                "/opt/homebrew/bin/gem",
                "ruby",
            ),
            (
                ManagerId::Bundler,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/bundle3.4",
                "/opt/homebrew/bin/bundle3.4",
                "ruby",
            ),
            (
                ManagerId::Cargo,
                "/opt/homebrew/Cellar/rust/1.86.0/bin/cargo",
                "/opt/homebrew/bin/cargo",
                "rust",
            ),
        ];

        for (manager, canonical_path, display_path, expected_formula) in cases {
            let db_path = temp_db_path(
                format!("{}-uninstall-homebrew-parent-formula", manager.as_str()).as_str(),
            );
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-instance-uninstall", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: canonical_path.to_string(),
                        display_path: PathBuf::from(display_path),
                        canonical_path: Some(PathBuf::from(canonical_path)),
                        alias_paths: vec![PathBuf::from(display_path)],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.89,
                        decision_margin: Some(0.28),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some(
                            "canonical path is inside Homebrew Cellar".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::SourceBuild),
                        competing_confidence: Some(0.35),
                    }],
                )
                .expect("manager install instance should persist");

            let plan = super::build_manager_uninstall_plan(&store, manager, false, false)
                .expect("dynamic manager uninstall plan should build");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            match plan.request {
                super::AdapterRequest::Uninstall(uninstall) => {
                    assert_eq!(uninstall.package.manager, ManagerId::HomebrewFormula);
                    assert_eq!(uninstall.package.name, expected_formula);
                }
                other => panic!("unexpected request: {other:?}"),
            }

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn npm_uninstall_plan_errors_when_formula_ownership_is_unresolved() {
        let db_path = temp_db_path("npm-uninstall-homebrew-formula-unresolved");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Npm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Npm,
                    instance_id: "npm-unresolved-uninstall".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/bin/npm".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/npm"),
                    canonical_path: Some(PathBuf::from("/opt/homebrew/bin/npm")),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/npm")],
                    is_active: true,
                    version: Some("10.9.2".to_string()),
                    provenance: InstallProvenance::Homebrew,
                    confidence: 0.80,
                    decision_margin: Some(0.20),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::HomebrewFormula,
                    update_strategy: StrategyKind::HomebrewFormula,
                    remediation_strategy: StrategyKind::HomebrewFormula,
                    explanation_primary: Some(
                        "npm path indicates Homebrew ownership but formula could not be resolved"
                            .to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: None,
                    competing_confidence: None,
                }],
            )
            .expect("npm install instance should persist");

        let error = super::build_manager_uninstall_plan(&store, ManagerId::Npm, false, false)
            .expect_err("npm uninstall should fail when formula ownership cannot be derived");
        assert!(error.contains("formula ownership could not be determined"));
        assert!(error.contains("helm managers instances npm"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn dynamic_homebrew_parent_managers_uninstall_plan_errors_when_formula_unresolved() {
        let cases = [
            (
                ManagerId::Pip,
                "/opt/homebrew/bin/pip3",
                "/opt/homebrew/bin/pip3",
            ),
            (
                ManagerId::RubyGems,
                "/opt/homebrew/bin/gem",
                "/opt/homebrew/bin/gem",
            ),
            (
                ManagerId::Bundler,
                "/opt/homebrew/bin/bundle3.4",
                "/opt/homebrew/bin/bundle3.4",
            ),
            (
                ManagerId::Cargo,
                "/opt/homebrew/bin/cargo",
                "/opt/homebrew/bin/cargo",
            ),
        ];

        for (manager, canonical_path, display_path) in cases {
            let db_path = temp_db_path(
                format!("{}-uninstall-homebrew-formula-unresolved", manager.as_str()).as_str(),
            );
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-unresolved-uninstall", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: canonical_path.to_string(),
                        display_path: PathBuf::from(display_path),
                        canonical_path: Some(PathBuf::from(canonical_path)),
                        alias_paths: vec![PathBuf::from(display_path)],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.80,
                        decision_margin: Some(0.20),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some(
                            "path indicates Homebrew ownership but formula could not be resolved"
                                .to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: None,
                        competing_confidence: None,
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_uninstall_plan(&store, manager, false, false)
                .expect_err("uninstall should fail when formula ownership cannot be derived");
            assert!(error.contains("formula ownership could not be determined"));
            assert!(error.contains(&format!("helm managers instances {}", manager.as_str())));

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn dynamic_homebrew_parent_managers_uninstall_plan_blocks_ambiguous_without_override() {
        let cases = [
            (
                ManagerId::Pip,
                "/opt/homebrew/Cellar/python@3.12/3.12.9_1/bin/pip3",
                "/opt/homebrew/bin/pip3",
            ),
            (
                ManagerId::RubyGems,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/gem",
                "/opt/homebrew/bin/gem",
            ),
            (
                ManagerId::Bundler,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/bundle3.4",
                "/opt/homebrew/bin/bundle3.4",
            ),
            (
                ManagerId::Cargo,
                "/opt/homebrew/Cellar/rust/1.86.0/bin/cargo",
                "/opt/homebrew/bin/cargo",
            ),
        ];

        for (manager, canonical_path, display_path) in cases {
            let db_path = temp_db_path(
                format!("{}-uninstall-ambiguous-no-override", manager.as_str()).as_str(),
            );
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-ambiguous-uninstall", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: canonical_path.to_string(),
                        display_path: PathBuf::from(display_path),
                        canonical_path: Some(PathBuf::from(canonical_path)),
                        alias_paths: vec![PathBuf::from(display_path)],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Unknown,
                        confidence: 0.49,
                        decision_margin: Some(0.04),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::InteractivePrompt,
                        update_strategy: StrategyKind::InteractivePrompt,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some(
                            "insufficient or conflicting provenance evidence".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.45),
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_uninstall_plan(&store, manager, false, false)
                .expect_err("ambiguous uninstall strategy should block without override");
            assert!(error.contains("ambiguous"));
            assert!(error.contains("allow-unknown-provenance"));

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn dynamic_homebrew_parent_managers_uninstall_preview_allows_ambiguous_without_override() {
        let cases = [
            (
                ManagerId::Pip,
                "/opt/homebrew/Cellar/python@3.12/3.12.9_1/bin/pip3",
                "/opt/homebrew/bin/pip3",
            ),
            (
                ManagerId::RubyGems,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/gem",
                "/opt/homebrew/bin/gem",
            ),
            (
                ManagerId::Bundler,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/bundle3.4",
                "/opt/homebrew/bin/bundle3.4",
            ),
            (
                ManagerId::Cargo,
                "/opt/homebrew/Cellar/rust/1.86.0/bin/cargo",
                "/opt/homebrew/bin/cargo",
            ),
        ];

        for (manager, canonical_path, display_path) in cases {
            let db_path =
                temp_db_path(format!("{}-uninstall-ambiguous-preview", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-ambiguous-preview", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: canonical_path.to_string(),
                        display_path: PathBuf::from(display_path),
                        canonical_path: Some(PathBuf::from(canonical_path)),
                        alias_paths: vec![PathBuf::from(display_path)],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Unknown,
                        confidence: 0.49,
                        decision_margin: Some(0.04),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::InteractivePrompt,
                        update_strategy: StrategyKind::InteractivePrompt,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some(
                            "insufficient or conflicting provenance evidence".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.45),
                    }],
                )
                .expect("manager install instance should persist");

            let plan = super::build_manager_uninstall_plan(&store, manager, false, true)
                .expect("preview path should allow ambiguous uninstall routing");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            assert!(plan.preview.unknown_override_required);
            assert!(!plan.preview.used_unknown_override);
            assert_eq!(plan.preview.strategy, "homebrew_formula");
            assert!(!plan.preview.legacy_fallback_used);

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn dynamic_homebrew_parent_managers_uninstall_plan_allows_ambiguous_with_override() {
        let cases = [
            (
                ManagerId::Pip,
                "/opt/homebrew/Cellar/python@3.12/3.12.9_1/bin/pip3",
                "/opt/homebrew/bin/pip3",
            ),
            (
                ManagerId::RubyGems,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/gem",
                "/opt/homebrew/bin/gem",
            ),
            (
                ManagerId::Bundler,
                "/opt/homebrew/Cellar/ruby/3.4.1/bin/bundle3.4",
                "/opt/homebrew/bin/bundle3.4",
            ),
            (
                ManagerId::Cargo,
                "/opt/homebrew/Cellar/rust/1.86.0/bin/cargo",
                "/opt/homebrew/bin/cargo",
            ),
        ];

        for (manager, canonical_path, display_path) in cases {
            let db_path = temp_db_path(
                format!("{}-uninstall-ambiguous-with-override", manager.as_str()).as_str(),
            );
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .upsert_detection(
                    ManagerId::HomebrewFormula,
                    &DetectionInfo {
                        installed: true,
                        executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                        version: Some("4.4.0".to_string()),
                    },
                )
                .expect("homebrew detection should persist");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-ambiguous-override", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: canonical_path.to_string(),
                        display_path: PathBuf::from(display_path),
                        canonical_path: Some(PathBuf::from(canonical_path)),
                        alias_paths: vec![PathBuf::from(display_path)],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::Unknown,
                        confidence: 0.49,
                        decision_margin: Some(0.04),
                        automation_level: AutomationLevel::NeedsConfirmation,
                        uninstall_strategy: StrategyKind::InteractivePrompt,
                        update_strategy: StrategyKind::InteractivePrompt,
                        remediation_strategy: StrategyKind::ManualRemediation,
                        explanation_primary: Some(
                            "insufficient or conflicting provenance evidence".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.45),
                    }],
                )
                .expect("manager install instance should persist");

            let plan = super::build_manager_uninstall_plan(&store, manager, true, false)
                .expect("override should allow ambiguous uninstall strategy planning");
            assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
            assert!(plan.preview.unknown_override_required);
            assert!(plan.preview.used_unknown_override);
            assert_eq!(plan.preview.strategy, "homebrew_formula");
            assert!(!plan.preview.legacy_fallback_used);

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn dynamic_homebrew_parent_managers_uninstall_read_only_blocks_mutation_and_allows_preview() {
        let cases = [
            (ManagerId::Npm, "npm"),
            (ManagerId::Pip, "pip3"),
            (ManagerId::RubyGems, "gem"),
            (ManagerId::Bundler, "bundle"),
            (ManagerId::Cargo, "cargo"),
        ];

        for (manager, executable_name) in cases {
            let db_path =
                temp_db_path(format!("{}-uninstall-read-only", manager.as_str()).as_str());
            let store = SqliteStore::new(&db_path);
            store
                .migrate_to_latest()
                .expect("store migration should succeed");
            store
                .replace_install_instances(
                    manager,
                    &[ManagerInstallInstance {
                        manager,
                        instance_id: format!("{}-read-only-uninstall", manager.as_str()),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: format!("/usr/bin/{}", executable_name),
                        display_path: PathBuf::from(format!("/usr/bin/{}", executable_name)),
                        canonical_path: Some(PathBuf::from(format!(
                            "/usr/bin/{}",
                            executable_name
                        ))),
                        alias_paths: vec![PathBuf::from(format!("/usr/bin/{}", executable_name))],
                        is_active: true,
                        version: Some("1.0.0".to_string()),
                        provenance: InstallProvenance::System,
                        confidence: 0.94,
                        decision_margin: Some(0.48),
                        automation_level: AutomationLevel::ReadOnly,
                        uninstall_strategy: StrategyKind::ReadOnly,
                        update_strategy: StrategyKind::ReadOnly,
                        remediation_strategy: StrategyKind::ReadOnly,
                        explanation_primary: Some(
                            "system installation is read-only in Helm".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.21),
                    }],
                )
                .expect("manager install instance should persist");

            let error = super::build_manager_uninstall_plan(&store, manager, false, false)
                .expect_err("read-only uninstall should block mutation");
            assert!(error.contains("read-only"));

            let plan = super::build_manager_uninstall_plan(&store, manager, false, true)
                .expect("preview should remain available for read-only uninstall");
            assert_eq!(plan.target_manager, manager);
            assert_eq!(plan.preview.strategy, "read_only");
            assert!(plan.preview.read_only_blocked);
            assert!(!plan.preview.legacy_fallback_used);

            let _ = fs::remove_file(db_path);
        }
    }

    #[test]
    fn npm_uninstall_plan_blocks_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("npm-uninstall-ambiguous-no-override");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Npm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Npm,
                    instance_id: "npm-ambiguous".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/node/22.14.0/bin/npm".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/npm"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/node/22.14.0/bin/npm",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/npm")],
                    is_active: true,
                    version: Some("10.9.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.49,
                    decision_margin: Some(0.04),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting npm provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.45),
                }],
            )
            .expect("npm install instance should persist");

        let error = super::build_manager_uninstall_plan(&store, ManagerId::Npm, false, false)
            .expect_err("ambiguous npm uninstall strategy should block without override");
        assert!(error.contains("ambiguous"));
        assert!(error.contains("allow-unknown-provenance"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn npm_uninstall_preview_allows_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("npm-uninstall-ambiguous-preview");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Npm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Npm,
                    instance_id: "npm-ambiguous-preview".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/node/22.14.0/bin/npm".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/npm"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/node/22.14.0/bin/npm",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/npm")],
                    is_active: true,
                    version: Some("10.9.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.49,
                    decision_margin: Some(0.04),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting npm provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.45),
                }],
            )
            .expect("npm install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Npm, false, true)
            .expect("preview path should allow ambiguous uninstall routing");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        assert!(plan.preview.unknown_override_required);
        assert!(!plan.preview.used_unknown_override);
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn npm_uninstall_plan_allows_ambiguous_provenance_with_override() {
        let db_path = temp_db_path("npm-uninstall-ambiguous-with-override");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::HomebrewFormula,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/brew")),
                    version: Some("4.4.0".to_string()),
                },
            )
            .expect("homebrew detection should persist");
        store
            .replace_install_instances(
                ManagerId::Npm,
                &[ManagerInstallInstance {
                    manager: ManagerId::Npm,
                    instance_id: "npm-ambiguous-override".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/opt/homebrew/Cellar/node/22.14.0/bin/npm".to_string(),
                    display_path: PathBuf::from("/opt/homebrew/bin/npm"),
                    canonical_path: Some(PathBuf::from(
                        "/opt/homebrew/Cellar/node/22.14.0/bin/npm",
                    )),
                    alias_paths: vec![PathBuf::from("/opt/homebrew/bin/npm")],
                    is_active: true,
                    version: Some("10.9.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.49,
                    decision_margin: Some(0.04),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting npm provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.45),
                }],
            )
            .expect("npm install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Npm, true, false)
            .expect("override should allow ambiguous uninstall strategy planning");
        assert_eq!(plan.target_manager, ManagerId::HomebrewFormula);
        assert!(plan.preview.unknown_override_required);
        assert!(plan.preview.used_unknown_override);
        assert_eq!(plan.preview.strategy, "homebrew_formula");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_uninstall_request_defaults_to_rustup_self_without_method_routing() {
        let db_path = temp_db_path("rustup-uninstall-default-self");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .set_manager_selected_install_method(ManagerId::Rustup, Some("homebrew"))
            .expect("rustup install method preference should persist");

        let (target_manager, request) =
            super::build_manager_mutation_request(&store, ManagerId::Rustup, "uninstall", None)
                .expect("rustup uninstall fallback should route to rustup self");
        assert_eq!(target_manager, ManagerId::Rustup);
        match request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::Rustup);
                assert_eq!(uninstall.package.name, "__self__");
            }
            other => panic!("unexpected request: {other:?}"),
        }

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_uninstall_plan_uses_active_instance_in_multi_install() {
        let db_path = temp_db_path("rustup-uninstall-multi-install");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "rustup-self-active".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                        display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                        canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                        alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                        is_active: true,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::RustupInit,
                        confidence: 0.92,
                        decision_margin: Some(0.30),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::RustupSelf,
                        update_strategy: StrategyKind::RustupSelf,
                        remediation_strategy: StrategyKind::RustupSelf,
                        explanation_primary: Some(
                            "active path is under CARGO_HOME/bin".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.45),
                    },
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "rustup-homebrew-inactive".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup".to_string(),
                        display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
                        canonical_path: Some(PathBuf::from(
                            "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
                        )),
                        alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
                        is_active: false,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.93,
                        decision_margin: Some(0.31),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some(
                            "inactive path is in Homebrew Cellar".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::RustupInit),
                        competing_confidence: Some(0.41),
                    },
                ],
            )
            .expect("install instances should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Rustup, false, false)
            .expect("multi-install uninstall plan should build");
        assert_eq!(plan.target_manager, ManagerId::Rustup);
        assert_eq!(plan.preview.strategy, "rustup_self");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_uninstall_plan_prefers_provenance_strategy_over_install_method() {
        let db_path = temp_db_path("rustup-uninstall-provenance-strategy");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .set_manager_selected_install_method(ManagerId::Rustup, Some("homebrew"))
            .expect("rustup install method preference should persist");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-instance".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::RustupInit,
                    confidence: 0.92,
                    decision_margin: Some(0.35),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::RustupSelf,
                    update_strategy: StrategyKind::RustupSelf,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some("rustup binary is under CARGO_HOME/bin".to_string()),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::Homebrew),
                    competing_confidence: Some(0.51),
                }],
            )
            .expect("install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Rustup, false, false)
            .expect("rustup uninstall plan should build");
        assert_eq!(plan.target_manager, ManagerId::Rustup);
        match plan.request {
            super::AdapterRequest::Uninstall(uninstall) => {
                assert_eq!(uninstall.package.manager, ManagerId::Rustup);
                assert_eq!(uninstall.package.name, "__self__");
            }
            other => panic!("unexpected request: {other:?}"),
        }
        assert_eq!(plan.preview.strategy, "rustup_self");
        assert!(!plan.preview.legacy_fallback_used);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_uninstall_plan_blocks_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("rustup-uninstall-unknown-no-override");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-unknown".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.44,
                    decision_margin: Some(0.04),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting rustup provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::RustupInit),
                    competing_confidence: Some(0.40),
                }],
            )
            .expect("install instance should persist");

        let error = super::build_manager_uninstall_plan(&store, ManagerId::Rustup, false, false)
            .expect_err("ambiguous uninstall strategy should block without override");
        assert!(error.contains("ambiguous"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_uninstall_preview_allows_ambiguous_provenance_without_override() {
        let db_path = temp_db_path("rustup-uninstall-unknown-preview");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-unknown-preview".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.44,
                    decision_margin: Some(0.04),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting rustup provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::RustupInit),
                    competing_confidence: Some(0.40),
                }],
            )
            .expect("install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Rustup, false, true)
            .expect("preview path should allow ambiguous uninstall routing");
        assert_eq!(plan.target_manager, ManagerId::Rustup);
        assert!(plan.preview.unknown_provenance);
        assert!(plan.preview.unknown_override_required);
        assert!(!plan.preview.used_unknown_override);
        assert!(plan.preview.requires_yes);
        assert!(
            plan.preview
                .summary_lines
                .iter()
                .any(|line| line.contains("requires explicit override"))
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_uninstall_preview_requires_yes_for_high_blast_radius() {
        let db_path = temp_db_path("rustup-uninstall-yes-gate");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-self".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::RustupInit,
                    confidence: 0.91,
                    decision_margin: Some(0.31),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::RustupSelf,
                    update_strategy: StrategyKind::RustupSelf,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "active rustup looks rustup-init managed".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: None,
                    competing_confidence: None,
                }],
            )
            .expect("install instance should persist");

        let plan = super::build_manager_uninstall_plan(&store, ManagerId::Rustup, false, false)
            .expect("rustup uninstall plan should build");
        assert!(plan.preview.requires_yes);
        assert!(
            plan.preview.blast_radius_score
                >= super::DEFAULT_MANAGER_UNINSTALL_SAFE_BLAST_RADIUS_THRESHOLD
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn rustup_uninstall_preview_generation_is_deterministic() {
        let db_path = temp_db_path("rustup-uninstall-preview-deterministic");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-deterministic".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::RustupInit,
                    confidence: 0.93,
                    decision_margin: Some(0.30),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::RustupSelf,
                    update_strategy: StrategyKind::RustupSelf,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "active rustup looks rustup-init managed".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: None,
                    competing_confidence: None,
                }],
            )
            .expect("install instance should persist");

        let first = super::build_manager_uninstall_plan(&store, ManagerId::Rustup, false, true)
            .expect("first preview should build")
            .preview;
        let second = super::build_manager_uninstall_plan(&store, ManagerId::Rustup, false, true)
            .expect("second preview should build")
            .preview;

        assert_eq!(first, second);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn managers_uninstall_requires_yes_message_includes_preview_hint() {
        let db_path = temp_db_path("managers-uninstall-yes-hint");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-self".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::RustupInit,
                    confidence: 0.92,
                    decision_margin: Some(0.32),
                    automation_level: AutomationLevel::Automatic,
                    uninstall_strategy: StrategyKind::RustupSelf,
                    update_strategy: StrategyKind::RustupSelf,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "active rustup looks rustup-init managed".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: None,
                    competing_confidence: None,
                }],
            )
            .expect("install instance should persist");

        let error = super::cmd_managers_mutation(
            Arc::new(store),
            GlobalOptions::default(),
            "uninstall",
            &["rustup".to_string()],
        )
        .expect_err("managers uninstall should require --yes");
        assert!(error.contains("requires --yes"));
        assert!(error.contains("helm managers uninstall <manager-id> --preview"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn managers_uninstall_unknown_provenance_requires_override_hint() {
        let db_path = temp_db_path("managers-uninstall-unknown-override-hint");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-unknown".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.49,
                    decision_margin: Some(0.05),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting rustup provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::RustupInit),
                    competing_confidence: Some(0.44),
                }],
            )
            .expect("install instance should persist");

        let error = super::cmd_managers_mutation(
            Arc::new(store),
            GlobalOptions::default(),
            "uninstall",
            &["rustup".to_string(), "--yes".to_string()],
        )
        .expect_err("ambiguous uninstall should require explicit unknown override");
        assert!(error.contains("allow-unknown-provenance"));
        assert!(error.contains("--preview"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn packages_uninstall_requires_yes_message_includes_preview_hint() {
        let db_path = temp_db_path("packages-uninstall-yes-hint");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let error = super::cmd_packages_mutation(
            Arc::new(store),
            GlobalOptions::default(),
            "uninstall",
            &[
                "git".to_string(),
                "--manager".to_string(),
                "homebrew_formula".to_string(),
            ],
        )
        .expect_err("packages uninstall should require --yes for homebrew blast radius");
        assert!(error.contains("requires --yes"));
        assert!(
            error.contains("helm packages uninstall <name|name@manager> --manager <id> --preview")
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn managers_uninstall_preview_smoke_succeeds_for_rustup_multi_install() {
        let db_path = temp_db_path("managers-uninstall-preview-rustup-multi-install");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "rustup-homebrew-active".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup".to_string(),
                        display_path: PathBuf::from("/opt/homebrew/bin/rustup"),
                        canonical_path: Some(PathBuf::from(
                            "/opt/homebrew/Cellar/rustup/1.28.2/bin/rustup",
                        )),
                        alias_paths: vec![PathBuf::from("/opt/homebrew/bin/rustup")],
                        is_active: true,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::Homebrew,
                        confidence: 0.93,
                        decision_margin: Some(0.31),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::HomebrewFormula,
                        update_strategy: StrategyKind::HomebrewFormula,
                        remediation_strategy: StrategyKind::HomebrewFormula,
                        explanation_primary: Some("active path is in Homebrew Cellar".to_string()),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::RustupInit),
                        competing_confidence: Some(0.41),
                    },
                    ManagerInstallInstance {
                        manager: ManagerId::Rustup,
                        instance_id: "rustup-self-inactive".to_string(),
                        identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                        identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                        display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                        canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                        alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                        is_active: false,
                        version: Some("1.28.2".to_string()),
                        provenance: InstallProvenance::RustupInit,
                        confidence: 0.92,
                        decision_margin: Some(0.30),
                        automation_level: AutomationLevel::Automatic,
                        uninstall_strategy: StrategyKind::RustupSelf,
                        update_strategy: StrategyKind::RustupSelf,
                        remediation_strategy: StrategyKind::RustupSelf,
                        explanation_primary: Some(
                            "inactive path is under CARGO_HOME/bin".to_string(),
                        ),
                        explanation_secondary: None,
                        competing_provenance: Some(InstallProvenance::Homebrew),
                        competing_confidence: Some(0.45),
                    },
                ],
            )
            .expect("install instances should persist");

        super::cmd_managers_mutation(
            Arc::new(store),
            GlobalOptions::default(),
            "uninstall",
            &["rustup".to_string(), "--preview".to_string()],
        )
        .expect("preview should succeed for rustup multi-install state");

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn managers_uninstall_preview_smoke_succeeds_for_unknown_provenance() {
        let db_path = temp_db_path("managers-uninstall-preview-rustup-unknown");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-unknown".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.44,
                    decision_margin: Some(0.04),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting rustup provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::RustupInit),
                    competing_confidence: Some(0.40),
                }],
            )
            .expect("install instance should persist");

        super::cmd_managers_mutation(
            Arc::new(store),
            GlobalOptions::default(),
            "uninstall",
            &["rustup".to_string(), "--preview".to_string()],
        )
        .expect("preview should succeed for unknown provenance");

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn managers_uninstall_unknown_provenance_without_preview_requires_override_hint() {
        let db_path = temp_db_path("managers-uninstall-unknown-no-preview-override-hint");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .replace_install_instances(
                ManagerId::Rustup,
                &[ManagerInstallInstance {
                    manager: ManagerId::Rustup,
                    instance_id: "rustup-unknown-no-preview".to_string(),
                    identity_kind: InstallInstanceIdentityKind::CanonicalPath,
                    identity_value: "/Users/test/.cargo/bin/rustup".to_string(),
                    display_path: PathBuf::from("/Users/test/.cargo/bin/rustup"),
                    canonical_path: Some(PathBuf::from("/Users/test/.cargo/bin/rustup")),
                    alias_paths: vec![PathBuf::from("/Users/test/.cargo/bin/rustup")],
                    is_active: true,
                    version: Some("1.28.2".to_string()),
                    provenance: InstallProvenance::Unknown,
                    confidence: 0.44,
                    decision_margin: Some(0.04),
                    automation_level: AutomationLevel::NeedsConfirmation,
                    uninstall_strategy: StrategyKind::InteractivePrompt,
                    update_strategy: StrategyKind::InteractivePrompt,
                    remediation_strategy: StrategyKind::ManualRemediation,
                    explanation_primary: Some(
                        "insufficient or conflicting rustup provenance evidence".to_string(),
                    ),
                    explanation_secondary: None,
                    competing_provenance: Some(InstallProvenance::RustupInit),
                    competing_confidence: Some(0.40),
                }],
            )
            .expect("install instance should persist");

        let error = super::cmd_managers_mutation(
            Arc::new(store),
            GlobalOptions::default(),
            "uninstall",
            &["rustup".to_string()],
        )
        .expect_err("unknown provenance without preview/override should fail");
        assert!(error.contains("allow-unknown-provenance"));
        assert!(error.contains("--preview"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn parse_manager_id_unknown_includes_managers_list_hint() {
        let error = parse_manager_id("nope").expect_err("unknown manager id should fail");
        assert!(error.contains("unknown manager id"));
        assert!(error.contains("helm managers list"));
    }

    #[test]
    fn updates_run_requires_yes_message_includes_preview_hint() {
        let db_path = temp_db_path("updates-run-yes-hint");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let error = cmd_updates_run(Arc::new(store), GlobalOptions::default(), &[])
            .expect_err("updates run should require --yes");
        assert!(error.contains("requires --yes"));
        assert!(error.contains("helm updates preview"));

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn updates_run_mixed_success_uses_stable_exit_code_marker() {
        let outcomes = [true, false, true];
        let failures = outcomes.iter().filter(|success| !**success).count();
        let marked = manager_operation_failure_error("upgrade", failures)
            .expect("mixed success should emit failure marker");
        let (code, message) = strip_exit_code_marker(marked.as_str());
        assert_eq!(code, Some(2));
        assert_eq!(message, "1 manager upgrade operations failed");
    }

    #[test]
    fn updates_run_json_envelope_schema_is_stable() {
        let payloads = build_json_payload_lines(
            "helm.cli.v1.updates.run",
            json!({
                "include_pinned": false,
                "allow_os_updates": false,
                "manager_filter": null,
                "results": [
                    {
                        "step_id": "npm:eslint",
                        "manager_id": "npm",
                        "package_name": "eslint",
                        "task_id": 42,
                        "success": true,
                        "error": null
                    }
                ],
                "total_steps": 1,
                "failed_steps": 0
            }),
            false,
            123,
        );
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["schema"], "helm.cli.v1.updates.run");
        assert_eq!(payloads[0]["schema_version"], 1);
        assert_eq!(payloads[0]["data"]["results"][0]["manager_id"], "npm");
        assert_eq!(payloads[0]["data"]["total_steps"], 1);
        assert_eq!(payloads[0]["data"]["failed_steps"], 0);
    }

    #[test]
    fn tasks_follow_machine_mode_error_contract_is_stable() {
        let marked = super::tasks_follow_machine_mode_error();
        let (exit_code, message) = strip_exit_code_marker(marked.as_str());
        assert_eq!(exit_code, Some(1));
        assert_eq!(message, super::TASKS_FOLLOW_MACHINE_MODE_UNSUPPORTED_ERROR);
        assert!(message.contains("--json/--ndjson"));
        assert!(message.contains("helm tasks logs <task-id>"));
    }

    #[test]
    fn cmd_tasks_follow_rejects_machine_mode_with_stable_exit_code() {
        let db_path = temp_db_path("tasks-follow-machine-mode");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        let args = vec!["42".to_string()];
        let options = GlobalOptions {
            json: true,
            ndjson: true,
            ..GlobalOptions::default()
        };

        let error = super::cmd_tasks_follow(&store, options, &args)
            .expect_err("tasks follow machine mode should fail deterministically");
        let (exit_code, message) = strip_exit_code_marker(error.as_str());
        assert_eq!(exit_code, Some(1));
        assert_eq!(message, super::TASKS_FOLLOW_MACHINE_MODE_UNSUPPORTED_ERROR);

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn parse_homebrew_keg_policy_arg_supports_expected_values() {
        assert_eq!(
            parse_homebrew_keg_policy_arg("keep").unwrap(),
            Some(HomebrewKegPolicy::Keep)
        );
        assert_eq!(
            parse_homebrew_keg_policy_arg("cleanup").unwrap(),
            Some(HomebrewKegPolicy::Cleanup)
        );
        assert_eq!(parse_homebrew_keg_policy_arg("default").unwrap(), None);
        assert!(parse_homebrew_keg_policy_arg("invalid").is_err());
    }

    #[test]
    fn coordinator_ps_command_path_is_absolute() {
        assert_eq!(super::coordinator_transport::PS_COMMAND_PATH, "/bin/ps");
    }

    #[test]
    fn resolve_redirect_url_supports_relative_targets() {
        let absolute_path = resolve_redirect_url(
            "https://github.com/jasoncavinder/Helm/releases/latest",
            "/jasoncavinder/Helm/releases/download/v0.17.6/helm",
        )
        .expect("absolute redirect path should resolve");
        assert_eq!(
            absolute_path,
            "https://github.com/jasoncavinder/Helm/releases/download/v0.17.6/helm"
        );

        let relative_path = resolve_redirect_url(
            "https://github.com/jasoncavinder/Helm/releases/latest?foo=bar",
            "download/v0.17.6/helm",
        )
        .expect("relative redirect path should resolve");
        assert_eq!(
            relative_path,
            "https://github.com/jasoncavinder/Helm/releases/download/v0.17.6/helm"
        );
    }

    #[test]
    fn resolve_update_redirect_target_rejects_disallowed_hosts() {
        let error = resolve_update_redirect_target(
            "https://github.com/jasoncavinder/Helm/releases/latest",
            "https://evil.example.com/update.json",
            "endpoint",
            Some("https://github.com/jasoncavinder/Helm/releases/latest"),
            SelfUpdateErrorKind::ManifestHttp,
        )
        .expect_err("disallowed redirect host must fail");
        assert_eq!(error.kind, SelfUpdateErrorKind::UrlPolicy);
        assert!(
            error.message.contains("not allowlisted"),
            "unexpected message: {}",
            error.message
        );
    }

    #[test]
    fn read_update_bytes_with_limit_rejects_oversized_payload() {
        let payload = vec![0x5Au8; 9];
        let error = read_update_bytes_with_limit(
            Cursor::new(payload),
            8,
            "https://github.com/jasoncavinder/Helm/releases/download/v0.17.6/helm",
        )
        .expect_err("oversized payload must fail");
        assert_eq!(error.kind, SelfUpdateErrorKind::AssetContract);
        assert!(
            error.message.contains("exceeds maximum allowed size"),
            "unexpected message: {}",
            error.message
        );
    }

    #[test]
    fn self_update_policy_blocks_channel_managed_paths() {
        assert!(!provenance_can_self_update(UpdatePolicy::ChannelManaged));
        assert!(!provenance_can_self_update(UpdatePolicy::Managed));
        assert!(!provenance_can_self_update(UpdatePolicy::None));
        assert!(provenance_can_self_update(UpdatePolicy::SelfManaged));
    }

    #[test]
    fn self_uninstall_recommended_actions_match_channel_contract() {
        let executable_path = Path::new("/tmp/helm");
        assert_eq!(
            self_uninstall_recommended_action(InstallChannel::DirectScript, executable_path),
            "helm self uninstall"
        );
        assert_eq!(
            self_uninstall_recommended_action(InstallChannel::Brew, executable_path),
            "brew uninstall helm-cli"
        );
        assert_eq!(
            self_uninstall_recommended_action(InstallChannel::Macports, executable_path),
            "sudo port uninstall helm-cli"
        );
        assert_eq!(
            self_uninstall_recommended_action(InstallChannel::Cargo, executable_path),
            "cargo uninstall helm-cli"
        );
        assert_eq!(
            self_uninstall_recommended_action(InstallChannel::Managed, executable_path),
            "follow managed organizational uninstall policy"
        );
        assert!(
            self_uninstall_recommended_action(InstallChannel::Unknown, executable_path)
                .contains("rm '/tmp/helm'")
        );
    }

    #[test]
    fn remove_install_marker_if_channel_respects_channel_match() {
        let direct_path = temp_file_path("direct-marker");
        fs::write(
            &direct_path,
            r#"{"channel":"direct-script","artifact":"helm-cli","installed_at":"2026-02-24T00:00:00Z","update_policy":"self","version":"0.17.3"}"#,
        )
        .expect("writes direct marker fixture");
        let (removed, warning) =
            remove_install_marker_if_channel(&direct_path, InstallChannel::DirectScript)
                .expect("direct marker removal should succeed");
        assert!(removed);
        assert!(warning.is_none());
        assert!(!direct_path.exists());

        let mismatch_path = temp_file_path("brew-marker");
        fs::write(
            &mismatch_path,
            r#"{"channel":"brew","artifact":"helm-cli","installed_at":"2026-02-24T00:00:00Z","update_policy":"channel","version":"0.17.3"}"#,
        )
        .expect("writes brew marker fixture");
        let (removed, warning) =
            remove_install_marker_if_channel(&mismatch_path, InstallChannel::DirectScript)
                .expect("mismatch marker should not hard-fail");
        assert!(!removed);
        assert!(
            warning
                .as_deref()
                .unwrap_or_default()
                .contains("does not match")
        );
        assert!(mismatch_path.exists());

        let _ = fs::remove_file(mismatch_path);
    }

    #[test]
    fn manager_self_heal_auto_disables_rubygems_for_system_executable() {
        let db_path = temp_db_path("rubygems-system");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::RubyGems,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/usr/bin/gem")),
                    version: Some("3.4.10".to_string()),
                },
            )
            .expect("ruby gems detection should persist");
        store
            .set_manager_enabled(ManagerId::RubyGems, true)
            .expect("ruby gems should be explicitly enabled before self-heal");

        apply_manager_enablement_self_heal(&store).expect("self-heal should succeed");

        let preference = store
            .list_manager_preferences()
            .expect("manager preferences should be readable")
            .into_iter()
            .find(|entry| entry.manager == ManagerId::RubyGems)
            .expect("ruby gems preference should exist");
        assert!(
            !preference.enabled,
            "ruby gems must be disabled when /usr/bin/gem is selected"
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn manager_self_heal_keeps_rubygems_enabled_for_non_system_executable() {
        let db_path = temp_db_path("rubygems-homebrew");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::RubyGems,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/opt/homebrew/bin/gem")),
                    version: Some("3.4.10".to_string()),
                },
            )
            .expect("ruby gems detection should persist");
        store
            .set_manager_enabled(ManagerId::RubyGems, true)
            .expect("ruby gems should be enabled for non-system executable");

        apply_manager_enablement_self_heal(&store).expect("self-heal should succeed");

        let preference = store
            .list_manager_preferences()
            .expect("manager preferences should be readable")
            .into_iter()
            .find(|entry| entry.manager == ManagerId::RubyGems)
            .expect("ruby gems preference should exist");
        assert!(
            preference.enabled,
            "ruby gems should remain enabled for non-system executable"
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn manager_self_heal_auto_disables_pip_for_system_executable() {
        let db_path = temp_db_path("pip-system");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");
        store
            .upsert_detection(
                ManagerId::Pip,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(PathBuf::from("/usr/bin/python3")),
                    version: Some("3.9.6".to_string()),
                },
            )
            .expect("pip detection should persist");
        store
            .set_manager_enabled(ManagerId::Pip, true)
            .expect("pip should be explicitly enabled before self-heal");

        apply_manager_enablement_self_heal(&store).expect("self-heal should succeed");

        let preference = store
            .list_manager_preferences()
            .expect("manager preferences should be readable")
            .into_iter()
            .find(|entry| entry.manager == ManagerId::Pip)
            .expect("pip preference should exist");
        assert!(
            !preference.enabled,
            "pip must be disabled when /usr/bin/python3 is selected"
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn ensure_cli_onboarding_completed_applies_accept_flags() {
        let db_path = temp_db_path("cli-onboarding-accept-flags");
        let store = SqliteStore::new(&db_path);
        store
            .migrate_to_latest()
            .expect("store migration should succeed");

        let options = GlobalOptions {
            accept_license: true,
            accept_defaults: true,
            ..GlobalOptions::default()
        };
        ensure_cli_onboarding_completed(&store, &options)
            .expect("accept flags should complete onboarding without prompts");

        assert!(
            store
                .cli_onboarding_completed()
                .expect("read onboarding completed"),
            "cli onboarding should be marked complete"
        );
        assert_eq!(
            store
                .cli_accepted_license_terms_version()
                .expect("read accepted license version"),
            Some(CLI_LICENSE_TERMS_VERSION.to_string())
        );

        let _ = fs::remove_file(db_path);
    }
}
