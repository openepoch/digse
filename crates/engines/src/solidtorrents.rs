//! SolidTorrents search engine implementation
//!
//! Scrapes the
//! SolidTorrents HTML search results page.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// SolidTorrents torrent search engine
pub struct SolidTorrentsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://solidtorrents.to";

impl SolidTorrentsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "solidtorrents".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "SolidTorrents - torrent search engine.".to_string(),
            website: Some(BASE_URL.to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create SolidTorrents HTTP client");

        SolidTorrentsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = ((query.offset / 20) + 1).to_string();
        let resp = self
            .client
            .get(format!("{}/search", BASE_URL))
            .header("User-Agent", "digse/0.1.0")
            .query(&[("q", query.query.as_str()), ("page", page.as_str())])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(html, query))
    }

    fn parse_html(&self, html: String, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};

        let document = Html::parse_document(&html);
        let mut results = Vec::new();

        let li_sel = match Selector::parse("li.search-result, li[class*='search-result']") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let torrent_sel = Selector::parse("a[class*='dl-torrent']").unwrap();
        let magnet_sel = Selector::parse("a[class*='dl-magnet']").unwrap();
        let title_sel = Selector::parse("h5[class*='title']").unwrap();
        let title_a_sel = Selector::parse("h5[class*='title'] a").unwrap();
        let categ_sel = Selector::parse("a[class*='category']").unwrap();
        let stats_sel = Selector::parse("div[class*='stats'] div").unwrap();

        for (i, item) in document.select(&li_sel).enumerate() {
            let torrentfile = item
                .select(&torrent_sel)
                .next()
                .and_then(|a| a.value().attr("href").map(|s| s.to_string()));
            let magnet = item
                .select(&magnet_sel)
                .next()
                .and_then(|a| a.value().attr("href").map(|s| s.to_string()));

            // skip entries that aren't actual torrents (e.g. anime links)
            if torrentfile.is_none() || magnet.is_none() {
                continue;
            }

            let title = item
                .select(&title_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let rel_url = item
                .select(&title_a_sel)
                .next()
                .and_then(|a| a.value().attr("href").map(|s| s.to_string()))
                .unwrap_or_default();
            let url = if rel_url.starts_with("http") {
                rel_url.clone()
            } else {
                format!("{}{}", BASE_URL, rel_url)
            };

            let category = item
                .select(&categ_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let stats: Vec<String> = item
                .select(&stats_sel)
                .map(|s| s.text().collect::<String>().trim().to_string())
                .collect();

            // stats layout: [0]? [1]=filesize [2]=leech [3]=seed [4]=date
            let filesize = stats.get(1).cloned().unwrap_or_default();
            let leech = stats.get(2).cloned().unwrap_or_default();
            let seed = stats.get(3).cloned().unwrap_or_default();
            let published = stats.get(4).cloned().unwrap_or_default();

            if title.is_empty() || url.is_empty() {
                continue;
            }

            let snippet = format!(
                "{} | Size: {} | Seeders: {} | Leechers: {}",
                category, filesize, seed, leech
            );

            let mut result = SearchResult::new(title, url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Torrents);

            result = result.with_extra("magnet", serde_json::json!(magnet.unwrap()));
            if let Some(tf) = torrentfile {
                result = result.with_extra("torrentfile", serde_json::json!(tf));
            }
            if !seed.is_empty() {
                result = result.with_extra("seeders", serde_json::json!(seed));
            }
            if !leech.is_empty() {
                result = result.with_extra("leechers", serde_json::json!(leech));
            }
            if !filesize.is_empty() {
                result = result.with_extra("filesize", serde_json::json!(filesize));
            }
            if !published.is_empty() {
                result = result.with_extra("published", serde_json::json!(published));
            }
            if !category.is_empty() {
                result = result.with_extra("metadata", serde_json::json!(category));
            }

            results.push(result);
        }

        results
    }
}

#[async_trait]
impl Engine for SolidTorrentsEngine {
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
        matches!(t, ResultType::Torrents | ResultType::Files | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("results".into(), "HTML".into());
        s
    }
}
