//! INA search engine implementation
//!
//! French video archive HTML scrape.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// INA (French video archive) search engine
pub struct InaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl InaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "ina".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "INA - Institut national de l'audiovisuel (French video).".to_string(),
            website: Some("https://www.ina.fr/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create INA HTTP client");

        InaEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.ina.fr";
        let page_size: i64 = 12;
        let start = ((query.offset as i64 / page_size) + 1) * page_size;
        let url = format!(
            "{}/ajax/recherche?q={}&espace=1&sort=pertinence&order=desc&offset={}&modified=size",
            base_url,
            urlencoding::encode(&query.query),
            start
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.1.0)")
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

        let doc = Html::parse_document(&text);
        let mut results = Vec::new();

        let result_sel = match Selector::parse("#searchHits > div") {
            Ok(s) => s,
            Err(_) => return Ok(results),
        };
        let a_sel = Selector::parse("a").unwrap();
        let title_sel = Selector::parse("div[class*='title-bloc-small']").unwrap();
        let content_sel = Selector::parse("div[class*='sous-titre-fonction']").unwrap();
        let date_sel = Selector::parse("div[class*='dateAgenda']").unwrap();
        let thumb_sel = Selector::parse("img").unwrap();

        for el in doc.select(&result_sel) {
            if results.len() >= query.count {
                break;
            }
            let href = el
                .select(&a_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("");
            if href.is_empty() {
                continue;
            }
            let url = format!("{}{}", base_url, href);
            let title = el
                .select(&title_sel)
                .next()
                .map(|t| html_escape(&t.text().collect::<String>()))
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let date_text = el
                .select(&date_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let content_text = el
                .select(&content_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let content = format!("{}{}", date_text, content_text);
            let thumbnail = el
                .select(&thumb_sel)
                .next()
                .and_then(|img| img.value().attr("data-src").or_else(|| img.value().attr("src")))
                .unwrap_or("")
                .to_string();

            let mut result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + results.len() + 1)
                .with_score(1.0 - (results.len() as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("thumbnail", serde_json::json!(thumbnail));
            if !date_text.is_empty() {
                result = result.with_extra("published", serde_json::json!(date_text));
            }
            results.push(result);
        }
        Ok(results)
    }
}

/// Unescape basic HTML entities (port of Python's html.unescape for the common cases).
fn html_escape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

#[async_trait]
impl Engine for InaEngine {
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
        matches!(t, ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://www.ina.fr".into());
        s
    }
}
