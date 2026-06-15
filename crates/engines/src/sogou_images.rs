//! Sogou Images search engine implementation
//!
//! The image
//! search results page embeds a JSON blob (`window.__INITIAL_STATE__ = {...};`)
//! containing the image list.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Sogou Images search engine (images, Chinese, embedded JSON)
pub struct SogouImagesEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl SogouImagesEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "sogou_images".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Sogou Images - Chinese image search.".to_string(),
            website: Some("https://pic.sogou.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Sogou Images HTTP client");
        SogouImagesEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://pic.sogou.com";
        let start = query.offset;
        let start_str = start.to_string();
        let resp = self
            .client
            .get(format!("{}/pics", base_url))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("query", query.query.as_str()),
                ("start", start_str.as_str()),
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
        let json_str = match extract_initial_state(&text) {
            Some(j) => j,
            None => return Ok(vec![]),
        };
        let state: SogouState = match serde_json::from_str(&json_str) {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let items = state
            .search_list
            .and_then(|s| s.search_list)
            .unwrap_or_default();
        let mut results = Vec::new();
        for item in items.iter() {
            let pic_url = item.pic_url.clone().unwrap_or_default();
            if pic_url.is_empty() {
                continue;
            }
            let url = item.url.clone().unwrap_or_default();
            results.push(
                SearchResult::new(item.title.clone().unwrap_or_default(), url)
                    .with_snippet(item.content_major.clone().unwrap_or_default())
                    .with_engine(self.name())
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(pic_url))
                    .with_extra("thumbnail", serde_json::json!(pic_url))
                    .with_extra(
                        "source",
                        serde_json::json!(item.ch_site_name.clone().unwrap_or_default()),
                    )
                    .with_extra("format", serde_json::json!("image")),
            );
        }
        Ok(results)
    }
}

/// Extract the JSON object assigned to `window.__INITIAL_STATE__ = {...};`.
fn extract_initial_state(text: &str) -> Option<String> {
    let marker = "window.__INITIAL_STATE__";
    let start_kw = text.find(marker)?;
    let after_eq = text[start_kw..].find('=')?;
    let mut idx = start_kw + after_eq + 1;
    let bytes = text.as_bytes();
    // skip whitespace
    while idx < bytes.len() && bytes[idx].is_ascii_whitespace() {
        idx += 1;
    }
    if idx >= bytes.len() || bytes[idx] != b'{' {
        return None;
    }
    // scan for the matching closing brace, accounting for nesting and strings
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let start = idx;
    while idx < bytes.len() {
        let c = bytes[idx];
        if in_string {
            if escape {
                escape = false;
            } else if c == b'\\' {
                escape = true;
            } else if c == b'"' {
                in_string = false;
            }
        } else if c == b'"' {
            in_string = true;
        } else if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(text[start..=idx].to_string());
            }
        }
        idx += 1;
    }
    None
}

#[derive(Debug, Serialize, Deserialize)]
struct SogouState {
    #[serde(default)]
    search_list: Option<SogouSearchList>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SogouSearchList {
    #[serde(default)]
    search_list: Option<Vec<SogouImage>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SogouImage {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    pic_url: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    content_major: Option<String>,
    #[serde(default)]
    ch_site_name: Option<String>,
}

#[async_trait]
impl Engine for SogouImagesEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Images | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://pic.sogou.com".into());
        s
    }
}
