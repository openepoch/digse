//! DuckDuckGo Web (general) engine implementation
//!
//! fetches general web results from
//! DuckDuckGo. The reference scrapes a deep-preload link from the DDG HTML and
//! then queries the JSON API. In this port we use the simpler, robust approach
//! of POSTing to the DDG HTML endpoint and parsing results (the same approach
//! used by the existing `duckduckgo.rs` engine), which avoids the brittle
//! `dp`/`vqd` token dance.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// DuckDuckGo web engine
pub struct DuckDuckGoWebEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl DuckDuckGoWebEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "duckduckgo_web".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "DuckDuckGo web search.".to_string(),
            website: Some("https://duckduckgo.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create DDG web HTTP client");

        DuckDuckGoWebEngine { metadata, client }
    }

    fn parse_html(&self, html: &str) -> Vec<DdgWebResult> {
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let selectors = [
            "div.result",
            "div.web-result",
            "div.results_links",
        ];
        let title_sel = Selector::parse("a.result__a, h2 a, a.result__url, a").unwrap();
        let snippet_sel = Selector::parse("a.result__snippet, .result__snippet, .snippet").unwrap();

        for sel_str in selectors.iter() {
            let sel = match Selector::parse(sel_str) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for element in document.select(&sel) {
                if let Some(a) = element.select(&title_sel).next() {
                    let title = a.text().collect::<String>().trim().to_string();
                    let url = a.value().attr("href").unwrap_or("").to_string();
                    if title.is_empty() || url.is_empty() {
                        continue;
                    }
                    let url = Self::unwrap_ddg_url(&url);
                    if !url.starts_with("http") {
                        continue;
                    }
                    let snippet = element
                        .select(&snippet_sel)
                        .next()
                        .map(|e| e.text().collect::<String>().trim().to_string())
                        .unwrap_or_default();
                    results.push(DdgWebResult {
                        title,
                        url,
                        body: snippet,
                    });
                }
            }
            if !results.is_empty() {
                break;
            }
        }

        // Fallback: bare links
        if results.is_empty() {
            let link_sel = Selector::parse("a[href]").unwrap();
            for element in document.select(&link_sel) {
                let url = element.value().attr("href").unwrap_or("").to_string();
                let title = element.text().collect::<String>().trim().to_string();
                if url.starts_with("http") && !url.contains("duckduckgo") && !title.is_empty() {
                    results.push(DdgWebResult {
                        title,
                        url,
                        body: String::new(),
                    });
                }
            }
        }
        results
    }

    /// Strip DDG redirect wrappers like //duckduckgo.com/l/?uddg=<url>
    fn unwrap_ddg_url(href: &str) -> String {
        if let Some(idx) = href.find("uddg=") {
            let rest = &href[idx + 5..];
            let end = rest.find('&').unwrap_or(rest.len());
            let encoded = &rest[..end];
            if let Ok(decoded) = urlencoding::decode(encoded) {
                return decoded.into_owned();
            }
        }
        href.to_string()
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.len() >= 500 {
            return Ok(vec![]);
        }
        let url = "https://duckduckgo.com/html/";
        let response = self
            .client
            .post(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&[("q", query.query.as_str()), ("kl", "us-en")])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }
        let text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let parsed = self.parse_html(&text);
        let mut results = Vec::new();
        for (i, r) in parsed.iter().enumerate() {
            let mut result = SearchResult::new(r.title.clone(), r.url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            if !r.body.is_empty() {
                result = result.with_snippet(r.body.clone());
            }
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

struct DdgWebResult {
    title: String,
    url: String,
    body: String,
}

#[async_trait]
impl Engine for DuckDuckGoWebEngine {
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

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://duckduckgo.com".to_string());
        settings.insert("search_endpoint".to_string(), "/html/".to_string());
        settings
    }
}
