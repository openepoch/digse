//! Z-Library (book/file) search engine implementation (HTML).
//!
//! Z-Library domains rotate and are sometimes seized; the engine is graceful
//! on any failure (returning an empty result set).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Z-Library shadow library book search engine.
pub struct ZlibraryEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

impl ZlibraryEngine {
    pub fn new() -> Self {
        let base_url = std::env::var("ZLIBRARY_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "https://zlibrary-global.se".to_string());
        let metadata = EngineMetadata {
            name: "zlibrary".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Z-Library - scholarly articles and general-interest books.".to_string(),
            website: Some(base_url.clone()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .danger_accept_invalid_certs(true) // upstream sets verify=False
            .build()
            .expect("Failed to create Z-Library HTTP client");
        ZlibraryEngine {
            metadata,
            client,
            base_url,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = (query.offset / 10) + 1;
        let url = format!(
            "{}/s/{}?page={}",
            self.base_url,
            urlencoding::encode(&query.query),
            page
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        if domain_is_seized(&html) {
            tracing::info!("zlibrary: domain appears seized; returning empty");
            return Ok(vec![]);
        }
        Ok(self.parse_html(&html, query))
    }

    fn parse_html(&self, html: &str, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // upstream: //div[@id="searchResultBox"]//div[contains(@class, "resItemBox")]
        let item_sel = match Selector::parse("#searchResultBox div.resItemBox, #searchResultBox div[class*='resItemBox']")
        {
            Ok(s) => s,
            Err(_) => return results,
        };
        let name_sel = match Selector::parse("[itemprop='name']") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let author_sel = match Selector::parse("div.authors a[itemprop='author'], .authors a[itemprop='author']") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let book_link_sel = match Selector::parse("a[href^='/book/']") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let cover_sel = match Selector::parse("img.cover, img[class*='cover']") {
            Ok(s) => s,
            Err(_) => return results,
        };

        for (i, item) in document.select(&item_sel).enumerate() {
            if i >= query.count {
                break;
            }
            // first <a href="/book/...">
            let href = match item.select(&book_link_sel).next() {
                Some(a) => a.value().attr("href").unwrap_or("").to_string(),
                None => continue,
            };
            if href.is_empty() {
                continue;
            }
            let url = format!("{}{}", self.base_url, href);

            let title = item
                .select(&name_sel)
                .next()
                .map(|n| n.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let authors: Vec<String> = item
                .select(&author_sel)
                .map(|a| a.text().collect::<String>().trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let thumbnail = item
                .select(&cover_sel)
                .next()
                .and_then(|im| {
                    im.value()
                        .attr("data-src")
                        .or_else(|| im.value().attr("src"))
                        .map(|s| s.to_string())
                })
                .filter(|s| !s.starts_with('/'))
                .unwrap_or_default();

            let snippet = if authors.is_empty() {
                String::new()
            } else {
                format!("Authors: {}", authors.join(", "))
            };

            let r = SearchResult::new(title, url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Files)
                .with_extra("author", serde_json::json!(authors.join(", ")))
                .with_extra("authors", serde_json::json!(authors))
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("file_format", serde_json::Value::Null);
            results.push(r);
        }
        results
    }
}

/// upstream checks the page <title> for the word "seized".
fn domain_is_seized(html: &str) -> bool {
    if let Some(start) = html.find("<title") {
        if let Some(end) = html[start..].find("</title>") {
            let title = &html[start..start + end];
            let lower = title.to_lowercase();
            return lower.contains("seized");
        }
    }
    false
}

#[async_trait]
impl Engine for ZlibraryEngine {
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
        matches!(t, ResultType::Files | ResultType::Academic | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), self.base_url.clone());
        s.insert("results".to_string(), "HTML".to_string());
        s.insert("zlib_year_from".to_string(), String::new());
        s.insert("zlib_year_to".to_string(), String::new());
        s.insert("zlib_ext".to_string(), String::new());
        s
    }
}
