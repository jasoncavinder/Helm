use std::sync::Arc;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::softwareupdate::{
    SoftwareUpdateSource, softwareupdate_detect_request, softwareupdate_list_request,
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
    fn detect(&self) -> AdapterResult<String> {
        let request = softwareupdate_detect_request(None);
        // sw_vers and softwareupdate are system binaries at fixed paths;
        // no PATH manipulation needed.
        run_and_collect_stdout(self.executor.as_ref(), request)
    }

    fn list_available(&self) -> AdapterResult<String> {
        let request = softwareupdate_list_request(None);
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
