//! Lucide icons search engine implementation
//!
//! fetches the lucide-static tags.json and
//! filters icon names/tags client-side. Category: images / icons.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Lucide icon search engine
pub struct LucideEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    cdn_base_url: String,
}

impl LucideEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "lucide".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Lucide - copyleft SVG icon library.".to_string(),
            website: Some("https://lucide.dev/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Lucide HTTP client");

        LucideEngine {
            metadata,
            client,
            cdn_base_url: "https://cdn.jsdelivr.net/npm/lucide-static".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let url = format!("{}/tags.json", self.cdn_base_url);
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
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
        let tags: serde_json::Map<String, serde_json::Value> = match serde_json::from_str(&text) {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        let query_parts: Vec<String> = query
            .query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        let mut results = Vec::new();
        let mut rank = 0usize;
        for (icon_name, value) in tags.iter() {
            if rank >= query.count {
                break;
            }
            let tags_list: Vec<String> = value
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            let matched = query_parts.iter().any(|part| {
                icon_name.to_lowercase().contains(part)
                    || tags_list.iter().any(|t| t.to_lowercase().contains(part))
            });
            if !matched {
                continue;
            }

            rank += 1;
            let img_src = format!("{}/icons/{}.svg", self.cdn_base_url, icon_name);
            let i = rank - 1;
            let result = SearchResult::new(icon_name.clone(), img_src.clone())
                .with_snippet(tags_list.join(", "))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src.clone()))
                .with_extra("thumbnail", serde_json::json!(img_src))
                .with_extra("format", serde_json::json!("SVG"))
                .with_extra("source", serde_json::json!("lucide"));
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for LucideEngine {
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
        settings.insert("cdn_base_url".to_string(), self.cdn_base_url.clone());
        settings.insert("tags_url".to_string(), "/tags.json".to_string());
        settings
    }
}
