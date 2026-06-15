//! SepiaSearch search engine implementation
//!
//! SepiaSearch is a PeerTube-based federated video search aggregator exposing
//! the PeerTube `/api/v1/search/videos` JSON endpoint.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// SepiaSearch search engine (videos, JSON API)
pub struct SepiaSearchEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl SepiaSearchEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "sepiasearch".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "SepiaSearch - federated PeerTube video search.".to_string(),
            website: Some("https://sepiasearch.org".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create SepiaSearch HTTP client");
        SepiaSearchEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://sepiasearch.org";
        let url = format!("{}/api/v1/search/videos", base_url.trim_end_matches('/'));
        let start = query.offset;
        let start_str = start.to_string();
        let count_str = "10".to_string();
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("search", query.query.as_str()),
                ("start", start_str.as_str()),
                ("count", count_str.as_str()),
                ("sort", "-match"),
                ("nsfw", "false"),
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
        let root: SepiaResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(_) => return Ok(vec![]),
        };
        let mut results = Vec::new();
        for v in root.data.iter().flatten() {
            let url = v
                .url
                .clone()
                .unwrap_or_else(|| v.uuid.clone().unwrap_or_default());
            if url.is_empty() {
                continue;
            }
            let title = v.name.clone().unwrap_or_default();
            let thumbnail = v
                .thumbnail_path
                .as_ref()
                .map(|p| format!("{}/static/previews/{}", base_url, p))
                .unwrap_or_default();
            let mut snippet_parts = Vec::new();
            if let Some(desc) = &v.description {
                let d = desc.trim();
                if !d.is_empty() {
                    snippet_parts.push(strip_html(d));
                }
            }
            if let Some(chan) = &v.channel {
                if let Some(name) = &chan.display_name {
                    snippet_parts.push(format!("Channel: {}", name));
                }
            }
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(snippet_parts.join(" | "))
                    .with_engine(self.name())
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("duration", serde_json::json!(v.duration.unwrap_or(0)))
                    .with_extra(
                        "views",
                        serde_json::json!(v.views.unwrap_or(0)),
                    )
                    .with_extra(
                        "published",
                        serde_json::json!(v.published_at.clone().unwrap_or_default()),
                    )
                    .with_extra(
                        "author",
                        serde_json::json!(
                            v.channel
                                .as_ref()
                                .and_then(|c| c.display_name.clone())
                                .unwrap_or_default()
                        ),
                    ),
            );
        }
        Ok(results)
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[derive(Debug, Serialize, Deserialize)]
struct SepiaResponse {
    #[serde(default)]
    data: Option<Vec<SepiaVideo>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SepiaVideo {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    thumbnail_path: Option<String>,
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    views: Option<i64>,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    channel: Option<SepiaChannel>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SepiaChannel {
    #[serde(default)]
    display_name: Option<String>,
}

#[async_trait]
impl Engine for SepiaSearchEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://sepiasearch.org".into());
        s.insert("api_version".into(), "v1".into());
        s
    }
}
