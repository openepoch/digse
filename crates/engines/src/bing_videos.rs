//! Bing Videos search engine implementation.
//! Scrapes bing.com/videos/asyncv2.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Bing Videos search engine.
pub struct BingVideosEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl BingVideosEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bing_videos".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Bing Videos - Microsoft's video search.".to_string(),
            website: Some("https://www.bing.com/videos".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Bing Videos HTTP client");
        BingVideosEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://www.bing.com/videos/asyncv2";
        let first = (query.offset * 35 + 1).to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept-Language", "en-US,en;q=0.9")
            .query(&[
                ("q", query.query.as_str()),
                ("async", "content"),
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
        let item_sel = match Selector::parse("div[id*='mc_vtvc_video']") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let vrh_sel = Selector::parse("div.vrhdata").unwrap();
        let meta_sel = Selector::parse("div.mc_vtvc_meta_block span").unwrap();
        let img_sel = Selector::parse("img.rms_img, img[class*='rms']").unwrap();
        let mut results = Vec::new();
        let mut idx = 0;
        for el in doc.select(&item_sel) {
            let vrhm = match el.select(&vrh_sel).next().and_then(|v| v.value().attr("vrhm")) {
                Some(m) => m,
                None => continue,
            };
            let meta: serde_json::Value = match serde_json::from_str(vrhm) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let murl = meta
                .get("murl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if murl.is_empty() {
                continue;
            }
            let title = meta
                .get("vt")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let duration = meta
                .get("du")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let info: Vec<String> = el
                .select(&meta_sel)
                .map(|s| s.text().collect::<Vec<_>>().join(" "))
                .collect();

            let mut thumbnail = String::new();
            if let Some(img) = el.select(&img_sel).next() {
                if let Some(src) = img
                    .value()
                    .attr("data-src-hq")
                    .or_else(|| img.value().attr("src"))
                {
                    thumbnail = src.to_string();
                }
            }

            results.push(
                SearchResult::new(title.clone(), murl)
                    .with_snippet(info.join(" - "))
                    .with_engine(self.name())
                    .with_rank(query.offset + idx + 1)
                    .with_score(1.0 - (idx as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("duration", serde_json::json!(duration)),
            );
            idx += 1;
        }
        results
    }
}

#[async_trait]
impl Engine for BingVideosEngine {
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
        matches!(t, ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://www.bing.com/videos/asyncv2".into());
        s
    }
}
