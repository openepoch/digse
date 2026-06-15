//! Piped search engine implementation (videos; JSON)
//!
//! Piped is a privacy-friendly YouTube
//! frontend. This implementation queries a configurable Piped backend
//! (default `https://pipedapi.kavin.rocks`) and links results to the
//! `piped.video` frontend.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Piped video search engine
pub struct PipedEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    backend_url: String,
    frontend_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PipedResponse {
    #[serde(default)]
    items: Vec<PipedItem>,
    #[serde(default)]
    nextpage: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PipedItem {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    uploaderName: String,
    #[serde(default)]
    uploaded: i64,
    #[serde(default)]
    duration: i64,
    #[serde(default)]
    views: i64,
    #[serde(default)]
    shortDescription: Option<String>,
}

impl PipedEngine {
    pub fn new() -> Self {
        let backend_url =
            std::env::var("PIPED_BACKEND_URL").unwrap_or_else(|_| "https://pipedapi.kavin.rocks".to_string());
        let frontend_url =
            std::env::var("PIPED_FRONTEND_URL").unwrap_or_else(|_| "https://piped.video".to_string());

        let metadata = EngineMetadata {
            name: "piped".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Piped - Privacy-friendly YouTube frontend.".to_string(),
            website: Some("https://github.com/TeamPiped/Piped/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Piped HTTP client");

        PipedEngine {
            metadata,
            client,
            backend_url,
            frontend_url,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!("{}/search", self.backend_url);

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("filter", "videos"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: PipedResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.items.iter().enumerate() {
            if item.title.is_empty() && item.url.is_empty() {
                continue;
            }
            let result_url = format!("{}{}", self.frontend_url, item.url);
            let iframe_src = format!("{}/embed{}", self.frontend_url, item.url);

            let content = item.shortDescription.clone().unwrap_or_default();
            let author = item.uploaderName.clone();

            let mut snippet_parts = Vec::new();
            if !author.is_empty() {
                snippet_parts.push(author.clone());
            }
            if !content.is_empty() {
                let truncated = if content.len() > 200 {
                    format!("{}...", &content[..200])
                } else {
                    content.clone()
                };
                snippet_parts.push(truncated);
            }

            results.push(
                SearchResult::new(&item.title, &result_url)
                    .with_snippet(snippet_parts.join(" | "))
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(item.thumbnail))
                    .with_extra("duration", serde_json::json!(item.duration))
                    .with_extra("views", serde_json::json!(item.views))
                    .with_extra("author", serde_json::json!(author))
                    .with_extra("published", serde_json::json!(item.uploaded))
                    .with_extra("iframe_src", serde_json::json!(iframe_src)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for PipedEngine {
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
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("backend_url".to_string(), self.backend_url.clone());
        s.insert("frontend_url".to_string(), self.frontend_url.clone());
        s.insert("piped_filter".to_string(), "videos".to_string());
        s
    }
}
