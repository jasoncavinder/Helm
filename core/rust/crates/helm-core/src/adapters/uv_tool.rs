//! Read-only uv tool command/output contracts, not a registered manager adapter.
//!
//! Latest-version observations are deliberately not `OutdatedPackage` values:
//! uv's lookup does not prove that a version satisfies the tool's constraints.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::execution::{CommandSpec, ProcessExitStatus, ProcessOutput};

const MAX_LIST_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UvToolListMode {
    Installed,
    LatestVersions,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UvToolObservation {
    pub name: String,
    /// Opaque Python-package version; no SemVer or lexical ordering is implied.
    pub installed_version: String,
    /// Uninterpreted uv `[required: ...]` annotation, including non-registry sources.
    pub requirement: Option<String>,
    pub executables: Vec<String>,
    /// Discovery only, never an approved or verified upgrade target.
    pub latest_version: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum UvToolListError {
    #[error("uv tool listing did not exit successfully")]
    ProcessFailed,
    #[error("uv tool listing exceeded the output limit")]
    OutputTooLarge,
    #[error("uv tool listing was not valid UTF-8")]
    InvalidEncoding,
    #[error("uv tool listing included diagnostics and is not authoritative")]
    NonAuthoritative,
    #[error("uv did not confirm an empty installed-tool inventory")]
    UnconfirmedEmptyInventory,
    #[error("unrecognized uv tool listing at line {line}")]
    InvalidLine { line: usize },
    #[error("duplicate uv tool identity at line {line}")]
    DuplicateTool { line: usize },
    #[error("uv tool listing omitted executables for the tool at line {line}")]
    MissingExecutables { line: usize },
}

/// Builds arguments only. The future process adapter must bind the selected
/// executable/store, a neutral working directory, task identity, and timeouts.
pub fn uv_tool_list_command(executable: impl Into<PathBuf>, mode: UvToolListMode) -> CommandSpec {
    let command = CommandSpec::new(executable)
        .args([
            "--color",
            "never",
            "--no-progress",
            "tool",
            "list",
            "--show-version-specifiers",
        ])
        .env("UV_PYTHON_DOWNLOADS", "never");
    match mode {
        UvToolListMode::Installed => command.arg("--offline"),
        UvToolListMode::LatestVersions => command.arg("--outdated"),
    }
}

/// Parses the uncolored, no-path listing requested above. An error must preserve
/// prior cached inventory, not replace it with an empty or partial snapshot.
pub fn parse_uv_tool_list(
    output: &ProcessOutput,
    mode: UvToolListMode,
) -> Result<Vec<UvToolObservation>, UvToolListError> {
    if output.status != ProcessExitStatus::ExitCode(0) {
        return Err(UvToolListError::ProcessFailed);
    }
    if output.stdout.len().saturating_add(output.stderr.len()) > MAX_LIST_OUTPUT_BYTES {
        return Err(UvToolListError::OutputTooLarge);
    }
    let stdout =
        std::str::from_utf8(&output.stdout).map_err(|_| UvToolListError::InvalidEncoding)?;
    let stderr =
        std::str::from_utf8(&output.stderr).map_err(|_| UvToolListError::InvalidEncoding)?;
    if stdout.trim().is_empty() && stderr.trim() == "No tools installed" {
        return Ok(Vec::new());
    }
    // uv may skip broken tools and still exit 0. Never publish that partial list
    // as a complete inventory. Do not copy potentially sensitive diagnostics.
    if !stderr.trim().is_empty() {
        return Err(UvToolListError::NonAuthoritative);
    }
    if stdout.trim().is_empty() {
        return match mode {
            UvToolListMode::Installed => Err(UvToolListError::UnconfirmedEmptyInventory),
            UvToolListMode::LatestVersions => Ok(Vec::new()),
        };
    }

    let mut tools = Vec::new();
    let mut names = BTreeSet::new();
    let mut current: Option<(usize, UvToolObservation)> = None;
    let mut executables = BTreeSet::new();
    for (index, text) in stdout.lines().enumerate() {
        let line = index + 1;
        if text.is_empty() {
            continue;
        }
        if text.chars().any(char::is_control) || text.trim() != text {
            return Err(UvToolListError::InvalidLine { line });
        }
        if let Some(executable) = text.strip_prefix("- ") {
            let Some((_, tool)) = current.as_mut() else {
                return Err(UvToolListError::InvalidLine { line });
            };
            if executable.is_empty()
                || executable.trim() != executable
                || executable.contains(['/', '\\'])
                || !executables.insert(executable.to_owned())
            {
                return Err(UvToolListError::InvalidLine { line });
            }
            tool.executables.push(executable.to_owned());
        } else {
            finish_tool(&mut current, &mut tools)?;
            let tool = parse_header(text, mode).ok_or(UvToolListError::InvalidLine { line })?;
            if !names.insert(tool.name.clone()) {
                return Err(UvToolListError::DuplicateTool { line });
            }
            executables.clear();
            current = Some((line, tool));
        }
    }
    finish_tool(&mut current, &mut tools)?;
    Ok(tools)
}

fn finish_tool(
    current: &mut Option<(usize, UvToolObservation)>,
    tools: &mut Vec<UvToolObservation>,
) -> Result<(), UvToolListError> {
    if let Some((line, tool)) = current.take() {
        if tool.executables.is_empty() {
            return Err(UvToolListError::MissingExecutables { line });
        }
        tools.push(tool);
    }
    Ok(())
}

fn parse_header(text: &str, mode: UvToolListMode) -> Option<UvToolObservation> {
    let (name, mut version_and_requirement) = text.split_once(" v")?;
    let name = normalize_name(name)?;
    let latest_version = match mode {
        UvToolListMode::Installed => None,
        UvToolListMode::LatestVersions => {
            let (prefix, latest) = version_and_requirement.rsplit_once(" [latest: ")?;
            let latest = latest.strip_suffix(']')?;
            if !is_version_token(latest) {
                return None;
            }
            version_and_requirement = prefix;
            Some(latest.to_owned())
        }
    };
    let (version, requirement) = match version_and_requirement.split_once(" [required: ") {
        Some((version, annotation)) => {
            let requirement = annotation.strip_suffix(']')?;
            if requirement.is_empty()
                || requirement.trim() != requirement
                || requirement.contains("] [")
                || requirement.contains(" [latest: ")
                || requirement.contains(" [required: ")
            {
                return None;
            }
            (version, Some(requirement.to_owned()))
        }
        None => (version_and_requirement, None),
    };
    if !is_version_token(version) {
        return None;
    }
    Some(UvToolObservation {
        name,
        installed_version: version.to_owned(),
        requirement,
        executables: Vec::new(),
        latest_version,
    })
}

// Framing validation only. PEP 440 comparison and constraint evaluation belong
// to the later candidate-resolution boundary, not this inventory parser.
fn is_version_token(value: &str) -> bool {
    value.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".!+_-".contains(&byte))
}

pub(crate) fn normalize_name(value: &str) -> Option<String> {
    if !value.as_bytes().first()?.is_ascii_alphanumeric()
        || !value.as_bytes().last()?.is_ascii_alphanumeric()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    {
        return None;
    }
    let mut normalized = String::with_capacity(value.len());
    let mut separator = false;
    for byte in value.bytes() {
        if b"._-".contains(&byte) {
            if !separator {
                normalized.push('-');
            }
            separator = true;
        } else {
            normalized.push(byte.to_ascii_lowercase() as char);
            separator = false;
        }
    }
    Some(normalized)
}
