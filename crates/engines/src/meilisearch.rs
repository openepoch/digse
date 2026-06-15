//! MeiliSearch search engine implementation
//!
//! queries a MeiliSearch instance via
//! its REST API. Category: general. Configurable via MEILISEARCH_BASE_URL and
//! MEILISEARCH_INDEX (and optional MEILISEARCH_AUTH_KEY).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// MeiliSearch engine
pub struct MeilisearchEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    index: String,
    auth_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct MeiliRequest<'a> {
    q: &'a str,
    offset: usize,
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct MeiliResponse {
    #[serde(default)]
    hits: Vec<serde_json::Value>,
}

impl MeilisearchEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "meilisearch".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "MeiliSearch - lightweight search backend.".to_string(),
            website: Some("https://www.meilisearch.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create MeiliSearch HTTP client");

        MeilisearchEngine {
            metadata,
            client,
            base_url: std::env::var("MEILISEARCH_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:7700".to_string()),
            index: std::env::var("MEILISEARCH_INDEX").unwrap_or_default(),
            auth_key: std::env::var("MEILISEARCH_AUTH_KEY").ok(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() || self.index.is_empty() {
            return Ok(vec![]);
        }
        let url = format!("{}/indexes/{}/search", self.base_url, self.index);
        let body = MeiliRequest {
            q: query.query.as_str(),
            offset: query.offset,
            limit: query.count,
        };

        let mut req = self
            .client
            .post(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body);
        if let Some(key) = &self.auth_key {
            req = req.header("Authorization", key);
        }
        let response = req.send().await.map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }
        let text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: MeiliResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, hit) in parsed.hits.iter().enumerate() {
            // Extract a best-effort title/url/content from the document.
            let title = hit
                .get("title")
                .and_then(|v| v.as_str())
                .or_else(|| hit.get("name").and_then(|v| v.as_str()))
                .or_else(|| hit.get("documentation").and_then(|v| v.as_str()))
                .unwrap_or("MeiliSearch result")
                .to_string();
            let url = hit
                .get("url")
                .and_then(|v| v.as_str())
                .or_else(|| hit.get("link").and_then(|v| v.as_str()))
                .unwrap_or(&url)
                .to_string();
            let content = hit
                .get("abstract")
                .and_then(|v| v.as_str())
                .or_else(|| hit.get("description").and_then(|v| v.as_str()))
                .or_else(|| hit.get("content").and_then(|v| v.as_str()))
                .unwrap_or("")
                .to_string();

            let mut result = SearchResult::new(title, url)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            if !content.is_empty() {
                result = result.with_snippet(content);
            }
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for MeilisearchEngine {
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
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("index".to_string(), self.index.clone());
        settings.insert("api_endpoint".to_string(), "/indexes/{index}/search".to_string());
        settings
    }
}
