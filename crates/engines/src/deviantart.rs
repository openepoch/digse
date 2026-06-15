//! Deviantart (images) search engine implementation
//!
//! Scrapes deviantart.com search results (HTML). DeviantArt's official API
//! requires OAuth; if `DEVART_API_KEY` is provided we could use it, but the
//! reference scrapes HTML, so we follow that path.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Deviantart image search engine
pub struct DeviantartEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl DeviantartEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "deviantart".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "DeviantArt images.".to_string(),
            website: Some("https://www.deviantart.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Deviantart HTTP client");

        DeviantartEngine { metadata, client }
    }

    fn parse_html(&self, html: &str) -> Vec<(String, String, String, String)> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();

        // Reference uses '//div[@class="V_S0t_"]/div/div/a' but DeviantArt's
        // class names rotate; fall back to <a> elements with aria-label and img.
        let candidates = ["div.V_S0t_ a[aria-label]", "a[aria-label][href]"];
        for sel_str in candidates {
            let sel = match Selector::parse(sel_str) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let img_sel = Selector::parse("img").unwrap();
            for el in doc.select(&sel) {
                let url = el.value().attr("href").unwrap_or("").to_string();
                let title = el
                    .value()
                    .attr("aria-label")
                    .unwrap_or("")
                    .to_string();
                let img_src = el
                    .select(&img_sel)
                    .next()
                    .and_then(|i| {
                        i.value()
                            .attr("srcset")
                            .or_else(|| i.value().attr("src"))
                    })
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                // Take the first URL from srcset if present
                let img_src = img_src.split_whitespace().next().unwrap_or("").to_string();
                let thumb = el
                    .select(&img_sel)
                    .next()
                    .and_then(|i| i.value().attr("src").map(|s| s.to_string()))
                    .unwrap_or_default();
                if !url.is_empty() && !title.is_empty() {
                    out.push((title, url, img_src, thumb));
                }
            }
            if !out.is_empty() {
                break;
            }
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://www.deviantart.com/search";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html")
            .query(&[("q", query.query.as_str())])
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

        let parsed = self.parse_html(&text);
        let mut results = Vec::new();
        for (i, (title, url, img_src, thumb)) in parsed.iter().enumerate() {
            let mut result = SearchResult::new(title.clone(), url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images);
            if !img_src.is_empty() {
                result = result.with_extra("img_src", serde_json::json!(img_src));
            }
            if !thumb.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumb));
            }
            result = result.with_extra("source", serde_json::json!("deviantart"));
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for DeviantartEngine {
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
        matches!(result_type, ResultType::Images | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert(
            "base_url".to_string(),
            "https://www.deviantart.com".to_string(),
        );
        settings.insert("api_key_env".to_string(), "DEVART_API_KEY".to_string());
        settings
    }
}
