//! Generic XPath search engine implementation.
//!
//! The upstream XPath engine is a *generic*, configuration-driven engine that
//! is fully driven by runtime settings (search_url, results_xpath, url_xpath,
//! title_xpath, content_xpath, etc.). Without that runtime configuration there
//! is nothing to query, so the registered engine returns an empty result set
//! with a log notice.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Generic XPath engine (configured at runtime via settings).
pub struct XpathEngine {
    metadata: EngineMetadata,
}

impl XpathEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "xpath".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Generic XPath engine - configured at runtime via settings.".to_string(),
            website: None,
        };
        XpathEngine { metadata }
    }
}

#[async_trait]
impl Engine for XpathEngine {
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
        tracing::info!("xpath engine: requires runtime config; returning empty");
        Ok(vec![])
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("search_url".to_string(), String::new());
        s.insert("results_xpath".to_string(), String::new());
        s.insert("url_xpath".to_string(), String::new());
        s.insert("title_xpath".to_string(), String::new());
        s.insert("content_xpath".to_string(), String::new());
        s.insert("method".to_string(), "GET".to_string());
        s.insert("paging".to_string(), "false".to_string());
        s
    }
}
