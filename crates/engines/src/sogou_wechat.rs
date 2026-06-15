//! Sogou WeChat search engine implementation
//!
//! Scrapes the
//! Sogou WeChat article search HTML.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Sogou WeChat article search engine
pub struct SogouWechatEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://weixin.sogou.com";

impl SogouWechatEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "sogou_wechat".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Sogou WeChat - search WeChat articles.".to_string(),
            website: Some("https://weixin.sogou.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Sogou WeChat HTTP client");

        SogouWechatEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = ((query.offset / 10) + 1).to_string();
        let resp = self
            .client
            .get(format!("{}/weixin", BASE_URL))
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("query", query.query.as_str()),
                ("page", page.as_str()),
                ("type", "2"),
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
        Ok(self.parse_html(html, query))
    }

    fn parse_html(&self, html: String, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};

        let document = Html::parse_document(&html);
        let mut results = Vec::new();

        let li_sel = match Selector::parse("li[id^='sogou_vr_']") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let h3_a_sel = Selector::parse("h3 a").unwrap();
        let txt_sel = Selector::parse("p.txt-info").unwrap();
        let txt_contains_sel = Selector::parse("p[class*='txt-info']").unwrap();
        let img_sel = Selector::parse("div.img-box a img").unwrap();

        for (i, item) in document.select(&li_sel).enumerate() {
            let a = match item.select(&h3_a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let title = a.text().collect::<String>().trim().to_string();
            let mut url = a.value().attr("href").unwrap_or("").trim().to_string();
            if url.starts_with("/link?url=") {
                url = format!("{}{}", BASE_URL, url);
            }

            if title.is_empty() || url.is_empty() {
                continue;
            }

            let content = item
                .select(&txt_sel)
                .next()
                .or_else(|| item.select(&txt_contains_sel).next())
                .map(|p| p.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let thumbnail = item
                .select(&img_sel)
                .next()
                .and_then(|i| i.value().attr("src").map(|s| s.to_string()))
                .map(|t| {
                    if t.starts_with("//") {
                        format!("https:{}", t)
                    } else {
                        t
                    }
                })
                .unwrap_or_default();

            let mut result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::News);

            if !thumbnail.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
            }

            results.push(result);
        }

        results
    }
}

#[async_trait]
impl Engine for SogouWechatEngine {
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
        matches!(t, ResultType::News | ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("results".into(), "HTML".into());
        s.insert("language".into(), "zh".into());
        s
    }
}
