//! Recoll search engine implementation
//!
//! Recoll is an offline
//! desktop full-text search tool (built on Xapian); it is normally fronted by
//! `recoll-webui`. digse has no access to a local index, so this engine is
//! registered but always returns an empty result list with an informational
//! log message.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Recoll offline desktop search engine (registered, no-op)
pub struct RecollEngine {
    metadata: EngineMetadata,
}

impl RecollEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "recoll".to_string(),
            category: EngineCategory::General,
            enabled: false,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Recoll - offline desktop full-text search (requires local index)."
                .to_string(),
            website: Some("https://www.lesbonscomptes.com/recoll/".to_string()),
        };
        RecollEngine { metadata }
    }
}

#[async_trait]
impl Engine for RecollEngine {
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
        // recoll requires a local index (recoll-webui); digse has none, so we
        // always return an empty result list.
        eprintln!("recoll requires local index; returning empty");
        Ok(vec![])
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::Files | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "".into());
        s.insert("mount_prefix".into(), "".into());
        s.insert("dl_prefix".into(), "".into());
        s.insert("search_dir".into(), "".into());
        s.insert("offline".into(), "true".into());
        s
    }
}
