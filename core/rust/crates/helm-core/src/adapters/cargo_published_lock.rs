use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::execution::{CommandSpec, ProcessExecutor, ProcessSpawnRequest};
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskType};

const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
// Cargo's cache is not a stable API. Recognize only observed crates.io layouts;
// new/ambiguous layouts require review, never an unlocked install fallback.
const CRATES_IO_CACHES: [&str; 2] = [
    "index.crates.io-1949cf8c6b5b557f",
    "github.com-1ecc6299db9ec823",
];

pub(crate) fn lock_error(detail: &str) -> CoreError {
    CoreError {
        manager: Some(ManagerId::Cargo),
        task: None,
        action: Some(ManagerAction::Upgrade),
        kind: CoreErrorKind::UnsupportedCapability,
        message: format!("[cargo_published_lock_unavailable] {detail}"),
    }
}

fn bounded_file(path: &Path) -> AdapterResult<Vec<u8>> {
    if !std::fs::symlink_metadata(path)
        .is_ok_and(|m| m.is_file() && m.len() <= MAX_METADATA_BYTES as u64)
    {
        return Err(lock_error(
            "Published Cargo metadata is missing, linked or oversized",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| {
            f.take(MAX_METADATA_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
        })
        .map_err(|_| lock_error("Published Cargo metadata could not be read"))?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(lock_error(
            "Published Cargo metadata exceeds the supported size",
        ));
    }
    Ok(bytes)
}

fn document(bytes: &[u8]) -> AdapterResult<toml::Value> {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| toml::from_str(text).ok())
        .ok_or_else(|| lock_error("Published Cargo metadata is not valid UTF-8 TOML"))
}

fn validate_identity(manifest: &[u8], lock: &[u8], name: &str, version: &str) -> AdapterResult<()> {
    let manifest = document(manifest)?;
    let package = manifest.get("package");
    if package
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str)
        != Some(name)
        || package
            .and_then(|p| p.get("version"))
            .and_then(toml::Value::as_str)
            != Some(version)
    {
        return Err(lock_error(
            "The published manifest does not match the reviewed package version",
        ));
    }
    let lock = document(lock)?;
    if !matches!(
        lock.get("version").and_then(toml::Value::as_integer),
        Some(3 | 4)
    ) {
        return Err(lock_error(
            "The published lockfile format requires manual review",
        ));
    }
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| lock_error("The published lockfile contains no package graph"))?;
    let roots: Vec<_> = packages
        .iter()
        .filter(|p| {
            p.get("name").and_then(toml::Value::as_str) == Some(name) && p.get("source").is_none()
        })
        .collect();
    if roots.len() != 1 || roots[0].get("version").and_then(toml::Value::as_str) != Some(version) {
        return Err(lock_error(
            "The published lockfile is missing or stale for the reviewed package version",
        ));
    }
    Ok(())
}

fn archive_member(
    executor: &dyn ProcessExecutor,
    archive: &Path,
    member: &str,
) -> AdapterResult<Vec<u8>> {
    let command = CommandSpec::new("/usr/bin/tar").args([
        "-xOf",
        archive
            .to_str()
            .ok_or_else(|| lock_error("Cargo archive path is not UTF-8"))?,
        member,
    ]);
    let mut request = ProcessSpawnRequest::new(
        ManagerId::Cargo,
        TaskType::Upgrade,
        ManagerAction::Upgrade,
        command,
    )
    .timeout(Duration::from_secs(30));
    request.private_output_limit = Some(MAX_METADATA_BYTES);
    // Do not turn cancellation, capture limits or process failures into a retry.
    run_and_collect_stdout(executor, request).map(String::into_bytes).map_err(|mut error| {
        if error.kind == CoreErrorKind::ProcessFailure {
            error.message = format!("[cargo_published_lock_unavailable] The published archive metadata could not be verified: {}", error.message);
        }
        error
    })
}

pub(crate) struct PublishedCargoLock {
    source: PathBuf,
    manifest: Vec<u8>,
    lock: Vec<u8>,
}

