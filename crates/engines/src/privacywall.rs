//! Privacywall search engine implementation
//!
//! The reference supports general, images and videos; digse
//! implements the general results path here.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Privacywall search engine (HTML scrape)
pub struct PrivacywallEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl PrivacywallEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "privacywall".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Privacywall - privacy-oriented web search.".to_string(),
            website: Some("https://www.privacywall.org".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Privacywall HTTP client");
        PrivacywallEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.privacywall.org";
        // Privacywall only supports first page for general search
        let page = (query.offset / 10) + 1;
        if page > 10 {
            return Ok(vec![]);
        }
        let resp = self
            .client
            .get(format!("{}/search/secure/", base_url))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("q", query.query.as_str()),
                ("safesearch", "on"),
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
        Ok(self.parse_general(&html))
    }

    fn parse_general(&self, html: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // Result cards live inside #pw-results-main
        let card_sel = match Selector::parse("#pw-results-main div.result-card") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let anchor_sel = match Selector::parse("a.result-url-anchor") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let title_sel = Selector::parse("div.result_title").unwrap();
        let desc_sel = Selector::parse("div.result-description").unwrap();

        for card in document.select(&card_sel) {
            let url = card
                .select(&anchor_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .trim()
                .to_string();
            let title = card
                .select(&title_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let content = card
                .select(&desc_sel)
                .next()
                .map(|d| d.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if url.is_empty() && title.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for PrivacywallEngine {
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
        s.insert("base_url".into(), "https://www.privacywall.org".into());
        s.insert("category".into(), "general".into());
        s
    }
}
