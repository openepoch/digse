//! Imgur search engine implementation
//!
//! HTML scrape of imgur search. The
//! official API needs a client id; this implementation uses the public HTML.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// Imgur image search engine
pub struct ImgurEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl ImgurEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "imgur".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Imgur - image search.".to_string(),
            website: Some("https://imgur.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Imgur HTTP client");

        ImgurEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://imgur.com";
        let pageno = (query.offset / query.count.max(1)).to_string();
        let url = format!(
            "{}/search/score/all?q={}&qs=thumbs&p={}",
            base_url,
            urlencoding::encode(&query.query),
            pageno
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.0.1)")
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

        // cards > div.post
        let result_sel = match Selector::parse("div.cards div.post") {
            Ok(s) => s,
            Err(_) => return Ok(results),
        };
        let a_sel = Selector::parse("a").unwrap();
        let img_sel = Selector::parse("a img").unwrap();

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
            let img = match el.select(&img_sel).next() {
                Some(i) => i,
                None => continue,
            };
            let thumbnail_src = img.value().attr("src").unwrap_or("").to_string();
            // skip if no preview
            if thumbnail_src.len() < 25 {
                continue;
            }
            let img_src = thumbnail_src.replace("b.", ".");
            let title = img.value().attr("alt").unwrap_or("").to_string();

            let result = SearchResult::new(title, url)
                .with_engine(self.name())
                .with_rank(query.offset + results.len() + 1)
                .with_score(1.0 - (results.len() as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(thumbnail_src))
                .with_extra("source", serde_json::json!("imgur"));
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for ImgurEngine {
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
        matches!(t, ResultType::Images | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://imgur.com".into());
        s
    }
}
