//! Global-tool lifecycle. uv owns resolution and installation; Helm owns scope,
//! review-target validation and observation of the actual result.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use uv_pep440::Version;

use super::manager::*;
use super::uv_tool::{UvToolObservation, normalize_name};
use super::uv_tool_manifest::{UvToolManifest, resolved_version, validate_options};
use super::uv_tool_process::{ProcessUvToolSource, UvToolContext, run_uv_request};
use super::uv_tool_scope::{UvExecutableSelection, UvScopeDiscovery, UvToolDiscovery};
use crate::execution::{
    CommandSpec, ProcessExecutor, ProcessExitStatus, ProcessOutput, ProcessSpawnRequest,
};
use crate::models::*;

pub(crate) const UV_CAPABILITIES: &[Capability] = &[
    Capability::Detect,
    Capability::Refresh,
    Capability::ListInstalled,
    Capability::ListOutdated,
    Capability::Search,
    Capability::Install,
    Capability::Upgrade,
    Capability::Uninstall,
];

pub struct UvToolAdapter {
    executor: Arc<dyn ProcessExecutor>,
    selection: Option<UvExecutableSelection>,
    tool_dir: Option<PathBuf>,
}

impl UvToolAdapter {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self {
            executor,
            selection: None,
            tool_dir: None,
        }
    }

    /// Explicit scoping for callers such as isolated lifecycle certification.
    pub fn with_scope(
        executor: Arc<dyn ProcessExecutor>,
        executable: PathBuf,
        tool_dir: PathBuf,
    ) -> Self {
        Self {
            executor,
            selection: Some(UvExecutableSelection::Selected(executable)),
            tool_dir: Some(tool_dir),
        }
    }

    fn discover(&self) -> AdapterResult<UvScopeDiscovery> {
        let selection = self
            .selection
            .clone()
            .or_else(|| {
                crate::execution::manager_selected_executable(ManagerId::Uv)
                    .map(UvExecutableSelection::Selected)
            })
            .map(Ok)
            .unwrap_or_else(discover_runtime_selection)?;
        UvToolDiscovery::new(self.executor.clone()).discover(selection, self.tool_dir.clone())
    }

    fn source(&self, context: &UvToolContext) -> ProcessUvToolSource {
        ProcessUvToolSource::new(self.executor.clone(), context.clone())
    }

    fn run(
        &self,
        context: &UvToolContext,
        action: ManagerAction,
        command: CommandSpec,
    ) -> AdapterResult<ProcessOutput> {
        context.validate_binding()?;
        let mut command = command
            .args(["--color", "never", "--no-progress", "--directory", "/"])
            .working_dir("/")
            .env("UV_TOOL_DIR", utf8(context.tool_dir())?)
            .env("UV_PYTHON_DOWNLOADS", "never")
            .env("LC_ALL", "C");
        for key in [
            "UV_PROJECT",
            "UV_WORKING_DIR",
            "UV_CONSTRAINT",
            "UV_OVERRIDE",
            "UV_EXCLUDE",
            "UV_BUILD_CONSTRAINT",
            "UV_SYSTEM_PYTHON",
            "UV_PREVIEW",
            "UV_PREVIEW_FEATURES",
            "UV_PYTHON_VERSION",
            "UV_PYTHON_PLATFORM",
            "UV_NO_CONFIG",
            "UV_CONFIG_FILE",
        ] {
            command = command.remove_env(key);
        }
        if action != ManagerAction::Install {
            command = command.remove_env("UV_PYTHON");
        }
        let mut request =
            ProcessSpawnRequest::new(ManagerId::Uv, task_type(action), action, command)
                .timeout(Duration::from_secs(
                    if matches!(
                        action,
                        ManagerAction::Install | ManagerAction::Upgrade | ManagerAction::Uninstall
                    ) {
                        600
                    } else {
                        120
                    },
                ))
                .idle_timeout(Duration::from_secs(60));
        request.private_output_limit = Some(4 * 1024 * 1024);
        let output = run_uv_request(self.executor.as_ref(), request)?;
        context.validate_binding()?;
        if output.status != ProcessExitStatus::ExitCode(0) {
            return Err(error(
                action,
                CoreErrorKind::ProcessFailure,
                "uv operation failed; installed state must be refreshed",
            ));
        }
        if output.stdout.len().saturating_add(output.stderr.len()) > 4 * 1024 * 1024 {
            return Err(error(
                action,
                CoreErrorKind::ParseFailure,
                "uv output exceeded its capture limit",
            ));
        }
        Ok(output)
    }

    fn observe(
        &self,
        context: &UvToolContext,
        tool: &UvToolObservation,
    ) -> AdapterResult<ToolEvidence> {
        context.validate_binding()?;
        let environment = context.tool_dir().join(&tool.name);
        require_owned_directory(&environment)?;
        let receipt_path = environment.join("uv-receipt.toml");
        let receipt = read_private_file(&receipt_path)?;
        let manifest = UvToolManifest::parse(&receipt, tool).map_err(|_| unsupported())?;
        manifest
            .validate_entrypoints(&environment)
            .map_err(|_| unsupported())?;
        let python = environment.join("bin/python");
        if !python.is_file() {
            return Err(unsupported());
        }
        let metadata = fs::metadata(&python).map_err(|_| unsupported())?;
        let evidence = ToolEvidence {
            receipt_path,
            receipt,
            manifest,
            python,
            python_stamp: stamp(&metadata),
        };
        evidence.validate()?;
        context.validate_binding()?;
        Ok(evidence)
    }

    fn resolve(
        &self,
        context: &UvToolContext,
        tool: &UvToolObservation,
        evidence: &ToolEvidence,
    ) -> AdapterResult<String> {
        let scratch = tempfile::Builder::new()
            .prefix("helm-uv-resolve-")
            .tempdir()
            .map_err(|_| unsupported())?;
        let configuration = effective_configuration(&evidence.manifest.options)?;
        let config_path = scratch.path().join("uv.toml");
        let requirements_path = scratch.path().join("requirements.in");
        let constraints_path = scratch.path().join("constraints.txt");
        write_private_file(&config_path, &configuration)?;
        write_private_file(
            &requirements_path,
            evidence.manifest.requirements.as_bytes(),
        )?;
        write_private_file(&constraints_path, evidence.manifest.constraints.as_bytes())?;
        let mut command = CommandSpec::new(context.executable()).args([
            "pip",
            "compile",
            utf8(&requirements_path)?,
            "--constraints",
            utf8(&constraints_path)?,
            "--python",
            utf8(&evidence.python)?,
            "--upgrade",
            "--no-build",
            "--no-header",
            "--no-annotate",
            "--config-file",
            utf8(&config_path)?,
        ]);
        // Do not reuse an output file or installed-environment preferences as resolution pins.
        command = command.env("UV_NO_CACHE", "true");
        evidence.validate()?;
        let output = self.run(context, ManagerAction::ListOutdated, command)?;
        evidence.validate()?;
        if configuration != effective_configuration(&evidence.manifest.options)? {
            return Err(stale());
        }
        resolved_version(&output.stdout, &tool.name).map_err(|_| {
            error(
                ManagerAction::ListOutdated,
                CoreErrorKind::ParseFailure,
                "uv resolution did not produce an authoritative target",
            )
        })
    }

    fn outdated(
        &self,
        context: &UvToolContext,
        tools: &[UvToolObservation],
    ) -> AdapterResult<Vec<OutdatedPackage>> {
        let mut outdated = Vec::new();
        for tool in tools {
            let evidence = self.observe(context, tool)?;
            let candidate = self.resolve(context, tool, &evidence)?;
            let installed: Version = tool.installed_version.parse().map_err(|_| unsupported())?;
            let resolved: Version = candidate.parse().map_err(|_| unsupported())?;
            if resolved > installed {
                outdated.push(OutdatedPackage {
                    package: PackageRef {
                        manager: ManagerId::Uv,
                        name: tool.name.clone(),
                    },
                    package_identifier: Some(context.package_identifier(&tool.name)),
                    installed_version: Some(tool.installed_version.clone()),
                    candidate_version: candidate,
                    pinned: false,
                    restart_required: false,
                    runtime_state: Default::default(),
                });
            }
        }
        // Never publish a result assembled across a changing store.
        if self.source(context).list_installed()? != tools {
            return Err(stale());
        }
        Ok(outdated)
    }

    fn mutate(
        &self,
        context: &UvToolContext,
        request: AdapterRequest,
    ) -> AdapterResult<AdapterResponse> {
        let action = request.action();
        let (package, target, expected) = match request {
            AdapterRequest::Install(r) => (r.package, r.target_name, r.version),
            AdapterRequest::Uninstall(r) => (r.package, r.target_name, r.version),
            AdapterRequest::Upgrade(r) => {
                (r.package.ok_or_else(unsupported)?, r.target_name, r.version)
            }
            _ => return Err(unsupported()),
        };
        if package.manager != ManagerId::Uv
            || normalize_name(&package.name).as_deref() != Some(package.name.as_str())
        {
            return Err(error(
                action,
                CoreErrorKind::InvalidInput,
                "uv requires one normalized distribution name",
            ));
        }
        let identifier = context.package_identifier(&package.name);
        if target
            .as_ref()
            .is_some_and(|target| target != &package.name && target != &identifier)
        {
            return Err(stale());
        }
        if expected
            .as_ref()
            .is_some_and(|version| version.parse::<Version>().is_err())
        {
            return Err(error(
                action,
                CoreErrorKind::InvalidInput,
                "uv requires a PEP 440 target version",
            ));
        }
        require_owned_directory(context.tool_dir())?;
        let before = self.source(context).list_installed()?;
        let existing = before.iter().find(|tool| tool.name == package.name);
        let evidence = existing
            .map(|tool| self.observe(context, tool))
            .transpose()?;
        let source_options = evidence
            .as_ref()
            .map(|e| e.manifest.options.clone())
            .unwrap_or_default();
        let configuration = effective_configuration(&source_options)?;
        let environment = uv_environment_fingerprint();
        let mut command = CommandSpec::new(context.executable());
        let expected = match action {
            ManagerAction::Install => {
                // Reinstall/force could overwrite another tool's entrypoints or alter saved policy.
                if existing.is_some() {
                    return Err(error(
                        action,
                        CoreErrorKind::InvalidInput,
                        "uv tool is already installed; use its reviewed upgrade",
                    ));
                }
                let specifier = expected.as_ref().map_or_else(
                    || package.name.clone(),
                    |version| format!("{}=={version}", package.name),
                );
                command = command.args(["tool", "install", &specifier, "--no-build"]);
                expected
            }
            ManagerAction::Upgrade => {
                let tool = existing.ok_or_else(stale)?;
                if expected.is_none() || target.as_deref() != Some(identifier.as_str()) {
                    return Err(stale());
                }
                let evidence = evidence.as_ref().ok_or_else(stale)?;
                let candidate = self.resolve(context, tool, evidence)?;
                if expected
                    .as_ref()
                    .is_some_and(|version| version != &candidate)
                    || candidate.parse::<Version>().map_err(|_| unsupported())?
                        <= tool
                            .installed_version
                            .parse::<Version>()
                            .map_err(|_| unsupported())?
                {
                    return Err(stale());
                }
                // uv treats this as an additional constraint, without replacing the saved requirement.
                command = command.args([
                    "tool",
                    "upgrade",
                    &format!("{}=={candidate}", package.name),
                    "--no-build",
                ]);
                Some(candidate)
            }
            ManagerAction::Uninstall => {
                let tool = existing.ok_or_else(stale)?;
                if expected
                    .as_ref()
                    .is_some_and(|v| v != &tool.installed_version)
                {
                    return Err(stale());
                }
                command = command.args(["tool", "uninstall", &package.name]);
                None
            }
            _ => return Err(unsupported()),
        };
        if let Some(evidence) = &evidence {
            evidence.validate()?;
            let bin = evidence
                .manifest
                .entrypoints
                .first()
                .and_then(|path| path.parent())
                .ok_or_else(unsupported)?;
            for path in &evidence.manifest.entrypoints {
                require_owned_directory(path.parent().ok_or_else(unsupported)?)?;
                if path.parent() != Some(bin) {
                    return Err(unsupported());
                }
            }
            command = command.env("UV_TOOL_BIN_DIR", utf8(bin)?);
        } else {
            let output = self.run(
                context,
                ManagerAction::Detect,
                CommandSpec::new(context.executable()).args([
                    "tool",
                    "dir",
                    "--bin",
                    "--no-config",
                    "--offline",
                ]),
            )?;
            if !output.stderr.is_empty() {
                return Err(unsupported());
            }
            let text = std::str::from_utf8(&output.stdout)
                .map_err(|_| unsupported())?
                .trim_end_matches(['\r', '\n']);
            let bin = Path::new(text);
            if !super::uv_tool_scope::valid_path(bin) {
                return Err(unsupported());
            }
            ensure_owned_directory(bin)?;
            command = command.env("UV_TOOL_BIN_DIR", text);
        }
        if self.source(context).list_installed()? != before
            || configuration != effective_configuration(&source_options)?
            || environment != uv_environment_fingerprint()
        {
            return Err(stale());
        }
        let scratch = tempfile::Builder::new()
            .prefix("helm-uv-action-")
            .tempdir()
            .map_err(|_| unsupported())?;
        let config_path = scratch.path().join("uv.toml");
        write_private_file(&config_path, &effective_configuration(&toml::Table::new())?)?;
        command = command.args(["--config-file", utf8(&config_path)?]);
        self.run(context, action, command)?;
        let after = self
            .source(context)
            .list_installed()
            .map_err(|_| verification_failed(action))?;
        let observed = after.iter().find(|tool| tool.name == package.name);
        if action == ManagerAction::Uninstall {
            if observed.is_some()
                || evidence.as_ref().is_some_and(|e| {
                    e.manifest
                        .entrypoints
                        .iter()
                        .any(|p| fs::symlink_metadata(p).is_ok())
                })
            {
                return Err(verification_failed(action));
            }
        } else {
            let observed = observed.ok_or_else(|| verification_failed(action))?;
            if expected
                .as_ref()
                .is_some_and(|version| version != &observed.installed_version)
            {
                return Err(verification_failed(action));
            }
            let after_evidence = self
                .observe(context, observed)
                .map_err(|_| verification_failed(action))?;
            if let Some(evidence) = &evidence {
                let after_config = effective_configuration(&after_evidence.manifest.options)
                    .map_err(|_| verification_failed(action))?;
                if evidence.manifest.policy != after_evidence.manifest.policy
                    || comparable_configuration(&configuration)?
                        != comparable_configuration(&after_config)?
                {
                    return Err(verification_failed(action));
                }
            }
        }
        let unaffected = |tools: &[UvToolObservation]| {
            tools
                .iter()
                .filter(|tool| tool.name != package.name)
                .cloned()
                .collect::<Vec<_>>()
        };
        if unaffected(&before) != unaffected(&after) {
            return Err(verification_failed(action));
        }
        Ok(AdapterResponse::Mutation(MutationResult {
            before_version: existing.map(|tool| tool.installed_version.clone()),
            after_version: observed.map(|tool| tool.installed_version.clone()),
            package,
            package_identifier: Some(identifier),
            action,
        }))
    }
}

