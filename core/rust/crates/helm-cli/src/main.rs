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
};
use helm_core::execution::{
    ManagerTimeoutProfile, TaskOutputRecord, TokioProcessExecutor,
    replace_manager_execution_preferences,
};
use helm_core::manager_policy::manager_enablement_eligibility;
use helm_core::models::{
    CachedSearchResult, Capability, DetectionInfo, HomebrewKegPolicy, InstalledPackage,
    ManagerAuthority, ManagerId, OutdatedPackage, PackageRef, PinKind, PinRecord, SearchQuery,
    TaskId, TaskLogLevel, TaskRecord, TaskStatus,
};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState, CancellationMode};
use helm_core::persistence::{DetectionStore, PackageStore, PinStore, SearchCacheStore, TaskStore};
use helm_core::registry;
use helm_core::sqlite::SqliteStore;
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
static CLI_TOKIO_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static EXECUTABLE_DISCOVERY_CACHE: OnceLock<Mutex<HashMap<ManagerId, Vec<String>>>> =
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
            COMPREPLY=( $(compgen -W "list search show install uninstall upgrade pin unpin keg-policy help" -- "${cur}") )
            ;;
        updates)
            COMPREPLY=( $(compgen -W "list summary preview run help" -- "${cur}") )
            ;;
        tasks)
            COMPREPLY=( $(compgen -W "list show logs output follow cancel help" -- "${cur}") )
            ;;
        managers)
            COMPREPLY=( $(compgen -W "list show detect enable disable install update uninstall executables install-methods priority help" -- "${cur}") )
            ;;
        settings)
            COMPREPLY=( $(compgen -W "list get set reset help" -- "${cur}") )
            ;;
        diagnostics)
            COMPREPLY=( $(compgen -W "summary task manager provenance export help" -- "${cur}") )
            ;;
        doctor)
            COMPREPLY=( $(compgen -W "summary task manager provenance export help" -- "${cur}") )
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
    _values 'subcommand' list search show install uninstall upgrade pin unpin keg-policy help
    ;;
  updates)
    _values 'subcommand' list summary preview run help
    ;;
  tasks)
    _values 'subcommand' list show logs output follow cancel help
    ;;
  managers)
    _values 'subcommand' list show detect enable disable install update uninstall executables install-methods priority help
    ;;
  settings)
    _values 'subcommand' list get set reset help
    ;;
  diagnostics)
    _values 'subcommand' summary task manager provenance export help
    ;;
  doctor)
    _values 'subcommand' summary task manager provenance export help
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
complete -c helm -n "__fish_seen_subcommand_from packages" -a "list search show install uninstall upgrade pin unpin keg-policy help"
complete -c helm -n "__fish_seen_subcommand_from updates" -a "list summary preview run help"
complete -c helm -n "__fish_seen_subcommand_from tasks" -a "list show logs output follow cancel help"
complete -c helm -n "__fish_seen_subcommand_from managers" -a "list show detect enable disable install update uninstall executables install-methods priority help"
complete -c helm -n "__fish_seen_subcommand_from settings" -a "list get set reset help"
complete -c helm -n "__fish_seen_subcommand_from diagnostics" -a "summary task manager provenance export help"
complete -c helm -n "__fish_seen_subcommand_from doctor" -a "summary task manager provenance export help"
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
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliPackageShowResult {
    name: String,
    manager: CliPackageManagerView,
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
                            | "keg-policy"
                    )
                );
            }
            if path.len() == 2 {
                return matches!(
                    (path[0].as_str(), path[1].as_str()),
                    ("keg-policy", "list")
                        | ("keg-policy", "get")
                        | ("keg-policy", "set")
                        | ("keg-policy", "reset")
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
                        | ("priority", "list")
                        | ("priority", "set")
                        | ("priority", "reset")
                );
            }
            false
        }
        Command::Settings => {
            path.is_empty()
                || (path.len() == 1
                    && matches!(first(path), Some("list" | "get" | "set" | "reset")))
        }
        Command::Diagnostics | Command::Doctor => {
            path.is_empty()
                || (path.len() == 1
                    && matches!(
                        first(path),
                        Some("summary" | "task" | "manager" | "provenance" | "export")
                    ))
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
        "unsupported packages subcommand '{}'; currently supported: list, search, show, install, uninstall, upgrade, pin, unpin, keg-policy",
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
    let (package_name, requested_manager) = parse_package_show_args(command_args)?;
    let enabled_map = manager_enabled_map(store)?;
    let installed = list_installed_for_enabled(store, &enabled_map)?;
    let outdated = list_outdated_for_enabled(store, &enabled_map)?;
    let mut rows: Vec<CliPackageManagerView> = Vec::new();

    let mut installed_map: HashMap<ManagerId, InstalledPackage> = HashMap::new();
    for package in installed {
        if package.package.name == package_name {
            installed_map.insert(package.package.manager, package);
        }
    }

    let mut outdated_map: HashMap<ManagerId, OutdatedPackage> = HashMap::new();
    for package in outdated {
        if package.package.name == package_name {
            outdated_map.insert(package.package.manager, package);
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
        });
    }

    if rows.is_empty() {
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
                package_name,
                manager.as_str()
            ));
        }
        return Err(format!("package '{}' not found", package_name));
    }

    if let Some(manager) = requested_manager {
        rows.retain(|row| row.manager_id == manager.as_str());
        if rows.is_empty() {
            return Err(format!(
                "package '{}' not found under manager '{}'",
                package_name,
                manager.as_str()
            ));
        }
    } else if rows.len() > 1 {
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

    if options.json {
        emit_json_payload(
            "helm.cli.v1.packages.show",
            json!({
                "package": CliPackageShowResult {
                    name: package_name,
                    manager,
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
    Ok(())
}

fn cmd_packages_mutation(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    subcommand: &str,
    command_args: &[String],
) -> Result<(), String> {
    let allow_version = matches!(subcommand, "install" | "pin");
    let parsed = parse_package_mutation_args(command_args, allow_version)?;
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
                        "mode": "detach"
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
                            "after_version": after_version
                        }),
                    );
                } else {
                    println!(
                        "Package {} {} via manager '{}' (task #{})",
                        package_name, subcommand, manager_id, task_id
                    );
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
                "action": subcommand
            }),
        );
    } else {
        println!(
            "Package {} {} via manager '{}'",
            parsed.package_name,
            subcommand,
            parsed.manager.as_str()
        );
    }

    Ok(())
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
            "unsupported managers subcommand '{}'; currently supported: list, show, detect, enable, disable, install, update, uninstall, executables, install-methods, priority",
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
            "  {} [{}|{}] {} exec={} method={} exec_diag={}{}",
            manager.manager_id,
            state,
            detected,
            version,
            executable,
            method,
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
    if command_args.len() != 1 {
        return Err(format!(
            "managers {} requires exactly one manager id",
            subcommand
        ));
    }
    let manager = parse_manager_id(&command_args[0])?;

    let (target_manager, request) =
        build_manager_mutation_request(store.as_ref(), manager, subcommand)?;
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
                    "mode": "detach"
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
                        "after_version": after_version
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
            }
            Ok(())
        }
        _ => Err(format!(
            "managers {} returned unexpected coordinator payload",
            subcommand
        )),
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
        return cmd_diagnostics_provenance(options);
    }
    if is_help_token(&command_args[0]) {
        if options.json {
            emit_help_json_payload(Some(Command::Doctor), &[], true);
        } else {
            print_doctor_help();
        }
        return Ok(());
    }
    cmd_diagnostics(store, options, command_args)
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
        return Ok(());
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

