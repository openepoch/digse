//! DokuWiki search engine implementation
//!
//! searches an arbitrary DokuWiki instance via
//! its OpenSearch-compatible search endpoint. The base URL is configurable.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// DokuWiki search engine
pub struct DokuEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

impl DokuEngine {
    pub fn new() -> Self {
        let base_url = std::env::var("DOKU_BASE_URL")
            .unwrap_or_else(|_| "https://www.dokuwiki.org".to_string());
        let metadata = EngineMetadata {
            name: "doku".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "DokuWiki search.".to_string(),
            website: Some("https://www.dokuwiki.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Doku HTTP client");

        DokuEngine {
            metadata,
            client,
            base_url,
        }
    }

    fn join_url(&self, href: &str) -> String {
        if href.starts_with("http") {
            href.to_string()
        } else if href.starts_with('/') {
            format!("{}{}", self.base_url, href)
        } else {
            format!("{}/{}", self.base_url, href)
        }
    }

    fn parse_html(&self, html: &str) -> Vec<(String, String, String)> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();

        // Quickhits
        let quick_sel = Selector::parse("div.search_quickresult ul li").unwrap_or_else(|_| {
            // A selector that matches nothing (empty :never pseudo-class)
            Selector::parse("span.__digse_never_match__").expect("fallback selector")
        });
        let wl_sel = Selector::parse("a.wikilink1").unwrap();
        for li in doc.select(&quick_sel) {
            if let Some(a) = li.select(&wl_sel).next() {
                let href = a.value().attr("href").unwrap_or("").to_string();
                if href.is_empty() {
                    continue;
                }
                let title = a
                    .value()
                    .attr("title")
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| a.text().collect::<String>().trim().to_string());
                out.push((title, self.join_url(&href), String::new()));
            }
        }

        // Search results (dl.search_results > dt > a + dd content)
        let dl_sel = match Selector::parse("dl.search_results > *") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let dt_sel = Selector::parse("dt").unwrap();
        let dd_sel = Selector::parse("dd").unwrap();
        let mut current_title: Option<String> = None;
        let mut current_url: Option<String> = None;
        for child in doc.select(&dl_sel) {
            if child.select(&dt_sel).next().is_some() {
                if let Some(a) = child.select(&wl_sel).next() {
                    let href = a.value().attr("href").unwrap_or("").to_string();
                    let title = a
                        .value()
                        .attr("title")
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| a.text().collect::<String>().trim().to_string());
                    current_title = Some(title);
                    current_url = if href.is_empty() {
                        None
                    } else {
                        Some(self.join_url(&href))
                    };
                }
            } else if child.select(&dd_sel).next().is_some() || child.value().name() == "dd" {
                let content = child.text().collect::<String>().trim().to_string();
                if let (Some(t), Some(u)) = (current_title.take(), current_url.take()) {
                    out.push((t, u, content));
                }
            }
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let encoded = urlencoding::encode(&query.query);
        let url = format!("{}/?do=search&id={}", self.base_url, encoded);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "text/html")
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
        for (i, (title, url, content)) in parsed.iter().enumerate() {
            if url.is_empty() {
                continue;
            }
            let title = if title.is_empty() {
                "DokuWiki result".to_string()
            } else {
                title.clone()
            };
            let mut result = SearchResult::new(title, url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            if !content.is_empty() {
                result = result.with_snippet(content.clone());
            }
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for DokuEngine {
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
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("search_url".to_string(), "/?do=search".to_string());
        settings
    }
}