impl ManagerAdapter for UvToolAdapter {
    fn descriptor(&self) -> &ManagerDescriptor {
        crate::registry::manager(ManagerId::Uv).expect("uv descriptor")
    }
    fn action_safety(&self, action: ManagerAction) -> ActionSafety {
        action.safety()
    }
    fn execute(&self, request: AdapterRequest) -> AdapterResult<AdapterResponse> {
        ensure_request_supported(self.descriptor(), &request)?;
        let scope = self.discover()?;
        if matches!(request, AdapterRequest::Detect(_)) {
            return Ok(AdapterResponse::Detection(match scope {
                UvScopeDiscovery::NotFound => DetectionInfo {
                    installed: false,
                    executable_path: None,
                    version: None,
                },
                UvScopeDiscovery::Ready(scope) => DetectionInfo {
                    installed: true,
                    executable_path: Some(scope.executable.canonical_path),
                    version: Some(scope.version),
                },
                UvScopeDiscovery::ToolStoreMissing {
                    executable,
                    version,
                    ..
                } => DetectionInfo {
                    installed: true,
                    executable_path: Some(executable.canonical_path),
                    version: Some(version),
                },
                UvScopeDiscovery::SelectionRequired(_) => {
                    return Err(error(
                        ManagerAction::Detect,
                        CoreErrorKind::InvalidInput,
                        "select one concrete uv installation before using its tools",
                    ));
                }
            }));
        }
        let scope = if let UvScopeDiscovery::ToolStoreMissing {
            ref reported_tool_dir,
            ..
        } = scope
        {
            if !matches!(request, AdapterRequest::Install(_)) {
                return Err(unsupported());
            }
            ensure_owned_directory(reported_tool_dir)?;
            self.discover()?
        } else {
            scope
        };
        let UvScopeDiscovery::Ready(scope) = scope else {
            return Err(unsupported());
        };
        let context = scope.context();
        match request {
            AdapterRequest::Refresh(_) | AdapterRequest::ListOutdated(_) => {
                let tools = self.source(context).list_installed()?;
                let outdated = self.outdated(context, &tools)?;
                Ok(AdapterResponse::SnapshotSync {
                    installed: Some(installed(context, tools)),
                    outdated: Some(outdated),
                })
            }
            AdapterRequest::ListInstalled(_) => Ok(AdapterResponse::InstalledPackages(installed(
                context,
                self.source(context).list_installed()?,
            ))),
            AdapterRequest::Search(request) => {
                let needle = request.query.text.to_lowercase();
                let packages = installed(context, self.source(context).list_installed()?);
                Ok(AdapterResponse::SearchResults(
                    packages
                        .into_iter()
                        .filter(|p| !needle.is_empty() && p.package.name.contains(&needle))
                        .map(|p| CachedSearchResult {
                            result: PackageCandidate {
                                package: p.package,
                                package_identifier: p.package_identifier,
                                version: p.installed_version,
                                summary: None,
                            },
                            source_manager: ManagerId::Uv,
                            originating_query: request.query.text.clone(),
                            cached_at: request.query.issued_at,
                        })
                        .collect(),
                ))
            }
            request => self.mutate(context, request),
        }
    }
}

