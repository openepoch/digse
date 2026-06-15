//! Resulthunter search engine implementation
//!
//! Resulthunter proxies Brave
//! results via an HTML scrape of `resulthunter.com/search`. Supports two
//! categories — `web` and `images` — selected by the query's `result_type`:
//! web results come from `div.organic-results-container > div > div.group`,
//! image results from `a.group` anchors whose `<img>` carries alt/src.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

const BASE_URL: &str = "https://resulthunter.com";

/// Resulthunter (Brave-backed) general/image search engine
pub struct ResulthunterEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl ResulthunterEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "resulthunter".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Resulthunter (Brave-backed) web/image search.".to_string(),
            website: Some("https://resulthunter.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Resulthunter HTTP client");

        ResulthunterEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = (query.offset / query.count.max(1)) + 1;
        let want_images = matches!(query.result_type, ResultType::Images);
        let search_type = if want_images { "images" } else { "web" };
        let offset_str = (pageno - 1).to_string();
        let safe = if query.safe_search { "strict" } else { "off" };

        let response = self
            .client
            .get(&format!("{}/search", BASE_URL))
            .query(&[
                ("q", query.query.as_str()),
                ("search_type", search_type),
                ("offset", offset_str.as_str()),
            ])
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Cookie", format!("safesearch={}", safe))
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }
        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        if want_images {
            self.parse_images(&text, query)
        } else {
            self.parse_web(&text, query)
        }
    }

    /// Web results: `div.organic-results-container div.group` items.
    fn parse_web(&self, html: &str, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let doc = Html::parse_document(html);
        let item_sel = match Selector::parse("div.organic-results-container div.group") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let a_sel = Selector::parse("a").unwrap();
        let h3_sel = Selector::parse("a h3").unwrap();
        let p_sel = Selector::parse("p").unwrap();

        let mut results = Vec::new();
        for (i, item) in doc.select(&item_sel).enumerate() {
            if results.len() >= query.count {
                break;
            }
            let a = match item.select(&a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let url = a.value().attr("href").unwrap_or("").trim().to_string();
            if url.is_empty() {
                continue;
            }
            let title = text_of(item.select(&h3_sel).next());
            if title.is_empty() {
                continue;
            }
            let content = text_of(item.select(&p_sel).next());

            let result = SearchResult::new(title, urljoin(BASE_URL, &url))
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("source", serde_json::json!("resulthunter"));
            results.push(result);
        }
        Ok(results)
    }

    /// Image results: `a.group` anchors within the results container.
    fn parse_images(&self, html: &str, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let doc = Html::parse_document(html);
        let item_sel = match Selector::parse("div.organic-results-container a.group") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let img_sel = Selector::parse("img").unwrap();

        let mut results = Vec::new();
        for (i, a) in doc.select(&item_sel).enumerate() {
            if results.len() >= query.count {
                break;
            }
            let href = a.value().attr("href").unwrap_or("").trim().to_string();
            if href.is_empty() {
                continue;
            }
            let img = match a.select(&img_sel).next() {
                Some(im) => im,
                None => continue,
            };
            let title = img.value().attr("alt").unwrap_or("").to_string();
            let thumbnail = img.value().attr("src").unwrap_or("").to_string();

            let result = SearchResult::new(title, urljoin(BASE_URL, &href))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("img_src", serde_json::json!(thumbnail))
                .with_extra("source", serde_json::json!("resulthunter"));
            results.push(result);
        }
        Ok(results)
    }
}

/// Join a base URL with a possibly-relative href.
fn urljoin(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else if let Some(rest) = href.strip_prefix("//") {
        format!("https:{}", rest)
    } else if let Some(path) = href.strip_prefix('/') {
        format!("{}/{}", base.trim_end_matches('/'), path)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), href)
    }
}

/// Concatenate the direct text content of an optional element.
fn text_of(el: Option<scraper::ElementRef>) -> String {
    match el {
        Some(e) => e.text().collect::<String>().trim().to_string(),
        None => String::new(),
    }
}

#[async_trait]
impl Engine for ResulthunterEngine {
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
        matches!(t, ResultType::Web | ResultType::Images | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), BASE_URL.to_string());
        s.insert("resulthunter_categ".to_string(), "web".to_string());
        s
    }
}
