//! Rumble search engine implementation
//!
//! Rumble is a video
//! platform; the search results are scraped from the HTML of
//! `https://rumble.com/search/video`.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Rumble search engine (videos, HTML scrape)
pub struct RumbleEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl RumbleEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "rumble".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Rumble - video sharing platform.".to_string(),
            website: Some("https://rumble.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Rumble HTTP client");
        RumbleEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://rumble.com";
        let page = (query.offset / 10) + 1;
        let mut params: Vec<(&str, &str)> = vec![("q", query.query.as_str())];
        let page_str;
        if page > 1 {
            page_str = page.to_string();
            params.push(("page", page_str.as_str()));
        }
        let resp = self
            .client
            .get(format!("{}/search/video", base_url))
            .header("User-Agent", "digse/0.1.0")
            .query(&params)
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
        Ok(self.parse(&html, base_url))
    }

    fn parse(&self, html: &str, base_url: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let item_sel = match Selector::parse("li.video-listing-entry") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let a_sel = match Selector::parse("a.video-item--a") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let img_sel = Selector::parse("img.video-item--img").unwrap();
        let title_sel = Selector::parse("h3.video-item--title").unwrap();
        let time_sel = Selector::parse("time.video-item--meta.video-item--time").unwrap();
        let views_sel =
            Selector::parse("span.video-item--meta.video-item--views").unwrap();
        let rumbles_sel =
            Selector::parse("span.video-item--meta.video-item--rumbles").unwrap();
        let author_sel = Selector::parse("div.ellipsis-1").unwrap();
        let length_sel = Selector::parse("span.video-item--duration").unwrap();

        for item in document.select(&item_sel) {
            let href = match item.select(&a_sel).next().and_then(|a| a.value().attr("href")) {
                Some(h) => h.to_string(),
                None => continue,
            };
            let url = if href.starts_with("http") {
                href
            } else {
                format!("{}{}", base_url, href)
            };
            let title = item
                .select(&title_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = item
                .select(&img_sel)
                .next()
                .and_then(|i| i.value().attr("src"))
                .unwrap_or("")
                .to_string();
            let published = item
                .select(&time_sel)
                .next()
                .and_then(|t| t.value().attr("datetime"))
                .unwrap_or("")
                .to_string();
            let views = item
                .select(&views_sel)
                .next()
                .and_then(|v| v.value().attr("data-value"))
                .unwrap_or("")
                .to_string();
            let rumbles = item
                .select(&rumbles_sel)
                .next()
                .and_then(|v| v.value().attr("data-value"))
                .unwrap_or("")
                .to_string();
            let author = item
                .select(&author_sel)
                .next()
                .map(|a| a.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let length = item
                .select(&length_sel)
                .next()
                .and_then(|l| l.value().attr("data-value"))
                .unwrap_or("")
                .to_string();
            if title.is_empty() {
                continue;
            }
            let content = if views.is_empty() && rumbles.is_empty() {
                String::new()
            } else {
                format!("{} views - {} rumbles", views, rumbles)
            };
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("views", serde_json::json!(views))
                    .with_extra("author", serde_json::json!(author))
                    .with_extra("duration", serde_json::json!(length)),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for RumbleEngine {
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
        matches!(t, ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://rumble.com".into());
        s
    }
}
