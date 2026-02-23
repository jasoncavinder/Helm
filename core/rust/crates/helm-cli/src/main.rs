use std::collections::{HashMap, hash_map::DefaultHasher};
use std::env;
use std::hash::{Hash, Hasher};
use std::io::IsTerminal;
use std::path::PathBuf;
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
    ProcessYarnSource, Rosetta2Adapter, RubyGemsAdapter, RustupAdapter, SetappAdapter,
    SoftwareUpdateAdapter, SparkleAdapter, UninstallRequest, UnpinRequest, UpgradeRequest,
    XcodeCommandLineToolsAdapter, YarnAdapter,
};
use helm_core::execution::{
    TokioProcessExecutor, clear_manager_selected_executables, set_manager_selected_executable,
};
use helm_core::models::{
    CachedSearchResult, Capability, DetectionInfo, HomebrewKegPolicy, InstalledPackage,
    ManagerAuthority, ManagerId, OutdatedPackage, PackageRef, PinKind, PinRecord, TaskId,
    TaskLogLevel, TaskRecord, TaskStatus,
};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState, CancellationMode};
use helm_core::persistence::{DetectionStore, PackageStore, PinStore, SearchCacheStore, TaskStore};
use helm_core::registry;
use helm_core::sqlite::SqliteStore;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;

mod provenance;

use provenance::{
    can_self_update as provenance_can_self_update, detect_install_provenance,
    recommended_action as provenance_recommended_action,
};

const TASK_FETCH_LIMIT: usize = 400;
const TASK_FOLLOW_MAX_WAIT_MS: u64 = 30_000;
const JSON_SCHEMA_VERSION: u32 = 1;
static CLI_TOKIO_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static EXECUTABLE_DISCOVERY_CACHE: OnceLock<Mutex<HashMap<ManagerId, Vec<String>>>> =
    OnceLock::new();
static COORDINATOR_REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static CLI_VERBOSE: AtomicBool = AtomicBool::new(false);
const BASH_COMPLETION_SCRIPT: &str = r#"_helm_complete() {
    local cur
    cur="${COMP_WORDS[COMP_CWORD]}"
    local commands="status refresh search ls packages updates tasks managers settings diagnostics self completion help"
    if [[ ${COMP_CWORD} -eq 1 ]]; then
        COMPREPLY=( $(compgen -W "${commands}" -- "${cur}") )
        return 0
    fi

    case "${COMP_WORDS[1]}" in
        packages)
            COMPREPLY=( $(compgen -W "list search show install uninstall upgrade pin unpin help" -- "${cur}") )
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
        self)
            if [[ ${COMP_CWORD} -ge 3 && "${COMP_WORDS[2]}" == "auto-check" ]]; then
                COMPREPLY=( $(compgen -W "status enable disable frequency help" -- "${cur}") )
            else
                COMPREPLY=( $(compgen -W "status check update auto-check help" -- "${cur}") )
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
    _values 'subcommand' list search show install uninstall upgrade pin unpin help
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
  self)
    if [[ "$words[3]" == "auto-check" ]]; then
      _values 'subcommand' status enable disable frequency help
    else
      _values 'subcommand' status check update auto-check help
    fi
    ;;
  completion)
    _values 'shell' bash zsh fish help
    ;;
esac
"#;
const FISH_COMPLETION_SCRIPT: &str = r#"complete -c helm -f
complete -c helm -n "__fish_use_subcommand" -a "status refresh search ls packages updates tasks managers settings diagnostics self completion help"
complete -c helm -n "__fish_seen_subcommand_from packages" -a "list search show install uninstall upgrade pin unpin help"
complete -c helm -n "__fish_seen_subcommand_from updates" -a "list summary preview run help"
complete -c helm -n "__fish_seen_subcommand_from tasks" -a "list show logs output follow cancel help"
complete -c helm -n "__fish_seen_subcommand_from managers" -a "list show detect enable disable install update uninstall executables install-methods priority help"
complete -c helm -n "__fish_seen_subcommand_from settings" -a "list get set reset help"
complete -c helm -n "__fish_seen_subcommand_from diagnostics" -a "summary task manager provenance export help"
complete -c helm -n "__fish_seen_subcommand_from self" -a "status check update auto-check help"
complete -c helm -n "__fish_seen_subcommand_from auto-check" -a "status enable disable frequency help"
complete -c helm -n "__fish_seen_subcommand_from completion" -a "bash zsh fish help"
"#;

