//! Brave Search API engine implementation.
//! Paid Brave Search API (requires API key).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Brave Search API engine (paid).
pub struct BraveapiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BraveItem {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    age: Option<String>,
    #[serde(default)]
    thumbnail: Option<BraveThumb>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BraveThumb {
    #[serde(default)]
    src: String,
}

impl BraveapiEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("BRAVEAPI_API_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                std::env::var("BRAVE_API_KEY")
                    .ok()
                    .filter(|s| !s.is_empty())
            });
        let metadata = EngineMetadata {
            name: "braveapi".to_string(),
            category: EngineCategory::General,
            enabled: api_key.is_some(),
            requires_auth: true,
            timeout_seconds: 15,
            description: "Brave Search API - paid web search.".to_string(),
            website: Some("https://api.search.brave.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Brave API HTTP client");
        BraveapiEngine {
            metadata,
            client,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                tracing::warn!("braveapi requires BRAVEAPI_API_KEY or BRAVE_API_KEY");
                return Ok(vec![]);
            }
        };
        let url = "https://api.search.brave.com/res/v1/web/search";
        let count = query.count.to_string();
        let offset = (query.offset * query.count).to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("X-Subscription-Token", api_key)
            .query(&[
                ("q", query.query.as_str()),
                ("count", count.as_str()),
                ("offset", offset.as_str()),
            ])
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
        let parsed: BraveResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let items = parsed.web.map(|w| w.results).unwrap_or_default();
        for (i, item) in items.iter().enumerate() {
            if item.url.is_empty() {
                continue;
            }
            let thumbnail = item
                .thumbnail
                .as_ref()
                .map(|t| t.src.clone())
                .unwrap_or_default();
            results.push(
                SearchResult::new(item.title.clone(), item.url.clone())
                    .with_snippet(item.description.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web)
                    .with_extra("published", serde_json::json!(item.age))
                    .with_extra("thumbnail", serde_json::json!(thumbnail)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for BraveapiEngine {
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
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "base_url".into(),
            "https://api.search.brave.com/res/v1/web/search".into(),
        );
        s
    }
}
