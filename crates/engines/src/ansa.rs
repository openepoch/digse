//! ANSA news search engine implementation (HTML)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// ANSA (Italian news agency) search engine
pub struct AnsaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl AnsaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "ansa".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "ANSA - Italy's oldest news agency.".to_string(),
            website: Some("https://www.ansa.it".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create ANSA HTTP client");

        AnsaEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.ansa.it";
        let start = (query.offset * 12).to_string();

        let resp = self.client
            .get("https://www.ansa.it/ricerca/ansait/search.shtml")
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("any", query.query.as_str()),
                ("start", start.as_str()),
                ("sort", "data:desc"),
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

        let art_sel = match Selector::parse("div.article") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let title_a_sel = Selector::parse("div.content h2.title a").unwrap();
        let text_sel = Selector::parse("div.content div.text").unwrap();
        let img_sel = Selector::parse("div.image a img").unwrap();

        for el in document.select(&art_sel) {
            let a = match el.select(&title_a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").to_string();
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let url = if href.starts_with("http") { href } else { format!("{}{}", base_url, href) };
            let content = el.select(&text_sel).next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = el.select(&img_sel).next()
                .and_then(|i| i.value().attr("src").map(|s| {
                    let s = s.to_string();
                    if s.starts_with("http") { s } else { format!("{}{}", base_url, s) }
                }))
                .unwrap_or_default();

            let r = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_result_type(ResultType::News)
                .with_extra("source", serde_json::json!("ansa"))
                .with_extra("img_src", serde_json::json!(thumbnail));
            results.push(r);
        }
        results
    }
}

#[async_trait]
impl Engine for AnsaEngine {
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
        matches!(t, ResultType::News | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://www.ansa.it".to_string());
        s
    }
}
