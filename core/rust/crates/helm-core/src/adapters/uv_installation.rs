//! Local layout evidence, not package-owner receipts or mutation authorization.

use std::fs;
use std::path::{Path, PathBuf};

use crate::adapters::manager::AdapterResult;
use crate::adapters::uv_tool_scope::{
    UvExecutableCandidate, canonical_path, collect_candidates, scope_error, valid_path,
};
use crate::models::{CoreErrorKind, InstallProvenance};

const MAX_ROOTS: usize = 16;
const MAX_ENTRIES: usize = 256;
const EXECUTABLE_SUFFIXES: &[&str] = &[
    "bin/uv",
    "uv",
    "uv-aarch64-apple-darwin/uv",
    "uv-x86_64-apple-darwin/uv",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UvManagedRoot {
    provenance: InstallProvenance,
    tool_root: PathBuf,
}

impl UvManagedRoot {
    /// The root contains uv versions, e.g. `<mise-data>/installs/uv` or `Cellar/uv`.
    pub fn new(provenance: InstallProvenance, tool_root: PathBuf) -> AdapterResult<Self> {
        if !matches!(
            provenance,
            InstallProvenance::Homebrew | InstallProvenance::Mise | InstallProvenance::Asdf
        ) || !valid_path(&tool_root)
        {
            return Err(scope_error(
                CoreErrorKind::InvalidInput,
                "uv installation root requires a supported owner and absolute path",
            ));
        }
        Ok(Self {
            provenance,
            tool_root,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UvInstallationRoots {
    roots: Vec<UvManagedRoot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UvLayoutEvidence {
    Unknown,
    ManagedLayout(InstallProvenance),
    Conflicting,
}

impl UvInstallationRoots {
    pub fn new(roots: Vec<UvManagedRoot>) -> AdapterResult<Self> {
        if roots.len() > MAX_ROOTS {
            return Err(scope_error(
                CoreErrorKind::InvalidInput,
                "too many uv installation roots",
            ));
        }
        Ok(Self { roots })
    }

    /// Capture roots once; no shell, manager command, or project configuration is read.
    pub fn current_environment() -> AdapterResult<Self> {
        Self::from_environment(|key| std::env::var_os(key).map(PathBuf::from))
    }

    fn from_environment(get: impl Fn(&str) -> Option<PathBuf>) -> AdapterResult<Self> {
        let path = |key| -> AdapterResult<Option<PathBuf>> {
            let value = get(key);
            if value.as_ref().is_some_and(|value| !valid_path(value)) {
                return Err(scope_error(
                    CoreErrorKind::InvalidInput,
                    "uv installation environment contains an invalid root",
                ));
            }
            Ok(value)
        };
        let home = path("HOME")?;
        let mise = match path("MISE_DATA_DIR")? {
            Some(root) => Some(root),
            None => path("XDG_DATA_HOME")?
                .map(|root| root.join("mise"))
                .or_else(|| home.as_ref().map(|home| home.join(".local/share/mise"))),
        };
        let asdf = path("ASDF_DATA_DIR")?.or_else(|| home.as_ref().map(|home| home.join(".asdf")));
        let mut roots = vec![
            UvManagedRoot::new(
                InstallProvenance::Homebrew,
                "/opt/homebrew/Cellar/uv".into(),
            )?,
            UvManagedRoot::new(InstallProvenance::Homebrew, "/usr/local/Cellar/uv".into())?,
        ];
        for (owner, root) in [
            (InstallProvenance::Mise, mise),
            (InstallProvenance::Asdf, asdf),
        ] {
            if let Some(root) = root {
                roots.push(UvManagedRoot::new(owner, root.join("installs/uv"))?);
            }
        }
        Self::new(roots)
    }

    /// Only the canonical executable counts. An alias in a managed prefix proves nothing.
    pub fn classify(&self, canonical_executable: &Path) -> UvLayoutEvidence {
        if !valid_path(canonical_executable) {
            return UvLayoutEvidence::Unknown;
        }
        let mut evidence = UvLayoutEvidence::Unknown;
        for root in &self.roots {
            let Ok(canonical_root) = canonical_path(&root.tool_root) else {
                continue;
            };
            let Ok(relative) = canonical_executable.strip_prefix(&canonical_root) else {
                continue;
            };
            let mut components = relative.components();
            if components.next().is_none() {
                continue;
            }
            let suffix = components.as_path();
            let matches = if root.provenance == InstallProvenance::Homebrew {
                suffix == Path::new("bin/uv")
            } else {
                EXECUTABLE_SUFFIXES
                    .iter()
                    .any(|known| suffix == Path::new(known))
            };
            if matches {
                match evidence {
                    UvLayoutEvidence::Unknown => {
                        evidence = UvLayoutEvidence::ManagedLayout(root.provenance);
                    }
                    UvLayoutEvidence::ManagedLayout(owner) if owner == root.provenance => {}
                    _ => return UvLayoutEvidence::Conflicting,
                }
            }
        }
        evidence
    }

    /// Enumerates fixed layouts without executing candidates or selecting an active version.
    /// Homebrew kegs are deliberately excluded: retained kegs are not separate installs.
    pub fn versioned_candidates(&self) -> AdapterResult<Vec<UvExecutableCandidate>> {
        let mut paths = Vec::new();
        let mut entries_seen = 0;
        for root in &self.roots {
            if root.provenance == InstallProvenance::Homebrew {
                continue;
            }
            match fs::symlink_metadata(&root.tool_root) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => return Err(discovery_failed()),
                Ok(_) => {}
            }
            let canonical_root = canonical_path(&root.tool_root)?;
            let entries = fs::read_dir(&root.tool_root).map_err(|_| discovery_failed())?;
            for entry in entries {
                entries_seen += 1;
                if entries_seen > MAX_ENTRIES {
                    return Err(scope_error(
                        CoreErrorKind::UnsupportedCapability,
                        "uv installation enumeration exceeded its entry limit",
                    ));
                }
                let entry = entry.map_err(|_| discovery_failed())?;
                let version = entry.path();
                if !fs::metadata(&version)
                    .map_err(|_| discovery_failed())?
                    .is_dir()
                {
                    continue;
                }
                let canonical_version = canonical_path(&version)?;
                if canonical_version.parent() != Some(canonical_root.as_path()) {
                    return Err(discovery_failed());
                }
                let mut found = false;
                for suffix in EXECUTABLE_SUFFIXES {
                    let executable = version.join(suffix);
                    match fs::symlink_metadata(&executable) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                        Err(_) => return Err(discovery_failed()),
                        Ok(_) => {}
                    }
                    let canonical = canonical_path(&executable)?;
                    if !canonical.starts_with(&canonical_version) {
                        return Err(discovery_failed());
                    }
                    paths.push(executable);
                    found = true;
                }
                if !found {
                    return Err(scope_error(
                        CoreErrorKind::UnsupportedCapability,
                        "uv installation has an unrecognized executable layout; select a concrete executable",
                    ));
                }
            }
        }
        paths.sort();
        paths.dedup();
        // These paths were observed above: disappearance must not yield a partial set.
        collect_candidates(paths, true)
    }
}

fn discovery_failed() -> crate::models::CoreError {
    scope_error(
        CoreErrorKind::ProcessFailure,
        "uv installation enumeration could not establish a complete local candidate set",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_roots_honor_overrides_without_project_or_shell_resolution() {
        let roots = UvInstallationRoots::from_environment(|key| match key {
            "HOME" => Some("/home/test".into()),
            "MISE_DATA_DIR" => Some("/custom/mise".into()),
            "ASDF_DATA_DIR" => Some("/custom/asdf".into()),
            "XDG_DATA_HOME" => Some("/custom/xdg".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(
            roots.roots[2].tool_root,
            Path::new("/custom/mise/installs/uv")
        );
        assert_eq!(
            roots.roots[3].tool_root,
            Path::new("/custom/asdf/installs/uv")
        );
    }

    #[test]
    fn environment_roots_use_xdg_then_home_and_reject_relative_overrides() {
        let roots = UvInstallationRoots::from_environment(|key| match key {
            "HOME" => Some("/home/test".into()),
            "XDG_DATA_HOME" => Some("/custom/xdg".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(
            roots.roots[2].tool_root,
            Path::new("/custom/xdg/mise/installs/uv")
        );
        assert_eq!(
            roots.roots[3].tool_root,
            Path::new("/home/test/.asdf/installs/uv")
        );
        assert!(
            UvInstallationRoots::from_environment(|key| {
                (key == "MISE_DATA_DIR").then(|| PathBuf::from("relative"))
            })
            .is_err()
        );
    }
}
