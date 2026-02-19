use std::path::Path;
use std::sync::Arc;

use crate::adapters::manager::AdapterResult;
use crate::adapters::process_utils::run_and_collect_stdout;
use crate::adapters::rosetta2::{
    Rosetta2DetectOutput, Rosetta2Source, rosetta2_detect_request, rosetta2_install_request,
};
use crate::execution::ProcessExecutor;

pub struct ProcessRosetta2Source {
    executor: Arc<dyn ProcessExecutor>,
}

impl ProcessRosetta2Source {
    pub fn new(executor: Arc<dyn ProcessExecutor>) -> Self {
        Self { executor }
    }
}

impl Rosetta2Source for ProcessRosetta2Source {
    fn detect(&self) -> AdapterResult<Rosetta2DetectOutput> {
        if !host_is_apple_silicon() {
            return Ok(Rosetta2DetectOutput {
                executable_path: None,
                version_output: String::new(),
            });
        }

        let runtime_path = Path::new("/Library/Apple/usr/libexec/oah/libRosettaRuntime");
        let executable_path = if runtime_path.exists() {
            Some(runtime_path.to_path_buf())
        } else {
            None
        };

        let request = rosetta2_detect_request(None);
        let version_output =
            run_and_collect_stdout(self.executor.as_ref(), request).unwrap_or_default();

        Ok(Rosetta2DetectOutput {
            executable_path,
            version_output,
        })
    }

    fn install(&self) -> AdapterResult<String> {
        let request = rosetta2_install_request(None);
        run_and_collect_stdout(self.executor.as_ref(), request)
    }
}

fn host_is_apple_silicon() -> bool {
    matches!(std::env::consts::ARCH, "aarch64" | "arm64")
}
