//! Ollama model search engine implementation (IT/repos; HTML scrape)
//!
//! Ollama hosts a model registry at
//! `https://ollama.com/search?q=<query>`. This scrapes the model cards.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Ollama model search engine
pub struct OllamaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl OllamaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "ollama".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Ollama - LLM model registry search.".to_string(),
            website: Some("https://ollama.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Ollama HTTP client");

        OllamaEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://ollama.com";

        let resp = self
            .client
            .get(format!("{}/search", base_url))
            .header("User-Agent", "digse/0.1.0")
            .query(&[("q", query.query.as_str())])
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

        let item_sel = match Selector::parse("li[x-test-model]") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let title_sel = Selector::parse("span[x-test-search-response-title]").unwrap();
        let content_sel = Selector::parse("p.max-w-lg.break-words.text-neutral-800.text-md").unwrap();
        let link_sel = Selector::parse("a").unwrap();
        let date_sel = Selector::parse("span.flex.items-center").unwrap();

        for (i, el) in document.select(&item_sel).enumerate() {
            let href = match el.select(&link_sel).next().and_then(|a| a.value().attr("href")) {
                Some(h) => h.to_string(),
                None => continue,
            };
            let url = if href.starts_with("http") {
                href
            } else {
                format!("https://ollama.com{}", href)
            };

            let title = el
                .select(&title_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "Ollama model".to_string());

            let content = el
                .select(&content_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let published = el
                .select(&date_sel)
                .next()
                .and_then(|d| d.value().attr("title"))
                .unwrap_or("")
                .to_string();

            results.push(
                SearchResult::new(&title, &url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::IT)
                    .with_extra("published", serde_json::json!(published)),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for OllamaEngine {
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
        s.insert("base_url".to_string(), "https://ollama.com".to_string());
        s
    }
}
