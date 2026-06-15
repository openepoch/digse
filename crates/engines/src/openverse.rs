//! Openverse search engine implementation (images; JSON)
//!
//! Openverse (formerly Creative
//! Commons search) hosts CC-licensed media. Queries the Openverse API at
//! `https://api.openverse.org/v1/images/`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Openverse image search engine
pub struct OpenverseEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const PAGE_SIZE: i64 = 20;

#[derive(Debug, Serialize, Deserialize)]
struct OpenverseResponse {
    #[serde(default)]
    results: Vec<OpenverseImage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenverseImage {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    foreign_landing_url: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    license_version: String,
    #[serde(default)]
    creator: String,
    #[serde(default)]
    creator_url: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    thumbnail: Option<String>,
}

impl OpenverseEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "openverse".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Openverse - CC-licensed media search.".to_string(),
            website: Some("https://openverse.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Openverse HTTP client");

        OpenverseEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.openverse.org/v1/images/";
        let page = ((query.offset / PAGE_SIZE as usize) + 1).to_string();
        let page_size = PAGE_SIZE.to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page.as_str()),
                ("page_size", page_size.as_str()),
                ("format", "json"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: OpenverseResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, img) in parsed.results.iter().enumerate() {
            let page_url = if img.foreign_landing_url.is_empty() {
                img.url.clone()
            } else {
                img.foreign_landing_url.clone()
            };
            if page_url.is_empty() && img.url.is_empty() {
                continue;
            }
            let title = if img.title.is_empty() {
                format!("Image by {}", if img.creator.is_empty() { "Unknown" } else { &img.creator })
            } else {
                img.title.clone()
            };

            let license_str = if !img.license.is_empty() {
                format!("{} {}", img.license, img.license_version).trim().to_string()
            } else {
                String::new()
            };

            let thumbnail = img.thumbnail.clone().unwrap_or_else(|| img.url.clone());

            results.push(
                SearchResult::new(&title, &page_url)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(img.url))
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("source", serde_json::json!(img.source))
                    .with_extra("author", serde_json::json!(img.creator))
                    .with_extra("license", serde_json::json!(license_str)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for OpenverseEngine {
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
        matches!(t, ResultType::Images | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://api.openverse.org/v1/images/".to_string());
        s.insert("page_size".to_string(), PAGE_SIZE.to_string());
        s
    }
}