fn installed(context: &UvToolContext, tools: Vec<UvToolObservation>) -> Vec<InstalledPackage> {
    tools
        .into_iter()
        .map(|tool| InstalledPackage {
            package_identifier: Some(context.package_identifier(&tool.name)),
            package: PackageRef {
                manager: ManagerId::Uv,
                name: tool.name,
            },
            installed_version: Some(tool.installed_version),
            pinned: false,
            runtime_state: Default::default(),
        })
        .collect()
}

struct ToolEvidence {
    receipt_path: PathBuf,
    receipt: Vec<u8>,
    manifest: UvToolManifest,
    python: PathBuf,
    python_stamp: String,
}

impl ToolEvidence {
    fn validate(&self) -> AdapterResult<()> {
        if read_private_file(&self.receipt_path)? != self.receipt
            || fs::metadata(&self.python).map(|m| stamp(&m)).ok().as_ref()
                != Some(&self.python_stamp)
        {
            return Err(stale());
        }
        self.manifest
            .validate_entrypoints(self.receipt_path.parent().ok_or_else(unsupported)?)
            .map_err(|_| stale())
    }
}

fn stamp(metadata: &fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!(
            "{}:{}:{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec()
        )
    }
    #[cfg(not(unix))]
    {
        format!("{}:{:?}", metadata.len(), metadata.modified())
    }
}

