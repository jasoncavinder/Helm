use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::setapp::{SetappDetectOutput, SetappSource, setapp_detect_request};
use crate::execution::ProcessExecutor;

pub struct ProcessSetappSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessSetappSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }
}

impl SetappSource for ProcessSetappSource {
    fn detect(&self) -> AdapterResult<SetappDetectOutput> {
        let app_path = locate_setapp_app();
        let executable_path = app_path
            .as_ref()
            .map(|path| resolve_setapp_executable_path(path.as_path()))
            .filter(|path| path.exists())
            .or(app_path.clone());

        let version_output = if let Some(app_path) = &app_path {
            let plist_path = app_path.join("Contents/Info.plist");
            let request = setapp_detect_request(None, &plist_path.to_string_lossy());
            run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default()
        } else {
            String::new()
        };

        Ok(SetappDetectOutput {
            executable_path,
            version_output,
        })
    }
}

fn locate_setapp_app() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications/Setapp.app")];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Applications/Setapp.app"));
    }
    candidates.into_iter().find(|path| path.exists())
}

fn resolve_setapp_executable_path(app_path: &Path) -> PathBuf {
    app_path.join("Contents/MacOS/Setapp")
}
