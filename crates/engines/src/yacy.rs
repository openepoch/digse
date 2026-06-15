//! YaCy distributed P2P search engine implementation (JSON).

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// YaCy P2P search engine.
pub struct YacyEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_urls: Vec<String>,
}

const DEFAULT_INSTANCES: &[&str] = &[
    "https://yacy.searchlab.eu",
    "https://search.lomig.me",
    "https://yacy.ecosys.eu",
    "https://search.webproject.link",
];

const PAGE_SIZE: usize = 10;

impl YacyEngine {
    pub fn new() -> Self {
        let base_urls: Vec<String> = std::env::var("YACY_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.split(',')
                    .map(|u| u.trim().trim_end_matches('/').to_string())
                    .filter(|u| !u.is_empty())
                    .collect()
            })
            .unwrap_or_else(|| {
                DEFAULT_INSTANCES.iter().map(|s| s.to_string()).collect()
            });

        let metadata = EngineMetadata {
            name: "yacy".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "YaCy - free distributed P2P search engine.".to_string(),
            website: Some("https://yacy.net/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create YaCy HTTP client");
        YacyEngine {
            metadata,
            client,
            base_urls,
        }
    }

    fn base_url(&self) -> &str {
        // deterministic selection; first configured instance
        self.base_urls
            .first()
            .map(|s| s.as_str())
            .unwrap_or(DEFAULT_INSTANCES[0])
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base = self.base_url();
        let offset = query.offset.to_string();
        let max = PAGE_SIZE.to_string();
        // contentdom=text, resource=global
        let mut req = self
            .client
            .get(format!("{}/yacysearch.json", base))
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("query", query.query.as_str()),
                ("startRecord", offset.as_str()),
                ("maximumRecords", max.as_str()),
                ("contentdom", "text"),
                ("resource", "global"),
            ]);
        if let Some(lang) = &query.language {
            let short = lang.split('-').next().unwrap_or(lang);
            req = req.query(&[("lr", short)]);
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => {
                // self-hosted / instance down -> graceful empty
                tracing::info!("yacy instance unreachable ({}): {}", base, e);
                return Ok(vec![]);
            }
        };
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: YacyResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let items = parsed
            .channels
            .first()
            .map(|c| c.items.clone())
            .unwrap_or_default();

        let mut results = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let url = if !item.link.is_empty() {
                item.link.clone()
            } else {
                item.url.clone()
            };
            if url.is_empty() {
                continue;
            }
            let title = item.title.clone();
            let content = strip_html(&item.description);
            let r = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("published", serde_json::json!(item.pub_date));
            results.push(r);
        }
        Ok(results)
    }
}

#[derive(Debug, Deserialize)]
struct YacyResponse {
    #[serde(default)]
    channels: Vec<YacyChannel>,
}

#[derive(Debug, Deserialize)]
struct YacyChannel {
    #[serde(default)]
    items: Vec<YacyItem>,
}

#[derive(Debug, Deserialize, Clone)]
struct YacyItem {
    #[serde(default)]
    link: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    pub_date: String,
}

/// Strip simple HTML tags from a description string.
fn strip_html(s: &str) -> String {
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
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[async_trait]
impl Engine for YacyEngine {
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
        s.insert("base_url".to_string(), self.base_url().to_string());
        s.insert("search_type".to_string(), "text".to_string());
        s.insert("search_mode".to_string(), "global".to_string());
        s.insert("page_size".to_string(), PAGE_SIZE.to_string());
        s.insert("results".to_string(), "JSON".to_string());
        s
    }
}
