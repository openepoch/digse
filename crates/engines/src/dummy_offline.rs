//! Dummy Offline search engine implementation
//!
//! The reference engine returns a
//! single dummy result dict; this Rust port returns an empty vec (as directed
//! for the offline dummy registration).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Dummy Offline engine (returns no results; registration placeholder)
pub struct DummyOfflineEngine {
    metadata: EngineMetadata,
}

impl DummyOfflineEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "dummy-offline".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Dummy Offline - offline placeholder engine.".to_string(),
            website: None,
        };
        DummyOfflineEngine { metadata }
    }
}

#[async_trait]
impl Engine for DummyOfflineEngine {
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
        tracing::info!("dummy-offline: returning empty");
        Ok(vec![])
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("type".to_string(), "dummy-offline".to_string());
        settings.insert("offline".to_string(), "true".to_string());
        settings
    }
}