fn require_owned_directory(path: &Path) -> AdapterResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| unsupported())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(unsupported());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no preconditions and reads the process credential.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            return Err(error(
                ManagerAction::Install,
                CoreErrorKind::InvalidInput,
                "uv tool storage must be privately user-owned; elevation is not supported",
            ));
        }
    }
    Ok(())
}

fn ensure_owned_directory(path: &Path) -> AdapterResult<()> {
    if !super::uv_tool_scope::valid_path(path) {
        return Err(unsupported());
    }
    match fs::symlink_metadata(path) {
        Ok(_) => require_owned_directory(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            ensure_owned_directory(path.parent().ok_or_else(unsupported)?)?;
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path).map_err(|_| unsupported())?;
            require_owned_directory(path)
        }
        Err(_) => Err(unsupported()),
    }
}

fn read_private_file(path: &Path) -> AdapterResult<Vec<u8>> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let mut file = options.open(path).map_err(|_| unsupported())?;
    let metadata = file.metadata().map_err(|_| unsupported())?;
    if !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(unsupported());
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| unsupported())?;
    if bytes.len() > 1024 * 1024
        || stamp(&metadata) != stamp(&file.metadata().map_err(|_| unsupported())?)
    {
        return Err(stale());
    }
    Ok(bytes)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> AdapterResult<()> {
    use std::io::Write;
    let mut options = File::options();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|_| unsupported())
}

