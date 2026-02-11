use std::time::SystemTime;

use crate::models::{ManagerId, PackageCandidate};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchQuery {
    pub text: String,
    pub issued_at: SystemTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedSearchResult {
    pub result: PackageCandidate,
    pub source_manager: ManagerId,
    pub originating_query: String,
    pub cached_at: SystemTime,
}
