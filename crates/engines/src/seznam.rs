//! Seznam search engine implementation
//!
//! Seznam is a Czech web
//! search engine; the reference fetches hidden form fields from the landing
//! page before issuing the actual search. digse issues the search directly.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Seznam search engine (general/web, HTML scrape)
pub struct SeznamEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl SeznamEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "seznam".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Seznam - Czech web search engine.".to_string(),
            website: Some("https://www.seznam.cz/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Seznam HTTP client");
        SeznamEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://search.seznam.cz";
        let url = format!("{}/?", base_url);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("q", query.query.as_str()),
                ("oq", query.query.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        // bail out if redirected to /verify (captcha)
        let final_url = resp.url().to_string();
        if final_url.contains("/verify") {
            return Ok(vec![]);
        }
        let html = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse(&html))
    }

    fn parse(&self, html: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // result containers
        let entry_sel = match Selector::parse(
            "#searchpage-root div.Layout--left div.f2c528",
        ) {
            Ok(s) => s,
            Err(_) => return results,
        };
        let data_sel = match Selector::parse("div.c8774a, div.e69e8d.a11657") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let h3_sel = Selector::parse("h3 a").unwrap();

        for entry in document.select(&entry_sel) {
            // skip entries without a content body
            if entry.select(&data_sel).next().is_none() {
                continue;
            }
            let title_el = match entry.select(&h3_sel).next() {
                Some(t) => t,
                None => continue,
            };
            let href = title_el.value().attr("href").unwrap_or("").to_string();
            let title = title_el.text().collect::<String>().trim().to_string();
            let content = entry
                .select(&data_sel)
                .next()
                .map(|d| d.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if href.is_empty() && title.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(title, href)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for SeznamEngine {
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
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://search.seznam.cz/".into());
        s
    }
}
