//! Demo offline engine implementation
//!
//! an example offline engine that
//! demonstrates the offline engine contract. In this port it simply returns an
//! empty result list (no local index is wired up).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Demo offline engine (returns an empty list)
pub struct DemoOfflineEngine {
    metadata: EngineMetadata,
}

impl DemoOfflineEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "demo_offline".to_string(),
            category: EngineCategory::General,
            enabled: false,
            requires_auth: false,
            timeout_seconds: 2,
            description: "Demo offline engine (example).".to_string(),
            website: None,
        };
        DemoOfflineEngine { metadata }
    }
}

#[async_trait]
impl Engine for DemoOfflineEngine {
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
        let mut settings = HashMap::new();
        settings.insert("engine_type".to_string(), "offline".to_string());
        settings
    }
}
