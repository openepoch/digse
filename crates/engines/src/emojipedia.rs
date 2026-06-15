//! Emojipedia search engine implementation
//!
//! HTML scrape of
//! `https://emojipedia.org/search?q=...` collecting `<a>` links inside
//! `<div class="EmojisList...">`. Category: general.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Emojipedia emoji search engine (HTML scrape)
pub struct EmojipediaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl EmojipediaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "emojipedia".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Emojipedia - emoji meaning and usage reference.".to_string(),
            website: Some("https://emojipedia.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Emojipedia HTTP client");

        EmojipediaEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://emojipedia.org";
        let url = format!("{}/search", base_url);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "text/html,application/xhtml+xml")
            .query(&[("q", query.query.as_str())])
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
        // ref: //div[starts-with(@class, "EmojisList")]/a
        let container_sel = match Selector::parse("div[class^='EmojisList']") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let a_sel = Selector::parse("a").unwrap();

        let mut results = Vec::new();
        let mut i = 0usize;
        for container in doc.select(&container_sel) {
            for a in container.select(&a_sel) {
                if i >= query.count {
                    break;
                }
                let href = a.value().attr("href").unwrap_or("").to_string();
                if href.is_empty() {
                    continue;
                }
                let abs_url = if href.starts_with("http") {
                    href
                } else {
                    format!("{}{}", base_url, href)
                };
                let title = a.text().collect::<String>().trim().to_string();
                if title.is_empty() {
                    continue;
                }
                let result = SearchResult::new(title, abs_url)
                    .with_snippet("")
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web)
                    .with_extra("source", serde_json::json!("emojipedia"));
                results.push(result);
                i += 1;
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for EmojipediaEngine {
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
        settings.insert("base_url".to_string(), "https://emojipedia.org".to_string());
        settings.insert("search_url".to_string(), "/search".to_string());
        settings
    }
}
