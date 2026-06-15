//! Naver search engine implementation
//!
//! Naver is the Korean search engine at search.naver.com.
//! It supports general / images / news / videos categories
//! via HTML scraping. This port implements the general (web) results path and
//! the news path; the general path is the default. Category: general.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Naver (Korean search engine) general web search engine
pub struct NaverEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    naver_category: String,
}

impl NaverEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "naver".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Naver - Korean search engine.".to_string(),
            website: Some("https://search.naver.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Naver HTTP client");

        NaverEngine {
            metadata,
            client,
            base_url: "https://search.naver.com".to_string(),
            naver_category: "general".to_string(),
        }
    }

    // general: ul.lst_total li.bx
    fn parse_general(&self, html: &str) -> Vec<(String, String, String, Option<String>)> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();
        let li_sel = match Selector::parse("ul.lst_total li.bx") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let title_sel = Selector::parse("a.link_tit").unwrap();
        let content_sel = Selector::parse("div.total_dsc_wrap a.api_txt_lines").unwrap();
        let thumb_sel = Selector::parse("div.thumb_single img").unwrap();
        for li in doc.select(&li_sel) {
            let title_a = match li.select(&title_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let title = title_a.text().collect::<String>().trim().to_string();
            let url = title_a.value().attr("href").unwrap_or("").to_string();
            if url.is_empty() {
                continue;
            }
            let content = li
                .select(&content_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = li
                .select(&thumb_sel)
                .next()
                .and_then(|i| i.value().attr("data-lazysrc").map(|s| s.to_string()));
            out.push((title, url, content, thumbnail));
        }
        out
    }

    // news: div.sds-comps-base-layout.sds-comps-full-layout
    fn parse_news(&self, html: &str) -> Vec<(String, String, String, Option<String>)> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();
        let item_sel = match Selector::parse("div.sds-comps-base-layout.sds-comps-full-layout") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let title_sel =
            Selector::parse("span.sds-comps-text-type-headline1").unwrap();
        let a_sel = Selector::parse("a[nocr='1']").unwrap();
        let content_sel =
            Selector::parse("span.sds-comps-text-type-body1").unwrap();
        let thumb_sel =
            Selector::parse("div.sds-comps-image.sds-rego-thumb-overlay img[src]").unwrap();
        for item in doc.select(&item_sel) {
            let title = item
                .select(&title_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let url = item
                .select(&a_sel)
                .next()
                .and_then(|a| a.value().attr("href").map(|s| s.to_string()))
                .unwrap_or_default();
            let content = item
                .select(&content_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() || content.is_empty() || url.is_empty() {
                continue;
            }
            let thumbnail = item
                .select(&thumb_sel)
                .next()
                .and_then(|i| i.value().attr("src").map(|s| s.to_string()));
            out.push((title, url, content, thumbnail));
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let (start_step, where_) = match self.naver_category.as_str() {
            "images" => (50, "image"),
            "news" => (10, "news"),
            "videos" => (48, "video"),
            _ => (15, "web"),
        };
        let pageno = (query.offset / 10).max(0) + 1;
        let start = ((pageno - 1) * start_step + 1).to_string();

        let url = format!("{}/search.naver", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html,application/xhtml+xml")
            .query(&[
                ("query", query.query.as_str()),
                ("start", start.as_str()),
                ("where", where_),
            ])
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

        let parsed = match self.naver_category.as_str() {
            "news" => self.parse_news(&text),
            _ => self.parse_general(&text),
        };

        let result_type = match self.naver_category.as_str() {
            "news" => ResultType::News,
            "images" => ResultType::Images,
            "videos" => ResultType::Videos,
            _ => ResultType::Web,
        };

        let mut results = Vec::new();
        for (i, (title, url, content, thumbnail)) in parsed.iter().enumerate() {
            if url.is_empty() {
                continue;
            }
            let title = if title.is_empty() {
                "Naver result".to_string()
            } else {
                title.clone()
            };
            let mut result = SearchResult::new(title, url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(result_type)
                .with_extra("source", serde_json::json!("naver"));
            if !content.is_empty() {
                result = result.with_snippet(content.clone());
            }
            if let Some(thumb) = thumbnail {
                if !thumb.is_empty() {
                    result = result.with_extra("thumbnail", serde_json::json!(thumb));
                }
            }
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for NaverEngine {
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
        matches!(
            result_type,
            ResultType::Web | ResultType::News | ResultType::Images | ResultType::Videos | ResultType::All
        )
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("naver_category".to_string(), self.naver_category.clone());
        settings.insert("language".to_string(), "ko".to_string());
        settings
    }
}
