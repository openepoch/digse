//! Bandcamp search engine implementation (HTML, music)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Bandcamp music search engine
pub struct BandcampEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl BandcampEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bandcamp".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Bandcamp - music search & discovery.".to_string(),
            website: Some("https://bandcamp.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Bandcamp HTTP client");

        BandcampEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://bandcamp.com";
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
        Ok(self.parse_html(&html))
    }

    fn parse_html(&self, html: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let li_sel = match Selector::parse("li.searchresult, li[class*='searchresult']") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let url_sel = Selector::parse("div.itemurl a").unwrap();
        let title_sel = Selector::parse("div.heading a").unwrap();
        let subhead_sel = Selector::parse("div.subhead").unwrap();
        let released_sel = Selector::parse("div.released").unwrap();
        let art_sel = Selector::parse("div.art img").unwrap();
        let itemtype_sel = Selector::parse("div.itemtype").unwrap();

        for el in document.select(&li_sel) {
            let a = match el.select(&url_sel).next() {
                Some(a) => a,
                None => continue,
            };
            // The URL text is the result's destination; the href carries the
            // search_item_id used to build the embed iframe.
            let url = a.text().collect::<String>().trim().to_string();
            if url.is_empty() {
                continue;
            }
            let title = el.select(&title_sel).next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let content = el.select(&subhead_sel).next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let released = el.select(&released_sel).next()
                .map(|r| r.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = el.select(&art_sel).next()
                .and_then(|i| i.value().attr("src").map(|s| s.to_string()))
                .unwrap_or_default();
            let itemtype = el.select(&itemtype_sel).next()
                .map(|t| t.text().collect::<String>().trim().to_lowercase())
                .unwrap_or_default();

            // Resolve search_item_id from the link href's query string.
            let result_id = a.value().attr("href")
                .and_then(extract_search_item_id)
                .unwrap_or_default();
            let iframe_src = if result_id.is_empty() {
                String::new()
            } else {
                match itemtype.as_str() {
                    "album" => format!(
                        "https://bandcamp.com/EmbeddedPlayer/album={}/size=large/bgcol=000/linkcol=fff/artwork=small",
                        result_id
                    ),
                    "track" => format!(
                        "https://bandcamp.com/EmbeddedPlayer/track={}/size=large/bgcol=000/linkcol=fff/artwork=small",
                        result_id
                    ),
                    _ => String::new(),
                }
            };

            let r = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_result_type(ResultType::Music)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("published", serde_json::json!(released))
                .with_extra("audio_src", serde_json::json!(iframe_src))
                .with_extra("iframe_src", serde_json::json!(iframe_src));
            results.push(r);
        }
        results
    }
}

/// Extract the `search_item_id` value from a query string like
/// `?search_item_id=12345&...`.
fn extract_search_item_id(href: &str) -> Option<String> {
    let q = href.split('?').nth(1)?;
    for pair in q.split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next()? == "search_item_id" {
            return it.next().map(|s| s.to_string());
        }
    }
    None
}

#[async_trait]
impl Engine for BandcampEngine {
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
        matches!(t, ResultType::Music | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://bandcamp.com".to_string());
        s
    }
}
