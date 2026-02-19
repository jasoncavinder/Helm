use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::sparkle::{SparkleDetectOutput, SparkleSource, sparkle_detect_request};
use crate::execution::ProcessExecutor;

pub struct ProcessSparkleSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessSparkleSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }
}

impl SparkleSource for ProcessSparkleSource {
    fn detect(&self) -> AdapterResult<SparkleDetectOutput> {
        let host_app = locate_sparkle_host_app();
        let version_output = if let Some(app_path) = &host_app {
            let framework_info_plist =
                app_path.join("Contents/Frameworks/Sparkle.framework/Resources/Info.plist");
            let request = sparkle_detect_request(None, &framework_info_plist.to_string_lossy());
            run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default()
        } else {
            String::new()
        };

        Ok(SparkleDetectOutput {
            executable_path: host_app,
            version_output,
        })
    }
}

fn locate_sparkle_host_app() -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join("Applications"));
    }

    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };

        for entry in entries.filter_map(Result::ok) {
            let app_path = entry.path();
            if !is_app_bundle(&app_path) {
                continue;
            }

            if app_uses_sparkle(&app_path) {
                return Some(app_path);
            }
        }
    }

    None
}

fn is_app_bundle(path: &Path) -> bool {
    path.is_dir() && path.extension().and_then(|ext| ext.to_str()) == Some("app")
}

fn app_uses_sparkle(app_path: &Path) -> bool {
    app_path
        .join("Contents/Frameworks/Sparkle.framework")
        .exists()
}
