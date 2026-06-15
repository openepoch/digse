//! Swisscows News search engine implementation
//!
//! Queries the
//! Swisscows `/news/search` JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
    TimeRange,
};

/// Swisscows News search engine
pub struct SwisscowsNewsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://api.swisscows.com";
const RESULTS_PER_PAGE: usize = 20;

#[derive(Debug, Serialize, Deserialize)]
struct SwisscowsNewsResponse {
    #[serde(default)]
    items: Vec<SwisscowsNewsItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SwisscowsNewsItem {
    #[serde(default)]
    uri: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    created: String,
    #[serde(default)]
    #[serde(rename = "og:image")]
    og_image: String,
}

impl SwisscowsNewsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "swisscows_news".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Swisscows News - news search.".to_string(),
            website: Some("https://swisscows.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Swisscows News HTTP client");

        SwisscowsNewsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let locale = "de-DE".to_string();
        let language = "de".to_string();
        let freshness = match query.time_range {
            Some(TimeRange::Day) => "Day",
            Some(TimeRange::Week) => "Week",
            Some(TimeRange::Month) => "Month",
            Some(TimeRange::Year) => "Year",
            None => "All",
        }
        .to_string();
        let items_count = RESULTS_PER_PAGE.to_string();
        let offset = query.offset.to_string();

        let resp = self
            .client
            .get(format!("{}/news/search", BASE_URL))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("query", query.query.as_str()),
                ("itemsCount", items_count.as_str()),
                ("region", locale.as_str()),
                ("language", language.as_str()),
                ("offset", offset.as_str()),
                ("freshness", freshness.as_str()),
                ("sortOrder", "Desc"),
                ("sortBy", "Created"),
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

        let parsed: SwisscowsNewsResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.items.iter().enumerate() {
            if item.uri.is_empty() {
                continue;
            }
            let mut result = SearchResult::new(item.title.clone(), item.uri.clone())
                .with_snippet(item.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::News);

            if !item.created.is_empty() {
                result = result.with_extra("published", serde_json::json!(item.created));
            }
            if !item.og_image.is_empty() {
                result = result.with_extra("img_src", serde_json::json!(item.og_image));
                result = result.with_extra("thumbnail", serde_json::json!(item.og_image));
            }
            result = result.with_extra("source", serde_json::json!("swisscows"));

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for SwisscowsNewsEngine {
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
        matches!(t, ResultType::News | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("results_per_page".into(), RESULTS_PER_PAGE.to_string());
        s.insert("results".into(), "JSON".into());
        s
    }
}
