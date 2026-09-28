use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::cargo_receipt::{
    CargoInstallReceipt, CargoUpgradeReceipt, key_parts, read_receipts, receipt_error,
};
use super::manager::AdapterResult;
use crate::models::CoreErrorKind;

#[derive(Debug, Eq, PartialEq)]
struct BinaryIdentity {
    device: u64,
    inode: u64,
    length: u64,
    mode: u32,
    modified: (i64, i64),
    changed: (i64, i64),
    link: Option<PathBuf>,
}

#[cfg(unix)]
fn binaries(root: &Path) -> AdapterResult<BTreeMap<PathBuf, BinaryIdentity>> {
    use std::os::unix::fs::MetadataExt;
    let bin = root.join("bin");
    let entries = match std::fs::read_dir(&bin) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if std::fs::symlink_metadata(&bin).is_ok() {
                return Err(receipt_error("Cargo binary directory is a dangling link"));
            }
            return Ok(BTreeMap::new());
        }
        Err(_) => return Err(receipt_error("Cargo binary directory is inaccessible")),
    };
    if !std::fs::symlink_metadata(&bin).is_ok_and(|metadata| metadata.is_dir()) {
        return Err(receipt_error(
            "Linked Cargo binary directories require manual review",
        ));
    }
    let mut result = BTreeMap::new();
    for (index, entry) in entries.enumerate() {
        if index >= 4096 {
            return Err(receipt_error(
                "Cargo binary inventory exceeds the supported scope",
            ));
        }
        let path = entry
            .map_err(|_| receipt_error("Cargo binary inventory is inaccessible"))?
            .path();
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| receipt_error("Cargo binary identity is inaccessible"))?;
        if !metadata.is_file() && !metadata.is_symlink() {
            return Err(receipt_error(
                "Cargo binary inventory has an unsupported entry",
            ));
        }
        let link = if metadata.is_symlink() {
            Some(
                std::fs::read_link(&path)
                    .map_err(|_| receipt_error("Cargo binary link is unreadable"))?,
            )
        } else {
            None
        };
        result.insert(
            path,
            BinaryIdentity {
                device: metadata.dev(),
                inode: metadata.ino(),
                length: metadata.len(),
                mode: metadata.mode(),
                modified: (metadata.mtime(), metadata.mtime_nsec()),
                changed: (metadata.ctime(), metadata.ctime_nsec()),
                link,
            },
        );
    }
    Ok(result)
}

#[cfg(not(unix))]
fn binaries(_root: &Path) -> AdapterResult<BTreeMap<PathBuf, BinaryIdentity>> {
    Err(receipt_error(
        "Verified Cargo installation is unsupported on this platform",
    ))
}

fn empty_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() == 0)
        && std::fs::File::open(path)
            .and_then(|mut file| file.read(&mut [0]))
            .is_ok_and(|count| count == 0)
}

fn receipts(root: &Path) -> AdapterResult<BTreeMap<String, CargoInstallReceipt>> {
    match std::fs::symlink_metadata(root.join(".crates2.json")) {
        Ok(metadata) if metadata.is_file() && metadata.len() == 0 => {
            // `cargo install --list` creates both empty files for a fresh root.
            // Existing legacy content must not be hidden by an empty modern file.
            let legacy = root.join(".crates.toml");
            if empty_file(&legacy) && empty_file(&root.join(".crates2.json")) {
                Ok(BTreeMap::new())
            } else {
                Err(receipt_error(
                    "Empty Cargo receipts do not describe a fresh root",
                ))
            }
        }
        Ok(metadata) if metadata.is_file() => read_receipts(root),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A legacy-only installation cannot be assumed empty or migrated by Helm.
            if std::fs::symlink_metadata(root.join(".crates.toml")).is_ok() {
                return Err(receipt_error(
                    "Legacy-only Cargo receipts require manual review",
                ));
            }
            Ok(BTreeMap::new())
        }
        _ => Err(receipt_error("Cargo receipt is linked or inaccessible")),
    }
}

pub(super) struct FreshInstall {
    root: PathBuf,
    name: String,
    receipts: BTreeMap<String, CargoInstallReceipt>,
    binaries: BTreeMap<PathBuf, BinaryIdentity>,
}

impl FreshInstall {
    /// None means an existing receipt must take the option-preserving reinstall path.
    pub(super) fn prepare(root: PathBuf, name: &str) -> AdapterResult<Option<Self>> {
        let receipts = receipts(&root)?;
        if receipts.keys().any(|key| key_parts(key).is_none()) {
            return Err(receipt_error(
                "Cargo receipt identities cannot be validated",
            ));
        }
        if receipts
            .keys()
            .any(|key| key_parts(key).is_some_and(|(found, _, _)| found == name))
        {
            return Ok(None);
        }
        let binaries = binaries(&root)?;
        Ok(Some(Self {
            root,
            name: name.into(),
            receipts,
            binaries,
        }))
    }

    pub(super) fn revalidate(&self) -> AdapterResult<()> {
        if receipts(&self.root)? != self.receipts || binaries(&self.root)? != self.binaries {
            return Err(receipt_error(
                "Cargo installation changed before install; review again",
            ));
        }
        Ok(())
    }

