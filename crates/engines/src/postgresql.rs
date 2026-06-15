//! PostgreSQL search engine implementation (offline DB backend)
//!
//! The reference requires a live
//! PostgreSQL connection (via psycopg2) and runs a user-supplied `SELECT`
//! query. We cannot add a DB driver to this crate, so this implementation is a
//! registered engine that returns an empty result set, matching the
//! "offline" engine semantics when the backend is unavailable.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// PostgreSQL offline search engine
pub struct PostgreSqlEngine {
    metadata: EngineMetadata,
}

impl PostgreSqlEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "postgresql".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "PostgreSQL - Full-text search over a configured database.".to_string(),
            website: Some("https://www.postgresql.org".to_string()),
        };

        PostgreSqlEngine { metadata }
    }
}

#[async_trait]
impl Engine for PostgreSqlEngine {
    fn name(&self) -> &str {
        &self.metadata.name
    }
    fn category(&self) -> EngineCategory {
        self.metadata.category
    }
    fn is_enabled(&self) -> bool {
        self.metadata.enabled
    }
    fn metadata(&self) -> EngineMetadata {
        self.metadata.clone()
    }
    async fn search(&self, _query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // A live PostgreSQL driver is not bundled with this crate; the engine
        // is registered but performs no query.
        tracing::info!("postgresql requires backend; returning empty");
        Ok(vec![])
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("engine_type".to_string(), "offline".to_string());
        s.insert("host".to_string(), "127.0.0.1".to_string());
        s.insert("port".to_string(), "5432".to_string());
        s.insert("limit".to_string(), "10".to_string());
        s
    }
}
