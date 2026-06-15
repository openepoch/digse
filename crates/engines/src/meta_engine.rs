//! Meta search engine implementation.
//!
//! A *meta* engine that forwards a query to another search instance (configured
//! via the `META_SEARCH_URL` env var) and parses its JSON `/search` response.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Meta search engine (queries another search instance over HTTP)
pub struct MetaSearchEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: Option<String>,
}

impl MetaSearchEngine {
    pub fn new() -> Self {
        let base_url = std::env::var("META_SEARCH_URL").ok().filter(|s| !s.is_empty());
        let metadata = EngineMetadata {
            name: "meta_engine".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Meta engine - proxies another search instance's JSON /search endpoint."
                .to_string(),
            website: None,
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create meta_engine HTTP client");
        MetaSearchEngine {
            metadata,
            client,
            base_url,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = match &self.base_url {
            Some(u) => u.clone(),
            None => return Ok(vec![]), // no instance configured -> graceful empty
        };
        let url = format!("{}/search", base_url.trim_end_matches('/'));
        let pageno = (query.offset / 10) + 1;
        let pageno_str = pageno.to_string();
        let params = [
            ("q", query.query.as_str()),
            ("pageno", pageno_str.as_str()),
            ("format", "json"),
        ];
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&params)
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
        let parsed: MetaSearchResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };
        let mut results = Vec::new();
        for item in parsed.results.iter() {
            let title = item.title.clone().unwrap_or_default();
            let url = item.url.clone().unwrap_or_default();
            if url.is_empty() && title.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(item.content.clone().unwrap_or_default())
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web),
            );
        }
        Ok(results)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct MetaSearchResponse {
    #[serde(default)]
    results: Vec<MetaSearchItem>,
    #[serde(default)]
    answers: Vec<serde_json::Value>,
    #[serde(default)]
    infoboxes: Vec<serde_json::Value>,
    #[serde(default)]
    suggestions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MetaSearchItem {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    content: Option<String>,
}

#[async_trait]
impl Engine for MetaSearchEngine {
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
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "base_url".into(),
            self.base_url.clone().unwrap_or_default(),
        );
        s.insert("format".into(), "json".into());
        s
    }
}