    pub(super) fn verify(&self, version: &str) -> AdapterResult<()> {
        self.verify_inner(version).map_err(|mut error| {
            error.kind = CoreErrorKind::ProcessFailure;
            error.message = format!("[cargo_receipt_unsupported] Cargo completed but the new installation or unrelated state could not be verified; files may have changed: {}", error.message);
            error
        })
    }

    fn verify_inner(&self, version: &str) -> AdapterResult<()> {
        let installed = CargoUpgradeReceipt::load(self.root.clone(), &self.name)?;
        let (key, options) = installed.review_identity();
        if installed.installed_version() != version
            || !options.features.is_empty()
            || options.all_features
            || options.no_default_features
            || options.profile != "release"
        {
            return Err(receipt_error(
                "New Cargo receipt does not match the requested default build",
            ));
        }
        let mut current = receipts(&self.root)?;
        current.remove(key);
        if current != self.receipts {
            return Err(receipt_error("Unrelated Cargo receipts changed"));
        }
        let mut current_bins = binaries(&self.root)?;
        for path in installed.binary_paths() {
            if self.binaries.contains_key(&path) {
                return Err(receipt_error("Cargo replaced a pre-existing binary"));
            }
            current_bins.remove(&path);
        }
        if current_bins != self.binaries {
            return Err(receipt_error("Unrelated Cargo binaries changed"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn installed(root: &Path, version: &str) {
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/tool"), "installed").unwrap();
        std::fs::write(
            root.join(".crates2.json"),
            json!({"installs": {
                format!("tool {version} (registry+https://github.com/rust-lang/crates.io-index)"): {
                    "version_req": format!("={version}"), "bins":["tool"], "features":[],
                    "all_features":false, "no_default_features":false, "profile":"release",
                    "target":"aarch64-apple-darwin", "rustc":"rustc 1.98.1"
                }
            }})
            .to_string(),
        )
        .unwrap();
    }

    #[test]
    fn fresh_install_accepts_verified_default_receipt_and_preserves_existing_binary() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("bin")).unwrap();
        std::fs::write(root.path().join("bin/other"), "unchanged").unwrap();
        let review = FreshInstall::prepare(root.path().into(), "tool")
            .unwrap()
            .unwrap();
        review.revalidate().unwrap();
        installed(root.path(), "1.0.0");
        review.verify("1.0.0").unwrap();
        assert!(
            FreshInstall::prepare(root.path().into(), "tool")
                .unwrap()
                .is_none()
        );
        assert!(review.revalidate().is_err());
        assert!(review.verify("2.0.0").is_err());
        std::fs::write(root.path().join("bin/other"), "changed").unwrap();
        assert_eq!(
            review.verify("1.0.0").unwrap_err().kind,
            CoreErrorKind::ProcessFailure
        );
    }

    #[test]
    fn binary_or_legacy_receipt_drift_fails_before_install() {
        let root = tempfile::tempdir().unwrap();
        let review = FreshInstall::prepare(root.path().into(), "tool")
            .unwrap()
            .unwrap();
        std::fs::create_dir(root.path().join("bin")).unwrap();
        std::fs::write(root.path().join("bin/other"), "new").unwrap();
        assert!(review.revalidate().is_err());
        std::fs::write(root.path().join(".crates.toml"), "[v1]\n").unwrap();
        assert!(FreshInstall::prepare(root.path().into(), "tool").is_err());
    }

    #[test]
    fn native_paired_empty_receipts_are_fresh_but_partial_or_legacy_content_is_not() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join(".crates2.json"), "").unwrap();
        assert!(FreshInstall::prepare(root.path().into(), "tool").is_err());
        std::fs::write(root.path().join(".crates.toml"), "").unwrap();
        let review = FreshInstall::prepare(root.path().into(), "tool")
            .unwrap()
            .unwrap();
        review.revalidate().unwrap();
        installed(root.path(), "1.0.0");
        review.verify("1.0.0").unwrap();
        std::fs::write(root.path().join(".crates2.json"), "").unwrap();
        std::fs::write(root.path().join(".crates.toml"), "[v1]\n").unwrap();
        assert!(FreshInstall::prepare(root.path().into(), "tool").is_err());
    }

    #[test]
    fn untracked_binary_replacement_and_changed_build_options_are_not_verified() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("bin")).unwrap();
        std::fs::write(root.path().join("bin/tool"), "untracked").unwrap();
        let review = FreshInstall::prepare(root.path().into(), "tool")
            .unwrap()
            .unwrap();
        installed(root.path(), "1.0.0");
        assert!(review.verify("1.0.0").is_err());
        let clean = tempfile::tempdir().unwrap();
        let review = FreshInstall::prepare(clean.path().into(), "tool")
            .unwrap()
            .unwrap();
        installed(clean.path(), "1.0.0");
        let path = clean.path().join(".crates2.json");
        let value = std::fs::read_to_string(&path)
            .unwrap()
            .replace("release", "dev");
        std::fs::write(path, value).unwrap();
        assert!(review.verify("1.0.0").is_err());
    }
}
