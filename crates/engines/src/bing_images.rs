//! Bing Images search engine implementation.
//! Scrapes bing.com/images/async.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Bing Images search engine.
pub struct BingImagesEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl BingImagesEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bing_images".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Bing Images - Microsoft's image search.".to_string(),
            website: Some("https://www.bing.com/images".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Bing Images HTTP client");
        BingImagesEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://www.bing.com/images/async";
        let first = ((query.offset) * 35 + 1).to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header(
                "Accept-Language",
                "en-US,en;q=0.9",
            )
            .query(&[
                ("q", query.query.as_str()),
                ("async", "1"),
                ("first", first.as_str()),
                ("count", "35"),
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
        let li_sel = match Selector::parse("ul.dgControl_list li, ul[class*='dgControl_list'] li")
        {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let a_sel = Selector::parse("a.iusc").unwrap();
        let mut results = Vec::new();
        let mut idx = 0;
        for el in doc.select(&li_sel) {
            // find the a.iusc with metadata `m`
            let anchor = el.select(&a_sel).next();
            let m_json = match anchor.and_then(|a| a.value().attr("m")) {
                Some(m) => m,
                None => continue,
            };
            let meta: serde_json::Value = match serde_json::from_str(m_json) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let purl = meta
                .get("purl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let murl = meta
                .get("murl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let turl = meta
                .get("turl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let desc = meta
                .get("desc")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            if purl.is_empty() && murl.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(desc.clone(), purl.clone())
                    .with_snippet(desc)
                    .with_engine(self.name())
                    .with_rank(query.offset + idx + 1)
                    .with_score(1.0 - (idx as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(murl))
                    .with_extra("thumbnail", serde_json::json!(turl))
                    .with_extra("source", serde_json::json!("bing")),
            );
            idx += 1;
        }
        results
    }
}

#[async_trait]
impl Engine for BingImagesEngine {
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
        s.insert("base_url".into(), "https://www.bing.com/images/async".into());
        s
    }
}
