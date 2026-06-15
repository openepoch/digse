//! Dailymotion (videos) search engine implementation
//!
//! Uses the official Dailymotion REST API to search videos.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Dailymotion video search engine
pub struct DailymotionEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct DailymotionResponse {
    #[serde(default)]
    list: Vec<DailymotionVideo>,
    #[serde(default)]
    page: i64,
    #[serde(default)]
    limit: i64,
    #[serde(default)]
    total: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DailymotionVideo {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    thumbnail_360_url: String,
    #[serde(default)]
    duration: i64,
    #[serde(default)]
    created_time: i64,
    #[serde(default)]
    allow_embed: bool,
}

impl DailymotionEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "dailymotion".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Dailymotion videos.".to_string(),
            website: Some("https://www.dailymotion.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Dailymotion HTTP client");

        DailymotionEngine { metadata, client }
    }

    /// Format duration (seconds) as "MM:SS" or "HH:MM:SS"
    fn format_duration(secs: i64) -> String {
        if secs <= 0 {
            return "0:00".to_string();
        }
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;
        if hours > 0 {
            format!("{}:{:02}:{:02}", hours, minutes, seconds)
        } else {
            format!("{}:{:02}", minutes, seconds)
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.dailymotion.com/videos";
        let page = ((query.offset / 10) + 1).to_string();
        let limit = query.count.to_string();

        let fields = "allow_embed,description,title,created_time,duration,url,thumbnail_360_url,id";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("search", query.query.as_str()),
                ("page", page.as_str()),
                ("limit", limit.as_str()),
                ("sort", "relevance"),
                ("fields", fields),
                ("family_filter", "false"),
                ("private", "false"),
                ("password_protected", "false"),
                ("thumbnail_ratio", "original"),
            ])
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

        let parsed: DailymotionResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, video) in parsed.list.iter().enumerate() {
            if video.url.is_empty() {
                continue;
            }
            let mut snippet = video.description.clone();
            if snippet.len() > 300 {
                snippet.truncate(300);
                snippet.push_str("...");
            }
            let duration = Self::format_duration(video.duration);
            let thumbnail = video.thumbnail_360_url.replacen("http://", "https://", 1);

            let mut result = SearchResult::new(video.title.clone(), video.url.clone())
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("duration", serde_json::json!(duration));

            if video.allow_embed && !video.id.is_empty() {
                result = result.with_extra(
                    "iframe_src",
                    serde_json::json!(format!(
                        "https://www.dailymotion.com/embed/video/{}",
                        video.id
                    )),
                );
            }
            if video.created_time > 0 {
                result = result
                    .with_extra("published", serde_json::json!(video.created_time));
            }

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for DailymotionEngine {
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
            "base_url".to_string(),
            "https://api.dailymotion.com".to_string(),
        );
        settings.insert("page_size".to_string(), "10".to_string());
        settings
    }
}
