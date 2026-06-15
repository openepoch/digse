//! PyPI search engine implementation
//!
//! PyPI has no official
//! search JSON API; the reference scrapes the HTML search page. We do the same.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// PyPI search engine (IT / Python packages, HTML scrape)
pub struct PypiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl PypiEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "pypi".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "PyPI - Python Package Index.".to_string(),
            website: Some("https://pypi.org".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create PyPI HTTP client");
        PypiEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://pypi.org";
        let page = (query.offset / 20) + 1;
        let page_str = page.to_string();
        let resp = self
            .client
            .get(format!("{}/search/", base_url))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page_str.as_str()),
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
        Ok(self.parse(&html, base_url))
    }

    fn parse(&self, html: &str, base_url: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let snippet_sel = match Selector::parse("a.package-snippet") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let name_sel = Selector::parse("span.package-snippet__name").unwrap();
        let version_sel = Selector::parse("span.package-snippet__version").unwrap();
        let created_sel = Selector::parse("span.package-snippet__created time").unwrap();
        let desc_sel = Selector::parse("p").unwrap();

        for a in document.select(&snippet_sel) {
            let href = a.value().attr("href").unwrap_or("");
            let url = if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{}{}", base_url, href)
            };
            let name = a
                .select(&name_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let version = a
                .select(&version_sel)
                .next()
                .map(|v| v.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let created = a
                .select(&created_sel)
                .next()
                .and_then(|t| t.value().attr("datetime"))
                .unwrap_or("")
                .to_string();
            let content = a
                .select(&desc_sel)
                .next()
                .map(|p| p.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let title = if version.is_empty() {
                name.clone()
            } else {
                format!("{} {}", name, version)
            };
            let mut snippet = content;
            if !created.is_empty() {
                snippet = if snippet.is_empty() {
                    format!("Published: {}", created)
                } else {
                    format!("{} | Published: {}", snippet, created)
                };
            }
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(snippet)
                    .with_engine(self.name())
                    .with_result_type(ResultType::IT)
                    .with_extra("package_name", serde_json::json!(name))
                    .with_extra("version", serde_json::json!(version))
                    .with_extra("published", serde_json::json!(created)),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for PypiEngine {
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
        matches!(t, ResultType::IT | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://pypi.org".into());
        s.insert("search_endpoint".into(), "/search/".into());
        s
    }
}
