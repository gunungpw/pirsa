//! Durable file and folder index for the launcher.
//!
//! Turso stores the metadata and the Tantivy-backed FTS index. The indexer runs
//! on its own thread/runtime so scanning and writes never block iced.

mod db;
mod indexer;
mod rank;
mod service;

use std::path::Path;

use pirsa_core::search_scope::{DriveInfo, SearchScope};

pub use pirsa_core::FileEntry;
pub use service::{IndexConfig, IndexHandle, start};

pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}

impl IndexConfig {
    /// Builds the index configuration from the user's folder-search scope.
    #[must_use]
    pub fn from_scope(scope: &SearchScope, profile: &Path, drives: &[DriveInfo]) -> Self {
        Self {
            roots: scope.roots(profile, drives),
            scope: scope.clone(),
            fingerprint: scope.fingerprint(),
        }
    }
}
