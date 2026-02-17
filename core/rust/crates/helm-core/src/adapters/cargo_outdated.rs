use std::collections::BTreeSet;

use crate::adapters::cargo::parse_cargo_installed;
use crate::adapters::manager::AdapterResult;
use crate::models::{CoreError, CoreErrorKind, ManagerAction, ManagerId, TaskType};

#[derive(serde::Serialize)]
struct OutdatedEntry {
    name: String,
    installed_version: String,
    candidate_version: String,
}

pub(crate) fn synthesize_outdated_payload<F>(
    manager: ManagerId,
    installed_raw: &str,
    mut resolve_latest: F,
) -> AdapterResult<String>
where
    F: FnMut(&str) -> AdapterResult<Option<String>>,
{
    let installed = parse_cargo_installed(installed_raw).map_err(|mut error| {
        error.manager = Some(manager);
        error
    })?;

    let mut seen = BTreeSet::new();
    let mut outdated = Vec::new();

    for package in installed {
        let name = package.package.name;
        if !seen.insert(name.clone()) {
            continue;
        }

        let Some(installed_version) = package.installed_version else {
            continue;
        };

        let Some(latest) = resolve_latest(&name)? else {
            continue;
        };

        if latest != installed_version {
            outdated.push(OutdatedEntry {
                name,
                installed_version,
                candidate_version: latest,
            });
        }
    }

    serde_json::to_string(&outdated).map_err(|e| CoreError {
        manager: Some(manager),
        task: Some(TaskType::Refresh),
        action: Some(ManagerAction::ListOutdated),
        kind: CoreErrorKind::ParseFailure,
        message: format!("failed to encode synthesized outdated payload: {e}"),
    })
}
