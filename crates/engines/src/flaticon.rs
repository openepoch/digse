//! Flaticon search engine implementation
//!
//! AJAX JSON endpoint at
//! `https://www.flaticon.com/ajax/search/{page}?word=...`. Categories in ref:
//! images, icons. Each item yields name, slug, png, png512 and tags.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Flaticon icon/image search engine (AJAX JSON API)
pub struct FlaticonEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct FlaticonResponse {
    #[serde(default)]
    items: Vec<FlaticonItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FlaticonItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    png: String,
    #[serde(default)]
    png512: String,
    #[serde(default)]
    team_name: String,
    #[serde(default)]
    tags: Vec<FlaticonTag>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct FlaticonTag {
    #[serde(default)]
    tag: String,
}

impl FlaticonEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "flaticon".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Flaticon - icon database search.".to_string(),
            website: Some("https://www.flaticon.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Flaticon HTTP client");

        FlaticonEngine { metadata, client }
    }

    // ref: url.replace(r"\/", "/")
    fn fix_url(url: &str) -> String {
        url.replace("\\/", "/")
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.flaticon.com";
        let page = query.offset + 1;
        let url = format!("{}/ajax/search/{}", base_url, page);
        let referer = format!("{}/search?word={}", base_url, query.query);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", referer)
            .query(&[("word", query.query.as_str())])
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

        let parsed: FlaticonResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.items.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let tags: Vec<String> = item
                .tags
                .iter()
                .filter(|t| !t.tag.is_empty())
                .map(|t| t.tag.clone())
                .collect();
            let title = if item.name.is_empty() {
                "Flaticon icon".to_string()
            } else {
                item.name.clone()
            };
            let result = SearchResult::new(title, Self::fix_url(&item.slug))
                .with_snippet(tags.join(", "))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(Self::fix_url(&item.png512)))
                .with_extra("thumbnail", serde_json::json!(Self::fix_url(&item.png)))
                .with_extra("format", serde_json::json!("PNG"))
                .with_extra("source", serde_json::json!("flaticon"))
                .with_extra("author", serde_json::json!(item.team_name));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for FlaticonEngine {
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
        settings.insert("base_url".to_string(), "https://www.flaticon.com".to_string());
        settings.insert(
            "search_endpoint".to_string(),
            "/ajax/search/{page}".to_string(),
        );
        settings
    }
}
