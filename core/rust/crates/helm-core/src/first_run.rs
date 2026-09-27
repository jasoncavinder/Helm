//! Non-executing observations for the first-run consent boundary.
//!
//! File evidence is deliberately not `DetectionInfo`: launchers, shims, and
//! macOS developer-tool stubs can exist without a usable manager behind them.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::models::ManagerId;
use crate::persistence::{DetectionStore, FirstRunExperience, PersistenceResult};

const MAX_SEARCH_DIRECTORIES: usize = 64;
const MAX_CANDIDATES_PER_MANAGER: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FirstRunLocalObservation {
    pub schema_version: u32,
    pub experience_id: FirstRunExperience,
    pub managers: Vec<FirstRunManagerObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FirstRunManagerObservation {
    pub manager_id: ManagerId,
    /// None preserves "not explicitly configured" rather than inventing consent.
    pub configured_enabled: Option<bool>,
    pub selected_executable_path: Option<String>,
    pub candidate_scan_status: CandidateScanStatus,
    pub inspected_path_count: usize,
    /// Observed regular files, not validated executables or distinct installations.
    pub candidate_paths: Vec<PathBuf>,
    /// Historical facts only, even when a candidate currently exists at the path.
    pub cached_detection: Option<CachedFirstRunDetection>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateScanStatus {
    Complete,
    Partial,
    NotSupported,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CachedFirstRunDetection {
    pub installed: bool,
    pub executable_path: Option<PathBuf>,
    pub version: Option<String>,
}

/// Explicit inputs keep tests and future CLI consumers independent of process
/// globals. Only known bin directories are inspected; there is no recursive scan.
#[derive(Clone, Debug, Default)]
pub struct LocalObservationContext {
    pub search_directories: Vec<PathBuf>,
    pub include_system_candidates: bool,
}

impl LocalObservationContext {
    pub fn from_environment() -> Self {
        let mut directories = Vec::new();
        for key in ["CARGO_HOME", "ASDF_DIR", "ASDF_DATA_DIR"] {
            if let Some(root) = absolute_environment_path(key) {
                directories.push(root.join("bin"));
            }
        }
        if let Some(home) = absolute_environment_path("HOME") {
            for suffix in [
                ".local/bin",
                ".cargo/bin",
                ".asdf/bin",
                ".asdf/shims",
                ".local/share/mise/shims",
                ".local/share/rtx/shims",
            ] {
                directories.push(home.join(suffix));
            }
        }
        directories.extend([
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/opt/local/bin"),
        ]);
        if let Some(path) = std::env::var_os("PATH") {
            directories.extend(std::env::split_paths(&path));
        }
        Self {
            search_directories: directories,
            include_system_candidates: true,
        }
    }
}

fn absolute_environment_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Reads only Helm's existing detection/preferences and regular-file metadata.
/// Does not initialize an adapter, submit tasks, repair preferences, scan Doctor,
/// write caches, accept terms, acknowledge first run, or grant operational consent.
pub fn observe_first_run_environment(
    store: &dyn DetectionStore,
    context: &LocalObservationContext,
) -> PersistenceResult<FirstRunLocalObservation> {
    // Read failures must not masquerade as a fresh profile or absent managers.
    let preferences = store.list_manager_preferences()?;
    let detections = store.list_detections()?;
    let mut seen_directories = HashSet::new();
    let mut directories = Vec::new();
    let mut scope_partial = false;
    for path in &context.search_directories {
        if !path.is_absolute() {
            scope_partial = true;
        } else if seen_directories.insert(path.clone()) {
            if directories.len() < MAX_SEARCH_DIRECTORIES {
                directories.push(path);
            } else {
                scope_partial = true;
            }
        }
    }
    let managers = ManagerId::ALL
        .into_iter()
        .map(|manager| {
            let preference = preferences.iter().find(|value| value.manager == manager);
            let cached = detections
                .iter()
                .find(|(id, _)| *id == manager)
                .map(|(_, value)| value);
            let selected = preference.and_then(|value| value.selected_executable_path.clone());
            let hints = first_run_executable_candidates(manager);
            let mut candidates = Vec::new();
            if let Some(path) = &selected {
                candidates.push(PathBuf::from(path));
            }
            if let Some(path) = cached.and_then(|value| value.executable_path.clone()) {
                candidates.push(path);
            }
            for hint in hints {
                if Path::new(hint).is_absolute() {
                    if context.include_system_candidates {
                        candidates.push(PathBuf::from(hint));
                    }
                } else {
                    candidates.extend(directories.iter().map(|directory| directory.join(hint)));
                }
            }
            let mut partial = scope_partial;
            let mut seen = HashSet::new();
            let mut inspected_path_count = 0;
            let mut candidate_paths = Vec::new();
            for path in candidates {
                if !path.is_absolute() {
                    partial = true;
                    continue;
                }
                if !seen.insert(path.clone()) {
                    continue;
                }
                if inspected_path_count == MAX_CANDIDATES_PER_MANAGER {
                    partial = true;
                    break;
                }
                inspected_path_count += 1;
                match std::fs::metadata(&path) {
                    Ok(metadata) if metadata.is_file() => candidate_paths.push(path),
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => partial = true,
                }
            }
            let candidate_scan_status = if hints.is_empty() && inspected_path_count == 0 {
                CandidateScanStatus::NotSupported
            } else if partial {
                CandidateScanStatus::Partial
            } else {
                CandidateScanStatus::Complete
            };
            FirstRunManagerObservation {
                manager_id: manager,
                configured_enabled: preference.map(|value| value.enabled),
                selected_executable_path: selected,
                candidate_scan_status,
                inspected_path_count,
                candidate_paths,
                cached_detection: cached.map(|value| CachedFirstRunDetection {
                    installed: value.installed,
                    executable_path: value.executable_path.clone(),
                    version: value.version.clone(),
                }),
            }
        })
        .collect();
    Ok(FirstRunLocalObservation {
        schema_version: 1,
        experience_id: FirstRunExperience::CURRENT,
        managers,
    })
}

// Intentionally narrower than adapter discovery: no versioned directory walk,
// app launch, package inventory, receipt lookup, or command execution. In
// particular, xcode-select's presence does not establish that CLT is installed.
fn first_run_executable_candidates(manager: ManagerId) -> &'static [&'static str] {
    match manager {
        ManagerId::HomebrewFormula | ManagerId::HomebrewCask => &["brew"],
        ManagerId::Mise => &["mise"],
        ManagerId::Asdf => &["asdf"],
        ManagerId::Rustup => &[
            "rustup",
            "/opt/homebrew/opt/rustup/bin/rustup",
            "/usr/local/opt/rustup/bin/rustup",
        ],
        ManagerId::Npm => &["npm"],
        ManagerId::Pnpm => &["pnpm"],
        ManagerId::Yarn => &["yarn"],
        ManagerId::Pip => &["python3", "pip3", "pip"],
        ManagerId::Pipx => &["pipx"],
        ManagerId::Uv => &["uv"],
        ManagerId::Poetry => &["poetry"],
        ManagerId::RubyGems => &["gem"],
        ManagerId::Bundler => &["bundle"],
        ManagerId::Cargo => &["cargo"],
        ManagerId::CargoBinstall => &["cargo-binstall"],
        ManagerId::MacPorts => &["port"],
        ManagerId::NixDarwin => &["darwin-rebuild"],
        ManagerId::Mas => &["mas"],
        ManagerId::DockerDesktop => &["/Applications/Docker.app/Contents/Resources/bin/docker"],
        ManagerId::Podman => &["podman"],
        ManagerId::Colima => &["colima"],
        ManagerId::XcodeCommandLineTools => &["/Library/Developer/CommandLineTools/usr/bin/clang"],
        ManagerId::SoftwareUpdate => &["/usr/sbin/softwareupdate"],
        ManagerId::Sparkle
        | ManagerId::Setapp
        | ManagerId::ParallelsDesktop
        | ManagerId::Rosetta2
        | ManagerId::FirmwareUpdates => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::DetectionInfo;
    use crate::persistence::{FirstRunStore, TaskStore};
    use crate::sqlite::SqliteStore;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> (PathBuf, SqliteStore, LocalObservationContext) {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "helm-first-run-observation-{}-{nonce}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("bin")).unwrap();
        let store = SqliteStore::new(root.join("helm.db"));
        store.migrate_to_latest().unwrap();
        let context = LocalObservationContext {
            search_directories: vec![root.join("bin")],
            include_system_candidates: false,
        };
        (root, store, context)
    }

    fn manager(snapshot: &FirstRunLocalObservation, id: ManagerId) -> &FirstRunManagerObservation {
        snapshot
            .managers
            .iter()
            .find(|value| value.manager_id == id)
            .unwrap()
    }

    #[test]
    fn first_run_observation_keeps_all_managers_without_inventing_installation_or_consent() {
        let (_root, store, context) = fixture();
        let result = observe_first_run_environment(&store, &context).unwrap();
        assert_eq!(result.schema_version, 1);
        assert_eq!(result.experience_id, FirstRunExperience::CURRENT);
        assert_eq!(result.managers.len(), ManagerId::ALL.len());
        assert!(
            result
                .managers
                .iter()
                .all(|entry| entry.configured_enabled.is_none()
                    && entry.cached_detection.is_none()
                    && entry.candidate_paths.is_empty())
        );
        assert_eq!(
            manager(&result, ManagerId::Sparkle).candidate_scan_status,
            CandidateScanStatus::NotSupported
        );
        assert_eq!(
            manager(&result, ManagerId::Mise).candidate_scan_status,
            CandidateScanStatus::Complete
        );
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(wire["experience_id"], "wayfinder-v0.20");
        for row in wire["managers"].as_array().unwrap() {
            assert!(row.get("installed").is_none());
            assert!(row.get("ready").is_none());
            assert!(row.get("version").is_none());
        }
    }

    #[test]
    fn first_run_observation_preserves_disabled_preferences_and_stale_selected_path() {
        let (root, store, context) = fixture();
        fs::write(root.join("bin/mise"), b"not even an executable").unwrap();
        let old = root.join("removed/mise");
        store.set_manager_enabled(ManagerId::Mise, false).unwrap();
        store
            .set_manager_selected_executable_path(ManagerId::Mise, old.to_str())
            .unwrap();
        store
            .set_manager_timeout_hard_seconds(ManagerId::Mise, Some(42))
            .unwrap();
        store
            .set_manager_timeout_idle_seconds(ManagerId::Mise, Some(21))
            .unwrap();
        store
            .set_manager_selected_install_method(ManagerId::Mise, Some("homebrew"))
            .unwrap();
        store
            .upsert_detection(
                ManagerId::Mise,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(old.clone()),
                    version: Some("old-version".into()),
                },
            )
            .unwrap();
        store.set_cli_onboarding_completed(true).unwrap();
        store
            .set_cli_accepted_license_terms_version(Some("accepted-test-version"))
            .unwrap();
        store.set_auto_check_for_updates(true).unwrap();
        store.set_safe_mode(true).unwrap();
        let preferences = store.list_manager_preferences().unwrap();
        let detections = store.list_detections().unwrap();
        let connection = rusqlite::Connection::open(store.database_path()).unwrap();
        let data_version = || {
            connection
                .query_row("PRAGMA data_version", [], |row| row.get::<_, i64>(0))
                .unwrap()
        };
        let before = data_version();
        let first = observe_first_run_environment(&store, &context).unwrap();
        let second = observe_first_run_environment(&store, &context).unwrap();
        assert_eq!(first, second);
        let mise = manager(&first, ManagerId::Mise);
        assert_eq!(mise.configured_enabled, Some(false));
        assert_eq!(mise.selected_executable_path.as_deref(), old.to_str());
        assert_eq!(mise.candidate_paths, vec![root.join("bin/mise")]);
        assert_eq!(
            mise.cached_detection.as_ref().unwrap().version.as_deref(),
            Some("old-version")
        );
        assert_eq!(preferences, store.list_manager_preferences().unwrap());
        assert_eq!(detections, store.list_detections().unwrap());
        assert_eq!(
            before,
            data_version(),
            "observation must not write any database table"
        );
        assert!(store.list_recent_tasks(10).unwrap().is_empty());
        assert!(
            !store
                .first_run_experience_state(FirstRunExperience::CURRENT)
                .unwrap()
                .acknowledged
        );
        assert!(store.cli_onboarding_completed().unwrap());
        assert_eq!(
            store
                .cli_accepted_license_terms_version()
                .unwrap()
                .as_deref(),
            Some("accepted-test-version")
        );
        assert!(store.auto_check_for_updates().unwrap());
        assert!(store.safe_mode().unwrap());
    }

    #[test]
    fn first_run_observation_never_promotes_cached_detection_to_current_evidence() {
        let (root, store, context) = fixture();
        store
            .upsert_detection(
                ManagerId::Cargo,
                &DetectionInfo {
                    installed: true,
                    executable_path: Some(root.join("missing/cargo")),
                    version: Some("1.0".into()),
                },
            )
            .unwrap();
        let result = observe_first_run_environment(&store, &context).unwrap();
        let cargo = manager(&result, ManagerId::Cargo);
        assert!(cargo.candidate_paths.is_empty());
        assert!(cargo.cached_detection.as_ref().unwrap().installed);
        assert_eq!(cargo.candidate_scan_status, CandidateScanStatus::Complete);
    }

    #[cfg(unix)]
    #[test]
    fn first_run_observation_never_executes_files_or_treats_aliases_as_install_instances() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let (root, store, mut context) = fixture();
        let script = root.join("bin/cargo");
        fs::write(&script, b"#!/bin/sh\n: > \"$0.executed\"\nexit 1\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        fs::create_dir(root.join("alias")).unwrap();
        symlink(&script, root.join("alias/cargo")).unwrap();
        symlink(root.join("missing"), root.join("bin/uv")).unwrap();
        context.search_directories.push(root.join("alias"));
        let result = observe_first_run_environment(&store, &context).unwrap();
        assert_eq!(manager(&result, ManagerId::Cargo).candidate_paths.len(), 2);
        assert!(manager(&result, ManagerId::Uv).candidate_paths.is_empty());
        assert!(!root.join("bin/cargo.executed").exists());
        assert!(!root.join("alias/cargo.executed").exists());
        assert!(store.list_install_instances(None).unwrap().is_empty());
        assert!(store.list_recent_tasks(10).unwrap().is_empty());
    }

    #[test]
    fn first_run_observation_marks_bounded_or_invalid_scope_partial() {
        let (root, store, mut context) = fixture();
        context.search_directories = (0..70)
            .map(|index| root.join(format!("bin-{index}")))
            .collect();
        context
            .search_directories
            .push(PathBuf::from("relative/bin"));
        let result = observe_first_run_environment(&store, &context).unwrap();
        let pip = manager(&result, ManagerId::Pip);
        assert_eq!(pip.inspected_path_count, 64 * 3);
        assert_eq!(pip.candidate_scan_status, CandidateScanStatus::Partial);
        assert!(pip.candidate_paths.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn first_run_observation_reports_metadata_failure_as_partial_not_absence() {
        let (root, store, context) = fixture();
        std::os::unix::fs::symlink("uv", root.join("bin/uv")).unwrap();
        let result = observe_first_run_environment(&store, &context).unwrap();
        assert_eq!(
            manager(&result, ManagerId::Uv).candidate_scan_status,
            CandidateScanStatus::Partial
        );
    }

    #[test]
    fn first_run_observation_does_not_search_recursively_or_infer_clt_from_xcode_select() {
        let (root, store, context) = fixture();
        fs::write(root.join("bin/xcode-select"), b"stub").unwrap();
        fs::create_dir_all(root.join("bin/nested")).unwrap();
        fs::write(root.join("bin/nested/uv"), b"uv").unwrap();
        let result = observe_first_run_environment(&store, &context).unwrap();
        assert!(manager(&result, ManagerId::Uv).candidate_paths.is_empty());
        assert!(
            manager(&result, ManagerId::XcodeCommandLineTools)
                .candidate_paths
                .is_empty()
        );
    }

    #[test]
    fn first_run_observation_read_failure_is_not_an_empty_snapshot() {
        let (_root, store, context) = fixture();
        let connection = rusqlite::Connection::open(store.database_path()).unwrap();
        connection
            .execute_batch("ALTER TABLE manager_preferences RENAME TO unavailable_preferences")
            .unwrap();
        assert!(observe_first_run_environment(&store, &context).is_err());
        connection
            .execute_batch(
                "ALTER TABLE unavailable_preferences RENAME TO manager_preferences;
             ALTER TABLE manager_detection RENAME TO unavailable_detection",
            )
            .unwrap();
        assert!(observe_first_run_environment(&store, &context).is_err());
    }
}
