//! Frinkiac search engine implementation
//!
//! Uses the Frinkiac JSON API
//! `https://frinkiac.com/api/search?q=...` returning a list of
//! `{Episode, Timestamp, ...}` caption frames. Category: images.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Frinkiac Simpsons frame/caption search engine (JSON API)
pub struct FrinkiacEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct FrinkiacFrame {
    #[serde(default)]
    #[serde(rename = "Episode")]
    episode: String,
    #[serde(default)]
    #[serde(rename = "Timestamp")]
    timestamp: i64,
}

impl FrinkiacEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "frinkiac".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Frinkiac - Simpsons screenshot/frame search.".to_string(),
            website: Some("https://frinkiac.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Frinkiac HTTP client");

        FrinkiacEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://frinkiac.com/api/search";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[("q", query.query.as_str())])
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

        let frames: Vec<FrinkiacFrame> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let base = "https://frinkiac.com";
        let mut results = Vec::new();
        for (i, frame) in frames.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let episode = &frame.episode;
            let timestamp = frame.timestamp;
            let result_url = format!(
                "{}/?p=caption&e={}&t={}",
                base, episode, timestamp
            );
            let thumb_url = format!("{}/img/{}/{}/medium.jpg", base, episode, timestamp);
            let image_url = format!("{}/img/{}/{}.jpg", base, episode, timestamp);

            let result = SearchResult::new(format!("Simpsons · {}", episode), result_url)
                .with_snippet("")
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(image_url))
                .with_extra("thumbnail", serde_json::json!(thumb_url))
                .with_extra("source", serde_json::json!("frinkiac"))
                .with_extra("episode", serde_json::json!(episode))
                .with_extra("timestamp", serde_json::json!(timestamp));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for FrinkiacEngine {
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
        settings.insert("base_url".to_string(), "https://frinkiac.com".to_string());
        settings.insert("search_endpoint".to_string(), "/api/search".to_string());
        settings
    }
}
