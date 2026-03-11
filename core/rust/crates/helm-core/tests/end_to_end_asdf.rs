use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use helm_core::adapters::asdf::{AsdfAdapter, AsdfDetectOutput, AsdfInstallSource, AsdfSource};
use helm_core::adapters::{
    AdapterRequest, AdapterResponse, DetectRequest, InstallRequest, ListInstalledRequest,
    ListOutdatedRequest, ManagerAdapter, SearchRequest, UninstallRequest, UpgradeRequest,
};
use helm_core::models::{ManagerId, PackageRef, SearchQuery};
use helm_core::orchestration::{AdapterRuntime, AdapterTaskTerminalState};

const VERSION_FIXTURE: &str = include_str!("fixtures/asdf/version.txt");
const SEARCH_FIXTURE: &str = include_str!("fixtures/asdf/plugin_list_all.txt");

#[derive(Default)]
struct AsdfState {
    plugins: BTreeSet<String>,
    installed_versions: HashMap<String, Vec<String>>,
    current_versions: HashMap<String, String>,
    latest_versions: HashMap<String, String>,
}

struct StatefulAsdfSource {
    state: Mutex<AsdfState>,
}

impl StatefulAsdfSource {
    fn new() -> Self {
        let mut plugins = BTreeSet::new();
        plugins.insert("nodejs".to_string());
        plugins.insert("python".to_string());
        plugins.insert("ruby".to_string());

        let installed_versions = HashMap::from([
            ("nodejs".to_string(), vec!["20.12.2".to_string()]),
            (
                "python".to_string(),
                vec!["3.11.9".to_string(), "3.12.2".to_string()],
            ),
            ("ruby".to_string(), vec!["3.3.1".to_string()]),
        ]);
        let current_versions = HashMap::from([
            ("nodejs".to_string(), "20.12.2".to_string()),
            ("python".to_string(), "3.12.2".to_string()),
        ]);
        let latest_versions = HashMap::from([
            ("nodejs".to_string(), "20.12.3".to_string()),
            ("python".to_string(), "3.13.0".to_string()),
            ("ruby".to_string(), "3.3.2".to_string()),
            ("terraform".to_string(), "1.9.8".to_string()),
        ]);

        Self {
            state: Mutex::new(AsdfState {
                plugins,
                installed_versions,
                current_versions,
                latest_versions,
            }),
        }
    }

    fn home_tool_versions_path() -> String {
        std::env::var("HOME")
            .map(|home| format!("{home}/.tool-versions"))
            .unwrap_or_else(|_| "/Users/test/.tool-versions".to_string())
    }
}

impl AsdfSource for StatefulAsdfSource {
    fn detect(&self) -> helm_core::adapters::AdapterResult<AsdfDetectOutput> {
        Ok(AsdfDetectOutput {
            executable_path: Some(PathBuf::from("/Users/test/.asdf/bin/asdf")),
            version_output: VERSION_FIXTURE.to_string(),
        })
    }

    fn list_current(&self) -> helm_core::adapters::AdapterResult<String> {
        let state = self.state.lock().expect("asdf state lock poisoned");
        let home_path = Self::home_tool_versions_path();
        let mut lines = Vec::new();
        for plugin in ["nodejs", "python"] {
            if let Some(version) = state.current_versions.get(plugin) {
                lines.push(format!("{plugin}          {version}          {home_path}"));
            }
        }
        Ok(lines.join("\n"))
    }

    fn list_plugins(&self) -> helm_core::adapters::AdapterResult<String> {
        let state = self.state.lock().expect("asdf state lock poisoned");
        Ok(state.plugins.iter().cloned().collect::<Vec<_>>().join("\n"))
    }

