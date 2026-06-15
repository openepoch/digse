//! ChinaSo search engine implementation.
//! Chinese-language news/images/videos, JSON API.
//! Default category is news (the ref module's default chinaso_category='news').

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// ChinaSo (Chinese search) engine - default news search.
pub struct ChinasoEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://www.chinaso.com";

#[derive(Debug, Serialize, Deserialize)]
struct ChinasoNewsResponse {
    #[serde(default)]
    data: Option<ChinasoNewsData>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ChinasoNewsData {
    #[serde(default)]
    data: Vec<ChinasoNewsEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ChinasoNewsEntry {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    timestamp: serde_json::Value,
}

impl ChinasoEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "chinaso".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "ChinaSo - Chinese-language news search.".to_string(),
            website: Some("https://www.chinaso.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create ChinaSo HTTP client");
        ChinasoEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!("{}/v5/general/v1/web/search", BASE_URL);
        let pn = (query.offset + 1).to_string();
        let ps = "10".to_string();

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("pn", pn.as_str()),
                ("ps", ps.as_str()),
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
        let parsed: ChinasoNewsResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let entries = parsed.data.map(|d| d.data).unwrap_or_default();
        let mut results = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            if entry.url.is_empty() {
                continue;
            }
            let published = match &entry.timestamp {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => String::new(),
            };
            results.push(
                SearchResult::new(strip_html(&entry.title), entry.url.clone())
                    .with_snippet(strip_html(&entry.snippet))
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::News)
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("source", serde_json::json!("chinaso")),
            );
        }
        Ok(results)
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[async_trait]
impl Engine for ChinasoEngine {
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
        matches!(t, ResultType::News | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://www.chinaso.com".into());
        s.insert("category".into(), "news".into());
        s
    }
}
