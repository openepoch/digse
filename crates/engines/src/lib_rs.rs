//! lib.rs (Rust packages) search engine implementation
//!
//! scrapes https://lib.rs search results
//! for Rust crates. Category: it / packages.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// lib.rs Rust packages search engine
pub struct LibRsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

impl LibRsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "lib_rs".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "lib.rs - Rust crates search.".to_string(),
            website: Some("https://lib.rs".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create lib.rs HTTP client");

        LibRsEngine {
            metadata,
            client,
            base_url: "https://lib.rs".to_string(),
        }
    }

    fn parse_html(&self, html: &str) -> Vec<(String, String, String, String, String, Vec<String>)> {
        // (title, url, content, version, popularity, tags)
        let doc = Html::parse_document(html);
        let mut out = Vec::new();

        let row_sel = match Selector::parse("body main div ol li a") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let title_sel = Selector::parse("div.h h4").unwrap();
        let content_sel = Selector::parse("div.h p").unwrap();
        let version_sel = Selector::parse("div.meta span.version").unwrap();
        let version_sel_alt = Selector::parse("div.meta span[class*='version']").unwrap();
        let downloads_sel = Selector::parse("div.meta span.downloads").unwrap();
        let tags_sel = Selector::parse("div.meta span[class*='k']").unwrap();

        for a in doc.select(&row_sel) {
            let href = a.value().attr("href").unwrap_or("").to_string();
            let url = if href.starts_with("http") {
                href.clone()
            } else {
                format!("{}{}", self.base_url, href)
            };
            if href.is_empty() {
                continue;
            }
            let title = a
                .select(&title_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let content = a
                .select(&content_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let version = a
                .select(&version_sel)
                .next()
                .or_else(|| a.select(&version_sel_alt).next())
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let popularity = a
                .select(&downloads_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let tags: Vec<String> = a
                .select(&tags_sel)
                .map(|e| e.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            if !title.is_empty() {
                out.push((title, url, content, version, popularity, tags));
            }
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let encoded = urlencoding::encode(&query.query);
        let url = format!("{}/search?q={}", self.base_url, encoded);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "text/html,application/xhtml+xml")
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
        for (i, (title, url, content, version, popularity, tags)) in parsed.iter().enumerate() {
            if url.is_empty() {
                continue;
            }
            let mut result = SearchResult::new(title.clone(), url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT);
            if !content.is_empty() {
                result = result.with_snippet(content.clone());
            }
            result = result.with_extra("package_name", serde_json::json!(title));
            if !version.is_empty() {
                result = result.with_extra("version", serde_json::json!(version));
            }
            if !popularity.is_empty() {
                result = result.with_extra("popularity", serde_json::json!(popularity));
            }
            if !tags.is_empty() {
                result = result.with_extra("tags", serde_json::json!(tags));
            }
            result = result.with_extra("source", serde_json::json!("lib.rs"));
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for LibRsEngine {
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
        matches!(result_type, ResultType::IT | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("search_url".to_string(), "/search".to_string());
        settings
    }
}
