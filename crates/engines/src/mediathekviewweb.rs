//! MediathekViewWeb search engine implementation
//!
//! queries the German public-broadcast
//! media catalog via its JSON API. Category: videos.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// MediathekViewWeb (German TV/video) search engine
pub struct MediathekviewwebEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct MvwRequest {
    queries: Vec<MvwQuery>,
    #[serde(rename = "sortBy")]
    sort_by: String,
    #[serde(rename = "sortOrder")]
    sort_order: String,
    future: bool,
    offset: usize,
    size: usize,
}

#[derive(Debug, Serialize)]
struct MvwQuery {
    fields: Vec<String>,
    query: String,
}

#[derive(Debug, Deserialize)]
struct MvwResponse {
    #[serde(default)]
    result: MvwResultOuter,
}

#[derive(Debug, Deserialize, Default)]
struct MvwResultOuter {
    #[serde(default)]
    results: Vec<MvwItem>,
}

#[derive(Debug, Deserialize, Default)]
struct MvwItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    topic: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    duration: i64,
    #[serde(default)]
    url_video_hd: String,
}

impl MediathekviewwebEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "mediathekviewweb".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "MediathekViewWeb - German public-broadcast media catalog.".to_string(),
            website: Some("https://mediathekviewweb.de/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create MediathekViewWeb HTTP client");

        MediathekviewwebEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let size = 10usize.min(query.count.max(1));
        let body = MvwRequest {
            queries: vec![MvwQuery {
                fields: vec!["title".to_string(), "topic".to_string()],
                query: query.query.clone(),
            }],
            sort_by: "timestamp".to_string(),
            sort_order: "desc".to_string(),
            future: true,
            offset: query.offset,
            size,
        };

        let response = self
            .client
            .post("https://mediathekviewweb.de/api/query")
            .header("User-Agent", "digse/0.1.0")
            .header("Content-Type", "text/plain")
            .header("Accept", "application/json")
            .body(serde_json::to_vec(&body).unwrap_or_default())
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
        let parsed: MvwResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.result.results.iter().enumerate() {
            let url = if item.url_video_hd.is_empty() {
                continue;
            } else {
                item.url_video_hd.replace("http://", "https://")
            };
            let channel = item.topic.clone();
            let title = format!("{}: {} ({})", channel, item.title, fmt_duration(item.duration));
            let iframe = url.clone();
            let result = SearchResult::new(title, url.clone())
                .with_snippet(item.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("iframe_src", serde_json::json!(iframe))
                .with_extra("duration", serde_json::json!(fmt_duration(item.duration)))
                .with_extra("author", serde_json::json!(channel));
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

fn fmt_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return "0:00".to_string();
    }
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{}:{:02}", m, s)
    }
}

#[async_trait]
impl Engine for MediathekviewwebEngine {
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
        matches!(result_type, ResultType::Videos | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert(
            "api_url".to_string(),
            "https://mediathekviewweb.de/api/query".to_string(),
        );
        settings.insert("language".to_string(), "de".to_string());
        settings
    }
}
