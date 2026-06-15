//! Fyyd search engine implementation
//!
//! Uses the Fyyd podcast API
//! `https://api.fyyd.de/0.2/search/podcast?term=...&count=N&page=M`. Category:
//! music (podcasts).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Fyyd podcast search engine (JSON API)
pub struct FyydEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct FyydResponse {
    #[serde(default)]
    data: Vec<FyydPodcast>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FyydPodcast {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "htmlURL")]
    html_url: String,
    #[serde(default, rename = "smallImageURL")]
    small_image_url: String,
    #[serde(default)]
    status_since: String,
    #[serde(default)]
    rank: f64,
    #[serde(default, rename = "episode_count")]
    episode_count: i64,
}

impl FyydEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "fyyd".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Fyyd - podcast search.".to_string(),
            website: Some("https://fyyd.de".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Fyyd HTTP client");

        FyydEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.fyyd.de/0.2/search/podcast";
        let count = query.count.to_string();
        // ref: page = pageno - 1
        let page = query.offset.to_string();

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("term", query.query.as_str()),
                ("count", count.as_str()),
                ("page", page.as_str()),
            ])
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

        let parsed: FyydResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, pod) in parsed.data.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let title = if pod.title.is_empty() {
                "Podcast".to_string()
            } else {
                pod.title.clone()
            };
            let url = if pod.html_url.is_empty() {
                pod.small_image_url.clone()
            } else {
                pod.html_url.clone()
            };
            let metadata_str = format!("Rank: {} || {} episodes", pod.rank, pod.episode_count);
            let snippet = if pod.description.is_empty() {
                metadata_str.clone()
            } else {
                format!("{} | {}", pod.description, metadata_str)
            };

            let result = SearchResult::new(title, url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Music)
                .with_extra("thumbnail", serde_json::json!(pod.small_image_url))
                .with_extra("published", serde_json::json!(pod.status_since))
                .with_extra("rank", serde_json::json!(pod.rank))
                .with_extra("episode_count", serde_json::json!(pod.episode_count))
                .with_extra("source", serde_json::json!("fyyd"));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for FyydEngine {
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
        matches!(result_type, ResultType::Music | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://api.fyyd.de".to_string());
        settings.insert(
            "search_endpoint".to_string(),
            "/0.2/search/podcast".to_string(),
        );
        settings
    }
}
