//! Reuters search engine implementation
//!
//! Reuters exposes an
//! internal JSON endpoint (`/pf/api/v3/content/fetch/articles-by-search-v2`)
//! that takes a URL-encoded JSON query argument.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Reuters news search engine (JSON API)
pub struct ReutersEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl ReutersEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "reuters".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Reuters - international news agency.".to_string(),
            website: Some("https://www.reuters.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Reuters HTTP client");
        ReutersEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.reuters.com";
        let results_per_page = 20;
        let offset = query.offset;
        // build the inner JSON query argument
        let inner = serde_json::json!({
            "keyword": query.query.as_str(),
            "offset": offset,
            "orderby": "relevance",
            "size": results_per_page,
            "website": "reuters",
        });
        let inner_str = serde_json::to_string(&inner).unwrap_or_default();
        let encoded = urlencoding::encode(&inner_str);
        let url = format!(
            "{}/pf/api/v3/content/fetch/articles-by-search-v2?query={}",
            base_url, encoded
        );
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
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
        let root: ReutersResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(_) => return Ok(vec![]),
        };
        let articles = root.result.and_then(|r| r.articles).unwrap_or_default();
        let mut results = Vec::new();
        for article in articles.iter() {
            let canonical = article.canonical_url.clone().unwrap_or_default();
            if canonical.is_empty() {
                continue;
            }
            let url = format!("{}{}", base_url, canonical);
            let title = article.web.clone().unwrap_or_default();
            let content = article.description.clone().unwrap_or_default();
            let published = article.display_time.clone().unwrap_or_default();
            let source_name = article
                .kicker
                .as_ref()
                .and_then(|k| k.name.clone())
                .unwrap_or_else(|| "Reuters".to_string());
            let thumbnail = article
                .thumbnail
                .as_ref()
                .and_then(|t| t.resizer_url.clone())
                .map(|u| {
                    if u.contains("&height=") {
                        u
                    } else {
                        format!("{}&height=80", u)
                    }
                })
                .unwrap_or_default();
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_result_type(ResultType::News)
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("source", serde_json::json!(source_name))
                    .with_extra("img_src", serde_json::json!(thumbnail)),
            );
        }
        Ok(results)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ReutersResponse {
    #[serde(default)]
    result: Option<ReutersResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ReutersResult {
    #[serde(default)]
    articles: Option<Vec<ReutersArticle>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ReutersArticle {
    #[serde(default)]
    canonical_url: Option<String>,
    #[serde(default)]
    web: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    display_time: Option<String>,
    #[serde(default)]
    kicker: Option<ReutersKicker>,
    #[serde(default)]
    thumbnail: Option<ReutersThumbnail>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ReutersKicker {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ReutersThumbnail {
    #[serde(default)]
    resizer_url: Option<String>,
}

#[async_trait]
impl Engine for ReutersEngine {
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
        matches!(t, ResultType::News | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://www.reuters.com".into());
        s.insert("sort_order".into(), "relevance".into());
        s.insert("results_per_page".into(), "20".into());
        s
    }
}
