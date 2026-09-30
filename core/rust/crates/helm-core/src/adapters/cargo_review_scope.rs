use std::path::{Path, PathBuf};

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::cargo_receipt::{CargoUpgradeReceipt, receipt_error};
use super::manager::AdapterResult;

pub const UNAVAILABLE: &str = "cargo-review-unavailable";
const PREFIX: &str = "cargo-review-v1:";

pub(crate) fn is_token(value: &str) -> bool {
    value.strip_prefix(PREFIX).is_some_and(|digest| {
        digest.len() == 64
            && digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

#[derive(Serialize)]
struct DirectoryIdentity {
    requested: PathBuf,
    canonical: PathBuf,
    device: u64,
    inode: u64,
}

#[derive(Serialize)]
struct FileIdentity {
    location: DirectoryIdentity,
    length: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
    mode: u32,
}

#[cfg(unix)]
fn location(path: &Path, directory: bool) -> AdapterResult<(DirectoryIdentity, std::fs::Metadata)> {
    use std::os::unix::fs::MetadataExt;
    if !path.is_absolute() || path.to_str().is_none() {
        return Err(receipt_error("Cargo review requires absolute UTF-8 paths"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| receipt_error("Cargo review path is unavailable"))?;
    let metadata = std::fs::metadata(&canonical)
        .map_err(|_| receipt_error("Cargo review metadata is unavailable"))?;
    if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
        return Err(receipt_error("Cargo review path has an unsupported type"));
    }
    Ok((
        DirectoryIdentity {
            requested: path.into(),
            canonical,
            device: metadata.dev(),
            inode: metadata.ino(),
        },
        metadata,
    ))
}

#[cfg(unix)]
fn file_identity(path: &Path) -> AdapterResult<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    let (location, metadata) = location(path, false)?;
    Ok(FileIdentity {
        location,
        length: metadata.len(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        changed_seconds: metadata.ctime(),
        changed_nanoseconds: metadata.ctime_nsec(),
        mode: metadata.mode(),
    })
}

/// Opaque local drift identity, not an authorization token or authenticity proof.
/// Only the selected package receipt is bound so an earlier package in the same
/// reviewed plan can update without invalidating every remaining package.
#[cfg(unix)]
pub(crate) fn execution_fingerprint(
    home: &Path,
    root: &Path,
    program: &Path,
    toolchain: Option<&str>,
    toolchain_cargo: Option<&Path>,
) -> AdapterResult<String> {
    let payload = (
        location(home, true)?.0,
        location(root, true)?.0,
        file_identity(program)?,
        toolchain,
        toolchain_cargo.map(file_identity).transpose()?,
    );
    let bytes = serde_json::to_vec(&payload)
        .map_err(|_| receipt_error("Cargo execution scope cannot be encoded"))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

#[cfg(not(unix))]
pub(crate) fn execution_fingerprint(
    _home: &Path,
    _root: &Path,
    _program: &Path,
    _toolchain: Option<&str>,
    _toolchain_cargo: Option<&Path>,
) -> AdapterResult<String> {
    Err(receipt_error(
        "Cargo reviewed scope is unsupported on this platform",
    ))
}

#[cfg(unix)]
pub(crate) fn fingerprint(
    home: &Path,
    receipt: &CargoUpgradeReceipt,
    program: &Path,
    toolchain: Option<&str>,
    toolchain_cargo: Option<&Path>,
    candidate: &str,
) -> AdapterResult<String> {
    let binaries = receipt
        .binary_paths()
        .map(|path| file_identity(&path))
        .collect::<AdapterResult<Vec<_>>>()?;
    let payload = (
        1,
        execution_fingerprint(home, receipt.root(), program, toolchain, toolchain_cargo)?,
        receipt.review_identity(),
        binaries,
        candidate,
    );
    let bytes = serde_json::to_vec(&payload)
        .map_err(|_| receipt_error("Cargo review cannot be encoded"))?;
    Ok(format!("{PREFIX}{:x}", Sha256::digest(bytes)))
}

#[cfg(not(unix))]
pub(crate) fn fingerprint(
    _home: &Path,
    _receipt: &CargoUpgradeReceipt,
    _program: &Path,
    _toolchain: Option<&str>,
    _toolchain_cargo: Option<&Path>,
    _candidate: &str,
) -> AdapterResult<String> {
    Err(receipt_error(
        "Cargo reviewed scope is unsupported on this platform",
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn execution_scope_binds_roots_and_resolved_toolchain_without_directory_mtime() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().join("home");
        let root = directory.path().join("root");
        let other = directory.path().join("other");
        for path in [&home, &root, &other] {
            std::fs::create_dir(path).unwrap();
        }
        let program = directory.path().join("proxy");
        let cargo = directory.path().join("toolchain-cargo");
        std::fs::write(&program, "proxy").unwrap();
        std::fs::write(&cargo, "cargo").unwrap();
        let scope = |home: &Path, root: &Path, toolchain: &str| {
            execution_fingerprint(home, root, &program, Some(toolchain), Some(&cargo)).unwrap()
        };
        let reviewed = scope(&home, &root, "stable");
        assert_ne!(scope(&other, &root, "stable"), reviewed);
        assert_ne!(scope(&home, &other, "stable"), reviewed);
        assert_ne!(scope(&home, &root, "nightly"), reviewed);
        std::fs::write(root.join("unrelated"), "new package receipt").unwrap();
        std::fs::write(home.join("unrelated"), "registry refresh").unwrap();
        assert_eq!(scope(&home, &root, "stable"), reviewed);
        std::fs::write(&cargo, "replaced cargo").unwrap();
        assert_ne!(scope(&home, &root, "stable"), reviewed);
    }
}
