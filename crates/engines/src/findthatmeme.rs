//! FindThatMeme search engine implementation
//!
//! POST JSON to
//! `https://findthatmeme.com/api/v1/search` with `{search, offset}`. Category:
//! images. Each item yields an image result with img_src, thumbnail, filesize,
//! source_page_url and source_site.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// FindThatMeme meme/image search engine (JSON POST API)
pub struct FindThatMemeEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct FindThatMemeItem {
    #[serde(default)]
    image_path: String,
    #[serde(default)]
    thumbnail: String,
    #[serde(default, rename = "type")]
    item_type: String,
    #[serde(default)]
    meme_file_size: i64,
    #[serde(default)]
    source_page_url: String,
    #[serde(default)]
    source_site: String,
    #[serde(default)]
    updated_at: String,
}

impl FindThatMemeEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "findthatmeme".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "FindThatMeme - meme image search.".to_string(),
            website: Some("https://findthatmeme.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create FindThatMeme HTTP client");

        FindThatMemeEngine { metadata, client }
    }

    fn humanize_bytes(bytes: i64) -> String {
        const KB: f64 = 1024.0;
        const MB: f64 = KB * 1024.0;
        const GB: f64 = MB * 1024.0;
        let b = bytes as f64;
        if b >= GB {
            format!("{:.2} GB", b / GB)
        } else if b >= MB {
            format!("{:.2} MB", b / MB)
        } else if b >= KB {
            format!("{:.2} KB", b / KB)
        } else {
            format!("{} B", bytes)
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://findthatmeme.com/api/v1/search";
        // offset = (pageno - 1) * 50; pageno is 1-based
        let start_index = query.offset * 50;
        let body = serde_json::json!({ "search": query.query, "offset": start_index });

        let response = self
            .client
            .post(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Content-Type", "application/json")
            .json(&body)
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

        let items: Vec<FindThatMemeItem> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let img = format!("https://s3.thehackerblog.com/findthatmeme/{}", item.image_path);
            let thumb = format!(
                "https://s3.thehackerblog.com/findthatmeme/thumb/{}",
                item.thumbnail
            );
            let img_src = if item.item_type == "IMAGE" {
                img.clone()
            } else {
                thumb.clone()
            };
            let published = item.updated_at.split('T').next().unwrap_or("").to_string();

            let result = SearchResult::new(
                if item.source_site.is_empty() {
                    "FindThatMeme".to_string()
                } else {
                    item.source_site.clone()
                },
                if item.source_page_url.is_empty() {
                    img.clone()
                } else {
                    item.source_page_url.clone()
                },
            )
            .with_snippet(format!("{} · {}", Self::humanize_bytes(item.meme_file_size), published))
            .with_engine(self.name())
            .with_rank(query.offset + i + 1)
            .with_score(1.0 - (i as f64 * 0.05))
            .with_result_type(ResultType::Images)
            .with_extra("img_src", serde_json::json!(img_src))
            .with_extra("thumbnail", serde_json::json!(thumb))
            .with_extra("source", serde_json::json!("findthatmeme"))
            .with_extra("filesize", serde_json::json!(Self::humanize_bytes(item.meme_file_size)))
            .with_extra("published", serde_json::json!(published));

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for FindThatMemeEngine {
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
        matches!(result_type, ResultType::Images | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert(
            "base_url".to_string(),
            "https://findthatmeme.com".to_string(),
        );
        settings.insert(
            "search_endpoint".to_string(),
            "/api/v1/search".to_string(),
        );
        settings
    }
}
