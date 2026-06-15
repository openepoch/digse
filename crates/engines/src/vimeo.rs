//! Vimeo search engine implementation
//!
//! Scrapes the search page, which embeds
//! the results as a `var data = {...};` JSON blob. Each entry is keyed by its
//! own `type` field, so the result is parsed dynamically via serde_json::Value.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Vimeo video search engine
pub struct VimeoEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

/// Extract the substring between `start` and `end` after the first occurrence
/// of `start`.
fn extr<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = text.find(start)? + start.len();
    let rest = &text[i..];
    let j = rest.find(end)?;
    Some(&rest[..j])
}

impl VimeoEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "vimeo".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Vimeo video search".to_string(),
            website: Some("https://vimeo.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Vimeo HTTP client");

        VimeoEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://vimeo.com/";
        let pageno = (query.offset / query.count.max(1)) + 1;
        let page_str = pageno.to_string();
        let url = format!(
            "{}/search/page:{}",
            base_url.trim_end_matches('/'),
            page_str
        );

        let response = self
            .client
            .get(&url)
            .query(&[("q", query.query.as_str())])
            .header("User-Agent", "Mozilla/5.0 (digse/0.1.0)")
            .header("Accept", "text/html,application/xhtml+xml")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        let data_blob = match extr(&text, "var data = ", ";\n") {
            Some(s) => s,
            None => return Ok(vec![]),
        };
        let data: serde_json::Value = match serde_json::from_str(data_blob) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        let entries = data
            .get("filtered")
            .and_then(|f| f.get("data"))
            .and_then(|d| d.as_array());

        let mut results = Vec::new();
        if let Some(arr) = entries {
            for (i, item) in arr.iter().enumerate() {
                if results.len() >= query.count {
                    break;
                }
                // Each item is keyed by its own `type` field (e.g. "video").
                let item_type = match item.get("type").and_then(|v| v.as_str()) {
                    Some(t) => t,
                    None => continue,
                };
                let video = match item.get(item_type) {
                    Some(v) => v,
                    None => continue,
                };
                let uri = video.get("uri").and_then(|v| v.as_str()).unwrap_or("");
                let videoid = uri.rsplit('/').next().unwrap_or("");
                if videoid.is_empty() {
                    continue;
                }
                let page_url = format!("{}{}", base_url, videoid);
                let title = video
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("vimeo")
                    .to_string();
                let thumbnail = video
                    .get("pictures")
                    .and_then(|p| p.get("sizes"))
                    .and_then(|s| s.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|s| s.get("link"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let published = video
                    .get("created_time")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let result = SearchResult::new(title, page_url)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("iframe_src", serde_json::json!(format!("https://player.vimeo.com/video/{}", videoid)))
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("source", serde_json::json!("vimeo"));
                results.push(result);
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for VimeoEngine {
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
        matches!(t, ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://vimeo.com".to_string());
        settings.insert("search_url".to_string(), "/search/page:{pageno}".to_string());
        settings
    }
}
