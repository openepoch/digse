//! System1 (s1search) search engine implementation
//!
//! System1 provides
//! search as a subdomain of `s1search.co` (e.g. `search.gmx.net`); the base URL
//! is configurable via the `S1SEARCH_URL` env var. Pagination tokens (`sc`)
//! require a session cache which digse does not maintain, so only the first
//! page is fetched.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// System1 (s1search) search engine (general/web, HTML scrape)
pub struct S1SearchEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

impl S1SearchEngine {
    pub fn new() -> Self {
        let base_url = std::env::var("S1SEARCH_URL")
            .unwrap_or_else(|_| "https://s1search.co".to_string());
        let metadata = EngineMetadata {
            name: "s1search".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "System1 (s1search.co) - advertising-driven web search.".to_string(),
            website: Some("https://s1search.co".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create s1search HTTP client");
        S1SearchEngine {
            metadata,
            client,
            base_url,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // Only first page is supported (pagination requires cached `sc` tokens).
        let page = (query.offset / 10) + 1;
        if page > 1 {
            return Ok(vec![]);
        }
        let url = format!("{}/serp", self.base_url);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("q", query.query.as_str()),
                ("page", "1"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse(&html))
    }

    fn parse(&self, html: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // Results live inside web-yahoo / web-google result containers.
        let container_sel = match Selector::parse(
            "div.web-yahoo div.__result, div.web-google div.__result",
        ) {
            Ok(s) => s,
            Err(_) => return results,
        };
        let title_anchor_sel = Selector::parse("a.title").unwrap();
        let desc_sel = Selector::parse("span.description, span").unwrap();

        for el in document.select(&container_sel) {
            let href = match el
                .select(&title_anchor_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
            {
                Some(h) => h.to_string(),
                None => continue,
            };
            let title = el
                .select(&title_anchor_sel)
                .next()
                .map(|a| a.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let content = el
                .select(&desc_sel)
                .next()
                .map(|d| d.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(title, href)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for S1SearchEngine {
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
        s.insert("base_url".into(), self.base_url.clone());
        s
    }
}
