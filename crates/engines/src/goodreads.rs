//! Goodreads search engine implementation
//!
//! HTML scrape of
//! `https://www.goodreads.com/search?q=...&page=N` collecting `<tr>` rows of
//! the results table. Category: general (ref `categories = []`).

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Goodreads book search engine (HTML scrape)
pub struct GoodreadsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl GoodreadsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "goodreads".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Goodreads - book search and reviews.".to_string(),
            website: Some("https://www.goodreads.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Goodreads HTTP client");

        GoodreadsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.goodreads.com";
        let page = query.offset + 1;
        let page_str = page.to_string();

        let response = self
            .client
            .get(format!("{}/search", base_url))
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "text/html,application/xhtml+xml")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page_str.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let html = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let doc = Html::parse_document(&html);
        let row_sel = match Selector::parse("table tr") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let title_sel = Selector::parse("a.bookTitle").unwrap();
        let author_sel = Selector::parse("a.authorName").unwrap();
        let info_sel = Selector::parse("span.uitext").unwrap();
        let cover_sel = Selector::parse("img.bookCover").unwrap();

        let mut results = Vec::new();
        for (i, row) in doc.select(&row_sel).enumerate() {
            if i >= query.count {
                break;
            }
            let href = match row.select(&title_sel).next() {
                Some(a) => a.value().attr("href").unwrap_or("").to_string(),
                None => continue,
            };
            let title = row
                .select(&title_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let url = if href.starts_with("http") {
                href
            } else {
                format!("{}{}", base_url, href)
            };
            let author = row
                .select(&author_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let info = row
                .select(&info_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = row
                .select(&cover_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .unwrap_or("")
                .to_string();

            let snippet = if info.is_empty() {
                author.clone()
            } else if author.is_empty() {
                info.clone()
            } else {
                format!("{} | {}", info, author)
            };

            let result = SearchResult::new(title, url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("author", serde_json::json!(author))
                .with_extra("source", serde_json::json!("goodreads"));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for GoodreadsEngine {
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
        settings.insert(
            "base_url".to_string(),
            "https://www.goodreads.com".to_string(),
        );
        settings.insert("search_endpoint".to_string(), "/search".to_string());
        settings
    }
}
