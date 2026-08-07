use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::sparkle::{
    SparkleApp, SparkleSource, sparkle_appcast_request, sparkle_plist_request,
    sparkle_system_version_request,
};
use crate::execution::ProcessExecutor;

const HELM_BUNDLE_IDENTIFIER: &str = "com.jasoncavinder.Helm";
const MAX_SCAN_DEPTH: usize = 4;

pub struct ProcessSparkleSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessSparkleSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }

    fn read_app(&self, app_path: &Path) -> Option<SparkleApp> {
        let info_plist = app_path.join("Contents/Info.plist");
        let request = sparkle_plist_request(None, &info_plist.to_string_lossy());
        let raw = run_and_collect_stdout(self.executor.as_ref(), request).ok()?;
        let info = serde_json::from_str::<Value>(&raw).ok()?;
        sparkle_app_from_info(app_path, &info)
    }
}

impl SparkleSource for ProcessSparkleSource {
    fn apps(&self) -> AdapterResult<Vec<SparkleApp>> {
        let mut roots = vec![PathBuf::from("/Applications")];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }

        let mut app_paths = Vec::new();
        for root in roots {
            collect_sparkle_apps(&root, 0, &mut app_paths);
        }
        app_paths.sort();
        app_paths.dedup();

        let mut apps = app_paths
            .iter()
            .filter_map(|path| self.read_app(path))
            .collect::<Vec<_>>();
        apps.sort_by(|left, right| {
            left.display_name
                .to_ascii_lowercase()
                .cmp(&right.display_name.to_ascii_lowercase())
                .then_with(|| left.bundle_path.cmp(&right.bundle_path))
        });
        Ok(apps)
    }

    fn appcast(&self, feed_url: &str) -> AdapterResult<String> {
        run_and_collect_stdout(
            self.executor.as_ref(),
            sparkle_appcast_request(None, feed_url),
        )
    }

    fn system_version(&self) -> AdapterResult<String> {
        run_and_collect_stdout(self.executor.as_ref(), sparkle_system_version_request(None))
            .map(|version| version.trim().to_string())
    }
}

fn collect_sparkle_apps(root: &Path, depth: usize, candidates: &mut Vec<PathBuf>) {
    if depth > MAX_SCAN_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(std::fs::DirEntry::path);

    for entry in entries {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() || !file_type.is_dir() {
            continue;
        }
        let path = entry.path();
        if is_app_bundle(&path) {
            if app_uses_sparkle(&path) {
                candidates.push(path);
            }
            continue;
        }
        collect_sparkle_apps(&path, depth + 1, candidates);
    }
}

fn is_app_bundle(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("app")
}

fn app_uses_sparkle(app_path: &Path) -> bool {
    app_path
        .join("Contents/Frameworks/Sparkle.framework")
        .is_dir()
}

fn plist_string(info: &Value, key: &str) -> Option<String> {
    let value = info.get(key)?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn sparkle_app_from_info(app_path: &Path, info: &Value) -> Option<SparkleApp> {
    let bundle_identifier = plist_string(info, "CFBundleIdentifier")?;
    if bundle_identifier == HELM_BUNDLE_IDENTIFIER {
        return None;
    }

    let display_name = plist_string(info, "CFBundleDisplayName")
        .or_else(|| plist_string(info, "CFBundleName"))
        .or_else(|| {
            app_path
                .file_stem()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })?;

    Some(SparkleApp {
        bundle_path: app_path.to_path_buf(),
        bundle_identifier,
        display_name,
        short_version: plist_string(info, "CFBundleShortVersionString"),
        bundle_version: plist_string(info, "CFBundleVersion"),
        feed_url: plist_string(info, "SUFeedURL"),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;

    use super::{HELM_BUNDLE_IDENTIFIER, plist_string, sparkle_app_from_info};

    #[test]
    fn plist_string_ignores_missing_and_blank_values() {
        let info = json!({
            "CFBundleIdentifier": "com.example.App",
            "SUFeedURL": "  "
        });
        assert_eq!(
            plist_string(&info, "CFBundleIdentifier").as_deref(),
            Some("com.example.App")
        );
        assert_eq!(plist_string(&info, "SUFeedURL"), None);
        assert_eq!(plist_string(&info, "Missing"), None);
        assert_eq!(HELM_BUNDLE_IDENTIFIER, "com.jasoncavinder.Helm");
    }

    #[test]
    fn app_metadata_excludes_helm_by_bundle_identifier() {
        let info = json!({
            "CFBundleIdentifier": HELM_BUNDLE_IDENTIFIER,
            "CFBundleName": "Renamed Helm",
            "CFBundleVersion": "1802",
            "SUFeedURL": "https://example.com/helm.xml"
        });

        assert!(sparkle_app_from_info(Path::new("/Applications/Anything.app"), &info).is_none());
    }

    #[test]
    fn app_metadata_preserves_external_bundle_identity() {
        let info = json!({
            "CFBundleIdentifier": "com.example.External",
            "CFBundleDisplayName": "External App",
            "CFBundleShortVersionString": "2.4",
            "CFBundleVersion": "2040",
            "SUFeedURL": "https://example.com/appcast.xml"
        });

        let app = sparkle_app_from_info(Path::new("/Applications/External.app"), &info)
            .expect("external Sparkle app should be retained");
        assert_eq!(app.bundle_identifier, "com.example.External");
        assert_eq!(app.display_name, "External App");
        assert_eq!(app.bundle_path, Path::new("/Applications/External.app"));
        assert_eq!(app.short_version.as_deref(), Some("2.4"));
        assert_eq!(app.bundle_version.as_deref(), Some("2040"));
        assert_eq!(
            app.feed_url.as_deref(),
            Some("https://example.com/appcast.xml")
        );
    }
}
