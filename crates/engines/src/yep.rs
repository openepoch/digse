//! Yep search engine implementation
//!
//! JSON endpoint at `api.yep.com/search`.
//! The response body is a 2-element array; the result list lives at index
//! `[1]["results"]`, each entry carrying `url` / `title` / `snippet`.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Yep general search engine (Brave-backed)
pub struct YepEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl YepEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "yep".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Yep (api.yep.com) general search.".to_string(),
            website: Some("https://yep.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Yep HTTP client");

        YepEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let limit = query.count.clamp(1, 20);
        // safesearch_map: {0:"off", 1:"moderate", 2:"strict"}
        let safe = if query.safe_search { "strict" } else { "off" };
        let limit_str = limit.to_string();

        let response = self
            .client
            .get("https://api.yep.com/search")
            .query(&[
                ("query", query.query.as_str()),
                ("safeSearch", safe),
                ("limit", limit_str.as_str()),
            ])
            .header("Referer", "https://yep.com/")
            .header("Origin", "https://yep.com")
            .header("Accept", "application/json")
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let body: Value = match response.json().await {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        // resp.json()[1]["results"]
        let results_arr = body
            .as_array()
            .and_then(|a| a.get(1))
            .and_then(|v| v.get("results"))
            .and_then(|v| v.as_array());

        let mut results = Vec::new();
        if let Some(arr) = results_arr {
            for (i, item) in arr.iter().enumerate() {
                if results.len() >= query.count {
                    break;
                }
                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("");
                if url.is_empty() || title.is_empty() {
                    continue;
                }
                let snippet = item
                    .get("snippet")
                    .and_then(|v| v.as_str())
                    .map(html_to_text)
                    .unwrap_or_default();

                let result = SearchResult::new(title.to_string(), url.to_string())
                    .with_snippet(snippet)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web)
                    .with_extra("source", serde_json::json!("yep"));
                results.push(result);
            }
        }
        Ok(results)
    }
}

/// Minimal HTML-to-text: strip tags, decode common entities, collapse whitespace.
fn html_to_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[async_trait]
impl Engine for YepEngine {
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
        s.insert("base_url".to_string(), "https://api.yep.com".to_string());
        s.insert("web_base_url".to_string(), "https://yep.com".to_string());
        s.insert("results_per_page".to_string(), "20".to_string());
        s
    }
}
