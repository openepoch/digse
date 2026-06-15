//! Command engine implementation.
//! Offline engine that runs shell commands.
//!
//! In this Rust port we do NOT execute arbitrary shell commands (security), so
//! the engine is registered and returns empty results with an informational log.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Command engine (offline) - placeholder that returns no results for safety.
pub struct CommandEngine {
    metadata: EngineMetadata,
}

impl CommandEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "command".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 4,
            description: "Command engine (offline) - runs arbitrary shell commands. Disabled for safety in digse.".to_string(),
            website: None,
        };
        CommandEngine { metadata }
    }
}

#[async_trait]
impl Engine for CommandEngine {
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
        tracing::info!("command engine: offline, returning empty (arbitrary shell execution disabled)");
        Ok(vec![])
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("engine_type".into(), "offline".into());
        s.insert("note".into(), "shell execution disabled for safety".into());
        s
    }
}