const PS_COMMAND_PATH: &str = coordinator_transport::PS_COMMAND_PATH;

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

#[derive(Debug, Clone)]
struct ParsedPackageMutationArgs {
    package_name: String,
    manager: ManagerId,
    version: Option<String>,
}

fn parse_package_show_args(command_args: &[String]) -> Result<(String, Option<ManagerId>), String> {
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

    Ok((package_name, selector_manager))
}

fn parse_package_mutation_args(
    command_args: &[String],
    allow_version: bool,
) -> Result<ParsedPackageMutationArgs, String> {
    if command_args.is_empty() {
        return Err("package mutation requires a package name".to_string());
    }

    let (package_name, mut selector_manager) = parse_package_selector(&command_args[0])?;
    let mut manager: Option<ManagerId> = selector_manager.take();
    let mut version: Option<String> = None;

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
            other => {
                return Err(format!("unsupported package mutation argument '{other}'"));
            }
        }
    }

    let manager = manager
        .ok_or_else(|| "package mutation requires --manager <id> or name@manager".to_string())?;

    Ok(ParsedPackageMutationArgs {
        package_name,
        manager,
        version,
    })
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
        let manager = parse_manager_id(manager_raw.trim())?;
        return Ok((name.trim().to_string(), Some(manager)));
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
            enabled_map
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
    let managers = list_managers(store.as_ref())?;
    let mut target_managers = managers
        .into_iter()
        .filter_map(|row| {
            let manager_id = row.manager_id.parse::<ManagerId>().ok()?;
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

    let detection_map: HashMap<ManagerId, helm_core::models::DetectionInfo> =
        detections.into_iter().collect();
    let preference_map: HashMap<ManagerId, helm_core::persistence::ManagerPreference> = preferences
        .into_iter()
        .map(|preference| (preference.manager, preference))
        .collect();

    let mut rows = Vec::with_capacity(registry::managers().len());
    for descriptor in registry::managers() {
        let detection = detection_map.get(&descriptor.id);
        let preference = preference_map.get(&descriptor.id);
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
    let selected_install_method = normalize_install_method(
        manager,
        preferences
            .get(&manager)
            .and_then(|preference| preference.selected_install_method.clone()),
    );

    Ok(CliManagerInstallMethodsStatus {
        manager_id: manager.as_str().to_string(),
        install_methods: manager_install_method_candidates(manager)
            .iter()
            .map(|candidate| (*candidate).to_string())
            .collect(),
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

    if manager_install_method_candidates(manager).contains(&raw) {
        return Ok(Some(raw.to_string()));
    }

    Err(format!(
        "unsupported install method '{}' for manager '{}' (supported: {})",
        raw,
        manager.as_str(),
        manager_install_method_candidates(manager).join(", ")
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

fn manager_install_method_candidates(id: ManagerId) -> &'static [&'static str] {
    match id {
        ManagerId::Mise => &["homebrew", "scriptInstaller", "macports", "cargoInstall"],
        ManagerId::Asdf => &["scriptInstaller", "homebrew"],
        ManagerId::Rustup => &["rustupInstaller", "homebrew"],
        ManagerId::HomebrewFormula => &["homebrew", "scriptInstaller"],
        ManagerId::SoftwareUpdate => &["softwareUpdate"],
        ManagerId::MacPorts => &["macports", "officialInstaller"],
        ManagerId::NixDarwin => &["scriptInstaller", "homebrew"],
        ManagerId::Npm => &["mise", "asdf", "homebrew", "officialInstaller"],
        ManagerId::Pnpm => &["corepack", "homebrew", "npm", "scriptInstaller"],
        ManagerId::Yarn => &["corepack", "homebrew", "npm", "scriptInstaller"],
        ManagerId::Poetry => &["pipx", "homebrew", "pip", "officialInstaller"],
        ManagerId::RubyGems => &["systemProvided", "homebrew", "asdf", "mise"],
        ManagerId::Bundler => &["gem", "systemProvided", "homebrew", "asdf", "mise"],
        ManagerId::Pip => &["systemProvided", "homebrew", "asdf", "mise"],
        ManagerId::Pipx => &["homebrew", "pip"],
        ManagerId::Cargo => &["rustupInstaller", "homebrew"],
        ManagerId::CargoBinstall => &["scriptInstaller", "cargoInstall", "homebrew"],
        ManagerId::Mas => &["homebrew", "macports", "appStore", "officialInstaller"],
        ManagerId::Sparkle => &["notManageable"],
        ManagerId::Setapp => &["setapp", "notManageable"],
        ManagerId::HomebrewCask => &["homebrew"],
        ManagerId::DockerDesktop => &["officialInstaller", "homebrew", "setapp"],
        ManagerId::Podman => &["officialInstaller", "homebrew", "macports"],
        ManagerId::Colima => &["homebrew", "macports", "mise"],
        ManagerId::ParallelsDesktop => &["officialInstaller", "setapp", "notManageable"],
        ManagerId::XcodeCommandLineTools => &["xcodeSelect", "appStore"],
        ManagerId::Rosetta2 => &["softwareUpdate"],
        ManagerId::FirmwareUpdates => &["systemProvided"],
    }
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

fn build_manager_mutation_request(
    store: &SqliteStore,
    manager: ManagerId,
    subcommand: &str,
) -> Result<(ManagerId, AdapterRequest), String> {
    let selected_method = manager_selected_install_method(store, manager);
    let homebrew_upgrade_request = |package_name: &str| {
        let policy = effective_homebrew_keg_policy(store, package_name);
        let cleanup_old_kegs = policy == HomebrewKegPolicy::Cleanup;
        AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(PackageRef {
                manager: ManagerId::HomebrewFormula,
                name: encode_homebrew_upgrade_target(package_name, cleanup_old_kegs),
            }),
        })
    };

    let (target_manager, request) = match subcommand {
        "install" => match manager {
            ManagerId::Mise => match selected_method.as_deref() {
                Some("homebrew") | None => (
                    ManagerId::HomebrewFormula,
                    AdapterRequest::Install(InstallRequest {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: "mise".to_string(),
                        },
                        version: None,
                    }),
                ),
                _ => {
                    return Err(format!(
                        "manager '{}' install is unsupported for selected method '{}'",
                        manager.as_str(),
                        selected_method.as_deref().unwrap_or("unknown")
                    ));
                }
            },
            ManagerId::Mas => match selected_method.as_deref() {
                Some("homebrew") | None => (
                    ManagerId::HomebrewFormula,
                    AdapterRequest::Install(InstallRequest {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: "mas".to_string(),
                        },
                        version: None,
                    }),
                ),
                _ => {
                    return Err(format!(
                        "manager '{}' install is unsupported for selected method '{}'",
                        manager.as_str(),
                        selected_method.as_deref().unwrap_or("unknown")
                    ));
                }
            },
            _ => {
                return Err(format!(
                    "manager '{}' does not currently support install",
                    manager.as_str()
                ));
            }
        },
        "update" => match manager {
            ManagerId::HomebrewFormula => (
                ManagerId::HomebrewFormula,
                AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: ManagerId::HomebrewFormula,
                        name: "__self__".to_string(),
                    }),
                }),
            ),
            ManagerId::Mise => match selected_method.as_deref() {
                Some("homebrew") | None => {
                    (ManagerId::HomebrewFormula, homebrew_upgrade_request("mise"))
                }
                _ => {
                    return Err(format!(
                        "manager '{}' update is unsupported for selected method '{}'",
                        manager.as_str(),
                        selected_method.as_deref().unwrap_or("unknown")
                    ));
                }
            },
            ManagerId::Mas => match selected_method.as_deref() {
                Some("homebrew") | None => {
                    (ManagerId::HomebrewFormula, homebrew_upgrade_request("mas"))
                }
                _ => {
                    return Err(format!(
                        "manager '{}' update is unsupported for selected method '{}'",
                        manager.as_str(),
                        selected_method.as_deref().unwrap_or("unknown")
                    ));
                }
            },
            ManagerId::Rustup => match selected_method.as_deref() {
                Some("homebrew") => (
                    ManagerId::HomebrewFormula,
                    homebrew_upgrade_request("rustup"),
                ),
                Some("rustupInstaller") | None => (
                    ManagerId::Rustup,
                    AdapterRequest::Upgrade(UpgradeRequest {
                        package: Some(PackageRef {
                            manager: ManagerId::Rustup,
                            name: "__self__".to_string(),
                        }),
                    }),
                ),
                _ => {
                    return Err(format!(
                        "manager '{}' update is unsupported for selected method '{}'",
                        manager.as_str(),
                        selected_method.as_deref().unwrap_or("unknown")
                    ));
                }
            },
            _ => {
                return Err(format!(
                    "manager '{}' does not currently support update",
                    manager.as_str()
                ));
            }
        },
        "uninstall" => match manager {
            ManagerId::Mise => match selected_method.as_deref() {
                Some("homebrew") | None => (
                    ManagerId::HomebrewFormula,
                    AdapterRequest::Uninstall(UninstallRequest {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: "mise".to_string(),
                        },
                    }),
                ),
                _ => {
                    return Err(format!(
                        "manager '{}' uninstall is unsupported for selected method '{}'",
                        manager.as_str(),
                        selected_method.as_deref().unwrap_or("unknown")
                    ));
                }
            },
            ManagerId::Mas => match selected_method.as_deref() {
                Some("homebrew") | None => (
                    ManagerId::HomebrewFormula,
                    AdapterRequest::Uninstall(UninstallRequest {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: "mas".to_string(),
                        },
                    }),
                ),
                _ => {
                    return Err(format!(
                        "manager '{}' uninstall is unsupported for selected method '{}'",
                        manager.as_str(),
                        selected_method.as_deref().unwrap_or("unknown")
                    ));
                }
            },
            ManagerId::Rustup => match selected_method.as_deref() {
                Some("homebrew") => (
                    ManagerId::HomebrewFormula,
                    AdapterRequest::Uninstall(UninstallRequest {
                        package: PackageRef {
                            manager: ManagerId::HomebrewFormula,
                            name: "rustup".to_string(),
                        },
                    }),
                ),
                Some("rustupInstaller") | None => (
                    ManagerId::Rustup,
                    AdapterRequest::Uninstall(UninstallRequest {
                        package: PackageRef {
                            manager: ManagerId::Rustup,
                            name: "__self__".to_string(),
                        },
                    }),
                ),
                _ => {
                    return Err(format!(
                        "manager '{}' uninstall is unsupported for selected method '{}'",
                        manager.as_str(),
                        selected_method.as_deref().unwrap_or("unknown")
                    ));
                }
            },
            _ => {
                return Err(format!(
                    "manager '{}' does not currently support uninstall",
                    manager.as_str()
                ));
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
            "keg-policy" => {
                print_packages_keg_policy_help();
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
    print_diagnostics_help_topic(path)
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
    println!("  packages [list|search|show|install|uninstall|upgrade|pin|unpin|keg-policy]");
    println!("                         Package listing/search/details and mutations");
    println!("  updates [list|summary|preview|run]");
    println!("                         List/summarize/preview/run package upgrades");
    println!("  tasks [list|show|logs|output|follow|cancel]");
    println!("                         Inspect task state/logs/output and cancellation status");
    println!(
        "  managers [list|show|detect|enable|disable|install|update|uninstall|executables|install-methods|priority]"
    );
    println!("                         Manager status, enablement, and selection controls");
    println!("  settings [list|get|set|reset]");
    println!("                         Read and update selected settings");
    println!("  diagnostics [summary|task|manager|provenance|export]");
    println!("                         Read diagnostics snapshots and export support data");
    println!("  doctor [summary|task|manager|provenance|export]");
    println!("                         Diagnostics alias; default shows provenance");
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
    println!("  uninstall <name|name@manager> --manager <id>");
    println!("  upgrade <name|name@manager> --manager <id>");
    println!("  pin <name|name@manager> --manager <id> [--version <v>]");
    println!("  unpin <name|name@manager> --manager <id>");
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
}

fn print_packages_install_help() {
    println!("USAGE:");
    println!("  helm packages install <name|name@manager> --manager <id> [--version <v>]");
    println!();
    println!("DESCRIPTION:");
    println!("  Install a package via the selected manager.");
}

fn print_packages_uninstall_help() {
    println!("USAGE:");
    println!("  helm packages uninstall <name|name@manager> --manager <id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Uninstall a package via the selected manager.");
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
}

fn print_packages_unpin_help() {
    println!("USAGE:");
    println!("  helm packages unpin <name|name@manager> --manager <id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Remove pin state for a package.");
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
    println!("  helm managers install <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Install a supported manager via selected install method routing.");
}

fn print_managers_update_help() {
    println!("USAGE:");
    println!("  helm managers update <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Update a supported manager via selected install method routing.");
}

fn print_managers_uninstall_help() {
    println!("USAGE:");
    println!("  helm managers uninstall <manager-id>");
    println!();
    println!("DESCRIPTION:");
    println!("  Uninstall a supported manager via selected install method routing.");
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
    println!("  helm doctor provenance");
    println!("  helm doctor <summary|task|manager|provenance|export> [args]");
    println!();
    println!("DESCRIPTION:");
    println!("  Alias for the diagnostics namespace.");
    println!("  Without subcommands, doctor defaults to install provenance output.");
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
        GlobalOptions, HomebrewKegPolicy, InstallChannel, ManagerId, SelfUpdateErrorKind,
        TASKS_FOLLOW_MACHINE_MODE_UNSUPPORTED_ERROR, UpdatePolicy, UpgradeExecutionStep,
        acquire_coordinator_bootstrap_lock, apply_manager_enablement_self_heal,
        build_json_payload_lines, classify_failure_class, cmd_tasks_follow, cmd_updates_run,
        command_help_topic_exists, coordinator_transport_for_cancel,
        coordinator_transport_for_submit, coordinator_transport_for_workflow,
        count_upgrade_step_failures, ensure_cli_onboarding_completed, exit_code_for_error,
        failure_class_hint, manager_operation_failure_error, mark_exit_code, parse_args,
        parse_args_with_tty, parse_homebrew_keg_policy_arg, parse_manager_id, parse_search_args,
        parse_structured_terminal_error_message, parse_updates_run_preview_args,
        provenance_can_self_update, raw_args_request_json, raw_args_request_ndjson,
        read_update_bytes_with_limit, remove_install_marker_if_channel, resolve_redirect_url,
        resolve_update_redirect_target, selected_executable_differs_from_default,
        self_uninstall_recommended_action, should_launch_coordinator_on_demand,
        strip_exit_code_marker, tasks_follow_machine_mode_error, upgrade_request_name,
    };
    use helm_core::execution::TaskOutputRecord;
    use helm_core::models::DetectionInfo;
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
        assert_eq!(super::PS_COMMAND_PATH, "/bin/ps");
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
