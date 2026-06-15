//! Library of Congress (loc) search engine implementation
//!
//! queries the photos/print/drawing endpoint
//! of the loc.gov JSON API. Category: images.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Library of Congress photos search engine
pub struct LocEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LocResponse {
    #[serde(default)]
    results: Vec<LocResult>,
    #[serde(default)]
    status: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LocResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    image_url: Vec<String>,
    #[serde(default)]
    item: LocItem,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LocItem {
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    created_published_date: Option<String>,
    #[serde(default)]
    summary: Vec<String>,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    part_of: Vec<String>,
    #[serde(default)]
    creators: Vec<LocCreator>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LocCreator {
    #[serde(default)]
    title: String,
}

impl LocEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "loc".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Library of Congress - photos, prints and drawings.".to_string(),
            website: Some("https://www.loc.gov/pictures/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create loc HTTP client");

        LocEngine {
            metadata,
            client,
            base_url: "https://www.loc.gov".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let page = (query.offset / 10).max(0) + 1;
        let page_str = page.to_string();
        let url = format!("{}/photos/", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("sp", page_str.as_str()),
                ("fo", "json"),
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
        let parsed: LocResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.results.iter().enumerate() {
            let url = match &item.item.link {
                Some(u) if !u.is_empty() => u.clone(),
                _ => continue,
            };
            if item.image_url.is_empty() {
                continue;
            }
            let mut title = item.title.clone();
            if title.starts_with('[') && title.ends_with(']') && title.len() >= 2 {
                title = title[1..title.len() - 1].to_string();
            }
            let img_src = item.image_url.last().cloned().unwrap_or_default();
            let thumbnail = item.image_url.first().cloned().unwrap_or_default();

            let mut content_items: Vec<String> = Vec::new();
            if let Some(d) = &item.item.created_published_date {
                content_items.push(d.clone());
            }
            if let Some(s) = item.item.summary.first() {
                content_items.push(s.clone());
            }
            if let Some(n) = item.item.notes.first() {
                content_items.push(n.clone());
            }
            if let Some(p) = item.item.part_of.first() {
                content_items.push(p.clone());
            }
            let author = item
                .item
                .creators
                .first()
                .map(|c| c.title.clone())
                .unwrap_or_default();

            let mut result = SearchResult::new(title.clone(), url.clone())
                .with_snippet(content_items.join(" / "))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("source", serde_json::json!("loc.gov"));
            if !author.is_empty() {
                result = result.with_extra("author", serde_json::json!(author));
            }
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for LocEngine {
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
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("endpoint".to_string(), "/photos/".to_string());
        settings
    }
}
