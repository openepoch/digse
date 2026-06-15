//! 360Search search engine implementation (HTML scrape, Chinese web search)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// 360Search (so.com) search engine
pub struct Search360Engine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl Search360Engine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "360search".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "360Search - Chinese web search (so.com).".to_string(),
            website: Some("https://www.so.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create 360Search HTTP client");

        Search360Engine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.so.com";
        let pageno = ((query.offset / 10) + 1).to_string();

        let resp = self.client
            .get(format!("{}/s", base_url))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("pn", pageno.as_str()),
                ("q", query.query.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html))
    }

    fn parse_html(&self, html: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let item_sel = match Selector::parse("li.res-list, li[class*='res-list']") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let title_sel = Selector::parse("h3.res-title a, h3 a").unwrap();
        let desc_sel = Selector::parse("p.res-desc, span.res-list-summary").unwrap();

        for el in document.select(&item_sel) {
            let a = match el.select(&title_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let url = a.value().attr("data-mdurl")
                .or_else(|| a.value().attr("href"))
                .unwrap_or("")
                .to_string();
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() || url.is_empty() {
                continue;
            }
            let content = el.select(&desc_sel).next()
                .map(|d| d.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for Search360Engine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://www.so.com".to_string());
        s
    }
}
