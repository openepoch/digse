//! Dummy engine implementation
//!
//! a no-op engine that always returns an
//! empty result list. Used for testing/scaffolding.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Dummy engine (returns empty results)
pub struct DummyEngine {
    metadata: EngineMetadata,
}

impl DummyEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "dummy".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 5,
            description: "Dummy engine (returns no results).".to_string(),
            website: None,
        };
        DummyEngine { metadata }
    }
}

#[async_trait]
impl Engine for DummyEngine {
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
        eprintln!("offline/demo engine: returning empty");
        Ok(vec![])
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}
