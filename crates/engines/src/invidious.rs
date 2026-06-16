//! Invidious search engine implementation
//!
//! YouTube proxy via the Invidious
//! JSON API. Requires a configured `base_url` (instance may be down — graceful).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Invidious (YouTube proxy) search engine
pub struct InvidiousEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct InvidiousVideo {
    #[serde(default, rename = "videoId")]
    video_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "type")]
    item_type: String,
    #[serde(default)]
    author: String,
    #[serde(default, rename = "viewCount")]
    view_count: i64,
    #[serde(default, rename = "lengthSeconds")]
    length_seconds: i64,
    #[serde(default)]
    published: i64,
    #[serde(default, rename = "videoThumbnails")]
    video_thumbnails: Vec<InvidiousThumb>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct InvidiousThumb {
    #[serde(default)]
    quality: String,
    #[serde(default)]
    url: String,
}

impl InvidiousEngine {
    pub fn new() -> Self {
        Self::with_base_url("https://invidious.snopyta.org")
    }

    pub fn with_base_url(base_url: &str) -> Self {
        let metadata = EngineMetadata {
            name: "invidious".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Invidious - privacy-friendly YouTube proxy.".to_string(),
            website: Some("https://api.invidious.io/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Invidious HTTP client");
        InvidiousEngine {
            metadata,
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = ((query.offset / query.count.max(1)) + 1).to_string();
        let url = format!(
            "{}/api/v1/search?q={}&page={}",
            self.base_url,
            urlencoding::encode(&query.query),
            pageno
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
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
        let videos: Vec<InvidiousVideo> = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, video) in videos.iter().enumerate() {
            if results.len() >= query.count {
                break;
            }
            if video.item_type != "video" || video.video_id.is_empty() {
                continue;
            }
            let url = format!("{}/watch?v={}", self.base_url, video.video_id);
            let thumbnail = video
                .video_thumbnails
                .iter()
                .find(|t| t.quality == "sddefault")
                .or_else(|| video.video_thumbnails.first())
                .map(|t| {
                    if t.url.starts_with("http") {
                        t.url.clone()
                    } else {
                        format!("{}{}", self.base_url, t.url)
                    }
                })
                .unwrap_or_default();
            let iframe_src = format!("{}/embed/{}", self.base_url, video.video_id);
            let duration = format_duration(video.length_seconds);

            let result = SearchResult::new(video.title.clone(), url)
                .with_snippet(video.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + results.len() + 1)
                .with_score(1.0 - (results.len() as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("duration", serde_json::json!(duration))
                .with_extra("views", serde_json::json!(video.view_count))
                .with_extra("iframe_src", serde_json::json!(iframe_src))
                .with_extra("author", serde_json::json!(video.author));
            let _ = i;
            results.push(result);
        }
        Ok(results)
    }
}

/// Format seconds as M:SS or H:MM:SS (mirrors Python's time.strftime logic).
fn format_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return String::new();
    }
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}

#[async_trait]
impl Engine for InvidiousEngine {
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
        matches!(t, ResultType::Videos | ResultType::Music | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), self.base_url.clone());
        s.insert("api_endpoint".into(), "/api/v1/search".into());
        s
    }
}
