//! eBay search engine implementation
//!
//! HTML scrape of the eBay search results
//! page using the `s-item` li items. Category: shopping.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// eBay shopping search engine (HTML scrape)
pub struct EBayEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl EBayEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "ebay".to_string(),
            category: EngineCategory::Shopping,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "eBay - online marketplace / shopping search.".to_string(),
            website: Some("https://www.ebay.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create eBay HTTP client");

        EBayEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.ebay.com";
        // pageno in ref is offset by 1; ref uses _sacat={pageno}
        let pageno = query.offset + 1;
        let encoded = urlencoding::encode(&query.query).to_string();
        let url = format!(
            "{}/sch/i.html?_nkw={}&_sacat={}",
            base_url, encoded, pageno
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html,application/xhtml+xml")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let html = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let doc = Html::parse_document(&html);
        let item_sel = match Selector::parse("li.s-item") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let link_sel = Selector::parse("a.s-item__link").unwrap();
        let title_sel = Selector::parse("h3.s-item__title").unwrap();
        let price_sel = Selector::parse("span.s-item__price").unwrap();
        let shipping_sel = Selector::parse("span.s-item__shipping").unwrap();
        let loc_sel = Selector::parse("span.s-item__location").unwrap();
        let img_sel = Selector::parse("img.s-item__image-img").unwrap();

        let mut results = Vec::new();
        for (i, el) in doc.select(&item_sel).enumerate() {
            if i >= query.count {
                break;
            }
            let url = el
                .select(&link_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .to_string();
            let title = el
                .select(&title_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() || title == "Shop on eBay" {
                continue;
            }
            let price = el
                .select(&price_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let shipping = el
                .select(&shipping_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let source_country = el
                .select(&loc_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = el
                .select(&img_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .unwrap_or("")
                .to_string();

            let mut content_parts = Vec::new();
            if !price.is_empty() {
                content_parts.push(price.clone());
            }
            if !shipping.is_empty() {
                content_parts.push(shipping);
            }
            if !source_country.is_empty() {
                content_parts.push(source_country);
            }

            let result = SearchResult::new(title, url)
                .with_snippet(content_parts.join(" | "))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Shopping)
                .with_extra("price", serde_json::json!(price))
                .with_extra("source", serde_json::json!("ebay"))
                .with_extra("thumbnail", serde_json::json!(thumbnail));

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for EBayEngine {
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
        matches!(result_type, ResultType::Shopping | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://www.ebay.com".to_string());
        settings.insert("search_url".to_string(), "/sch/i.html".to_string());
        settings
    }
}
