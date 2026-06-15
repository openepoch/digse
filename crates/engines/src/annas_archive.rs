//! Anna's Archive search engine implementation (HTML, files/books)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Anna's Archive book/library search engine
pub struct AnnasArchiveEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl AnnasArchiveEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "annas_archive".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "Anna's Archive - shadow library metasearch for books and articles.".to_string(),
            website: Some("https://annas-archive.gl".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create Anna's Archive HTTP client");

        AnnasArchiveEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://annas-archive.org";
        let page = ((query.offset / 10) + 1).to_string();

        let resp = self.client
            .get(format!("{}/search", base_url))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html, base_url))
    }

    fn parse_html(&self, html: &str, base_url: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // Each result is a `div.flex` inside the `js-aarecord-list-outer` container.
        let item_sel = match Selector::parse(
            "main div[class*='js-aarecord-list-outer'] div.flex"
        ) {
            Ok(s) => s,
            Err(_) => return results,
        };
        let a_sel = Selector::parse("a").unwrap();
        let title_sel = Selector::parse("a.js-vim-focus").unwrap();
        let img_sel = Selector::parse("img").unwrap();

        for el in document.select(&item_sel) {
            // first child <a> holds the result href
            let href = match el.select(&a_sel).next() {
                Some(a) => a.value().attr("href").unwrap_or("").to_string(),
                None => continue,
            };
            let title = match el.select(&title_sel).next() {
                Some(t) => t.text().collect::<String>().trim().to_string(),
                None => el.select(&a_sel).next()
                    .map(|a| a.text().collect::<String>().trim().to_string())
                    .unwrap_or_default(),
            };
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let url = if href.starts_with("http") { href } else { format!("{}{}", base_url, href) };
            let thumbnail = el.select(&img_sel).next()
                .and_then(|i| i.value().attr("src").map(|s| s.to_string()))
                .unwrap_or_default();

            let r = SearchResult::new(title, url)
                .with_engine(self.name())
                .with_result_type(ResultType::Files)
                .with_extra("thumbnail", serde_json::json!(thumbnail));
            results.push(r);
        }
        results
    }
}

#[async_trait]
impl Engine for AnnasArchiveEngine {
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
        matches!(t, ResultType::Files | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://anns-archive.org".to_string());
        s
    }
}
