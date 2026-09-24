//! Explicitly scoped, offline uv inventory through the shared process/runtime boundary.
//! Not registered by the app or CLI; write capabilities remain a separate gate.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::adapters::manager::{
    AdapterRequest, AdapterResponse, AdapterResult, ManagerAdapter, ensure_request_supported,
};
use crate::adapters::uv_tool::{
    UvToolListMode, UvToolObservation, parse_uv_tool_list, uv_tool_list_command,
};
use crate::adapters::uv_tool_scope::UvScopeBinding;
use crate::execution::{
    CommandSpec, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
    spawn_validated,
};
use crate::models::{
    ActionSafety, Capability, CoreError, CoreErrorKind, DetectionInfo, InstalledPackage,
    ManagerAction, ManagerDescriptor, ManagerId, PackageRef, TaskType,
};

pub(crate) const UV_READ_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
];
const OUTPUT_LIMIT: usize = 4 * 1024 * 1024;

/// Paths must already be selected by the caller; there is no PATH/store fallback.
/// This binds commands to a scope, not a claim of filesystem ownership or eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UvToolContext {
    executable: PathBuf,
    tool_dir: PathBuf,
    binding: Option<UvScopeBinding>,
}

impl UvToolContext {
    pub fn new(executable: PathBuf, tool_dir: PathBuf) -> AdapterResult<Self> {
        for path in [&executable, &tool_dir] {
            if !path.is_absolute()
                || path.file_name().is_none()
                || path
                    .to_str()
                    .is_none_or(|text| text.chars().any(char::is_control))
                || path
                    .components()
                    .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
            {
                return Err(uv_error(
                    ManagerAction::Detect,
                    CoreErrorKind::InvalidInput,
                    "uv requires explicit absolute executable and tool-store paths",
                ));
            }
        }
        Ok(Self {
            executable: executable.components().collect(),
            tool_dir: tool_dir.components().collect(),
            binding: None,
        })
    }

    pub(crate) fn with_binding(mut self, binding: UvScopeBinding) -> Self {
        self.binding = Some(binding);
        self
    }

    fn validate_binding(&self) -> AdapterResult<()> {
        if let Some(binding) = &self.binding {
            binding.validate()?;
        }
        Ok(())
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }
    pub fn tool_dir(&self) -> &Path {
        &self.tool_dir
    }

    fn scope_command(&self, command: CommandSpec) -> CommandSpec {
        // Installed inventory needs neither project/index configuration nor network access.
        command
            .arg("--directory")
            .arg("/")
            .arg("--no-config")
            .arg("--no-cache")
            .working_dir("/")
            .env(
                "UV_TOOL_DIR",
                self.tool_dir.to_str().expect("validated UTF-8 path"),
            )
            .env("UV_PYTHON_DOWNLOADS", "never")
            .env("UV_OFFLINE", "true")
    }

    fn request(&self, action: ManagerAction) -> ProcessSpawnRequest {
        let (command, task_type, seconds) = if action == ManagerAction::Detect {
            (
                CommandSpec::new(&self.executable).arg("--version"),
                TaskType::Detection,
                10,
            )
        } else {
            (
                uv_tool_list_command(&self.executable, UvToolListMode::Installed),
                TaskType::Refresh,
                60,
            )
        };
        let mut request = ProcessSpawnRequest::new(
            ManagerId::Uv,
            task_type,
            action,
            self.scope_command(command),
        )
        .timeout(Duration::from_secs(seconds))
        .idle_timeout(Duration::from_secs(10));
        request.private_output_limit = Some(OUTPUT_LIMIT);
        request
    }

    fn package_identifier(&self, name: &str) -> String {
        let digest = Sha256::digest(
            self.tool_dir
                .to_str()
                .expect("validated UTF-8 path")
                .as_bytes(),
        );
        format!("uv-tool:{digest:x}:{name}")
    }
}

pub struct ProcessUvToolSource {
    executor: Arc<dyn ProcessExecutor>,
    context: UvToolContext,
}

