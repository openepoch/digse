//! Bitchute video search engine implementation.
//! Uses the Bitchute JSON API (POST).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Bitchute video search engine.
pub struct BitchuteEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct BitchuteRequest {
    offset: usize,
    limit: usize,
    query: String,
    sensitivity_id: &'static str,
    sort: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
struct BitchuteResponse {
    #[serde(default)]
    videos: Vec<BitchuteItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BitchuteItem {
    #[serde(default)]
    video_name: String,
    #[serde(default)]
    video_id: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    channel: BitchuteChannel,
    #[serde(default)]
    date_published: String,
    #[serde(default)]
    duration: serde_json::Value,
    #[serde(default)]
    view_count: serde_json::Value,
    #[serde(default)]
    thumbnail_url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BitchuteChannel {
    #[serde(default)]
    channel_name: String,
}

impl BitchuteEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bitchute".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Bitchute - peer-to-peer video sharing.".to_string(),
            website: Some("https://bitchute.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Bitchute HTTP client");
        BitchuteEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.bitchute.com/api/beta/search/videos";
        let limit = query.count.max(1);
        let offset = query.offset * limit;
        let body = BitchuteRequest {
            offset,
            limit,
            query: query.query.clone(),
            sensitivity_id: "normal",
            sort: "new",
        };

        let resp = self
            .client
            .post(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Content-Type", "application/json")
            .json(&body)
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
        let parsed: BitchuteResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.videos.iter().enumerate() {
            let page_url = format!("https://www.bitchute.com/video/{}", item.video_id);
            let iframe = format!("https://www.bitchute.com/embed/{}", item.video_id);
            let views = match &item.view_count {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => String::new(),
            };
            let duration = match &item.duration {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => String::new(),
            };
            results.push(
                SearchResult::new(item.video_name.clone(), page_url)
                    .with_snippet(item.description.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("author", serde_json::json!(item.channel.channel_name))
                    .with_extra("published", serde_json::json!(item.date_published))
                    .with_extra("duration", serde_json::json!(duration))
                    .with_extra("views", serde_json::json!(views))
                    .with_extra("thumbnail", serde_json::json!(item.thumbnail_url))
                    .with_extra("iframe_src", serde_json::json!(iframe)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for BitchuteEngine {
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
        s.insert(
            "base_url".into(),
            "https://api.bitchute.com/api/beta/search/videos".into(),
        );
        s
    }
}
