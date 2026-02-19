use std::path::Path;
use std::sync::Arc;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::xcode_command_line_tools::{
    XcodeCommandLineToolsDetectOutput, XcodeCommandLineToolsSource,
    xcode_command_line_tools_detect_request, xcode_command_line_tools_list_outdated_request,
    xcode_command_line_tools_upgrade_request,
};
use crate::execution::ProcessExecutor;

pub struct ProcessXcodeCommandLineToolsSource {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessXcodeCommandLineToolsSource {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }
}

impl XcodeCommandLineToolsSource for ProcessXcodeCommandLineToolsSource {
    fn detect(&self) -> AdapterResult<XcodeCommandLineToolsDetectOutput> {
        let clang_path = Path::new("/Library/Developer/CommandLineTools/usr/bin/clang");
        let executable_path = if clang_path.exists() {
            Some(clang_path.to_path_buf())
        } else {
            None
        };

        let request = xcode_command_line_tools_detect_request(None);
        let version_output =
            run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default();

        Ok(XcodeCommandLineToolsDetectOutput {
            executable_path,
            version_output,
        })
    }

    fn list_outdated(&self) -> AdapterResult<String> {
        let request = xcode_command_line_tools_list_outdated_request(None);
        Ok(run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default())
    }

    fn upgrade(&self, label: &str) -> AdapterResult<String> {
        let request = xcode_command_line_tools_upgrade_request(None, label);
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}
