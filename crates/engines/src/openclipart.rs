//! OpenClipArt search engine implementation (images; HTML scrape)
//!
//! Scrapes the OpenClipArt
//! gallery at `https://openclipart.org/search/`.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// OpenClipArt image search engine
pub struct OpenClipartEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl OpenClipartEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "openclipart".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "OpenClipArt - Public domain clipart.".to_string(),
            website: Some("https://openclipart.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create OpenClipArt HTTP client");

        OpenClipartEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://openclipart.org";
        let page = (query.offset / 10) + 1;
        let page_str = page.to_string();

        let resp = self
            .client
            .get(format!("{}/search/", base_url))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("query", query.query.as_str()),
                ("p", page_str.as_str()),
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

        let item_sel = match Selector::parse("div.gallery div.artwork") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let a_sel = Selector::parse("a").unwrap();
        let img_sel = Selector::parse("img").unwrap();

        for (i, el) in document.select(&item_sel).enumerate() {
            let a = match el.select(&a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").to_string();
            if href.is_empty() {
                continue;
            }
            let page_url = if href.starts_with("http") {
                href
            } else {
                format!("https://openclipart.org{}", href)
            };

            let img = match el.select(&img_sel).next() {
                Some(im) => im,
                None => continue,
            };
            let img_src_raw = img.value().attr("src").unwrap_or("").to_string();
            let alt = img.value().attr("alt").unwrap_or("").to_string();

            let img_src = if img_src_raw.starts_with("http") {
                img_src_raw
            } else {
                format!("https://openclipart.org{}", img_src_raw)
            };

            let title = if alt.is_empty() {
                format!("OpenClipArt result {}", i + 1)
            } else {
                alt
            };

            results.push(
                SearchResult::new(&title, &page_url)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(img_src))
                    .with_extra("thumbnail", serde_json::json!(img_src))
                    .with_extra("source", serde_json::json!("openclipart")),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for OpenClipartEngine {
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
        s.insert("base_url".to_string(), "https://openclipart.org".to_string());
        s
    }
}