/// Tool commands ignore project configuration. Mirror the supported global options
/// for pip's resolver, intentionally excluding its unrelated [pip] overrides.
fn effective_configuration(receipt_options: &toml::Table) -> AdapterResult<Vec<u8>> {
    if environment_flag("UV_NO_CONFIG")? {
        return toml::to_string(receipt_options)
            .map(String::into_bytes)
            .map_err(|_| unsupported());
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut config = toml::Table::new();
    let paths = if let Some(path) = std::env::var_os("UV_CONFIG_FILE") {
        vec![PathBuf::from(path)]
    } else {
        let user = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|home| home.join(".config")));
        let directories = std::env::var_os("XDG_CONFIG_DIRS")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "/etc/xdg".into());
        let system = std::env::split_paths(&directories)
            .take_while(|path| !path.as_os_str().is_empty())
            .collect::<Vec<_>>();
        let mut paths = if environment_flag("UV_NO_SYSTEM_CONFIG")? {
            Vec::new()
        } else {
            system_configuration(system)?
        };
        if let Some(user) = user {
            paths.push(user.join("uv/uv.toml"));
        }
        paths
    };
    for path in paths {
        if !super::uv_tool_scope::valid_path(&path) {
            return Err(unsupported());
        }
        match fs::symlink_metadata(&path) {
            Err(e)
                if e.kind() == std::io::ErrorKind::NotFound
                    && std::env::var_os("UV_CONFIG_FILE").is_none() =>
            {
                continue;
            }
            Err(_) => return Err(unsupported()),
            Ok(_) => {}
        }
        let bytes = read_private_file(&path)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| unsupported())?;
        let mut values: toml::Table = toml::from_str(text).map_err(|_| unsupported())?;
        values.remove("pip");
        validate_options(&values).map_err(|_| unsupported())?;
        merge_options(&mut config, values)?;
    }
    let mut receipt_options = receipt_options.clone();
    restore_index_credentials(&config, &mut receipt_options);
    merge_options(&mut config, receipt_options)?;
    toml::to_string(&config)
        .map(String::into_bytes)
        .map_err(|_| unsupported())
}

