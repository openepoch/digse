//! Geizhals search engine implementation
//!
//! HTML scrape of
//! `https://geizhals.de/?fs=...&pg=N` collecting
//! `article.listview__item` elements. Category: shopping.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Geizhals German price-comparison search engine (HTML scrape)
pub struct GeizhalsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl GeizhalsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "geizhals".to_string(),
            category: EngineCategory::Shopping,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Geizhals - German price comparison.".to_string(),
            website: Some("https://geizhals.de".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Geizhals HTTP client");

        GeizhalsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://geizhals.de";
        // Strip optional "sort:X" modifier (ref SORT_RE). We keep the query as-is
        // for the search term if no recognized sort token is present.
        let (clean_query, sort) = parse_sort(&query.query);
        let page = query.offset + 1;
        let page_str = page.to_string();

        let mut args: Vec<(&str, String)> = vec![
            ("fs", clean_query.clone()),
            ("pg", page_str),
            ("toggle_all", "1".to_string()),
        ];
        if let Some(s) = sort {
            args.push(("sort", s));
        }

        let response = self
            .client
            .get(format!("{}/", base_url))
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html,application/xhtml+xml")
            .query(&args)
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
        let item_sel = match Selector::parse("article[class*='listview__item']") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let name_sel = Selector::parse("h3[class*='listview__name']").unwrap();
        let link_sel = Selector::parse("a[class*='listview__name-link']").unwrap();
        let img_sel = Selector::parse("img[class*='listview__image']").unwrap();
        let price_sel = Selector::parse("a[class*='listview__price-link']").unwrap();
        let rating_sel = Selector::parse("div[class*='stars-rating-label']").unwrap();
        let offercount_sel = Selector::parse("div[class*='listview__offercount']").unwrap();
        let spec_sel = Selector::parse("div[class*='specs-grid__item']").unwrap();
        let dt_sel = Selector::parse("dt").unwrap();
        let dd_sel = Selector::parse("dd").unwrap();

        let mut results = Vec::new();
        for (i, el) in doc.select(&item_sel).enumerate() {
            if i >= query.count {
                break;
            }
            let href = el
                .select(&link_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .to_string();
            let title = el
                .select(&name_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let url = format!("{}/{}", base_url, href.trim_start_matches('/'));
            let thumbnail = el
                .select(&img_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .unwrap_or("")
                .to_string();

            let mut content_parts = Vec::new();
            for spec in el.select(&spec_sel) {
                let dt = spec
                    .select(&dt_sel)
                    .next()
                    .map(|t| t.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                let dd = spec
                    .select(&dd_sel)
                    .next()
                    .map(|t| t.text().collect::<String>().trim().to_string())
                    .unwrap_or_default();
                if !dt.is_empty() || !dd.is_empty() {
                    content_parts.push(format!("{}: {}", dt, dd));
                }
            }

            let rating = el
                .select(&rating_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let offercount = el
                .select(&offercount_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let mut metadata_parts = Vec::new();
            if !rating.is_empty() {
                metadata_parts.push(rating);
            }
            if !offercount.is_empty() {
                metadata_parts.push(offercount);
            }

            let price_raw = el
                .select(&price_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            // ref: split on whitespace, take 2nd token as the price value
            let price = price_raw
                .split_whitespace()
                .nth(1)
                .map(|p| format!("Bestes Angebot: {}€", p))
                .unwrap_or_default();

            let mut snippet = content_parts.join(" | ");
            if !metadata_parts.is_empty() {
                if !snippet.is_empty() {
                    snippet.push_str(" | ");
                }
                snippet.push_str(&metadata_parts.join(", "));
            }

            let mut result = SearchResult::new(title, url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Shopping)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("source", serde_json::json!("geizhals"));
            if !price.is_empty() {
                result = result.with_extra("price", serde_json::json!(price));
            }
            results.push(result);
        }

        Ok(results)
    }
}

// Parse optional `sort:<key>` token from the query. Ref SORT_RE: r"sort:(\w+)"
// mapped through sort_order_map (relevance->None, price/asc->p, desc->-p).
fn parse_sort(query: &str) -> (String, Option<String>) {
    let marker = "sort:";
    if let Some(pos) = query.find(marker) {
        let after = &query[pos + marker.len()..];
        let key: String = after.chars().take_while(|c| c.is_alphanumeric()).collect();
        let cleaned: String = format!("{}{}", &query[..pos], &query[pos + marker.len() + key.len()..]);
        let cleaned = cleaned.trim().to_string();
        let mapped = match key.as_str() {
            "relevance" => None,
            "price" | "asc" => Some("p".to_string()),
            "desc" => Some("-p".to_string()),
            _ => None,
        };
        return (cleaned, mapped);
    }
    (query.to_string(), None)
}

#[async_trait]
impl Engine for GeizhalsEngine {
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
        settings.insert("base_url".to_string(), "https://geizhals.de".to_string());
        settings.insert("sort_order".to_string(), "relevance".to_string());
        settings
    }
}
