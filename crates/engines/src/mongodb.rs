//! MongoDB search engine implementation
//!
//! The reference connects to a
//! MongoDB instance via `pymongo` and queries a collection. Since this Rust
//! port cannot add new driver crates (no Cargo.toml changes) and offline
//! engines require a configured backend, the engine is registered with full
//! metadata but `search()` returns an empty result list with an informational
//! log. Category: general.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// MongoDB (offline) search engine
pub struct MongoDbEngine {
    metadata: EngineMetadata,
    host: String,
    port: u16,
    database: String,
    collection: String,
    key: String,
    exact_match_only: bool,
}

impl MongoDbEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "mongodb".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "MongoDB - query a local MongoDB collection.".to_string(),
            website: Some("https://www.mongodb.com".to_string()),
        };

        MongoDbEngine {
            metadata,
            host: std::env::var("MONGODB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("MONGODB_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(27017),
            database: std::env::var("MONGODB_DATABASE").unwrap_or_default(),
            collection: std::env::var("MONGODB_COLLECTION").unwrap_or_default(),
            key: std::env::var("MONGODB_KEY").unwrap_or_else(|_| "name".to_string()),
            exact_match_only: std::env::var("MONGODB_EXACT_MATCH")
                .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
        }
    }
}

#[async_trait]
impl Engine for MongoDbEngine {
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
        tracing::info!("mongodb requires backend configuration; returning empty");
        Ok(vec![])
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("host".to_string(), self.host.clone());
        settings.insert("port".to_string(), self.port.to_string());
        settings.insert("database".to_string(), self.database.clone());
        settings.insert("collection".to_string(), self.collection.clone());
        settings.insert("key".to_string(), self.key.clone());
        settings.insert(
            "exact_match_only".to_string(),
            self.exact_match_only.to_string(),
        );
        settings
    }
}
