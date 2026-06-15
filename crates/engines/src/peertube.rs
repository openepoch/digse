//! PeerTube search engine implementation (videos; JSON)
//!
//! Queries a PeerTube instance's
//! search API (default `https://peer.tube`). Federated video hosting.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// PeerTube video search engine
pub struct PeertubeEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PeertubeResponse {
    #[serde(default)]
    data: Vec<PeertubeVideo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PeertubeVideo {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    duration: i64,
    #[serde(default)]
    views: i64,
    #[serde(default)]
    publishedAt: String,
    #[serde(default)]
    embedUrl: String,
    #[serde(default)]
    thumbnailUrl: String,
    #[serde(default)]
    previewUrl: String,
    #[serde(default)]
    account: Option<PeertubeAccount>,
    #[serde(default)]
    channel: Option<PeertubeChannel>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PeertubeAccount {
    #[serde(default)]
    displayName: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PeertubeChannel {
    #[serde(default)]
    displayName: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    host: String,
}

impl PeertubeEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "peertube".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "PeerTube - Federated video hosting.".to_string(),
            website: Some("https://joinpeertube.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create PeerTube HTTP client");

        PeertubeEngine {
            metadata,
            client,
            base_url: "https://peer.tube".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let start = query.offset.to_string();

        let resp = self
            .client
            .get(format!("{}/api/v1/search/videos", self.base_url))
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("search", query.query.as_str()),
                ("searchTarget", "search-index"),
                ("resultType", "videos"),
                ("start", start.as_str()),
                ("count", "10"),
                ("sort", "-match"),
                ("nsfw", "both"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: PeertubeResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, v) in parsed.data.iter().enumerate() {
            if v.name.is_empty() && v.url.is_empty() {
                continue;
            }
            let url = if v.url.starts_with("http") {
                v.url.clone()
            } else {
                format!("{}{}", self.base_url, v.url)
            };

            let thumbnail = if !v.thumbnailUrl.is_empty() {
                v.thumbnailUrl.clone()
            } else {
                v.previewUrl.clone()
            };

            let author = v
                .account
                .as_ref()
                .map(|a| {
                    if a.displayName.is_empty() {
                        a.name.clone()
                    } else {
                        a.displayName.clone()
                    }
                })
                .unwrap_or_default();

            // strip HTML from description (PeerTube returns HTML)
            let content: String = strip_html(&v.description);

            let mut metadata_parts = Vec::new();
            if let Some(ch) = &v.channel {
                if !ch.displayName.is_empty() {
                    metadata_parts.push(ch.displayName.clone());
                }
                if !ch.name.is_empty() && !ch.host.is_empty() {
                    metadata_parts.push(format!("{}@{}", ch.name, ch.host));
                }
            }
            if !v.tags.is_empty() {
                metadata_parts.push(v.tags.join(", "));
            }

            results.push(
                SearchResult::new(&v.name, &url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("duration", serde_json::json!(v.duration))
                    .with_extra("views", serde_json::json!(v.views))
                    .with_extra("author", serde_json::json!(author))
                    .with_extra("published", serde_json::json!(v.publishedAt))
                    .with_extra("iframe_src", serde_json::json!(v.embedUrl))
                    .with_extra("metadata", serde_json::json!(metadata_parts.join(" | "))),
            );
        }
        Ok(results)
    }
}

/// Naive HTML tag stripper (PeerTube descriptions are HTML).
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[async_trait]
impl Engine for PeertubeEngine {
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
        s.insert("base_url".to_string(), self.base_url.clone());
        s
    }
}
