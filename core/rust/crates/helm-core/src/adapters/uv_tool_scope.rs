//! Conservative discovery for the staged uv read adapter, not mutation ownership proof.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::adapters::manager::AdapterResult;
use crate::adapters::uv_tool_process::{UvToolContext, checked_version, run_uv_request};
use crate::execution::{
    CommandSpec, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskType};

const DISCOVERY_OUTPUT_LIMIT: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub enum UvExecutableSelection {
    /// An explicit selection never falls back to another executable on failure.
    Selected(PathBuf),
    /// Nonrecursive search; only direct `uv` children are considered.
    SearchDirectories(Vec<PathBuf>),
}

impl UvExecutableSelection {
    /// Capture the service environment, ignoring relative PATH entries rather than
    /// searching the current project. No login shell or shim resolution is used.
    pub fn current_environment() -> Self {
        let mut directories = std::env::var_os("PATH")
            .as_deref()
            .map(std::env::split_paths)
            .into_iter()
            .flatten()
            .filter(|path| valid_path(path))
            .collect::<Vec<_>>();
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
            && valid_path(&home)
        {
            directories.push(home.join(".local/bin"));
            directories.push(home.join(".cargo/bin"));
        }
        directories
            .extend(["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"].map(PathBuf::from));
        Self::SearchDirectories(directories)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UvExecutableCandidate {
    pub canonical_path: PathBuf,
    pub aliases: Vec<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UvResolvedToolScope {
    pub executable: UvExecutableCandidate,
    pub version: String,
    pub reported_tool_dir: PathBuf,
    context: UvToolContext,
}

impl UvResolvedToolScope {
    pub fn context(&self) -> &UvToolContext {
        &self.context
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UvScopeDiscovery {
    NotFound,
    SelectionRequired(Vec<UvExecutableCandidate>),
    /// Missing storage is an observation, never permission to clear cached inventory.
    ToolStoreMissing {
        executable: UvExecutableCandidate,
        version: String,
        reported_tool_dir: PathBuf,
    },
    Ready(UvResolvedToolScope),
}

pub struct UvToolDiscovery {
    executor: Arc<dyn ProcessExecutor>,
}

impl UvToolDiscovery {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    /// Runs on the same blocking/task boundary as the existing adapter sources.
    /// None uses uv's environment-selected/default store; an override must be absolute.
    pub fn discover(
        &self,
        selection: UvExecutableSelection,
        tool_dir_override: Option<PathBuf>,
    ) -> AdapterResult<UvScopeDiscovery> {
        let tool_dir_override =
            tool_dir_override.or_else(|| std::env::var_os("UV_TOOL_DIR").map(PathBuf::from));
        if tool_dir_override
            .as_ref()
            .is_some_and(|path| !valid_path(path))
        {
            return Err(scope_error(
                CoreErrorKind::InvalidInput,
                "uv tool directory override must be an absolute path",
            ));
        }
        let candidates = discover_candidates(selection)?;
        if candidates.is_empty() {
            return Ok(UvScopeDiscovery::NotFound);
        }
        if candidates.len() > 1 {
            return Ok(UvScopeDiscovery::SelectionRequired(candidates));
        }
        let executable = candidates.into_iter().next().expect("one candidate");
        let selected = &executable.aliases[0];
        let executable_identity = executable_identity(&executable.canonical_path)?;
        validate_executable_binding(selected, &executable.canonical_path, &executable_identity)?;
        let version_output = self.run(
            &executable.canonical_path,
            &["--version"],
            tool_dir_override.as_deref(),
        )?;
        let version = checked_version(&version_output)?;
        validate_executable_binding(selected, &executable.canonical_path, &executable_identity)?;
        let output = self.run(
            &executable.canonical_path,
            &["tool", "dir"],
            tool_dir_override.as_deref(),
        )?;
        validate_executable_binding(selected, &executable.canonical_path, &executable_identity)?;
        let reported_tool_dir = parse_tool_dir(&output)?;
        if let Some(expected) = &tool_dir_override
            && !same_directory(expected, &reported_tool_dir)
        {
            return Err(scope_error(
                CoreErrorKind::ParseFailure,
                "uv reported a different tool directory than requested",
            ));
        }
        let canonical_tool_dir = match fs::symlink_metadata(&reported_tool_dir) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(UvScopeDiscovery::ToolStoreMissing {
                    executable,
                    version,
                    reported_tool_dir,
                });
            }
            Err(_) => {
                return Err(scope_error(
                    CoreErrorKind::ProcessFailure,
                    "uv tool directory is inaccessible",
                ));
            }
            Ok(_) => canonical_path(&reported_tool_dir)?,
        };
        let tool_identity = directory_identity(&canonical_tool_dir)?;
        let binding = UvScopeBinding {
            selected_executable: selected.clone(),
            canonical_executable: executable.canonical_path.clone(),
            executable_identity,
            configured_tool_dir: tool_dir_override,
            reported_tool_dir: reported_tool_dir.clone(),
            canonical_tool_dir: canonical_tool_dir.clone(),
            tool_identity,
        };
        binding.validate()?;
        let context = UvToolContext::new(executable.canonical_path.clone(), canonical_tool_dir)?
            .with_binding(binding);
        Ok(UvScopeDiscovery::Ready(UvResolvedToolScope {
            executable,
            version,
            reported_tool_dir,
            context,
        }))
    }

    fn run(
        &self,
        executable: &Path,
        args: &[&str],
        tool_dir: Option<&Path>,
    ) -> AdapterResult<ProcessOutput> {
        let mut command = CommandSpec::new(executable)
            .args([
                "--color",
                "never",
                "--no-progress",
                "--offline",
                "--no-config",
                "--no-cache",
                "--directory",
                "/",
            ])
            .args(args.iter().copied())
            .working_dir("/")
            .env("UV_PYTHON_DOWNLOADS", "never")
            .env("UV_OFFLINE", "true");
        if let Some(tool_dir) = tool_dir {
            command = command.env("UV_TOOL_DIR", tool_dir.to_str().expect("validated path"));
        }
        let mut request = ProcessSpawnRequest::new(
            ManagerId::Uv,
            TaskType::Detection,
            ManagerAction::Detect,
            command,
        )
        .timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(10));
        request.private_output_limit = Some(DISCOVERY_OUTPUT_LIMIT);
        let output = run_uv_request(self.executor.as_ref(), request)?;
        if output.stdout.len().saturating_add(output.stderr.len()) > DISCOVERY_OUTPUT_LIMIT {
            return Err(scope_error(
                CoreErrorKind::ParseFailure,
                "uv discovery output exceeded its capture limit",
            ));
        }
        Ok(output)
    }
}

fn discover_candidates(
    selection: UvExecutableSelection,
) -> AdapterResult<Vec<UvExecutableCandidate>> {
    let (paths, selected) = match selection {
        UvExecutableSelection::Selected(path) => (vec![path], true),
        UvExecutableSelection::SearchDirectories(dirs) => {
            if dirs.iter().any(|path| !valid_path(path)) || dirs.len() > 256 {
                return Err(scope_error(
                    CoreErrorKind::InvalidInput,
                    "uv search requires a bounded list of absolute directories",
                ));
            }
            (
                dirs.into_iter().map(|path| path.join("uv")).collect(),
                false,
            )
        }
    };
    collect_candidates(paths, selected)
}

pub(crate) fn collect_candidates(
    paths: Vec<PathBuf>,
    selected: bool,
) -> AdapterResult<Vec<UvExecutableCandidate>> {
    let mut candidates: Vec<UvExecutableCandidate> = Vec::new();
    for path in paths {
        if !valid_path(&path) {
            return Err(scope_error(
                CoreErrorKind::InvalidInput,
                "uv executable must be an absolute path",
            ));
        }
        match fs::symlink_metadata(&path) {
            Err(error) if !selected && error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                return Err(scope_error(
                    CoreErrorKind::ProcessFailure,
                    "selected uv executable is unavailable",
                ));
            }
            Ok(_) => {}
        }
        let canonical = canonical_path(&path)?;
        // Do not invoke a project-dependent version-manager dispatcher as uv.
        if path.components().any(|part| part.as_os_str() == "shims")
            || canonical
                .components()
                .any(|part| part.as_os_str() == "shims")
            || canonical
                .file_name()
                .is_some_and(|name| ["mise", "asdf", "rtx"].iter().any(|shim| name == *shim))
        {
            return Err(scope_error(
                CoreErrorKind::UnsupportedCapability,
                "uv shim requires selection of a concrete executable",
            ));
        }
        executable_identity(&canonical)?;
        let alias: PathBuf = path.components().collect();
        if let Some(existing) = candidates
            .iter_mut()
            .find(|item| item.canonical_path == canonical)
        {
            if !existing.aliases.contains(&alias) {
                existing.aliases.push(alias);
            }
        } else {
            candidates.push(UvExecutableCandidate {
                canonical_path: canonical,
                aliases: vec![alias],
            });
        }
    }
    Ok(candidates)
}

fn parse_tool_dir(output: &ProcessOutput) -> AdapterResult<PathBuf> {
    if output.status != ProcessExitStatus::ExitCode(0) {
        return Err(scope_error(
            CoreErrorKind::ProcessFailure,
            "uv tool directory query failed",
        ));
    }
    let invalid = || {
        scope_error(
            CoreErrorKind::ParseFailure,
            "uv did not report one authoritative absolute tool directory",
        )
    };
    if !output.stderr.is_empty() || output.stdout.len() > DISCOVERY_OUTPUT_LIMIT {
        return Err(invalid());
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| invalid())?;
    let line = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    let path = PathBuf::from(line);
    if !valid_path(&path) {
        return Err(invalid());
    }
    Ok(path.components().collect())
}

pub(crate) fn valid_path(path: &Path) -> bool {
    path.is_absolute()
        && path.file_name().is_some()
        && path
            .to_str()
            .is_some_and(|text| !text.chars().any(char::is_control))
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
}

fn same_directory(left: &Path, right: &Path) -> bool {
    left.components().eq(right.components())
        || matches!((left.canonicalize(), right.canonicalize()), (Ok(left), Ok(right)) if left == right)
}

pub(crate) fn canonical_path(path: &Path) -> AdapterResult<PathBuf> {
    let canonical = path.canonicalize().map_err(|_| {
        scope_error(
            CoreErrorKind::ProcessFailure,
            "uv scope path cannot be resolved",
        )
    })?;
    if !valid_path(&canonical) {
        return Err(scope_error(
            CoreErrorKind::InvalidInput,
            "uv scope path is unsupported",
        ));
    }
    Ok(canonical)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FileIdentity {
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl FileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        let _ = metadata;
        Self {
            #[cfg(unix)]
            device: metadata.dev(),
            #[cfg(unix)]
            inode: metadata.ino(),
        }
    }
}

fn executable_identity(path: &Path) -> AdapterResult<FileIdentity> {
    let metadata = fs::metadata(path).map_err(|_| {
        scope_error(
            CoreErrorKind::ProcessFailure,
            "uv executable is inaccessible",
        )
    })?;
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    };
    #[cfg(not(unix))]
    let executable = true;
    if !metadata.is_file() || !executable {
        return Err(scope_error(
            CoreErrorKind::InvalidInput,
            "uv selection is not an executable file",
        ));
    }
    Ok(FileIdentity::from_metadata(&metadata))
}

