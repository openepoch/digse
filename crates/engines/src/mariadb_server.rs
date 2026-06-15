//! MariaDB server search engine implementation
//!
//! The reference connects
//! to a MariaDB instance via the `mariadb` python driver and runs a SELECT
//! query. Since this Rust port cannot add new driver crates (no Cargo.toml
//! changes) and offline engines require a configured backend, the engine is
//! registered with full metadata but `search()` returns an empty result list
//! with an informational log. Category: general.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// MariaDB server (offline) search engine
pub struct MariadbServerEngine {
    metadata: EngineMetadata,
    host: String,
    port: u16,
    database: String,
    query_str: String,
}

impl MariadbServerEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "mariadb_server".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "MariaDB server - query a local MariaDB/MySQL-compatible database."
                .to_string(),
            website: Some("https://mariadb.org".to_string()),
        };

        MariadbServerEngine {
            metadata,
            host: std::env::var("MARIADB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: std::env::var("MARIADB_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3306),
            database: std::env::var("MARIADB_DATABASE").unwrap_or_default(),
            query_str: std::env::var("MARIADB_QUERY_STR").unwrap_or_default(),
        }
    }
}

#[async_trait]
impl Engine for MariadbServerEngine {
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
        tracing::info!(
            "mariadb_server requires backend configuration; returning empty"
        );
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
        settings.insert("query_str".to_string(), self.query_str.clone());
        settings
    }
}
