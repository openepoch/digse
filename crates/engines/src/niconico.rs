//! Niconico search engine implementation (videos; HTML scrape)
//!
//! Niconico is a Japanese video
//! hosting service. The reference implementation scrapes HTML from
//! `https://www.nicovideo.jp/search/{query}`.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Niconico video search engine
pub struct NiconicoEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl NiconicoEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "niconico".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Niconico - Japanese video hosting service.".to_string(),
            website: Some("https://www.nicovideo.jp/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Niconico HTTP client");

        NiconicoEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.nicovideo.jp";
        let page = (query.offset / 10) + 1;
        let page_str = page.to_string();

        let resp = self
            .client
            .get(format!("{}/search/{}", base_url, query.query))
            .header("User-Agent", "digse/0.0.1")
            .query(&[("page", page_str.as_str())])
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

        let item_sel = match Selector::parse("li[data-video-item]") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let thumb_sel = Selector::parse("a.itemThumbWrap").unwrap();
        let length_sel = Selector::parse("span.videoLength").unwrap();
        let title_sel = Selector::parse("p.itemTitle a").unwrap();
        let desc_sel = Selector::parse("p.itemDescription").unwrap();
        let img_sel = Selector::parse("img.thumb").unwrap();

        for (i, el) in document.select(&item_sel).enumerate() {
            // video id from the thumb link href
            let href = match el.select(&thumb_sel).next().and_then(|a| a.value().attr("href")) {
                Some(h) => h.to_string(),
                None => continue,
            };
            let video_id = href.split('?').next().unwrap_or(&href).rsplit('/').next().unwrap_or("").to_string();
            if video_id.is_empty() {
                continue;
            }

            let url = format!("https://www.nicovideo.jp/watch/{}", video_id);
            let iframe_src = format!("https://embed.nicovideo.jp/watch/{}", video_id);

            let title = el
                .select(&title_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_else(|| "Untitled".to_string());

            let content = el
                .select(&desc_sel)
                .next()
                .and_then(|d| d.value().attr("title"))
                .unwrap_or("")
                .to_string();

            let length = el
                .select(&length_sel)
                .next()
                .map(|l| l.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let thumbnail = el
                .select(&img_sel)
                .next()
                .and_then(|im| im.value().attr("src"))
                .unwrap_or("")
                .to_string();

            results.push(
                SearchResult::new(&title, &url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("duration", serde_json::json!(length))
                    .with_extra("iframe_src", serde_json::json!(iframe_src)),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for NiconicoEngine {
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
        s.insert("base_url".to_string(), "https://www.nicovideo.jp".to_string());
        s.insert("language".to_string(), "ja".to_string());
        s
    }
}
