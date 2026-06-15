//! Freesound search engine implementation
//!
//! Uses the Freesound API v2
//! `search/text` endpoint with a token from `FREESOUND_API_KEY`. Category:
//! music. Graceful empty when no key is configured.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Freesound sound search engine (API v2; requires token)
pub struct FreesoundEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FreesoundResponse {
    #[serde(default)]
    results: Vec<FreesoundSound>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FreesoundSound {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    created: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    download: String,
}

impl FreesoundEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("FREESOUND_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());
        let metadata = EngineMetadata {
            name: "freesound".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Freesound - collaborative sound database (API token required)."
                .to_string(),
            website: Some("https://freesound.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Freesound HTTP client");

        FreesoundEngine {
            metadata,
            client,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                tracing::info!("freesound: FREESOUND_API_KEY not set; returning empty");
                return Ok(vec![]);
            }
        };
        let page = query.offset + 1;
        let page_str = page.to_string();
        let url = "https://freesound.org/apiv2/search/text/";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("query", query.query.as_str()),
                ("page", page_str.as_str()),
                ("fields", "name,url,download,created,description,type"),
                ("token", api_key.as_str()),
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

        let parsed: FreesoundResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, sound) in parsed.results.iter().enumerate() {
            if i >= query.count {
                break;
            }
            // ref truncates the description to 128 chars
            let desc: String = sound.description.chars().take(128).collect();
            let title = if sound.name.is_empty() {
                "Freesound".to_string()
            } else {
                sound.name.clone()
            };
            let result = SearchResult::new(title, sound.url.clone())
                .with_snippet(desc)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Music)
                .with_extra("audio_src", serde_json::json!(sound.download))
                .with_extra("published", serde_json::json!(sound.created))
                .with_extra("source", serde_json::json!("freesound"));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for FreesoundEngine {
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
        settings.insert("base_url".to_string(), "https://freesound.org".to_string());
        settings.insert("search_endpoint".to_string(), "/apiv2/search/text".to_string());
        if self.api_key.is_some() {
            settings.insert("requires_api_key".to_string(), "true".to_string());
        }
        settings
    }
}
