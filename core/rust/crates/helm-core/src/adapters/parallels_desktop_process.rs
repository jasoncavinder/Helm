use std::path::PathBuf;
use std::sync::Arc;

use crate::adapters::manager::AdapterResult;
use crate::adapters::parallels_desktop::{
    ParallelsDesktopDetectOutput, ParallelsDesktopSource, parallels_desktop_detect_request,
};
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::execution::ProcessExecutor;

pub struct ProcessParallelsDesktopSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessParallelsDesktopSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }
}

impl ParallelsDesktopSource for ProcessParallelsDesktopSource {
    fn detect(&self) -> AdapterResult<ParallelsDesktopDetectOutput> {
        let app_path = locate_parallels_desktop_app();
        let executable_path = app_path
            .as_ref()
            .map(resolve_parallels_desktop_executable_path)
            .filter(|path| path.exists())
            .or(app_path.clone());

        let version_output = if let Some(app_path) = &app_path {
            let plist_path = app_path.join("Contents/Info.plist");
            let request = parallels_desktop_detect_request(None, &plist_path.to_string_lossy());
            run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default()
        } else {
            String::new()
        };

        Ok(ParallelsDesktopDetectOutput {
            executable_path,
            version_output,
        })
    }
}

fn locate_parallels_desktop_app() -> Option<PathBuf> {
    let mut candidates = vec![PathBuf::from("/Applications/Parallels Desktop.app")];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Applications/Parallels Desktop.app"));
    }
    candidates.into_iter().find(|path| path.exists())
}

fn resolve_parallels_desktop_executable_path(app_path: &PathBuf) -> PathBuf {
    app_path.join("Contents/MacOS/Parallels Desktop")
}