fn uv_environment_fingerprint() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    let mut values = std::env::vars_os()
        .filter(|(key, _)| key.to_string_lossy().starts_with("UV_"))
        .collect::<Vec<_>>();
    values.sort();
    values
}

fn environment_flag(key: &str) -> AdapterResult<bool> {
    match std::env::var(key) {
        Err(std::env::VarError::NotPresent) => Ok(false),
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(unsupported()),
        },
        Err(_) => Err(unsupported()),
    }
}

fn merge_options(base: &mut toml::Table, overlay: toml::Table) -> AdapterResult<()> {
    for (key, mut value) in overlay {
        // uv Combine places higher-precedence array entries first.
        if let (Some(high), Some(low)) = (
            value.as_array_mut(),
            base.get(&key).and_then(toml::Value::as_array),
        ) {
            high.extend(low.iter().cloned());
            if high.len() > 1024 {
                return Err(unsupported());
            }
        }
        base.insert(key, value);
    }
    Ok(())
}

fn system_configuration(mut roots: Vec<PathBuf>) -> AdapterResult<Vec<PathBuf>> {
    roots.push(PathBuf::from("/etc"));
    for root in roots {
        let path = root.join("uv/uv.toml");
        if !super::uv_tool_scope::valid_path(&path) {
            return Err(unsupported());
        }
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(vec![path]),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(unsupported()),
        }
    }
    Ok(Vec::new())
}