#[derive(Default, Debug, Clone, Copy)]
struct GlobalOptions {
    json: bool,
    verbose: bool,
    execution_mode: ExecutionMode,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionMode {
    #[default]
    Wait,
    Detach,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
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
    selected_install_method: Option<String>,
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
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoordinatorResponse {
    ok: bool,
    task_id: Option<u64>,
    job_id: Option<String>,
    payload: Option<CoordinatorPayload>,
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
    let (options, command, command_args) = match parse_args(env::args().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("helm: {error}");
            return ExitCode::from(1);
        }
    };
    set_verbose_enabled(options.verbose);
    verbose_log(format!(
        "parsed invocation: command={:?}, args={:?}, json={}, execution_mode={:?}, verbose={}",
        command, command_args, options.json, options.execution_mode, options.verbose
    ));

    if matches!(command, Command::Help) {
        if !print_help_topic(&command_args) {
            print_help();
        }
        return ExitCode::SUCCESS;
    }

    if matches!(command, Command::Version) {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }

    if matches!(command, Command::Completion) {
        return cmd_completion(&command_args)
            .map(|_| ExitCode::SUCCESS)
            .unwrap_or_else(|error| {
                eprintln!("helm: {error}");
                ExitCode::from(1)
            });
    }

    if let Some(help_path) = extract_help_path(&command_args)
        && print_command_help_topic(command, &help_path)
    {
        return ExitCode::SUCCESS;
    }

    let store = match open_store() {
        Ok(store) => store,
        Err(error) => {
            eprintln!("helm: failed to open state store: {error}");
            return ExitCode::from(1);
        }
    };

    match command {
        Command::Status => cmd_status(store.as_ref(), options),
        Command::Refresh => cmd_refresh(store.clone(), options, &command_args),
        Command::Ls => cmd_packages(store.clone(), options, &command_args),
        Command::Search => cmd_search(store.as_ref(), options, &command_args),
        Command::Packages => cmd_packages(store.clone(), options, &command_args),
        Command::Updates => cmd_updates(store.clone(), options, &command_args),
        Command::Tasks => cmd_tasks(store.as_ref(), options, &command_args),
        Command::Managers => cmd_managers(store.clone(), options, &command_args),
        Command::Settings => cmd_settings(store.as_ref(), options, &command_args),
        Command::Diagnostics => cmd_diagnostics(store.as_ref(), options, &command_args),
        Command::SelfCmd => cmd_self(store.clone(), options, &command_args),
        Command::InternalCoordinator => cmd_internal_coordinator(store.clone(), &command_args),
        Command::Completion | Command::Help | Command::Version => Ok(()),
    }
    .map(|_| ExitCode::SUCCESS)
    .unwrap_or_else(|error| {
        eprintln!("helm: {error}");
        ExitCode::from(1)
    })
}

fn parse_args(args: Vec<String>) -> Result<(GlobalOptions, Command, Vec<String>), String> {
    if args.is_empty() {
        if std::io::stdout().is_terminal() {
            print_tui_placeholder();
        }
        return Ok((GlobalOptions::default(), Command::Help, Vec::new()));
    }

    let mut options = GlobalOptions {
        verbose: env_flag_enabled("HELM_CLI_VERBOSE"),
        ..GlobalOptions::default()
    };
    let mut filtered = Vec::new();
    let mut wait_flag = false;
    let mut detach_flag = false;
    for arg in args {
        match arg.as_str() {
            "--json" => {
                options.json = true;
                continue;
            }
            "-v" | "--verbose" => {
                options.verbose = true;
                continue;
            }
            "--wait" => {
                wait_flag = true;
                options.execution_mode = ExecutionMode::Wait;
                continue;
            }
            "--detach" => {
                detach_flag = true;
                options.execution_mode = ExecutionMode::Detach;
                continue;
            }
            _ => {}
        }
        filtered.push(arg);
    }

    if wait_flag && detach_flag {
        return Err("flags --wait and --detach are mutually exclusive".to_string());
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

    let command =
        parse_top_level_command(first).ok_or_else(|| format!("unknown command '{first}'"))?;

    Ok((options, command, filtered[1..].to_vec()))
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
    let runtime = build_adapter_runtime(store)?;

    match target {
        ManagerTarget::All => {
            if options.execution_mode == ExecutionMode::Detach {
                let response = coordinator_start_workflow(CoordinatorWorkflowRequest::RefreshAll)?;
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
            if failures > 0 {
                return Err(format!("{failures} manager refresh operations failed"));
            }
            Ok(())
        }
        ManagerTarget::One(manager) => {
            if options.execution_mode == ExecutionMode::Detach {
                let response =
                    coordinator_start_workflow(CoordinatorWorkflowRequest::RefreshManager {
                        manager_id: manager.as_str().to_string(),
                    })?;
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
                return Err(failure.unwrap_or_else(|| "refresh failed".to_string()));
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
        if command_args.len() < 2 {
            return Err("packages search requires a query argument".to_string());
        }
        return cmd_search_query(store.as_ref(), options, &command_args[1]);
    }

    if command_args[0] == "show" {
        return cmd_packages_show(store.as_ref(), options, &command_args[1..]);
    }

    if matches!(
        command_args[0].as_str(),
        "install" | "uninstall" | "upgrade" | "pin" | "unpin"
    ) {
        return cmd_packages_mutation(store, options, command_args[0].as_str(), &command_args[1..]);
    }

    Err(format!(
        "unsupported packages subcommand '{}'; currently supported: list, search, show, install, uninstall, upgrade, pin, unpin",
        command_args[0]
    ))
}

fn cmd_search(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() {
        return Err("search requires a query argument".to_string());
    }
    cmd_search_query(store, options, &command_args[0])
}

fn cmd_search_query(
    store: &SqliteStore,
    options: GlobalOptions,
    query: &str,
) -> Result<(), String> {
    let enabled_map = manager_enabled_map(store)?;
    let results = search_local_for_enabled(store, &enabled_map, query)?;

    if options.json {
        emit_json_payload(
            "helm.cli.v1.search.local",
            json!({
                "query": query,
                "results": results
            }),
        );
        return Ok(());
    }

    if results.is_empty() {
        println!("No local search results for query '{query}'.");
        return Ok(());
    }

    println!("Search Results (local cache)");
    for result in results {
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
    )?;
    let plan_steps = serialize_upgrade_plan_steps(&steps);

    if options.json {
        emit_json_payload(
            "helm.cli.v1.updates.preview",
            json!({
                "include_pinned": parsed.include_pinned,
                "allow_os_updates": parsed.allow_os_updates,
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
        "updates run requested include_pinned={} allow_os_updates={} mode={:?}",
        parsed.include_pinned, parsed.allow_os_updates, options.execution_mode
    ));
    if !parsed.yes {
        return Err("updates run requires --yes".to_string());
    }

    if options.execution_mode == ExecutionMode::Detach {
        let response = coordinator_start_workflow(CoordinatorWorkflowRequest::UpdatesRun {
            include_pinned: parsed.include_pinned,
            allow_os_updates: parsed.allow_os_updates,
        })?;
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
                    "allow_os_updates": parsed.allow_os_updates
                }),
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
    )?;

    if steps.is_empty() {
        if options.json {
            emit_json_payload(
                "helm.cli.v1.updates.run",
                json!({
                    "include_pinned": parsed.include_pinned,
                    "allow_os_updates": parsed.allow_os_updates,
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
        let request_name = if step.manager == ManagerId::HomebrewFormula && step.cleanup_old_kegs {
            encode_homebrew_upgrade_target(&step.package_name, true)
        } else {
            step.package_name.clone()
        };
        let request = AdapterRequest::Upgrade(UpgradeRequest {
            package: Some(PackageRef {
                manager: step.manager,
                name: request_name,
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
                "results": results,
                "total_steps": steps.len(),
                "failed_steps": failures
            }),
        );
    } else {
        println!("Upgrade Run Results");
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

    if failures > 0 {
        return Err(format!("{failures} upgrade steps failed"));
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
    let available = output.is_some();
    let command = output.as_ref().and_then(|entry| entry.command.clone());
    let stdout = output.as_ref().and_then(|entry| entry.stdout.clone());
    let stderr = output.as_ref().and_then(|entry| entry.stderr.clone());

    if options.json {
        emit_json_payload(
            "helm.cli.v1.tasks.output",
            json!({
                "task_id": task_id,
                "available": available,
                "command": command,
                "stdout": stdout,
                "stderr": stderr
            }),
        );
        return Ok(());
    }

    if !available {
        println!(
            "Task output for #{} is not available in this CLI process session.",
            task_id
        );
        println!("Use 'tasks logs' for persisted lifecycle logs.");
        return Ok(());
    }

    println!("Task Output #{}", task_id);
    println!("  command: {}", command.as_deref().unwrap_or("-"));
    println!("  stdout:");
    println!("{}", stdout.as_deref().unwrap_or(""));
    println!("  stderr:");
    println!("{}", stderr.as_deref().unwrap_or(""));
    Ok(())
}

fn cmd_tasks_follow(
    store: &SqliteStore,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if options.json {
        return Err("tasks follow does not support --json streaming yet".to_string());
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
                    "  selected_install_method: {}",
                    row.selected_install_method.as_deref().unwrap_or("-")
                );
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
            "  {} [{}|{}] {} exec={} method={}{}",
            manager.manager_id, state, detected, version, executable, method, flags
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
                let response = coordinator_start_workflow(CoordinatorWorkflowRequest::DetectAll)?;
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
            let runtime = build_adapter_runtime(store)?;
            let rows = tokio_runtime.block_on(detect_all_no_timeout(&runtime));
            let failures = emit_manager_results(
                options,
                "helm.cli.v1.managers.detect.all",
                "Detection completed",
                rows,
            );
            if failures > 0 {
                return Err(format!("{failures} manager detection operations failed"));
            }
            Ok(())
        }
        ManagerTarget::One(manager) => {
            let response = coordinator_submit_request(
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
    let response =
        coordinator_submit_request(target_manager, submit_request, options.execution_mode)?;
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

#[derive(Debug, Clone)]
struct SelfUpdateTarget {
    method: String,
    formula: Option<String>,
    supported: bool,
    reason: Option<String>,
}

#[derive(Debug, Clone)]
struct SelfUpdateSnapshotStatus {
    update_available: bool,
    latest_version: Option<String>,
}

fn detect_self_update_target(_store: &SqliteStore) -> SelfUpdateTarget {
    if let Ok(formula) = env::var("HELM_SELF_UPDATE_FORMULA")
        && !formula.trim().is_empty()
    {
        return SelfUpdateTarget {
            method: "homebrew_formula".to_string(),
            formula: Some(formula.trim().to_string()),
            supported: true,
            reason: None,
        };
    }

    let executable = env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
        .map(|path| path.to_string_lossy().to_string());
    if let Some(path) = executable.as_deref()
        && let Some(formula) = detect_homebrew_formula_from_path(path)
    {
        return SelfUpdateTarget {
            method: "homebrew_formula".to_string(),
            formula: Some(formula),
            supported: true,
            reason: None,
        };
    }

    SelfUpdateTarget {
        method: "manual".to_string(),
        formula: None,
        supported: false,
        reason: executable.map(|path| {
            format!(
                "unsupported install path '{}' (set HELM_SELF_UPDATE_FORMULA to override)",
                path
            )
        }),
    }
}

fn detect_homebrew_formula_from_path(path: &str) -> Option<String> {
    let components = PathBuf::from(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let cellar_index = components.iter().position(|part| part == "Cellar")?;
    let formula = components.get(cellar_index + 1)?.trim();
    if formula.is_empty() {
        return None;
    }
    Some(formula.to_string())
}

fn refresh_self_update_snapshot(store: Arc<SqliteStore>, formula: &str) -> Result<(), String> {
    if formula.trim().is_empty() {
        return Err("self check failed: empty update formula".to_string());
    }
    let runtime = build_adapter_runtime(store)?;
    let tokio_runtime = cli_tokio_runtime()?;
    let manager = ManagerId::HomebrewFormula;
    verbose_log(format!(
        "running self check refresh via manager '{}' formula '{}'",
        manager.as_str(),
        formula
    ));
    tokio_runtime.block_on(refresh_single_manager(&runtime, manager))
}

fn self_update_snapshot_status(
    store: &SqliteStore,
    formula: &str,
) -> Result<Option<SelfUpdateSnapshotStatus>, String> {
    let formula = formula.trim();
    if formula.is_empty() {
        return Ok(None);
    }

    let outdated = store
        .list_outdated()
        .map_err(|error| format!("failed to list outdated packages for self status: {error}"))?;
    if let Some(package) = outdated.iter().find(|package| {
        package.package.manager == ManagerId::HomebrewFormula && package.package.name == formula
    }) {
        return Ok(Some(SelfUpdateSnapshotStatus {
            update_available: true,
            latest_version: Some(package.candidate_version.clone()),
        }));
    }

    let installed = store
        .list_installed()
        .map_err(|error| format!("failed to list installed packages for self status: {error}"))?;
    if installed.iter().any(|package| {
        package.package.manager == ManagerId::HomebrewFormula && package.package.name == formula
    }) {
        return Ok(Some(SelfUpdateSnapshotStatus {
            update_available: false,
            latest_version: None,
        }));
    }

    Ok(None)
}

fn cmd_self(
    store: Arc<SqliteStore>,
    options: GlobalOptions,
    command_args: &[String],
) -> Result<(), String> {
    if command_args.is_empty() || is_help_token(&command_args[0]) {
        print_self_help();
        return Ok(());
    }

    let target = detect_self_update_target(store.as_ref());
    verbose_log(format!(
        "self command target method={} formula={:?} reason={:?}",
        target.method, target.formula, target.reason
    ));

    match command_args[0].as_str() {
        "auto-check" => cmd_self_auto_check(store.as_ref(), options, &command_args[1..]),
        "status" => {
            let auto_check_for_updates = read_setting(store.as_ref(), "auto_check_for_updates")
                .unwrap_or_else(|_| "false".to_string());
            let auto_check_frequency_minutes =
                read_setting(store.as_ref(), "auto_check_frequency_minutes")
                    .unwrap_or_else(|_| "1440".to_string());
            let update_snapshot = match target.formula.as_ref() {
                Some(formula) if target.method == "homebrew_formula" => {
                    self_update_snapshot_status(store.as_ref(), formula)
                }
                _ => Ok(None),
            }?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.status",
                    json!({
                        "current_version": env!("CARGO_PKG_VERSION"),
                        "channel": "stable",
                        "auto_check_for_updates": auto_check_for_updates,
                        "auto_check_frequency_minutes": auto_check_frequency_minutes,
                        "update_method": target.method,
                        "target_formula": target.formula,
                        "can_self_update": target.supported,
                        "reason": target.reason,
                        "update_available": update_snapshot.as_ref().map(|status| status.update_available),
                        "latest_version": update_snapshot.and_then(|status| status.latest_version)
                    }),
                );
            } else {
                println!("Helm Self Status");
                println!("  current_version: {}", env!("CARGO_PKG_VERSION"));
                println!("  channel: stable");
                println!("  auto_check_for_updates: {auto_check_for_updates}");
                println!("  auto_check_frequency_minutes: {auto_check_frequency_minutes}");
                println!("  update_method: {}", target.method);
                println!(
                    "  target_formula: {}",
                    target.formula.as_deref().unwrap_or("-")
                );
                println!("  can_self_update: {}", target.supported);
                println!("  reason: {}", target.reason.as_deref().unwrap_or("-"));
                if let Some(snapshot) = update_snapshot {
                    println!("  update_available: {}", snapshot.update_available);
                    println!(
                        "  latest_version: {}",
                        snapshot.latest_version.as_deref().unwrap_or("-")
                    );
                } else {
                    println!("  update_available: unknown");
                }
            }
            Ok(())
        }
        "check" => {
            let checked_at = json_generated_at_unix();
            if !target.supported {
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.check",
                        json!({
                            "checked": false,
                            "checked_at": checked_at,
                            "update_available": null,
                            "latest_version": null,
                            "update_method": target.method,
                            "target_formula": target.formula,
                            "reason": target.reason.unwrap_or_else(|| "self-update check is unavailable for this installation".to_string())
                        }),
                    );
                } else {
                    println!("Self-update check unavailable for this installation.");
                    if let Some(reason) = target.reason {
                        println!("  reason: {reason}");
                    }
                    println!("  checked_at: {checked_at}");
                }
                return Ok(());
            }

            refresh_self_update_snapshot(
                store.clone(),
                target.formula.as_deref().unwrap_or("helm"),
            )?;
            let snapshot = self_update_snapshot_status(
                store.as_ref(),
                target.formula.as_deref().unwrap_or("helm"),
            )?;
            if options.json {
                emit_json_payload(
                    "helm.cli.v1.self.check",
                    json!({
                        "checked": true,
                        "checked_at": checked_at,
                        "update_method": target.method,
                        "target_formula": target.formula,
                        "update_available": snapshot.as_ref().map(|status| status.update_available),
                        "latest_version": snapshot.and_then(|status| status.latest_version),
                        "reason": null
                    }),
                );
            } else {
                println!("Self-update check completed.");
                println!("  checked_at: {checked_at}");
                println!(
                    "  update_available: {}",
                    snapshot
                        .as_ref()
                        .map(|status| status.update_available.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                );
                println!(
                    "  latest_version: {}",
                    snapshot
                        .and_then(|status| status.latest_version)
                        .unwrap_or_else(|| "-".to_string())
                );
            }
            Ok(())
        }
        "update" => {
            if !target.supported {
                let reason = target.reason.unwrap_or_else(|| {
                    "self-update is unavailable for this installation".to_string()
                });
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.update",
                        json!({
                            "accepted": false,
                            "update_method": target.method,
                            "target_formula": target.formula,
                            "reason": reason
                        }),
                    );
                }
                return Err(format!("self update unavailable: {}", reason));
            }
            let formula = target.formula.ok_or_else(|| {
                "self update unavailable: missing formula mapping for this installation".to_string()
            })?;
            let response = coordinator_submit_request(
                ManagerId::HomebrewFormula,
                CoordinatorSubmitRequest::Upgrade {
                    package_name: Some(formula.clone()),
                },
                options.execution_mode,
            )?;
            let task_id = response
                .task_id
                .ok_or_else(|| "coordinator response missing task id".to_string())?;

            if options.execution_mode == ExecutionMode::Detach {
                if options.json {
                    emit_json_payload(
                        "helm.cli.v1.self.update",
                        json!({
                            "accepted": true,
                            "mode": "detach",
                            "task_id": task_id,
                            "update_method": target.method,
                            "target_formula": formula
                        }),
                    );
                } else {
                    println!("Self-update submitted (task #{}).", task_id);
                }
                return Ok(());
            }

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
                            "helm.cli.v1.self.update",
                            json!({
                                "accepted": true,
                                "mode": "wait",
                                "task_id": task_id,
                                "update_method": target.method,
                                "target_formula": formula,
                                "manager_id": manager_id,
                                "package_name": package_name,
                                "action": action,
                                "before_version": before_version,
                                "after_version": after_version
                            }),
                        );
                    } else {
                        println!("Self-update completed (task #{}).", task_id);
                        println!("  method: {}", target.method);
                        println!("  target_formula: {}", formula);
                    }
                    Ok(())
                }
                _ => Err(format!(
                    "self update task {} completed with unexpected coordinator payload",
                    task_id
                )),
            }
        }
        _ => Err(format!(
            "unsupported self subcommand '{}'; currently supported: status, check, update, auto-check",
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
        if options.json {
            emit_json_payload(
                "helm.cli.v1.self.auto_check.status",
                json!({
                    "enabled": enabled,
                    "frequency_minutes": frequency_minutes
                }),
            );
        } else {
            println!("Self auto-check status");
            println!("  enabled: {}", enabled);
            println!("  frequency_minutes: {}", frequency_minutes);
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
        print_diagnostics_help();
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

fn cmd_completion(command_args: &[String]) -> Result<(), String> {
    if command_args.is_empty() || is_help_token(&command_args[0]) {
        print_completion_help();
        return Ok(());
    }

    if command_args.len() != 1 {
        return Err(
            "completion requires exactly one shell argument: bash, zsh, or fish".to_string(),
        );
    }

    match command_args[0].as_str() {
        "bash" => {
            print!("{BASH_COMPLETION_SCRIPT}");
            Ok(())
        }
        "zsh" => {
            print!("{ZSH_COMPLETION_SCRIPT}");
            Ok(())
        }
        "fish" => {
            print!("{FISH_COMPLETION_SCRIPT}");
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
    let mut socket_path: Option<PathBuf> = None;
    let mut index = 0usize;
    while index < command_args.len() {
        match command_args[index].as_str() {
            "--state-dir" | "--socket" => {
                if index + 1 >= command_args.len() {
                    return Err("__coordinator__ serve --state-dir requires a value".to_string());
                }
                socket_path = Some(PathBuf::from(command_args[index + 1].as_str()));
                index += 2;
            }
            other => {
                return Err(format!(
                    "unsupported __coordinator__ serve argument '{}'",
                    other
                ));
            }
        }
    }

    socket_path.map(Ok).unwrap_or_else(coordinator_socket_path)
}

fn coordinator_socket_path() -> Result<PathBuf, String> {
    let db_path = database_path()?;
    let mut hasher = DefaultHasher::new();
    db_path.hash(&mut hasher);
    let suffix = format!("{:x}", hasher.finish());
    let root = env::var("TMPDIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    Ok(root.join(format!("helm-cli-coordinator-{suffix}")))
}

fn coordinator_ready_file(state_dir: &std::path::Path) -> PathBuf {
    state_dir.join("ready.json")
}

fn coordinator_requests_dir(state_dir: &std::path::Path) -> PathBuf {
    state_dir.join("requests")
}

fn coordinator_responses_dir(state_dir: &std::path::Path) -> PathBuf {
    state_dir.join("responses")
}

fn coordinator_request_file(state_dir: &std::path::Path, request_id: &str) -> PathBuf {
    coordinator_requests_dir(state_dir).join(format!("{request_id}.json"))
}

fn coordinator_response_file(state_dir: &std::path::Path, request_id: &str) -> PathBuf {
    coordinator_responses_dir(state_dir).join(format!("{request_id}.json"))
}

fn next_coordinator_request_id() -> String {
    let counter = COORDINATOR_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{counter}", std::process::id())
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
    std::fs::write(&temp_path, rendered).map_err(|error| {
        format!(
            "failed to write temp json file '{}': {error}",
            temp_path.display()
        )
    })?;
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

fn reset_coordinator_state_dir(state_dir: &std::path::Path) -> Result<(), String> {
    if state_dir.exists() {
        std::fs::remove_dir_all(state_dir).map_err(|error| {
            format!(
                "failed to reset coordinator state directory '{}': {error}",
                state_dir.display()
            )
        })?;
    }
    std::fs::create_dir_all(coordinator_requests_dir(state_dir).as_path()).map_err(|error| {
        format!(
            "failed to create coordinator requests directory '{}': {error}",
            coordinator_requests_dir(state_dir).display()
        )
    })?;
    std::fs::create_dir_all(coordinator_responses_dir(state_dir).as_path()).map_err(|error| {
        format!(
            "failed to create coordinator responses directory '{}': {error}",
            coordinator_responses_dir(state_dir).display()
        )
    })?;
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

    for _ in 0..60 {
        if let Ok(response) = send_coordinator_request_once(socket_path, &CoordinatorRequest::Ping)
            && response.ok
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }

    Err("coordinator daemon did not become ready in time".to_string())
}

fn coordinator_send_request(
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
            if !start_if_needed {
                return Err(error);
            }
            verbose_log(format!(
                "coordinator request kind='{}' failed before startup: {}; attempting launch-on-demand",
                coordinator_request_kind(request),
                error
            ));
            spawn_coordinator_daemon(socket_path.as_path())?;
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

    let timeout = Duration::from_secs(30);
    let started = Instant::now();
    while started.elapsed() < timeout {
        if response_file.exists() {
            let response = read_json_file::<CoordinatorResponse>(response_file.as_path())?;
            let _ = std::fs::remove_file(response_file.as_path());
            return Ok(response);
        }
        thread::sleep(Duration::from_millis(25));
    }

    let _ = std::fs::remove_file(request_file.as_path());
    Err(format!(
        "timed out waiting for coordinator response in '{}'",
        socket_path.display()
    ))
}

fn coordinator_submit_request(
    manager: ManagerId,
    request: CoordinatorSubmitRequest,
    execution_mode: ExecutionMode,
) -> Result<CoordinatorResponse, String> {
    let wait = execution_mode == ExecutionMode::Wait;
    let response = coordinator_send_request(
        &CoordinatorRequest::Submit {
            manager_id: manager.as_str().to_string(),
            request,
            wait,
        },
        true,
    )?;

    if response.ok {
        return Ok(response);
    }

    Err(response
        .error
        .unwrap_or_else(|| "coordinator submit request failed".to_string()))
}

fn coordinator_cancel_task(task_id: u64) -> Result<(), String> {
    let response = coordinator_send_request(&CoordinatorRequest::Cancel { task_id }, false)?;
    if response.ok {
        return Ok(());
    }
    Err(response
        .error
        .unwrap_or_else(|| format!("failed to cancel task '{}'", task_id)))
}

fn coordinator_start_workflow(
    workflow: CoordinatorWorkflowRequest,
) -> Result<CoordinatorResponse, String> {
    let response = coordinator_send_request(&CoordinatorRequest::StartWorkflow { workflow }, true)?;
    if response.ok {
        return Ok(response);
    }
    Err(response
        .error
        .unwrap_or_else(|| "coordinator workflow request failed".to_string()))
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
    write_json_file(
        coordinator_ready_file(socket_path.as_path()).as_path(),
        &json!({
            "pid": std::process::id(),
            "started_at": json_generated_at_unix()
        }),
    )?;
    verbose_log("coordinator ready and processing requests");
    loop {
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
            thread::sleep(Duration::from_millis(25));
            continue;
        }

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
                    error: None,
                },
                Err(error) => CoordinatorResponse {
                    ok: false,
                    task_id: Some(task_id),
                    job_id: None,
                    payload: None,
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
                        error: None,
                    },
                    Some(AdapterTaskTerminalState::Failed(error)) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        error: Some(format_core_error(error)),
                    },
                    Some(AdapterTaskTerminalState::Cancelled(Some(error))) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        error: Some(format_core_error(error)),
                    },
                    Some(AdapterTaskTerminalState::Cancelled(None)) => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
                        error: Some(format!("task {} was cancelled", task_id.0)),
                    },
                    None => CoordinatorResponse {
                        ok: false,
                        task_id: Some(task_id.0),
                        job_id: None,
                        payload: None,
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
            if failures > 0 {
                return Err(format!("{failures} manager refresh operations failed"));
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
            if failures > 0 {
                return Err(format!("{failures} manager detection operations failed"));
            }
            Ok(())
        }
        CoordinatorWorkflowRequest::UpdatesRun {
            include_pinned,
            allow_os_updates,
        } => {
            let steps = collect_upgrade_execution_steps(
                store.as_ref(),
                &runtime,
                include_pinned,
                allow_os_updates,
            )?;
            for step in &steps {
                let request_name =
                    if step.manager == ManagerId::HomebrewFormula && step.cleanup_old_kegs {
                        encode_homebrew_upgrade_target(&step.package_name, true)
                    } else {
                        step.package_name.clone()
                    };
                let request = AdapterRequest::Upgrade(UpgradeRequest {
                    package: Some(PackageRef {
                        manager: step.manager,
                        name: request_name,
                    }),
                });
                let _ =
                    tokio_runtime.block_on(submit_request_wait(&runtime, step.manager, request));
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
    let task_view = task_to_cli_task(task);

    let logs = store
        .list_task_logs(TaskId(task_id), TASK_FETCH_LIMIT)
        .map_err(|error| format!("failed to list task logs: {error}"))?
        .into_iter()
        .map(task_log_to_cli_record)
        .collect::<Vec<_>>();

    let output = helm_core::execution::task_output(TaskId(task_id));
    let output_payload = json!({
        "available": output.is_some(),
        "command": output.as_ref().and_then(|entry| entry.command.clone()),
        "stdout": output.as_ref().and_then(|entry| entry.stdout.clone()),
        "stderr": output.as_ref().and_then(|entry| entry.stderr.clone())
    });

    if options.json {
        emit_json_payload(
            "helm.cli.v1.diagnostics.task",
            json!({
                "task": task_view,
                "logs": logs,
                "output": output_payload
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
    if let Some(last_log) = logs.last() {
        println!(
            "  last_log: [{}] [{}] {}",
            last_log.created_at_unix, last_log.level, last_log.message
        );
    } else {
        println!("  last_log: -");
    }
    if output.is_some() {
        println!("  output_available: true");
        println!(
            "  command: {}",
            output_payload["command"].as_str().unwrap_or("-")
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
    for task in tasks {
        match task.status {
            TaskStatus::Queued => queued_tasks = queued_tasks.saturating_add(1),
            TaskStatus::Running => running_tasks = running_tasks.saturating_add(1),
            TaskStatus::Completed => completed_tasks = completed_tasks.saturating_add(1),
            TaskStatus::Failed => {
                failed_tasks = failed_tasks.saturating_add(1);
                failed_task_ids.push(task.id.0);
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
    let auto_check_for_updates =
        read_setting(store, "auto_check_for_updates").unwrap_or_else(|_| "false".to_string());
    let auto_check_frequency_minutes =
        read_setting(store, "auto_check_frequency_minutes").unwrap_or_else(|_| "1440".to_string());
    if options.json {
        emit_json_payload(
            "helm.cli.v1.settings.list",
            json!({
                "safe_mode": safe_mode,
                "homebrew_keg_auto_cleanup": homebrew_auto_cleanup,
                "database_path": db_path,
                "auto_check_for_updates": auto_check_for_updates,
                "auto_check_frequency_minutes": auto_check_frequency_minutes
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

    clear_manager_selected_executables();
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
        set_manager_selected_executable(manager, selected.map(PathBuf::from));
    }
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
        Some(AdapterTaskTerminalState::Failed(error)) => Err(format_core_error(error)),
        Some(AdapterTaskTerminalState::Cancelled(Some(error))) => Err(format_core_error(error)),
        Some(AdapterTaskTerminalState::Cancelled(None)) => {
            Err(format!("task {} was cancelled", task_id.0))
        }
        None => Err(format!(
            "task {} reached terminal state without outcome payload",
            task_id.0
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
    let payload = json!({
        "schema": schema,
        "schema_version": JSON_SCHEMA_VERSION,
        "generated_at": json_generated_at_unix(),
        "data": data
    });
    println!("{payload}");
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
    let preferences = store
        .list_manager_preferences()
        .map_err(|error| format!("failed to list manager preferences: {error}"))?;
    let mut map: HashMap<ManagerId, bool> = HashMap::new();
    for preference in preferences {
        map.insert(preference.manager, preference.enabled);
    }

    for manager in ManagerId::ALL {
        map.entry(manager)
            .or_insert_with(|| default_enabled_for_manager(manager));
    }
    Ok(map)
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
        let enabled = preference
            .map(|preference| preference.enabled)
            .unwrap_or_else(|| default_enabled_for_manager(descriptor.id));

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
            selected_executable_path: preference.and_then(|preference| {
                preference
                    .selected_executable_path
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
            }),
            selected_install_method: preference.and_then(|preference| {
                preference
                    .selected_install_method
                    .as_ref()
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
            }),
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
    let preferred_executable_path = preferences
        .get(&manager)
        .and_then(|preference| normalize_nonempty(preference.selected_executable_path.clone()));
    let selected_executable_path = resolve_selected_executable_path(
        preferred_executable_path,
        default_executable_path.clone(),
    );

    Ok(CliManagerExecutableStatus {
        manager_id: manager.as_str().to_string(),
        active_executable_path,
        executable_paths,
        default_executable_path,
        selected_executable_path,
    })
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
    raw.parse::<ManagerId>()
        .map_err(|_| format!("unknown manager id '{raw}'"))
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

fn parse_updates_run_preview_args(
    command_args: &[String],
    allow_yes: bool,
) -> Result<ParsedUpdatesRunPreviewArgs, String> {
    let mut include_pinned = false;
    let mut allow_os_updates = false;
    let mut yes = false;

    for arg in command_args {
        match arg.as_str() {
            "--include-pinned" => include_pinned = true,
            "--allow-os-updates" => allow_os_updates = true,
            "--yes" if allow_yes => yes = true,
            "--yes" => {
                return Err("--yes is only valid for 'updates run'".to_string());
            }
            other => {
                return Err(format!("unsupported updates argument '{}'", other));
            }
        }
    }

    Ok(ParsedUpdatesRunPreviewArgs {
        include_pinned,
        allow_os_updates,
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

fn print_tui_placeholder() {
    eprintln!("helm: TUI mode is not implemented yet; showing command help.");
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
        Command::SelfCmd => print_self_help_topic(path),
        Command::Completion => print_completion_help_topic(path),
        Command::InternalCoordinator | Command::Help | Command::Version => false,
    }
}

fn print_packages_help_topic(path: &[String]) -> bool {
    if path.is_empty() {
        print_packages_help();
        return true;
    }
    if path.len() != 1 {
        return false;
    }
    match path[0].as_str() {
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
        _ => false,
    }
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

fn print_help() {
    println!("Helm CLI");
    println!();
    println!("USAGE:");
    println!("  helm [--json] [-v|--verbose] [--wait|--detach] <command> [subcommand]");
    println!("  helm -V | --version");
    println!("  helm help");
    println!("  helm <command> help");
    println!("  helm help <command> [subcommand]");
    println!();
    println!("COMMANDS:");
    println!("  status                 Show overall snapshot summary");
    println!("  refresh                Run detection + refresh pipeline");
    println!("  search <query>         Search local package cache");
    println!("  ls                     List installed packages (alias)");
    println!("  packages [list|search|show|install|uninstall|upgrade|pin|unpin]");
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
    println!("  self [status|check|update|auto-check]");
    println!("                         Helm self-update status/check/update namespace");
    println!("  completion [bash|zsh|fish]");
    println!("                         Generate shell completion scripts");
    println!("  help                   Show this help");
    println!();
    println!("GLOBAL FLAGS:");
    println!("  --json                 Emit JSON output");
    println!("  -v, --verbose          Emit verbose diagnostics to stderr");
    println!("  --wait                 Wait for task completion (default)");
    println!("  --detach               Return after task submission");
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
    println!("  helm search <query>");
    println!();
    println!("DESCRIPTION:");
    println!("  Search local cached package metadata for the query.");
}

fn print_packages_help() {
    println!("USAGE:");
    println!("  helm packages <subcommand> [args]");
    println!("  helm ls [--limit <n>]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  list [--limit <n>]");
    println!("  search <query>");
    println!("  show <name> [--manager <id>]");
    println!("  install <name|name@manager> --manager <id> [--version <v>]");
    println!("  uninstall <name|name@manager> --manager <id>");
    println!("  upgrade <name|name@manager> --manager <id>");
    println!("  pin <name|name@manager> --manager <id> [--version <v>]");
    println!("  unpin <name|name@manager> --manager <id>");
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
    println!("  helm packages search <query>");
    println!();
    println!("DESCRIPTION:");
    println!("  Search local cached package metadata.");
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

fn print_updates_help() {
    println!("USAGE:");
    println!("  helm updates list [--manager <id>] [--limit <n>]");
    println!("  helm updates summary");
    println!("  helm updates preview [--include-pinned] [--allow-os-updates]");
    println!("  helm updates run --yes [--include-pinned] [--allow-os-updates]");
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
    println!("  helm updates preview [--include-pinned] [--allow-os-updates]");
    println!();
    println!("DESCRIPTION:");
    println!("  Build and display ordered upgrade steps without executing.");
}

fn print_updates_run_help() {
    println!("USAGE:");
    println!("  helm updates run --yes [--include-pinned] [--allow-os-updates]");
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
        "  Implemented keys: safe_mode, homebrew_keg_auto_cleanup, auto_check_for_updates, auto_check_frequency_minutes."
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
    println!("  helm self update");
    println!("  helm self auto-check status");
    println!("  helm self auto-check enable");
    println!("  helm self auto-check disable");
    println!("  helm self auto-check frequency <minutes>");
    println!();
    println!("DESCRIPTION:");
    println!("  Self-update namespace for Helm.");
    println!(
        "  Homebrew-formula installs are supported for check/update; other install paths return guidance."
    );
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
    println!("  helm self update");
    println!();
    println!("DESCRIPTION:");
    println!("  Apply Helm self-update for supported install methods.");
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