impl PublishedCargoLock {
    /// After Cargo has fetched an exact crates.io candidate, verify the packaged
    /// lock and the exact source files Cargo install will read. No extraction writes.
    pub(crate) fn load(
        executor: &dyn ProcessExecutor,
        home: &Path,
        name: &str,
        version: &str,
    ) -> AdapterResult<Self> {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
            || semver::Version::parse(version).is_err()
        {
            return Err(lock_error("An exact Cargo package identity is required"));
        }
        let package = format!("{name}-{version}");
        for directory in ["registry/src", "registry/cache"] {
            let entries = std::fs::read_dir(home.join(directory))
                .map_err(|_| lock_error("The Cargo registry cache is unavailable"))?;
            for (index, entry) in entries.enumerate() {
                if index >= 128 {
                    return Err(lock_error(
                        "The Cargo registry cache exceeds the supported scope",
                    ));
                }
                let entry = entry
                    .map_err(|_| lock_error("The Cargo registry cache cannot be inspected"))?;
                if !CRATES_IO_CACHES
                    .iter()
                    .any(|known| entry.file_name() == *known)
                    && (entry.path().join(&package).exists()
                        || entry.path().join(format!("{package}.crate")).exists())
                {
                    return Err(lock_error(
                        "The candidate exists in an unsupported Cargo registry cache",
                    ));
                }
            }
        }
        let mut candidates = Vec::new();
        for registry in CRATES_IO_CACHES {
            let source = home.join("registry/src").join(registry).join(&package);
            let archive = home
                .join("registry/cache")
                .join(registry)
                .join(format!("{package}.crate"));
            if source.exists() || archive.exists() {
                candidates.push((source, archive));
            }
        }
        if candidates.len() != 1 {
            return Err(lock_error(
                "The crates.io package cache is missing or ambiguous",
            ));
        }
        let (source, archive) = candidates.remove(0);
        for directory in ["registry", "registry/src", "registry/cache"] {
            if !std::fs::symlink_metadata(home.join(directory)).is_ok_and(|m| m.is_dir()) {
                return Err(lock_error(
                    "Linked Cargo cache directories require manual review",
                ));
            }
        }
        for parent in [source.parent(), archive.parent()].into_iter().flatten() {
            if !std::fs::symlink_metadata(parent).is_ok_and(|m| m.is_dir()) {
                return Err(lock_error(
                    "Linked Cargo registry directories require manual review",
                ));
            }
        }
        if !std::fs::symlink_metadata(&source).is_ok_and(|m| m.is_dir())
            || !std::fs::symlink_metadata(&archive)
                .is_ok_and(|m| m.is_file() && m.len() <= MAX_ARCHIVE_BYTES)
        {
            return Err(lock_error(
                "The crates.io package archive is linked, missing or oversized",
            ));
        }
        let manifest = archive_member(executor, &archive, &format!("{package}/Cargo.toml"))?;
        let lock = archive_member(executor, &archive, &format!("{package}/Cargo.lock"))?;
        validate_identity(&manifest, &lock, name, version)?;
        let result = Self {
            source,
            manifest,
            lock,
        };
        result.revalidate()?;
        Ok(result)
    }

    pub(crate) fn revalidate(&self) -> AdapterResult<()> {
        if bounded_file(&self.source.join("Cargo.toml"))? != self.manifest
            || bounded_file(&self.source.join("Cargo.lock"))? != self.lock
        {
            return Err(lock_error(
                "Cached Cargo sources differ from the published package; review the cache before retrying",
            ));
        }
        Ok(())
    }
}

/// `cargo info` normally discovers local workspaces/config. Run at the filesystem
/// root instead, and reject root-level overrides rather than touching a project.
pub(crate) fn info_working_directory() -> AdapterResult<PathBuf> {
    let root = PathBuf::from("/");
    if ["Cargo.toml", ".cargo/config", ".cargo/config.toml"]
        .iter()
        .any(|p| root.join(p).exists())
    {
        return Err(lock_error(
            "Root-level Cargo workspace or configuration requires manual review",
        ));
    }
    Ok(root)
}

