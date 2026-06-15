//! Nyaa search engine implementation (anime torrents; HTML scrape)
//!
//! Nyaa.si is an anime BitTorrent tracker.
//! The reference implementation scrapes the torrent list table.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Nyaa torrent search engine
pub struct NyaaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl NyaaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "nyaa".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Nyaa.si - Anime BitTorrent tracker.".to_string(),
            website: Some("https://nyaa.si/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Nyaa HTTP client");

        NyaaEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://nyaa.si";
        let page = (query.offset / 10) + 1;
        let page_str = page.to_string();

        let resp = self
            .client
            .get(base_url)
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("q", query.query.as_str()),
                ("p", page_str.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html, query))
    }

    fn parse_html(&self, html: &str, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // rows in the torrent table (skip header rows)
        let row_sel = match Selector::parse("table.torrent-list tr") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let category_sel = Selector::parse("td:nth-child(1) a").unwrap();
        let title_cell_sel = Selector::parse("td:nth-child(2)").unwrap();
        let links_sel = Selector::parse("td:nth-child(3) a").unwrap();
        let filesize_sel = Selector::parse("td:nth-child(4)").unwrap();
        let seeds_sel = Selector::parse("td:nth-child(6)").unwrap();
        let leeches_sel = Selector::parse("td:nth-child(7)").unwrap();
        let downloads_sel = Selector::parse("td:nth-child(8)").unwrap();
        let title_a_sel = Selector::parse("a").unwrap();

        for (i, row) in document.select(&row_sel).enumerate() {
            // skip header rows (th)
            let has_th = row.select(&Selector::parse("th").unwrap()).next().is_some();
            if has_th {
                continue;
            }

            // category
            let category = row
                .select(&category_sel)
                .next()
                .and_then(|a| a.value().attr("title"))
                .unwrap_or("")
                .to_string();

            // title + page href from last <a> in the 2nd cell
            let title_cell = match row.select(&title_cell_sel).next() {
                Some(c) => c,
                None => continue,
            };
            let page_a = match title_cell.select(&title_a_sel).last() {
                Some(a) => a,
                None => continue,
            };
            let title = page_a.text().collect::<String>().trim().to_string();
            let href = page_a.value().attr("href").unwrap_or("").to_string();
            if title.is_empty() || href.is_empty() {
                continue;
            }
            let page_url = if href.starts_with("http") {
                href
            } else {
                format!("https://nyaa.si{}", href)
            };

            // magnet + torrent links
            let mut magnet_link = String::new();
            let mut torrent_link = String::new();
            for link in row.select(&links_sel) {
                if let Some(u) = link.value().attr("href") {
                    if u.contains("magnet") {
                        magnet_link = u.to_string();
                    } else {
                        torrent_link = u.to_string();
                    }
                }
            }

            let seed = row
                .select(&seeds_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let leech = row
                .select(&leeches_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let downloads = row
                .select(&downloads_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let filesize = row
                .select(&filesize_sel)
                .next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let content = format!("Category: \"{}\". Downloaded {} times.", category, downloads);

            results.push(
                SearchResult::new(&title, &page_url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Torrents)
                    .with_extra("seeders", serde_json::json!(seed))
                    .with_extra("leechers", serde_json::json!(leech))
                    .with_extra("filesize", serde_json::json!(filesize))
                    .with_extra("magnet", serde_json::json!(magnet_link))
                    .with_extra("torrentfile", serde_json::json!(torrent_link)),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for NyaaEngine {
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
        s.insert("base_url".to_string(), "https://nyaa.si".to_string());
        s
    }
}
