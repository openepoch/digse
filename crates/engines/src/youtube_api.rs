//! YouTube Data API v3 search engine implementation (JSON, paid).
//!
//! Requires `YOUTUBE_API_KEY`. Without a key the engine returns an empty
//! result set gracefully.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// YouTube Data API v3 search engine.
pub struct YoutubeApiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

const BASE_YOUTUBE_URL: &str = "https://www.youtube.com/watch?v=";

impl YoutubeApiEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("YOUTUBE_API_KEY")
            .ok()
            .filter(|s| !s.is_empty() && s != "unset");
        let metadata = EngineMetadata {
            name: "youtube_api".to_string(),
            category: EngineCategory::Videos,
            enabled: api_key.is_some(),
            requires_auth: true,
            timeout_seconds: 20,
            description: "YouTube Data API v3 - video search.".to_string(),
            website: Some("https://www.youtube.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create YouTube API HTTP client");
        YoutubeApiEngine {
            metadata,
            client,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                tracing::info!("youtube_api requires YOUTUBE_API_KEY; returning empty");
                return Ok(vec![]);
            }
        };

        let mut url = format!(
            "https://www.googleapis.com/youtube/v3/search?part=snippet&q={}&maxResults=20&key={}",
            urlencoding::encode(&query.query),
            key
        );
        if let Some(lang) = &query.language {
            let short = lang.split('-').next().unwrap_or(lang);
            url.push_str(&format!("&relevanceLanguage={}", short));
        }

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: YtResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };
        if parsed.error.is_some() {
            tracing::info!("youtube_api returned an error; returning empty");
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        let items = parsed.items.unwrap_or_default();
        for (i, item) in items.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let video_id = match &item.id.video_id {
                Some(v) if !v.is_empty() => v.clone(),
                _ => continue, // ignore channels/playlists
            };
            let snippet = &item.snippet;
            let url = format!("{}{}", BASE_YOUTUBE_URL, video_id);
            let title = snippet.title.clone();
            let content = snippet.description.clone();
            let thumbnail = snippet
                .thumbnails
                .high
                .url
                .clone()
                .unwrap_or_default();
            let iframe_src = format!("https://www.youtube-nocookie.com/embed/{}", video_id);

            let r = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("video_id", serde_json::json!(video_id))
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("published", serde_json::json!(snippet.published_at))
                .with_extra("author", serde_json::json!(snippet.channel_title))
                .with_extra("iframe_src", serde_json::json!(iframe_src));
            results.push(r);
        }
        Ok(results)
    }
}

#[derive(Debug, Deserialize)]
struct YtResponse {
    #[serde(default)]
    error: Option<YtError>,
    #[serde(default)]
    items: Option<Vec<YtItem>>,
}

#[derive(Debug, Deserialize)]
struct YtError {
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct YtItem {
    #[serde(default)]
    id: YtId,
    #[serde(default)]
    snippet: YtSnippet,
}

#[derive(Debug, Deserialize, Default)]
struct YtId {
    #[serde(default)]
    video_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct YtSnippet {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    channel_title: String,
    #[serde(default)]
    thumbnails: YtThumbnails,
}

#[derive(Debug, Deserialize, Default)]
struct YtThumbnails {
    #[serde(default)]
    high: YtThumb,
}

#[derive(Debug, Deserialize, Default)]
struct YtThumb {
    #[serde(default)]
    url: Option<String>,
}

#[async_trait]
impl Engine for YoutubeApiEngine {
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
        s.insert("base_url".to_string(), "https://www.googleapis.com/youtube/v3".to_string());
        s.insert("results".to_string(), "JSON".to_string());
        s.insert("requires_api_key".to_string(), "true".to_string());
        s
    }
}
