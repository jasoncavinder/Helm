use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::models::{ManagerId, PackageCandidate};

pub const LIBRARY_RESULT_PROVENANCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryResultOrigin {
    Local,
    LocalCache,
    Remote,
    Deferred,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryResultDiscoverySource {
    ManagerSnapshot,
    CatalogSync,
    ManagerSearch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LibraryResultProvenance {
    pub schema_version: u32,
    pub origin: LibraryResultOrigin,
    pub discovery_source: LibraryResultDiscoverySource,
    pub source_manager: ManagerId,
    pub originating_query: Option<String>,
}

impl LibraryResultProvenance {
    pub fn manager_snapshot(source_manager: ManagerId) -> Self {
        Self {
            schema_version: LIBRARY_RESULT_PROVENANCE_SCHEMA_VERSION,
            origin: LibraryResultOrigin::Local,
            discovery_source: LibraryResultDiscoverySource::ManagerSnapshot,
            source_manager,
            originating_query: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub issued_at: SystemTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CachedSearchResult {
    pub result: PackageCandidate,
    pub source_manager: ManagerId,
    pub originating_query: String,
    pub cached_at: SystemTime,
}

impl CachedSearchResult {
    /// Projects persisted search truth without claiming that a cache read is a live remote result.
    pub fn library_result_provenance(&self) -> LibraryResultProvenance {
        let originating_query = self.originating_query.trim();
        LibraryResultProvenance {
            schema_version: LIBRARY_RESULT_PROVENANCE_SCHEMA_VERSION,
            origin: LibraryResultOrigin::LocalCache,
            discovery_source: if originating_query.is_empty() {
                LibraryResultDiscoverySource::CatalogSync
            } else {
                LibraryResultDiscoverySource::ManagerSearch
            },
            source_manager: self.source_manager,
            originating_query: (!originating_query.is_empty())
                .then(|| originating_query.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CachedSearchResult, LIBRARY_RESULT_PROVENANCE_SCHEMA_VERSION, LibraryResultDiscoverySource,
        LibraryResultOrigin, LibraryResultProvenance,
    };
    use crate::models::{ManagerId, PackageCandidate, PackageRef};
    use std::time::{Duration, UNIX_EPOCH};

    fn result(originating_query: &str, cached_at_seconds: u64) -> CachedSearchResult {
        CachedSearchResult {
            result: PackageCandidate {
                package: PackageRef {
                    manager: ManagerId::Cargo,
                    name: "ripgrep".to_string(),
                },
                package_identifier: None,
                version: Some("14.1.1".to_string()),
                summary: None,
            },
            source_manager: ManagerId::Cargo,
            originating_query: originating_query.to_string(),
            cached_at: UNIX_EPOCH + Duration::from_secs(cached_at_seconds),
        }
    }

    #[test]
    fn manager_snapshot_is_local_without_query() {
        let provenance = LibraryResultProvenance::manager_snapshot(ManagerId::Rustup);

        assert_eq!(provenance.origin, LibraryResultOrigin::Local);
        assert_eq!(
            provenance.discovery_source,
            LibraryResultDiscoverySource::ManagerSnapshot
        );
        assert_eq!(provenance.source_manager, ManagerId::Rustup);
        assert_eq!(provenance.originating_query, None);
    }

    #[test]
    fn interactive_cache_result_preserves_manager_search_without_claiming_remote_delivery() {
        let provenance = result("  ripgrep  ", 1_800_000_000).library_result_provenance();

        assert_eq!(
            provenance.schema_version,
            LIBRARY_RESULT_PROVENANCE_SCHEMA_VERSION
        );
        assert_eq!(provenance.origin, LibraryResultOrigin::LocalCache);
        assert_eq!(
            provenance.discovery_source,
            LibraryResultDiscoverySource::ManagerSearch
        );
        assert_eq!(provenance.source_manager, ManagerId::Cargo);
        assert_eq!(provenance.originating_query.as_deref(), Some("ripgrep"));
    }

    #[test]
    fn catalog_cache_result_omits_an_empty_query() {
        let provenance = result("   ", 1_800_000_001).library_result_provenance();

        assert_eq!(provenance.origin, LibraryResultOrigin::LocalCache);
        assert_eq!(
            provenance.discovery_source,
            LibraryResultDiscoverySource::CatalogSync
        );
        assert_eq!(provenance.originating_query, None);
    }

    #[test]
    fn provenance_serialization_uses_versioned_snake_case_contract_values() {
        let value =
            serde_json::to_value(result("ripgrep", 1_800_000_002).library_result_provenance())
                .expect("provenance should serialize");

        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["origin"], "local_cache");
        assert_eq!(value["discovery_source"], "manager_search");
        assert_eq!(value["source_manager"], "cargo");
        assert!(value.get("cached_at_unix").is_none());
    }
}
