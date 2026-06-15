//! Generic JSON engine implementation
//!
//! This is a *generic*,
//! settings-driven engine: the search URL, JSON result path, and field queries
//! are all configured at runtime via `settings.yml`. Without runtime config the
//! engine cannot know where to fetch from, so it returns an empty result and
//! logs an informational message.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result, SearchQuery, SearchResult, ResultType,
};

/// Generic, config-driven JSON engine
pub struct JsonEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    search_url: Option<String>,
    results_query: String,
    url_query: Option<String>,
    url_prefix: String,
    title_query: Option<String>,
    content_query: Option<String>,
    thumbnail_query: Option<String>,
    thumbnail_prefix: String,
}

impl JsonEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "json_engine".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Generic JSON engine - config-driven JSON search.".to_string(),
            website: None,
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create json_engine HTTP client");
        JsonEngine {
            metadata,
            client,
            search_url: None,
            results_query: String::new(),
            url_query: None,
            url_prefix: String::new(),
            title_query: None,
            content_query: None,
            thumbnail_query: None,
            thumbnail_prefix: String::new(),
        }
    }

    pub fn with_config(
        search_url: String,
        results_query: String,
        url_query: String,
        title_query: String,
    ) -> Self {
        let mut engine = Self::new();
        engine.search_url = Some(search_url);
        engine.results_query = results_query;
        engine.url_query = Some(url_query);
        engine.title_query = Some(title_query);
        engine
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let search_url = match &self.search_url {
            Some(u) => u.clone(),
            None => {
                tracing::info!("json_engine: requires runtime config; returning empty");
                return Ok(vec![]);
            }
        };
        let url_query = match &self.url_query {
            Some(q) if !q.is_empty() => q.clone(),
            _ => {
                tracing::info!("json_engine: missing url_query config; returning empty");
                return Ok(vec![]);
            }
        };
        let title_query = match &self.title_query {
            Some(q) if !q.is_empty() => q.clone(),
            _ => {
                tracing::info!("json_engine: missing title_query config; returning empty");
                return Ok(vec![]);
            }
        };

        // Substitute {query} and {pageno} placeholders
        let pageno = (query.offset / query.count.max(1)) + 1;
        let filled = search_url
            .replace("{query}", &urlencoding::encode(&query.query))
            .replace("{pageno}", &pageno.to_string());

        let response = match self
            .client
            .get(&filled)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::info!("json_engine: request failed: {}; returning empty", e);
                return Ok(vec![]);
            }
        };
        if !response.status().is_success() {
            return Ok(vec![]);
        }
        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };
        let root: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        // Pick the results array
        let items = if self.results_query.is_empty() {
            match &root {
                serde_json::Value::Array(a) => a.clone(),
                _ => return Ok(vec![]),
            }
        } else {
            match query_json(&root, &self.results_query) {
                Some(serde_json::Value::Array(a)) => a,
                _ => return Ok(vec![]),
            }
        };

        let mut results = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let url_val = match query_json(item, &url_query) {
                Some(v) => to_string_value(&v),
                None => continue,
            };
            let title_val = match query_json(item, &title_query) {
                Some(v) => to_string_value(&v),
                None => continue,
            };
            if url_val.is_empty() || title_val.is_empty() {
                continue;
            }
            let url = format!("{}{}", self.url_prefix, url_val);

            let content = self
                .content_query
                .as_ref()
                .and_then(|q| query_json(item, q))
                .map(|v| to_string_value(&v))
                .unwrap_or_default();

            let mut result = SearchResult::new(title_val, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            if let Some(thumb_q) = &self.thumbnail_query {
                if !thumb_q.is_empty() {
                    if let Some(thumb) = query_json(item, thumb_q) {
                        let t = format!("{}{}", self.thumbnail_prefix, to_string_value(&thumb));
                        result = result.with_extra("thumbnail", serde_json::json!(t));
                    }
                }
            }
            results.push(result);
        }
        Ok(results)
    }
}

/// Convert a JSON value to a string.
fn to_string_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// Slash-separated JSON path query.
fn query_json(data: &serde_json::Value, q: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = q.split('/').filter(|p| !p.is_empty()).collect();
    query_inner(data, &parts)
}

fn query_inner(data: &serde_json::Value, parts: &[&str]) -> Option<serde_json::Value> {
    if parts.is_empty() {
        return None;
    }
    let key = parts[0];
    match data {
        serde_json::Value::Object(map) => {
            if let Some(v) = map.get(key) {
                if parts.len() == 1 {
                    return Some(v.clone());
                }
                return query_inner(v, &parts[1..]);
            }
            // search nested objects
            for v in map.values() {
                if let Some(found) = query_inner(v, parts) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            // by index
            if let Ok(idx) = key.parse::<usize>() {
                if let Some(v) = arr.get(idx) {
                    if parts.len() == 1 {
                        return Some(v.clone());
                    }
                    return query_inner(v, &parts[1..]);
                }
            }
            // otherwise iterate
            for v in arr {
                if let Some(found) = query_inner(v, parts) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

#[async_trait]
impl Engine for JsonEngine {
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
        if let Some(u) = &self.search_url {
            s.insert("search_url".into(), u.clone());
        }
        s.insert("results_query".into(), self.results_query.clone());
        if let Some(q) = &self.url_query {
            s.insert("url_query".into(), q.clone());
        }
        if let Some(q) = &self.title_query {
            s.insert("title_query".into(), q.clone());
        }
        s
    }
}
