//! media.ccc.de video search engine implementation.
//! JSON API for CCC conference recordings.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// media.ccc.de (CCC conference recordings) video search engine.
pub struct CccMediaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct CccResponse {
    #[serde(default)]
    events: Vec<CccEvent>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CccEvent {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    frontend_link: String,
    #[serde(default)]
    thumb_url: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    length: i64,
    #[serde(default)]
    recordings: Vec<CccRecording>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CccRecording {
    #[serde(default)]
    mime_type: String,
    #[serde(default)]
    recording_url: String,
}

impl CccMediaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "ccc_media".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "media.ccc.de - CCC conference recordings.".to_string(),
            website: Some("https://media.ccc.de".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create CCC media HTTP client");
        CccMediaEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.media.ccc.de/public/events/search";
        let page = (query.offset + 1).to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[("q", query.query.as_str()), ("page", page.as_str())])
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
        let parsed: CccResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.events.iter().enumerate() {
            if item.frontend_link.is_empty() {
                continue;
            }
            // Prefer mp4 video recordings, else first video recording.
            let mut iframe_src = String::new();
            let mut fallback = String::new();
            for rec in &item.recordings {
                if rec.mime_type.starts_with("video") {
                    if iframe_src.is_empty() {
                        fallback = rec.recording_url.clone();
                    }
                    if rec.mime_type == "video/mp4" {
                        iframe_src = rec.recording_url.clone();
                    }
                }
            }
            if iframe_src.is_empty() {
                iframe_src = fallback;
            }
            results.push(
                SearchResult::new(item.title.clone(), item.frontend_link.clone())
                    .with_snippet(item.description.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(item.thumb_url))
                    .with_extra("published", serde_json::json!(item.date))
                    .with_extra("duration", serde_json::json!(item.length))
                    .with_extra("iframe_src", serde_json::json!(iframe_src)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for CccMediaEngine {
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
        s.insert("base_url".into(), "https://api.media.ccc.de".into());
        s
    }
}
