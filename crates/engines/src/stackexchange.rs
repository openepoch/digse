//! Stack Exchange search engine implementation
//!
//! Queries the
//! Stack Exchange advanced-search API v2.3. Mirrors the existing
//! `stackoverflow` engine but uses the generic `q` parameter and a
//! configurable `api_site`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Stack Exchange Q&A search engine
pub struct StackExchangeEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const SEARCH_API: &str = "https://api.stackexchange.com/2.3/search/advanced";
const PAGE_SIZE: usize = 10;

#[derive(Debug, Serialize, Deserialize)]
struct StackExchangeResponse {
    #[serde(default)]
    items: Vec<StackExchangeItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StackExchangeItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    question_id: i64,
    #[serde(default)]
    score: i64,
    #[serde(default)]
    #[serde(rename = "answer_count")]
    answer_count: i64,
    #[serde(default)]
    is_answered: bool,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    owner: StackExchangeOwner,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StackExchangeOwner {
    #[serde(default)]
    display_name: String,
}

impl StackExchangeEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "stackexchange".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Stack Exchange - Q&A search across the Stack Exchange network.".to_string(),
            website: Some("https://stackexchange.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Stack Exchange HTTP client");

        StackExchangeEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = (query.offset / PAGE_SIZE) + 1;
        let page = pageno.to_string();
        let pagesize = PAGE_SIZE.to_string();

        let resp = self
            .client
            .get(SEARCH_API)
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page.as_str()),
                ("pagesize", pagesize.as_str()),
                ("site", "stackoverflow"),
                ("sort", "activity"),
                ("order", "desc"),
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

        let parsed: StackExchangeResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.items.iter().enumerate() {
            // Decode basic HTML entities that the API returns in titles.
            let title = decode_html_entities(&item.title);
            let url = format!("https://stackoverflow.com/q/{}", item.question_id);

            let mut content = format!("[{}]", item.tags.join(", "));
            content.push(' ');
            content.push_str(&item.owner.display_name);
            if item.is_answered {
                content.push_str(" // is answered");
            }
            content.push_str(&format!(" // score: {}", item.score));
            let content = decode_html_entities(&content);

            let mut result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score((item.score as f64).max(1.0))
                .with_result_type(ResultType::IT);

            result = result.with_extra("score", serde_json::json!(item.score));
            result = result.with_extra("answer_count", serde_json::json!(item.answer_count));
            result = result.with_extra("is_answered", serde_json::json!(item.is_answered));
            result = result.with_extra("question_id", serde_json::json!(item.question_id));
            result = result.with_extra("tags", serde_json::json!(item.tags.join(", ")));
            result = result.with_extra("author", serde_json::json!(item.owner.display_name));

            results.push(result);
        }

        Ok(results)
    }
}

/// Decode a minimal subset of HTML entities (mirrors Python's `html.unescape`
/// for the entities the Stack Exchange API commonly emits).
fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
}

#[async_trait]
impl Engine for StackExchangeEngine {
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
        matches!(t, ResultType::IT | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://api.stackexchange.com".into());
        s.insert("api_endpoint".into(), SEARCH_API.into());
        s.insert("api_site".into(), "stackoverflow".into());
        s.insert("api_sort".into(), "activity".into());
        s.insert("api_order".into(), "desc".into());
        s.insert("pagesize".into(), PAGE_SIZE.to_string());
        s
    }
}