    fn list_installed_versions(&self, plugin: &str) -> helm_core::adapters::AdapterResult<String> {
        let state = self.state.lock().expect("asdf state lock poisoned");
        let versions = state
            .installed_versions
            .get(plugin)
            .cloned()
            .unwrap_or_default();
        Ok(versions
            .into_iter()
            .map(|version| format!("  {version}"))
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn search_plugins(&self, _query: &SearchQuery) -> helm_core::adapters::AdapterResult<String> {
        Ok(SEARCH_FIXTURE.to_string())
    }

    fn latest_version(&self, plugin: &str) -> helm_core::adapters::AdapterResult<String> {
        let state = self.state.lock().expect("asdf state lock poisoned");
        Ok(state
            .latest_versions
            .get(plugin)
            .cloned()
            .unwrap_or_else(|| "1.0.0".to_string()))
    }

    fn add_plugin(&self, plugin: &str) -> helm_core::adapters::AdapterResult<String> {
        let mut state = self.state.lock().expect("asdf state lock poisoned");
        state.plugins.insert(plugin.to_string());
        Ok(String::new())
    }

    fn install_plugin(
        &self,
        plugin: &str,
        version: Option<&str>,
    ) -> helm_core::adapters::AdapterResult<String> {
        let mut state = self.state.lock().expect("asdf state lock poisoned");
        let resolved_version = version
            .map(str::to_string)
            .or_else(|| state.latest_versions.get(plugin).cloned())
            .unwrap_or_else(|| "latest".to_string());
        state.plugins.insert(plugin.to_string());
        let versions = state
            .installed_versions
            .entry(plugin.to_string())
            .or_default();
        if !versions
            .iter()
            .any(|existing| existing == &resolved_version)
        {
            versions.push(resolved_version.clone());
            versions.sort();
        }
        Ok(String::new())
    }

    fn uninstall_plugin(
        &self,
        plugin: &str,
        version: &str,
    ) -> helm_core::adapters::AdapterResult<String> {
        let mut state = self.state.lock().expect("asdf state lock poisoned");
        if let Some(versions) = state.installed_versions.get_mut(plugin) {
            versions.retain(|existing| existing != version);
        }
        if state
            .current_versions
            .get(plugin)
            .is_some_and(|current| current == version)
        {
            state.current_versions.remove(plugin);
        }
        Ok(String::new())
    }

    fn set_home_version(
        &self,
        plugin: &str,
        version: &str,
    ) -> helm_core::adapters::AdapterResult<String> {
        let mut state = self.state.lock().expect("asdf state lock poisoned");
        state
            .current_versions
            .insert(plugin.to_string(), version.to_string());
        Ok(String::new())
    }

    fn install_self(
        &self,
        _source: AsdfInstallSource,
    ) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }

    fn self_uninstall(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }

    fn self_update(&self) -> helm_core::adapters::AdapterResult<String> {
        Ok(String::new())
    }
}

fn build_runtime() -> AdapterRuntime {
    let adapter: Arc<dyn ManagerAdapter> = Arc::new(AsdfAdapter::new(StatefulAsdfSource::new()));
    AdapterRuntime::new([adapter]).expect("runtime creation should succeed")
}

#[tokio::test]
async fn asdf_detect_list_search_and_mutate_through_orchestration() {
    let runtime = build_runtime();

    let detect_task = runtime
        .submit(ManagerId::Asdf, AdapterRequest::Detect(DetectRequest))
        .await
        .unwrap();
    let detect_snapshot = runtime
        .wait_for_terminal(detect_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match detect_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Detection(info))) => {
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("0.16.0"));
            assert_eq!(
                info.executable_path,
                Some(PathBuf::from("/Users/test/.asdf/bin/asdf"))
            );
        }
        other => panic!("expected asdf detection response, got {other:?}"),
    }

    let installed_task = runtime
        .submit(
            ManagerId::Asdf,
            AdapterRequest::ListInstalled(ListInstalledRequest),
        )
        .await
        .unwrap();
    let installed_snapshot = runtime
        .wait_for_terminal(installed_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match installed_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::InstalledPackages(packages))) => {
            assert_eq!(packages.len(), 4);
            assert!(packages.iter().any(|package| {
                package.package.name == "python"
                    && package.installed_version.as_deref() == Some("3.12.2")
                    && package.runtime_state.is_default
            }));
        }
        other => panic!("expected asdf installed packages, got {other:?}"),
    }

    let outdated_task = runtime
        .submit(
            ManagerId::Asdf,
            AdapterRequest::ListOutdated(ListOutdatedRequest),
        )
        .await
        .unwrap();
    let outdated_snapshot = runtime
        .wait_for_terminal(outdated_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match outdated_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::OutdatedPackages(packages))) => {
            assert_eq!(packages.len(), 3);
            assert!(packages.iter().any(|package| {
                package.package.name == "nodejs" && package.candidate_version == "20.12.3"
            }));
        }
        other => panic!("expected asdf outdated packages, got {other:?}"),
    }

    let search_task = runtime
        .submit(
            ManagerId::Asdf,
            AdapterRequest::Search(SearchRequest {
                query: SearchQuery {
                    text: "python".to_string(),
                    issued_at: SystemTime::now(),
                },
            }),
        )
        .await
        .unwrap();
    let search_snapshot = runtime
        .wait_for_terminal(search_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match search_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::SearchResults(results))) => {
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].result.package.name, "python");
        }
        other => panic!("expected asdf search results, got {other:?}"),
    }

    let install_task = runtime
        .submit(
            ManagerId::Asdf,
            AdapterRequest::Install(InstallRequest {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: "terraform".to_string(),
                },
                target_name: None,
                version: Some("1.9.8".to_string()),
            }),
        )
        .await
        .unwrap();
    let install_snapshot = runtime
        .wait_for_terminal(install_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match install_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "terraform");
            assert_eq!(mutation.before_version, None);
            assert_eq!(mutation.after_version.as_deref(), Some("1.9.8"));
        }
        other => panic!("expected asdf install mutation, got {other:?}"),
    }

    let uninstall_task = runtime
        .submit(
            ManagerId::Asdf,
            AdapterRequest::Uninstall(UninstallRequest {
                package: PackageRef {
                    manager: ManagerId::Asdf,
                    name: "python".to_string(),
                },
                target_name: None,
                version: Some("3.12.2".to_string()),
            }),
        )
        .await
        .unwrap();
    let uninstall_snapshot = runtime
        .wait_for_terminal(uninstall_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match uninstall_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "python");
            assert_eq!(mutation.before_version.as_deref(), Some("3.12.2"));
            assert_eq!(mutation.after_version, None);
        }
        other => panic!("expected asdf uninstall mutation, got {other:?}"),
    }

    let upgrade_task = runtime
        .submit(
            ManagerId::Asdf,
            AdapterRequest::Upgrade(UpgradeRequest {
                package: Some(PackageRef {
                    manager: ManagerId::Asdf,
                    name: "nodejs".to_string(),
                }),
                target_name: None,
                version: None,
            }),
        )
        .await
        .unwrap();
    let upgrade_snapshot = runtime
        .wait_for_terminal(upgrade_task, Some(Duration::from_secs(5)))
        .await
        .unwrap();
    match upgrade_snapshot.terminal_state {
        Some(AdapterTaskTerminalState::Succeeded(AdapterResponse::Mutation(mutation))) => {
            assert_eq!(mutation.package.name, "nodejs");
            assert_eq!(mutation.before_version.as_deref(), Some("20.12.2"));
            assert_eq!(mutation.after_version.as_deref(), Some("20.12.3"));
        }
        other => panic!("expected asdf upgrade mutation, got {other:?}"),
    }
}
