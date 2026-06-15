//! TinEye reverse image search engine implementation
//!
//! TinEye performs reverse
//! image search by URL. The reference engine reads the image URL from the
//! `search_urls` of an `online_url_search` processor; since this port models
//! searches by text query, the query is treated as the image URL when it
//! starts with `http(s)://` or `data:`. Otherwise the engine returns empty
//! results.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// TinEye reverse image search engine
pub struct TinEyeEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://tineye.com";

#[derive(Debug, Serialize, Deserialize)]
struct TinEyeResponse {
    #[serde(default)]
    matches: Vec<TinEyeMatch>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TinEyeMatch {
    #[serde(default)]
    image_url: String,
    #[serde(default)]
    domain: String,
    #[serde(default)]
    width: serde_json::Value,
    #[serde(default)]
    height: serde_json::Value,
    #[serde(default)]
    format: String,
    #[serde(default)]
    filesize: serde_json::Value,
    #[serde(default)]
    backlinks: Vec<TinEyeBacklink>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TinEyeBacklink {
    #[serde(default)]
    url: String,
    #[serde(default)]
    backlink: String,
    #[serde(default)]
    image_name: String,
    #[serde(default)]
    crawl_date: String,
}

impl TinEyeEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "tineye".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "TinEye - reverse image search by URL.".to_string(),
            website: Some("https://tineye.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create TinEye HTTP client");

        TinEyeEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // Treat the query text as the image URL.
        let image_url = query.query.trim();
        if !(image_url.starts_with("http://")
            || image_url.starts_with("https://")
            || image_url.starts_with("data:"))
        {
            tracing::info!("tineye: query is not an image URL; returning empty");
            return Ok(vec![]);
        }

        let pageno = (query.offset / 20) + 1;
        let page = pageno.to_string();
        let endpoint = format!("{}/api/v1/result_json/", BASE_URL);

        let resp = self
            .client
            .get(&endpoint)
            .header("User-Agent", "digse/0.1.0")
            .header("Connection", "keep-alive")
            .query(&[("url", image_url), ("page", page.as_str())])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        // 400/422 are client errors (bad image / unsupported format).
        if resp.status().as_u16() == 400 || resp.status().as_u16() == 422 {
            return Ok(vec![]);
        }
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let parsed: TinEyeResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, m) in parsed.matches.iter().enumerate() {
            let backlink = match m.backlinks.first() {
                Some(b) => b,
                None => continue,
            };
            let url = backlink.backlink.clone();
            let img_src = backlink.url.clone();
            let title = if !backlink.image_name.is_empty() {
                backlink.image_name.clone()
            } else {
                m.domain.clone()
            };
            if url.is_empty() {
                continue;
            }

            let mut result = SearchResult::new(title, url)
                .with_snippet(format!("Found on {}", m.domain))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images);

            if !m.image_url.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(m.image_url));
            }
            if !img_src.is_empty() {
                result = result.with_extra("img_src", serde_json::json!(img_src));
                result = result.with_extra("source", serde_json::json!(img_src));
            }
            if !m.format.is_empty() {
                result = result.with_extra("format", serde_json::json!(m.format));
            }
            result = result.with_extra("width", serde_json::json!(m.width));
            result = result.with_extra("height", serde_json::json!(m.height));
            if !m.domain.is_empty() {
                result = result.with_extra("source_domain", serde_json::json!(m.domain));
            }

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for TinEyeEngine {
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
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("search_string".into(), "/api/v1/result_json/".into());
        s.insert("engine_type".into(), "online_url_search".into());
        s.insert("results".into(), "JSON".into());
        s
    }
}
