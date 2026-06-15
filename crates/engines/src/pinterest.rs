//! Pinterest search engine implementation (images; JSON)
//!
//! Queries Pinterest's internal
//! BaseSearchResource API. The request encodes a JSON payload in the `data`
//! query parameter.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Pinterest image search engine
pub struct PinterestEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct PinterestResponse {
    #[serde(default)]
    resource_response: PinterestResourceResponse,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PinterestResourceResponse {
    #[serde(default)]
    bookmark: String,
    #[serde(default)]
    data: PinterestData,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PinterestData {
    #[serde(default)]
    results: Vec<PinterestPin>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PinterestPin {
    #[serde(default)]
    id: String,
    #[serde(default, rename = "type")]
    pin_type: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    grid_title: String,
    #[serde(default)]
    rich_summary: Option<PinterestRichSummary>,
    #[serde(default)]
    images: PinterestImages,
    #[serde(default)]
    pinner: PinterestPinner,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PinterestRichSummary {
    #[serde(default)]
    display_description: String,
    #[serde(default)]
    site_name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PinterestImages {
    #[serde(default)]
    orig: PinterestImageVariant,
    #[serde(rename = "236x", default)]
    small: PinterestImageVariant,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PinterestImageVariant {
    #[serde(default)]
    url: String,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PinterestPinner {
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    username: String,
}

impl PinterestEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "pinterest".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Pinterest - Image discovery and sharing.".to_string(),
            website: Some("https://www.pinterest.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Pinterest HTTP client");

        PinterestEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.pinterest.com";
        let page = (query.offset / 10) + 1;

        // Build the JSON payload for the `data` query parameter, matching the
        // reference implementation's structure.
        let data_json = serde_json::json!({
            "options": {
                "query": query.query,
                "bookmarks": [String::new()],
                "page": page,
            },
            "context": {},
        })
        .to_string();

        let resp = self
            .client
            .get(format!("{}/resource/BaseSearchResource/get/", base_url))
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .header("X-Pinterest-AppState", "active")
            .header("X-Pinterest-Source-Url", "/ideas/")
            .header("X-Pinterest-PWS-Handler", "www/ideas.js")
            .query(&[("data", data_json.as_str())])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: PinterestResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, pin) in parsed.resource_response.data.results.iter().enumerate() {
            // skip story pins
            if pin.pin_type == "story" {
                continue;
            }
            let url = if !pin.link.is_empty() {
                pin.link.clone()
            } else if !pin.id.is_empty() {
                format!("{}/pin/{}/", base_url, pin.id)
            } else {
                continue;
            };

            let title = if !pin.title.is_empty() {
                pin.title.clone()
            } else if !pin.grid_title.is_empty() {
                pin.grid_title.clone()
            } else {
                format!("Pinterest result {}", i + 1)
            };

            let img_src = pin.images.orig.url.clone();
            if img_src.is_empty() {
                continue;
            }
            let thumbnail = pin.images.small.url.clone();

            let description = pin
                .rich_summary
                .as_ref()
                .map(|r| r.display_description.clone())
                .unwrap_or_default();
            let source = pin
                .rich_summary
                .as_ref()
                .map(|r| r.site_name.clone())
                .unwrap_or_default();
            let resolution = format!("{}x{}", pin.images.orig.width, pin.images.orig.height);
            let author = format!(
                "{} ({})",
                pin.pinner.full_name, pin.pinner.username
            );

            results.push(
                SearchResult::new(&title, &url)
                    .with_snippet(description)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(img_src))
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("source", serde_json::json!(source))
                    .with_extra("author", serde_json::json!(author))
                    .with_extra("resolution", serde_json::json!(resolution)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for PinterestEngine {
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
        s.insert("base_url".to_string(), "https://www.pinterest.com".to_string());
        s
    }
}
