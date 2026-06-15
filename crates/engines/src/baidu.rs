//! Baidu search engine implementation (JSON, general web)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Baidu search engine (general web, JSON API)
pub struct BaiduEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BaiduResponse {
    #[serde(default)]
    feed: BaiduFeed,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BaiduFeed {
    #[serde(default)]
    entry: Vec<BaiduEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BaiduEntry {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    abs: String,
    #[serde(default)]
    time: serde_json::Value,
}

impl BaiduEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "baidu".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Baidu - Chinese-language web search.".to_string(),
            website: Some("https://www.baidu.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Baidu HTTP client");

        BaiduEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let endpoint = "https://www.baidu.com/s";
        let rn = query.count.min(10).to_string();
        let pn = query.offset.to_string();

        let resp = self.client
            .get(endpoint)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("wd", query.query.as_str()),
                ("rn", rn.as_str()),
                ("pn", pn.as_str()),
                ("tn", "json"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: BaiduResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, entry) in parsed.feed.entry.iter().enumerate() {
            if entry.title.is_empty() || entry.url.is_empty() {
                continue;
            }
            // unescape HTML entities such as &amp; &#39; &quot;
            let title = html_unescape(&entry.title);
            let content = html_unescape(&entry.abs);

            let mut r = SearchResult::new(title, entry.url.clone())
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);

            if let Some(ts) = entry.time.as_i64() {
                r = r.with_extra("published", serde_json::json!(ts));
            }
            results.push(r);
        }
        Ok(results)
    }
}

/// Minimal HTML entity unescape (covers the common cases Baidu emits).
fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

#[async_trait]
impl Engine for BaiduEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://www.baidu.com".to_string());
        s.insert("category".to_string(), "general".to_string());
        s
    }
}
