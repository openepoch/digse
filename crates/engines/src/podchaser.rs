//! Podchaser search engine implementation (music/podcasts; JSON)
//!
//! Queries the Podchaser public
//! podcast API. The reference sets an `Accept:
//! application/prs.podchaser.v2+json` header.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Podchaser podcast search engine
pub struct PodchaserEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const PAGE_SIZE: i64 = 25;

#[derive(Debug, Serialize, Deserialize)]
struct PodchaserResponse {
    #[serde(default)]
    entities: Vec<PodchaserPodcast>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PodchaserPodcast {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    feed_url: String,
    #[serde(default)]
    image_url: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    number_of_episodes: i64,
    #[serde(default)]
    categories: Vec<PodchaserCategory>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PodchaserCategory {
    #[serde(default)]
    text: String,
}

impl PodchaserEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "podchaser".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Podchaser - Podcast database and discovery.".to_string(),
            website: Some("https://www.podchaser.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Podchaser HTTP client");

        PodchaserEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://api.podchaser.com";
        let offset = query.offset.to_string();
        let limit = PAGE_SIZE.to_string();

        let resp = self
            .client
            .get(format!("{}/podcasts", base_url))
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/prs.podchaser.v2+json")
            .query(&[
                ("filters[term]", query.query.as_str()),
                ("limit", limit.as_str()),
                ("offset", offset.as_str()),
                ("sort_direction", "desc"),
                ("sort_order", "SORT_ORDER_RELEVANCE"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: PodchaserResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, pod) in parsed.entities.iter().enumerate() {
            if pod.title.is_empty() {
                continue;
            }
            let url = if pod.feed_url.is_empty() {
                format!("https://www.podchaser.com/podcasts/{}", pod.id)
            } else {
                pod.feed_url.clone()
            };

            let mut metadata_parts = vec![format!("{} episodes", pod.number_of_episodes)];
            if !pod.categories.is_empty() {
                let cats: Vec<&str> = pod.categories.iter().map(|c| c.text.as_str()).collect();
                metadata_parts.push(cats.join(", "));
            }

            results.push(
                SearchResult::new(&pod.title, &url)
                    .with_snippet(pod.description.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Music)
                    .with_extra("thumbnail", serde_json::json!(pod.image_url))
                    .with_extra("published", serde_json::json!(pod.created_at))
                    .with_extra("episodes", serde_json::json!(pod.number_of_episodes))
                    .with_extra("metadata", serde_json::json!(metadata_parts.join(" | "))),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for PodchaserEngine {
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
        matches!(t, ResultType::Music | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://api.podchaser.com".to_string());
        s.insert("page_size".to_string(), PAGE_SIZE.to_string());
        s
    }
}