fn restore_index_credentials(config: &toml::Table, receipt: &mut toml::Table) {
    let parse = |table: &toml::Table| {
        table
            .get("index-url")
            .and_then(toml::Value::as_str)
            .and_then(|value| url::Url::parse(value).ok())
    };
    let (Some(mut stored), Some(mut configured)) = (parse(receipt), parse(config)) else {
        return;
    };
    if !stored.username().is_empty() || stored.password().is_some() {
        return;
    }
    let _ = configured.set_username("");
    let _ = configured.set_password(None);
    stored.set_path(stored.path().trim_end_matches('/').to_owned().as_str());
    configured.set_path(configured.path().trim_end_matches('/').to_owned().as_str());
    if stored == configured {
        receipt.insert("index-url".into(), config["index-url"].clone());
    }
}

fn comparable_configuration(bytes: &[u8]) -> AdapterResult<toml::Table> {
    let mut table: toml::Table =
        toml::from_str(std::str::from_utf8(bytes).map_err(|_| unsupported())?)
            .map_err(|_| unsupported())?;
    table.remove("no-build");
    // uv saves effective options back into the receipt. Re-combining with the
    // same global options can duplicate sources without changing their priority.
    for (_, value) in table.iter_mut() {
        if let Some(array) = value.as_array_mut() {
            let mut unique = Vec::new();
            for value in array.drain(..) {
                if !unique.contains(&value) {
                    unique.push(value);
                }
            }
            *array = unique;
        }
    }
    Ok(table)
}

fn discover_runtime_selection() -> AdapterResult<UvExecutableSelection> {
    let UvExecutableSelection::SearchDirectories(mut directories) =
        UvExecutableSelection::current_environment()
    else {
        unreachable!("environment discovery is directory-based")
    };
    directories.retain(|directory| {
        let path = directory.join("uv");
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        !path
            .components()
            .chain(canonical.components())
            .any(|part| part.as_os_str() == "shims")
            && !canonical
                .file_name()
                .is_some_and(|name| ["mise", "asdf", "rtx"].iter().any(|shim| name == *shim))
    });
    for candidate in super::uv_installation::UvInstallationRoots::current_environment()?
        .versioned_candidates()?
    {
        if let Some(parent) = candidate.canonical_path.parent() {
            directories.push(parent.to_owned());
        }
    }
    directories.sort();
    directories.dedup();
    Ok(UvExecutableSelection::SearchDirectories(directories))
}

