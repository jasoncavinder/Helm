use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::models::{ManagerId, PackageCandidate};

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
