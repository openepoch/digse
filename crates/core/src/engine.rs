//! Core engine trait and types

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::Result;
use crate::query::SearchQuery;
use crate::result::SearchResult;

/// Engine categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineCategory {
    General,
    Images,
    Videos,
    Music,
    News,
    Science,
    IT,
    Files,
    Social,
    Maps,
    Shopping,
    Weather,
}

impl EngineCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            EngineCategory::General => "general",
            EngineCategory::Images => "images",
            EngineCategory::Videos => "videos",
            EngineCategory::Music => "music",
            EngineCategory::News => "news",
            EngineCategory::Science => "science",
            EngineCategory::IT => "it",
            EngineCategory::Files => "files",
            EngineCategory::Social => "social",
            EngineCategory::Maps => "maps",
            EngineCategory::Shopping => "shopping",
            EngineCategory::Weather => "weather",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "general" => Some(EngineCategory::General),
            "images" => Some(EngineCategory::Images),
            "videos" => Some(EngineCategory::Videos),
            "music" => Some(EngineCategory::Music),
            "news" => Some(EngineCategory::News),
            "science" => Some(EngineCategory::Science),
            "it" => Some(EngineCategory::IT),
            "files" => Some(EngineCategory::Files),
            "social" => Some(EngineCategory::Social),
            "maps" => Some(EngineCategory::Maps),
            "shopping" => Some(EngineCategory::Shopping),
            "weather" => Some(EngineCategory::Weather),
            _ => None,
        }
    }
}

/// Engine metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineMetadata {
    pub name: String,
    pub category: EngineCategory,
    pub enabled: bool,
    pub requires_auth: bool,
    pub timeout_seconds: u64,
    pub description: String,
    pub website: Option<String>,
}

/// Core engine trait
#[async_trait]
pub trait Engine: Send + Sync {
    /// Get engine name
    fn name(&self) -> &str;

    /// Get engine category
    fn category(&self) -> EngineCategory;

    /// Check if engine is enabled
    fn is_enabled(&self) -> bool;

    /// Get engine metadata
    fn metadata(&self) -> EngineMetadata;

    /// Perform search
    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;

    /// Check if engine supports specific result type
    fn supports_result_type(&self, result_type: &crate::result::ResultType) -> bool;

    /// Get engine-specific settings
    fn settings(&self) -> HashMap<String, String>;
}