//! 360Search Videos search engine implementation (JSON API)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// 360Search Videos (tv.360kan.com) search engine
pub struct Search360VideosEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Videos360Response {
    #[serde(default)]
    data: Videos360Data,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Videos360Data {
    #[serde(default)]
    result: Vec<Videos360Item>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Videos360Item {
    #[serde(default)]
    title: String,
    #[serde(default)]
    play_url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    cover_img: String,
    #[serde(default)]
    publish_time: serde_json::Value,
}

impl Search360VideosEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "360search_videos".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "360Search Videos - video search (tv.360kan.com).".to_string(),
            website: Some("https://tv.360kan.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create 360Search Videos HTTP client");

        Search360VideosEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://tv.360kan.com";
        let start = (((query.offset / 10) + 1) * 10).to_string();

        let resp = self.client
            .get(format!("{}/v1/video/list", base_url))
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("count", "10"),
                ("q", query.query.as_str()),
                ("start", start.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: Videos360Response = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.data.result.iter().enumerate() {
            if item.title.is_empty() || item.play_url.is_empty() {
                continue;
            }
            let mut r = SearchResult::new(item.title.clone(), item.play_url.clone())
                .with_snippet(item.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("thumbnail", serde_json::json!(item.cover_img))
                .with_extra("iframe_src", serde_json::json!(item.play_url));

            if let Some(ts) = item.publish_time.as_i64() {
                r = r.with_extra("published", serde_json::json!(ts));
            }
            results.push(r);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for Search360VideosEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Videos | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://tv.360kan.com".to_string());
        s
    }
}
