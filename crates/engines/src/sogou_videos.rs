//! Sogou Videos search engine implementation
//!
//! Queries Sogou's
//! internal short-video JSON API and returns video results.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Sogou Videos search engine
pub struct SogouVideosEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://v.sogou.com";

#[derive(Debug, Serialize, Deserialize, Default)]
struct SogouVideosResponse {
    #[serde(default)]
    data: SogouVideosData,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SogouVideosData {
    #[serde(default)]
    list: Vec<SogouVideosItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SogouVideosItem {
    #[serde(default)]
    #[serde(rename = "titleEsc")]
    title_esc: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    site: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    duration: String,
    #[serde(default)]
    picurl: String,
}

impl SogouVideosEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "sogou_videos".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Sogou Videos - Chinese short video search.".to_string(),
            website: Some("https://v.sogou.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Sogou Videos HTTP client");

        SogouVideosEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let endpoint = format!("{}/api/video/shortVideoV2", BASE_URL);
        // Paging is 1-based; our offset is a 0-based result offset.
        let page = ((query.offset / 10) + 1).to_string();
        let pagesize = "10";

        let resp = self
            .client
            .get(&endpoint)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("page", page.as_str()),
                ("pagesize", pagesize),
                ("query", query.query.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }

        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let parsed: SogouVideosResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, entry) in parsed.data.list.iter().enumerate() {
            if entry.title_esc.is_empty() || entry.url.is_empty() {
                continue;
            }

            let mut video_url = entry.url.clone();
            if video_url.starts_with("/vc/np") {
                video_url = format!("{}{}", BASE_URL, video_url);
            }

            let mut result = SearchResult::new(entry.title_esc.clone(), video_url)
                .with_snippet(entry.site.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Videos);

            if !entry.picurl.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(entry.picurl));
            }
            if !entry.duration.is_empty() {
                result = result.with_extra("duration", serde_json::json!(entry.duration));
            }
            if !entry.date.is_empty() {
                result = result.with_extra("published", serde_json::json!(entry.date));
            }
            if !entry.site.is_empty() {
                result = result.with_extra("source", serde_json::json!(entry.site));
            }

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for SogouVideosEngine {
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
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("results".into(), "JSON".into());
        s.insert("language".into(), "zh".into());
        s
    }
}
