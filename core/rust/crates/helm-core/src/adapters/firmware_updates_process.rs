use std::path::Path;
use std::sync::Arc;

use crate::adapters::firmware_updates::{
    FirmwareUpdatesDetectOutput, FirmwareUpdatesSource, firmware_updates_history_request,
};
use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::execution::ProcessExecutor;

pub struct ProcessFirmwareUpdatesSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessFirmwareUpdatesSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }
}

impl FirmwareUpdatesSource for ProcessFirmwareUpdatesSource {
    fn detect(&self) -> AdapterResult<FirmwareUpdatesDetectOutput> {
        let softwareupdate_path = Path::new("/usr/sbin/softwareupdate");
        let executable_path = if softwareupdate_path.exists() {
            Some(softwareupdate_path.to_path_buf())
        } else {
            None
        };

        let request = firmware_updates_history_request(None);
        let history_output =
            run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default();

        Ok(FirmwareUpdatesDetectOutput {
            executable_path,
            history_output,
        })
    }

    fn history(&self) -> AdapterResult<String> {
        let request = firmware_updates_history_request(None);
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
