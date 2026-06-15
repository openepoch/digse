//! Pixabay search engine implementation
//!
//! Royalty-free images/videos via the
//! public `/<type>/search/<q>/` JSON endpoint (no API key). The response embeds
//! a `sources` object whose members are ordered by ascending quality; we sort
//! by the numeric size prefix of each key to recover that ordering reliably.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Pixabay royalty-free media search engine
pub struct PixabayEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl PixabayEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "pixabay".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Pixabay - royalty-free images and videos.".to_string(),
            website: Some("https://pixabay.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to create Pixabay HTTP client");

        PixabayEngine { metadata, client }
    }

    pub fn new_general() -> Self {
        Self::new()
    }

    /// Numeric size embedded in a sources key like "180px", used to order
    /// sources by ascending quality.
    fn size_rank(key: &str) -> i64 {
        let mut n = 0i64;
        let mut got = false;
        for ch in key.chars() {
            if ch.is_ascii_digit() {
                n = n * 10 + (ch as i64 - '0' as i64);
                got = true;
            } else if got {
                break;
            }
        }
        if got { n } else { i64::MAX }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://pixabay.com";
        let media = "images"; // ref also supports "videos"
        let pageno = (query.offset / query.count.max(1)) + 1;
        let page_str = pageno.to_string();
        let encoded = urlencoding::encode(&query.query);
        let url = format!(
            "{}/{}/search/{}/?pagi={}",
            base_url, media, encoded, page_str
        );

        // 302 = no results on this page (redirect to first page) -> empty.
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.1.0) Pixabay")
            .header("Accept", "application/json")
            .header("x-bootstrap-cache-miss", "1")
            .header("x-fetch-bootstrap", "1")
            .header("Cookie", "g_rated=1")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let status = response.status().as_u16();
        if status == 302 || !response.status().is_success() {
            return Ok(vec![]);
        }

        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };
        let json: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        let results_arr = json
            .get("page")
            .and_then(|p| p.get("results"))
            .and_then(|r| r.as_array());

        let mut results = Vec::new();
        if let Some(arr) = results_arr {
            for (i, item) in arr.iter().enumerate() {
                if results.len() >= query.count {
                    break;
                }
                let media_type = item.get("mediaType").and_then(|v| v.as_str()).unwrap_or("");
                let href = item.get("href").and_then(|v| v.as_str()).unwrap_or("");
                if href.is_empty() {
                    continue;
                }
                let page_url = format!("{}{}", base_url, href);
                let title = item
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("pixabay")
                    .to_string();
                let content = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let mut result = SearchResult::new(title, page_url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05));

                let sources = item.get("sources").and_then(|s| s.as_object());
                match media_type {
                    "photo" | "illustration" | "vector" => {
                        result = result.with_result_type(ResultType::Images);
                        if let Some(src) = sources {
                            let mut pairs: Vec<(i64, &str)> = src
                                .iter()
                                .filter_map(|(k, v)| v.as_str().map(|s| (Self::size_rank(k), s)))
                                .collect();
                            pairs.sort_by_key(|(n, _)| *n);
                            if let Some((_, thumb)) = pairs.first() {
                                result = result.with_extra("thumbnail", serde_json::json!(thumb));
                            }
                            if let Some((_, full)) = pairs.last() {
                                result = result.with_extra("img_src", serde_json::json!(full));
                            }
                        }
                    }
                    "video" => {
                        result = result.with_result_type(ResultType::Videos);
                        if let Some(src) = sources {
                            if let Some(t) = src.get("thumbnail").and_then(|v| v.as_str()) {
                                result = result.with_extra("thumbnail", serde_json::json!(t));
                            }
                            if let Some(e) = src.get("embed").and_then(|v| v.as_str()) {
                                result = result.with_extra("iframe_src", serde_json::json!(e));
                            }
                        }
                        if let Some(d) = item.get("duration").and_then(|v| v.as_i64()) {
                            result = result.with_extra("duration", serde_json::json!(d));
                        }
                        if let Some(u) = item.get("uploadDate").and_then(|v| v.as_str()) {
                            result = result.with_extra("published", serde_json::json!(u));
                        }
                    }
                    _ => continue,
                }
                result = result.with_extra("source", serde_json::json!("pixabay"));
                results.push(result);
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for PixabayEngine {
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
        matches!(t, ResultType::Images | ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://pixabay.com".to_string());
        s.insert("pixabay_type".to_string(), "images".to_string());
        s
    }
}
