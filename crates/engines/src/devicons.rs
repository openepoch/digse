//! Devicons (icons) search engine implementation
//!
//! fetches the devicon.json manifest from
//! the jsDelivr CDN and filters icons whose name/altnames/tags match the query.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Devicons search engine
pub struct DeviconsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DeviconEntry {
    #[serde(default)]
    name: String,
    #[serde(default)]
    altnames: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    color: String,
    #[serde(default)]
    versions: DeviconVersions,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DeviconVersions {
    #[serde(default)]
    svg: Vec<String>,
    #[serde(default)]
    font: Vec<String>,
}

impl DeviconsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "devicons".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Devicon - programming icons.".to_string(),
            website: Some("https://devicon.dev/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Devicons HTTP client");

        DeviconsEngine { metadata, client }
    }

    fn matches(&self, entry: &DeviconEntry, query_parts: &[String]) -> bool {
        for part in query_parts {
            if entry.name.contains(part) {
                return true;
            }
            for tag in entry.altnames.iter().chain(entry.tags.iter()) {
                if tag.contains(part) {
                    return true;
                }
            }
        }
        false
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let cdn = "https://cdn.jsdelivr.net/gh/devicons/devicon@latest/devicon.json";

        let response = self
            .client
            .get(cdn)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let entries: Vec<DeviconEntry> = match serde_json::from_str(&text) {
            Ok(e) => e,
            Err(_) => return Ok(vec![]),
        };

        let query_parts: Vec<String> = query
            .query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        if query_parts.is_empty() {
            return Ok(vec![]);
        }

        let cdn_base = "https://cdn.jsdelivr.net/gh/devicons/devicon@latest";
        let mut results = Vec::new();
        for entry in entries.iter() {
            if !self.matches(entry, &query_parts) {
                continue;
            }
            for image_type in entry.versions.svg.iter() {
                let img_src = format!(
                    "{}/icons/{}/{}-{}.svg",
                    cdn_base, entry.name, entry.name, image_type
                );
                let title = entry.name.clone();
                let content = format!("Base color: {}", entry.color);
                let result = SearchResult::new(title, img_src.clone())
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + results.len() + 1)
                    .with_score(1.0 - (results.len() as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(img_src))
                    .with_extra("thumbnail", serde_json::json!(img_src))
                    .with_extra("format", serde_json::json!("SVG"));
                results.push(result);
                if results.len() >= query.count {
                    return Ok(results);
                }
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for DeviconsEngine {
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

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Images | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert(
            "cdn_base_url".to_string(),
            "https://cdn.jsdelivr.net/gh/devicons/devicon@latest".to_string(),
        );
        settings
    }
}
