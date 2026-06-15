//! Elasticsearch search engine implementation
//!
//! The reference engine queries an
//! Elasticsearch cluster via the `_search` API using one of several query
//! DSL types (match, simple_query_string, term, terms, custom). The Rust port
//! requires a driver/backend configuration that cannot be added to Cargo.toml,
//! so `search()` is a registered placeholder that returns empty.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Elasticsearch search engine (requires a configured backend)
pub struct ElasticsearchEngine {
    metadata: EngineMetadata,
}

impl ElasticsearchEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "elasticsearch".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Elasticsearch - search backend (requires configured instance)."
                .to_string(),
            website: Some("https://www.elastic.co/elasticsearch/".to_string()),
        };
        ElasticsearchEngine { metadata }
    }
}

#[async_trait]
impl Engine for ElasticsearchEngine {
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
        tracing::info!("elasticsearch requires backend; returning empty");
        Ok(vec![])
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("type".to_string(), "elasticsearch".to_string());
        settings.insert("base_url".to_string(), "http://localhost:9200".to_string());
        settings.insert("index".to_string(), "".to_string());
        settings.insert("query_type".to_string(), "match".to_string());
        settings
    }
}