pub(crate) fn rustup_proxy(program: &Path) -> Option<PathBuf> {
    if let Ok(target) = std::fs::canonicalize(program)
        && target.file_name().is_some_and(|name| name == "rustup")
        && target.is_file()
    {
        return Some(target);
    }
    let sibling = program.parent()?.join("rustup");
    let cargo = std::fs::metadata(program).ok()?;
    let rustup = std::fs::metadata(&sibling).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if cargo.is_file() && cargo.dev() == rustup.dev() && cargo.ino() == rustup.ino() {
            return Some(sibling);
        }
    }
    None
}

pub(crate) fn active_toolchain(output: &str) -> AdapterResult<&str> {
    let mut lines = output.lines().filter(|line| !line.trim().is_empty());
    let line = lines
        .next()
        .ok_or_else(|| lock_error("Rustup did not identify its selected toolchain"))?;
    let name = line.split_whitespace().next().unwrap_or_default();
    let reason = line.trim().strip_prefix(name).unwrap_or_default().trim();
    if lines.next().is_some()
        || name.is_empty()
        || name.len() > 512
        || name.starts_with('-')
        || name.starts_with('.')
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"/_+-.".contains(&byte))
        || (name.contains('/') && !Path::new(name).is_absolute())
        || (!reason.is_empty() && !(reason.starts_with('(') && reason.ends_with(')')))
    {
        return Err(lock_error(
            "The selected Rustup toolchain cannot be bound safely",
        ));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] = b"[package]\nname = 'example'\nversion = '1.2.3'\n";
    const LOCK: &[u8] = b"version = 4\n[[package]]\nname = 'example'\nversion = '1.2.3'\n";

    #[test]
    fn published_root_must_match_exact_identity() {
        assert!(validate_identity(MANIFEST, LOCK, "example", "1.2.3").is_ok());
        assert!(validate_identity(MANIFEST, LOCK, "example", "1.2.4").is_err());
        assert!(validate_identity(MANIFEST, LOCK, "other", "1.2.3").is_err());
        for bad in [
            "",
            "version = 4",
            "version = 8",
            "version = 4\n[[package]]\nname = 'example'\nversion = '1.2.2'",
            "version = 4\n[[package]]\nname = 'example'\nversion = '1.2.3'\nsource = 'registry+https://private.example'",
            "version = 4\n[[package]]\nname = 'example'\nversion = '1.2.3'\n[[package]]\nname = 'example'\nversion = '1.2.3'",
        ] {
            assert!(
                validate_identity(MANIFEST, bad.as_bytes(), "example", "1.2.3").is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn cache_changes_and_missing_lock_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let state = PublishedCargoLock {
            source: root.path().to_path_buf(),
            manifest: MANIFEST.to_vec(),
            lock: LOCK.to_vec(),
        };
        std::fs::write(root.path().join("Cargo.toml"), MANIFEST).unwrap();
        assert!(state.revalidate().is_err());
        std::fs::write(root.path().join("Cargo.lock"), LOCK).unwrap();
        assert!(state.revalidate().is_ok());
        std::fs::write(root.path().join("Cargo.lock"), b"version = 4").unwrap();
        assert!(state.revalidate().is_err());
    }

    #[test]
    fn rustup_context_is_bound_before_changing_info_working_directory() {
        assert_eq!(
            active_toolchain(
                "stable-aarch64-apple-darwin (overridden by '/project/rust-toolchain.toml')\n"
            )
            .unwrap(),
            "stable-aarch64-apple-darwin"
        );
        assert_eq!(
            active_toolchain("/opt/toolchains/custom (environment override)").unwrap(),
            "/opt/toolchains/custom"
        );
        for invalid in [
            "",
            "../toolchain",
            "relative/toolchain",
            "--bad",
            "stable\nnightly",
            "tool;chain",
            "/path/with space (override)",
        ] {
            assert!(active_toolchain(invalid).is_err(), "{invalid}");
        }
        let root = tempfile::tempdir().unwrap();
        let cargo = root.path().join("cargo");
        let rustup = root.path().join("rustup");
        std::fs::write(&rustup, b"proxy").unwrap();
        std::fs::hard_link(&rustup, &cargo).unwrap();
        assert_eq!(rustup_proxy(&cargo), Some(rustup));
        std::fs::write(root.path().join("native-cargo"), b"native").unwrap();
        assert_eq!(rustup_proxy(&root.path().join("native-cargo")), None);
    }
}
