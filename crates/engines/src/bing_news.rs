//! Bing News search engine implementation.
//! Scrapes bing.com/news/infinitescrollajax.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Bing News search engine.
pub struct BingNewsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl BingNewsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bing_news".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Bing News - Microsoft's news search.".to_string(),
            website: Some("https://www.bing.com/news".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Bing News HTTP client");
        BingNewsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://www.bing.com/news/infinitescrollajax";
        let page = query.offset;
        let first = (page * 10 + 1).to_string();
        let sfx = page.to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept-Language", "en-US,en;q=0.9")
            .query(&[
                ("q", query.query.as_str()),
                ("InfiniteScroll", "1"),
                ("first", first.as_str()),
                ("SFX", sfx.as_str()),
                ("form", "PTFTNR"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&text, query))
    }

    fn parse_html(&self, html: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let doc = Html::parse_document(html);
        let item_sel = match Selector::parse("div.newsitem, div[class*='newsitem']") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let title_sel = Selector::parse("a.title").unwrap();
        let snippet_sel = Selector::parse("div.snippet").unwrap();
        let img_sel = Selector::parse("a.imagelink img").unwrap();
        let mut results = Vec::new();
        let mut idx = 0;
        for el in doc.select(&item_sel) {
            let link = match el.select(&title_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let url = link.value().attr("href").unwrap_or("").to_string();
            if url.is_empty() {
                continue;
            }
            let title: String = link.text().collect::<Vec<_>>().join(" ");
            let snippet: String = el
                .select(&snippet_sel)
                .next()
                .map(|s| s.text().collect::<Vec<_>>().join(" "))
                .unwrap_or_default();

            let mut thumbnail = String::new();
            if let Some(img) = el.select(&img_sel).next() {
                if let Some(src) = img.value().attr("src") {
                    thumbnail = if src.starts_with("https://www.bing.com") || src.starts_with("http") {
                        src.to_string()
                    } else {
                        format!("https://www.bing.com/{}", src)
                    };
                }
            }
            let source = link.value().attr("data-author").unwrap_or("").to_string();

            results.push(
                SearchResult::new(title.trim().to_string(), url)
                    .with_snippet(snippet.trim().to_string())
                    .with_engine(self.name())
                    .with_rank(query.offset + idx + 1)
                    .with_score(1.0 - (idx as f64 * 0.05))
                    .with_result_type(ResultType::News)
                    .with_extra("source", serde_json::json!(source))
                    .with_extra("img_src", serde_json::json!(thumbnail))
                    .with_extra("thumbnail", serde_json::json!(thumbnail)),
            );
            idx += 1;
        }
        results
    }
}

#[async_trait]
impl Engine for BingNewsEngine {
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
        matches!(t, ResultType::News | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "base_url".into(),
            "https://www.bing.com/news/infinitescrollajax".into(),
        );
        s
    }
}
