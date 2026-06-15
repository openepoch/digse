//! Mixcloud search engine implementation
//!
//! queries the Mixcloud API for
//! cloudcasts (DJ mixes/shows). Category: music.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Mixcloud (music) search engine
pub struct MixcloudEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Deserialize)]
struct MixcloudResponse {
    #[serde(default)]
    data: Vec<MixcloudItem>,
}

#[derive(Debug, Deserialize, Default)]
struct MixcloudItem {
    #[serde(default)]
    url: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    created_time: String,
    #[serde(default)]
    user: MixcloudUser,
    #[serde(default)]
    pictures: MixcloudPictures,
}

#[derive(Debug, Deserialize, Default)]
struct MixcloudUser {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct MixcloudPictures {
    #[serde(default)]
    medium: String,
}

impl MixcloudEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "mixcloud".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Mixcloud - DJ mixes and radio shows.".to_string(),
            website: Some("https://www.mixcloud.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Mixcloud HTTP client");

        MixcloudEngine {
            metadata,
            client,
            base_url: "https://api.mixcloud.com".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let offset = query.offset.to_string();
        let limit = "10".to_string();
        let url = format!("{}/search/", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("type", "cloudcast"),
                ("limit", limit.as_str()),
                ("offset", offset.as_str()),
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
        let parsed: MixcloudResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.data.iter().enumerate() {
            if item.url.is_empty() {
                continue;
            }
            let iframe = format!(
                "https://www.mixcloud.com/widget/iframe/?feed={}",
                item.url
            );
            let published = if item.created_time.len() >= 10 {
                item.created_time[..10].to_string()
            } else {
                item.created_time.clone()
            };
            let result = SearchResult::new(item.name.clone(), item.url.clone())
                .with_snippet(item.user.name.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Music)
                .with_extra("iframe_src", serde_json::json!(iframe))
                .with_extra("artist", serde_json::json!(item.user.name))
                .with_extra("thumbnail", serde_json::json!(item.pictures.medium))
                .with_extra("published", serde_json::json!(published));
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for MixcloudEngine {
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
        matches!(result_type, ResultType::Music | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("search_url".to_string(), "/search/".to_string());
        settings
    }
}
