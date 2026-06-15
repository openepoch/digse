//! Odysee search engine implementation (videos; JSON)
//!
//! Odysee is a decentralized video
//! hosting platform (LBRY). Queries the lighthouse search API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Odysee video search engine
pub struct OdyseeEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const RESULTS_PER_PAGE: i64 = 20;

#[derive(Debug, Serialize, Deserialize)]
struct OdyseeItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    claimId: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    thumbnail_url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    release_time: String,
    #[serde(default)]
    duration: i64,
}

impl OdyseeEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "odysee".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Odysee - Decentralized video hosting (LBRY).".to_string(),
            website: Some("https://odysee.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Odysee HTTP client");

        OdyseeEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://lighthouse.odysee.tv/search";
        let from = (query.offset as i64).to_string();
        let size = RESULTS_PER_PAGE.to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("s", query.query.as_str()),
                ("size", size.as_str()),
                ("from", from.as_str()),
                (
                    "include",
                    "channel,thumbnail_url,title,description,duration,release_time",
                ),
                ("mediaType", "video"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: Vec<OdyseeItem> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.iter().enumerate() {
            if item.title.is_empty() && item.name.is_empty() {
                continue;
            }
            let url = format!("https://odysee.com/{}:{}", item.name, item.claimId);
            let iframe_url = format!("https://odysee.com/$/embed/{}:{}", item.name, item.claimId);
            let thumbnail = if !item.thumbnail_url.is_empty() {
                format!(
                    "https://thumbnails.odycdn.com/optimize/s:390:0/quality:85/plain/{}",
                    item.thumbnail_url
                )
            } else {
                String::new()
            };

            results.push(
                SearchResult::new(&item.title, &url)
                    .with_snippet(item.description.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("duration", serde_json::json!(item.duration))
                    .with_extra("author", serde_json::json!(item.channel))
                    .with_extra("published", serde_json::json!(item.release_time))
                    .with_extra("iframe_src", serde_json::json!(iframe_url)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for OdyseeEngine {
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
        s.insert("base_url".to_string(), "https://lighthouse.odysee.tv/search".to_string());
        s.insert("results_per_page".to_string(), RESULTS_PER_PAGE.to_string());
        s
    }
}