fn utf8(path: &Path) -> AdapterResult<&str> {
    path.to_str().ok_or_else(unsupported)
}
fn task_type(action: ManagerAction) -> TaskType {
    match action {
        ManagerAction::Install => TaskType::Install,
        ManagerAction::Uninstall => TaskType::Uninstall,
        ManagerAction::Upgrade => TaskType::Upgrade,
        ManagerAction::Search => TaskType::Search,
        ManagerAction::Detect => TaskType::Detection,
        _ => TaskType::Refresh,
    }
}
fn error(action: ManagerAction, kind: CoreErrorKind, message: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Uv),
        task: Some(task_type(action)),
        action: Some(action),
        kind,
        message: message.into(),
    }
}
fn unsupported() -> CoreError {
    error(
        ManagerAction::Refresh,
        CoreErrorKind::UnsupportedCapability,
        "uv source, receipt, permissions, or configuration requires manual review; cached tools are retained",
    )
}
fn stale() -> CoreError {
    error(
        ManagerAction::Upgrade,
        CoreErrorKind::InvalidInput,
        "uv tool scope or reviewed target changed; refresh and review again",
    )
}
fn verification_failed(action: ManagerAction) -> CoreError {
    error(
        action,
        CoreErrorKind::ProcessFailure,
        "uv operation finished but its installed result is unverified; refresh before retrying",
    )
}

#[cfg(test)]
mod configuration_tests {
    use super::*;

    #[test]
    fn uv_precedence_keeps_private_sources_and_high_priority_arrays_first() {
        let mut low: toml::Table = toml::from_str(
            "index-url = 'https://private.example/simple'\nfind-links = ['/system']",
        )
        .unwrap();
        merge_options(
            &mut low,
            toml::from_str("find-links = ['/user']\nno-index = true").unwrap(),
        )
        .unwrap();
        assert_eq!(
            low["index-url"].as_str(),
            Some("https://private.example/simple")
        );
        assert_eq!(low["find-links"][0].as_str(), Some("/user"));
        assert_eq!(low["find-links"][1].as_str(), Some("/system"));
    }

    #[test]
    fn credential_restoration_never_crosses_index_identity() {
        let config: toml::Table =
            toml::from_str("index-url = 'https://user:secret@private.example/simple/'").unwrap();
        let mut receipt: toml::Table =
            toml::from_str("index-url = 'https://private.example/simple'").unwrap();
        restore_index_credentials(&config, &mut receipt);
        assert_eq!(receipt["index-url"], config["index-url"]);
        let mut other: toml::Table =
            toml::from_str("index-url = 'https://another.example/simple'").unwrap();
        let before = other.clone();
        restore_index_credentials(&config, &mut other);
        assert_eq!(other, before);
    }

    #[test]
    fn system_config_uses_first_existing_directory_not_a_colon_joined_path() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        fs::create_dir_all(second.join("uv")).unwrap();
        fs::write(second.join("uv/uv.toml"), "no-index = true").unwrap();
        assert_eq!(
            system_configuration(vec![first.clone(), second.clone()]).unwrap(),
            vec![second.join("uv/uv.toml")]
        );
        fs::create_dir_all(first.join("uv")).unwrap();
        fs::write(first.join("uv/uv.toml"), "no-index = false").unwrap();
        assert_eq!(
            system_configuration(vec![first.clone(), second]).unwrap(),
            vec![first.join("uv/uv.toml")]
        );
    }

    #[test]
    fn verification_ignores_duplicate_sources_but_not_changed_source_priority() {
        let before = comparable_configuration(b"find-links = ['/a', '/b']").unwrap();
        assert_eq!(
            before,
            comparable_configuration(b"no-build = true\nfind-links = ['/a', '/b', '/b']").unwrap()
        );
        assert_ne!(
            before,
            comparable_configuration(b"find-links = ['/b', '/a']").unwrap()
        );
    }
}
