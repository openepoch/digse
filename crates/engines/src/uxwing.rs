//! UXwing icon/image search engine implementation (HTML).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// UXwing free icons/images search engine.
pub struct UxwingEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://uxwing.com";

impl UxwingEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "uxwing".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "UXwing - free icons and vector images.".to_string(),
            website: Some(BASE_URL.to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create UXwing HTTP client");
        UxwingEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!("{}/?s={}", BASE_URL, urlencoding::encode(&query.query));
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
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

        let article_sel = match Selector::parse("article[id^='post']") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let a_sel = match Selector::parse("a") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let img_sel = match Selector::parse("img") {
            Ok(s) => s,
            Err(_) => return results,
        };

        for (i, article) in document.select(&article_sel).enumerate() {
            if i >= query.count {
                break;
            }
            // Derive tags from CSS classes starting with "category" or "tag".
            let mut tags: Vec<String> = Vec::new();
            if let Some(class_attr) = article.value().attr("class") {
                for css_class in class_attr.split_whitespace() {
                    for prefix in ["category", "tag"] {
                        if let Some(rest) = css_class.strip_prefix(prefix) {
                            let tag = rest.replace('-', " ");
                            // title-case the tag
                            let titled: String = tag
                                .split_whitespace()
                                .map(|w| {
                                    let mut c = w.chars();
                                    match c.next() {
                                        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                                        None => String::new(),
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join(" ");
                            tags.push(titled);
                        }
                    }
                }
            }

            let a = match article.select(&a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").to_string();
            if href.is_empty() {
                continue;
            }
            let img = match article.select(&img_sel).next() {
                Some(i) => i,
                None => continue,
            };
            let img_src = img.value().attr("src").unwrap_or("").to_string();
            let title = img.value().attr("alt").unwrap_or("").to_string();
            let content = tags.join(", ");

            let r = SearchResult::new(title, href)
                .with_snippet(content.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(img_src))
                .with_extra("source", serde_json::json!("uxwing"))
                .with_extra("tags", serde_json::json!(content));
            results.push(r);
        }
        results
    }
}

#[async_trait]
impl Engine for UxwingEngine {
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
        s.insert("base_url".to_string(), BASE_URL.to_string());
        s.insert("results".to_string(), "HTML".to_string());
        s
    }
}
