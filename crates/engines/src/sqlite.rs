//! SQLite search engine implementation
//!
//! This is an
//! *offline* engine that opens a local SQLite database file and runs a
//! configured `query_str`. A Rust port cannot add a SQL driver at this stage,
//! and a configured database file may not be present, so this implementation
//! is a registered engine that returns an empty result set with an
//! informational log line.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// SQLite-backed (offline) search engine
pub struct SqliteEngine {
    metadata: EngineMetadata,
}

impl SqliteEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "sqlite".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "SQLite - offline SQL database search backend.".to_string(),
            website: Some("https://www.sqlite.org".to_string()),
        };
        SqliteEngine { metadata }
    }
}

#[async_trait]
impl Engine for SqliteEngine {
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
        tracing::info!("sqlite requires backend; returning empty");
        Ok(vec![])
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::Files | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("engine_type".into(), "offline".into());
        s.insert("database".into(), "".into());
        s.insert("query_str".into(), "".into());
        s
    }
}
