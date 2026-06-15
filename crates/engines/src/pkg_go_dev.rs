//! pkg.go.dev search engine implementation (IT/packages; HTML scrape)
//!
//! Queries the Go package registry
//! at `https://pkg.go.dev/search` and scrapes the result snippets.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// pkg.go.dev (Go packages) search engine
pub struct PkgGoDevEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl PkgGoDevEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "pkg_go_dev".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "pkg.go.dev - Go package and module search.".to_string(),
            website: Some("https://pkg.go.dev/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create pkg.go.dev HTTP client");

        PkgGoDevEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://pkg.go.dev";
        let limit = 50.to_string();

        let resp = self
            .client
            .get(format!("{}/search", base_url))
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("q", query.query.as_str()),
                ("m", "package"),
                ("limit", limit.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html, query))
    }

    fn parse_html(&self, html: &str, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // SearchSnippet blocks under the SearchResults container
        let item_sel = match Selector::parse("div.SearchSnippet") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let header_sel = Selector::parse("div.SearchSnippet-headerContainer h2 a").unwrap();
        let synopsis_sel = Selector::parse("p.SearchSnippet-synopsis").unwrap();
        let version_sel = Selector::parse("div.SearchSnippet-infoLabel span strong").unwrap();
        let license_sel = Selector::parse("span[data-test-id='snippet-license'] a").unwrap();
        let popularity_sel =
            Selector::parse("div.SearchSnippet-infoLabel a strong").unwrap();

        for (i, el) in document.select(&item_sel).enumerate() {
            let a = match el.select(&header_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").to_string();
            if href.is_empty() {
                continue;
            }
            let url = if href.starts_with("http") {
                href
            } else {
                format!("https://pkg.go.dev{}", href)
            };

            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }

            let content = el
                .select(&synopsis_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            // The version is the first <strong> inside the info label; the
            // license name and popularity appear in their labelled spans/links.
            let version = el
                .select(&version_sel)
                .next()
                .map(|v| v.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let license_name = el
                .select(&license_sel)
                .next()
                .map(|l| l.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let license_url = el
                .select(&license_sel)
                .next()
                .and_then(|l| l.value().attr("href"))
                .map(|u| {
                    if u.starts_with("http") {
                        u.to_string()
                    } else {
                        format!("https://pkg.go.dev{}", u)
                    }
                })
                .unwrap_or_default();
            let popularity = el
                .select(&popularity_sel)
                .next()
                .map(|p| p.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let mut snippet_parts = Vec::new();
            if !version.is_empty() {
                snippet_parts.push(format!("Version: {}", version));
            }
            if !popularity.is_empty() {
                snippet_parts.push(format!("Importers: {}", popularity));
            }
            if !license_name.is_empty() {
                snippet_parts.push(format!("License: {}", license_name));
            }
            if !content.is_empty() {
                snippet_parts.push(content.clone());
            }

            results.push(
                SearchResult::new(&title, &url)
                    .with_snippet(snippet_parts.join(" | "))
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::IT)
                    .with_extra("package_name", serde_json::json!(title))
                    .with_extra("version", serde_json::json!(version))
                    .with_extra("popularity", serde_json::json!(popularity))
                    .with_extra("license_name", serde_json::json!(license_name))
                    .with_extra("license_url", serde_json::json!(license_url)),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for PkgGoDevEngine {
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
        matches!(t, ResultType::IT | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://pkg.go.dev".to_string());
        s.insert("max_result_count".to_string(), "50".to_string());
        s
    }
}
