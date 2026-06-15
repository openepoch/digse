//! Valkey (Redis-like) in-memory key/value store search engine implementation.
//!
//! The upstream engine requires a Valkey/Redis driver and a running backend,
//! which we cannot add here. The engine is registered and returns an empty
//! result set with a log notice.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Valkey server (offline key/value store) engine.
pub struct ValkeyServerEngine {
    metadata: EngineMetadata,
}

impl ValkeyServerEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "valkey_server".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Valkey - in-memory key/value store search (requires backend)."
                .to_string(),
            website: Some("https://valkey.io".to_string()),
        };
        ValkeyServerEngine { metadata }
    }
}

#[async_trait]
impl Engine for ValkeyServerEngine {
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
        tracing::info!("valkey_server requires backend; returning empty");
        Ok(vec![])
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("engine_type".to_string(), "offline".to_string());
        s.insert("host".to_string(), "127.0.0.1".to_string());
        s.insert("port".to_string(), "6379".to_string());
        s.insert("db".to_string(), "0".to_string());
        s.insert("exact_match_only".to_string(), "true".to_string());
        s
    }
}