fn directory_identity(path: &Path) -> AdapterResult<FileIdentity> {
    let metadata = fs::metadata(path).map_err(|_| {
        scope_error(
            CoreErrorKind::ProcessFailure,
            "uv tool directory is inaccessible",
        )
    })?;
    if !metadata.is_dir() {
        return Err(scope_error(
            CoreErrorKind::InvalidInput,
            "uv tool store is not a directory",
        ));
    }
    Ok(FileIdentity::from_metadata(&metadata))
}

fn validate_executable_binding(
    selected: &Path,
    canonical: &Path,
    identity: &FileIdentity,
) -> AdapterResult<()> {
    if canonical_path(selected)? != canonical || &executable_identity(canonical)? != identity {
        return Err(scope_error(
            CoreErrorKind::ProcessFailure,
            "uv executable changed; rediscovery is required",
        ));
    }
    Ok(())
}

/// Read-time evidence only. It never authorizes mutation or claims manager ownership.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UvScopeBinding {
    selected_executable: PathBuf,
    canonical_executable: PathBuf,
    executable_identity: FileIdentity,
    configured_tool_dir: Option<PathBuf>,
    reported_tool_dir: PathBuf,
    canonical_tool_dir: PathBuf,
    tool_identity: FileIdentity,
}

impl UvScopeBinding {
    pub(crate) fn validate(&self) -> AdapterResult<()> {
        validate_executable_binding(
            &self.selected_executable,
            &self.canonical_executable,
            &self.executable_identity,
        )?;
        if let Some(configured) = &self.configured_tool_dir
            && canonical_path(configured)? != self.canonical_tool_dir
        {
            return Err(scope_error(
                CoreErrorKind::ProcessFailure,
                "uv configured tool directory changed; rediscovery is required",
            ));
        }
        if canonical_path(&self.reported_tool_dir)? != self.canonical_tool_dir
            || directory_identity(&self.canonical_tool_dir)? != self.tool_identity
        {
            return Err(scope_error(
                CoreErrorKind::ProcessFailure,
                "uv tool directory changed; rediscovery is required",
            ));
        }
        Ok(())
    }
}

pub(crate) fn scope_error(kind: CoreErrorKind, message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Uv),
        task: Some(TaskType::Detection),
        action: Some(ManagerAction::Detect),
        kind,
        message: message.into(),
    }
}
