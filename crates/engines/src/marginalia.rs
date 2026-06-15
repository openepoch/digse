//! Marginalia Search engine implementation
//!
//! an independent open-source search
//! engine operating out of Sweden. Requires an API key (MARGINALIA_API_KEY).
//! Category: general.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Marginalia Search engine
pub struct MarginaliaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MarginaliaResponse {
    #[serde(default)]
    results: Vec<MarginaliaResult>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MarginaliaResult {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    quality: serde_json::Value,
    #[serde(default)]
    format: serde_json::Value,
    #[serde(default)]
    details: serde_json::Value,
}

impl MarginaliaEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("MARGINALIA_API_KEY").ok();
        let metadata = EngineMetadata {
            name: "marginalia".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: api_key.is_some(),
            timeout_seconds: 15,
            description: "Marginalia Search - independent open-source search engine.".to_string(),
            website: Some("https://marginalia.nu".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Marginalia HTTP client");

        MarginaliaEngine {
            metadata,
            client,
            base_url: "https://api2.marginalia-search.com".to_string(),
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        if self.api_key.is_none() {
            tracing::warn!("marginalia requires MARGINALIA_API_KEY");
            return Ok(vec![]);
        }
        let api_key = self.api_key.as_ref().unwrap();
        let page = (query.offset / 20).max(0) + 1;
        let page_str = page.to_string();
        let count_str = "20";
        let nsfw = if query.safe_search { "1" } else { "0" };

        let response = self
            .client
            .get(format!("{}/search", self.base_url).as_str())
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("API-Key", api_key)
            .query(&[
                ("page", page_str.as_str()),
                ("count", count_str),
                ("nsfw", nsfw),
                ("query", query.query.as_str()),
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
        let parsed: MarginaliaResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.results.iter().enumerate() {
            if item.url.is_empty() {
                continue;
            }
            let result = SearchResult::new(item.title.clone(), item.url.clone())
                .with_snippet(item.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for MarginaliaEngine {
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
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("api_endpoint".to_string(), "/search".to_string());
        settings.insert("results_per_page".to_string(), "20".to_string());
        settings
    }
}