impl ProcessUvToolSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>, context: UvToolContext) -> Self {
        Self { executor, context }
    }

    pub fn context(&self) -> &UvToolContext {
        &self.context
    }

    fn run(&self, action: ManagerAction) -> AdapterResult<ProcessOutput> {
        self.context.validate_binding()?;
        let output = run_uv_request(self.executor.as_ref(), self.context.request(action))?;
        self.context.validate_binding()?;
        Ok(output)
    }

    pub fn detect(&self) -> AdapterResult<DetectionInfo> {
        let output = self.run(ManagerAction::Detect)?;
        let version = checked_version(&output)?;
        Ok(DetectionInfo {
            installed: true,
            executable_path: Some(self.context.executable.clone()),
            version: Some(version),
        })
    }

    pub fn list_installed(&self) -> AdapterResult<Vec<UvToolObservation>> {
        // Recheck the same selected executable on every read, not a stale cached version.
        self.detect()?;
        let output = self.run(ManagerAction::ListInstalled)?;
        parse_uv_tool_list(&output, UvToolListMode::Installed).map_err(|error| {
            uv_error(
                ManagerAction::ListInstalled,
                CoreErrorKind::ParseFailure,
                &error.to_string(),
            )
        })
    }
}

pub(crate) fn run_uv_request(
    executor: &dyn ProcessExecutor,
    request: ProcessSpawnRequest,
) -> AdapterResult<ProcessOutput> {
    let action = request.action;
    let process = spawn_validated(executor, request)
        .map_err(|error| uv_error(action, error.kind, "uv process could not start"))?;
    tokio::runtime::Handle::current()
        .block_on(process.wait())
        .map_err(|error| {
            uv_error(
                action,
                error.kind,
                "uv process did not produce a complete capture",
            )
        })
}

pub(crate) fn checked_version(output: &ProcessOutput) -> AdapterResult<String> {
    let failure = || {
        uv_error(
            ManagerAction::Detect,
            CoreErrorKind::UnsupportedCapability,
            "uv inventory requires a recognized stable 0.12 release from 0.12.9 through 0.12.18",
        )
    };
    if output.status != ProcessExitStatus::ExitCode(0) {
        return Err(uv_error(
            ManagerAction::Detect,
            CoreErrorKind::ProcessFailure,
            "uv version check did not exit successfully",
        ));
    }
    if output.stdout.len().saturating_add(output.stderr.len()) > OUTPUT_LIMIT
        || !output.stderr.is_empty()
    {
        return Err(uv_error(
            ManagerAction::Detect,
            CoreErrorKind::ParseFailure,
            "uv version check did not produce authoritative output",
        ));
    }
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|_| failure())?
        .trim();
    if text.chars().any(char::is_control) {
        return Err(failure());
    }
    let rest = text.strip_prefix("uv ").ok_or_else(failure)?;
    let version = rest.split_once(' ').map_or(rest, |(version, _)| version);
    let patch = version.strip_prefix("0.12.").ok_or_else(failure)?;
    if patch.is_empty() || !patch.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(failure());
    }
    let patch: u32 = patch.parse().map_err(|_| failure())?;
    if !(9..=18).contains(&patch) || version != format!("0.12.{patch}") {
        return Err(failure());
    }
    if let Some((_, suffix)) = rest.split_once(' ')
        && !(suffix.starts_with('(') && suffix.ends_with(')'))
    {
        return Err(failure());
    }
    Ok(version.to_owned())
}

pub struct UvToolReadAdapter {
    source: ProcessUvToolSource,
}

impl UvToolReadAdapter {
    pub fn new(source: ProcessUvToolSource) -> Self {
        Self { source }
    }
}

impl ManagerAdapter for UvToolReadAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        crate::registry::manager(ManagerId::Uv).expect("uv descriptor")
    }

    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }

    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        ensure_request_supported(self.descriptor(), &request)?;
        match request {
            AdapterRequest::Detect(_) => Ok(AdapterResponse::Detection(self.source.detect()?)),
            AdapterRequest::Refresh(_) | AdapterRequest::ListInstalled(_) => {
                let tools = self.source.list_installed()?;
                let installed = tools
                    .into_iter()
                    .map(|tool| InstalledPackage {
                        package_identifier: Some(
                            self.source.context.package_identifier(&tool.name),
                        ),
                        package: PackageRef {
                            manager: ManagerId::Uv,
                            name: tool.name,
                        },
                        installed_version: Some(tool.installed_version),
                        pinned: false,
                        runtime_state: Default::default(),
                    })
                    .collect();
                // Do not infer any update state from local inventory or latest discovery.
                Ok(AdapterResponse::SnapshotSync {
                    installed: Some(installed),
                    outdated: None,
                })
            }
            _ => Err(uv_error(
                request.action(),
                CoreErrorKind::UnsupportedCapability,
                "uv action is not enabled by the read-only integration",
            )),
        }
    }
}

fn uv_error(action: ManagerAction, kind: CoreErrorKind, message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Uv),
        task: Some(if action == ManagerAction::Detect {
            TaskType::Detection
        } else {
            TaskType::Refresh
        }),
        action: Some(action),
        kind,
        message: message.to_owned(),
    }
}
