use std::path::Path;
use std::sync::Arc;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::softwareupdate::{
    SoftwareUpdateDetectOutput, SoftwareUpdateSource, softwareupdate_detect_request,
    softwareupdate_list_request, softwareupdate_upgrade_request,
};
use crate::execution::ProcessExecutor;

pub struct ProcessSoftwareUpdateSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessSoftwareUpdateSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }
}

impl SoftwareUpdateSource for ProcessSoftwareUpdateSource {
    fn detect(&self) -> AdapterResult<SoftwareUpdateDetectOutput> {
        // Phase 1: instant filesystem check — sw_vers is a system binary at a fixed path
        let sw_vers_path = Path::new("/usr/bin/sw_vers");
        let executable_path = if sw_vers_path.exists() {
            Some(sw_vers_path.to_path_buf())
        } else {
            None
        };

        // Phase 2: best-effort version (timeout is non-fatal)
        let request = softwareupdate_detect_request(None);
        let version_output =
            run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default();

        Ok(SoftwareUpdateDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_available(&self) -> AdapterResult<String> {
        let request = softwareupdate_list_request(None);
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn install_all_updates(&self) -> AdapterResult<String> {
        let request = softwareupdate_upgrade_request(None);
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
