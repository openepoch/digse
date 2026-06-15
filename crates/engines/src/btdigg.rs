//! BTDigg torrent search engine implementation.
//! HTML scrape of btdig.com.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// BTDigg torrent search engine.
pub struct BtdiggEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://btdig.com";

impl BtdiggEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "btdigg".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "BTDigg - BitTorrent DHT search engine.".to_string(),
            website: Some("https://btdig.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create BTDigg HTTP client");
        BtdiggEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let encoded = urlencoding::encode(&query.query);
        let pageno = query.offset.to_string();
        let url = format!(
            "{base}/search?q={q}&p={p}",
            base = BASE_URL,
            q = encoded,
            p = pageno,
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
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
        let item_sel = match Selector::parse("div.one_result") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let name_sel = Selector::parse("div.torrent_name a").unwrap();
        let excerpt_sel = Selector::parse("div.torrent_excerpt").unwrap();
        let size_sel = Selector::parse("span.torrent_size").unwrap();
        let files_sel = Selector::parse("span.torrent_files").unwrap();
        let magnet_sel = Selector::parse("div.torrent_magnet a").unwrap();

        let mut results = Vec::new();
        let mut idx = 0;
        for el in doc.select(&item_sel) {
            let link = match el.select(&name_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = link.value().attr("href").unwrap_or("").to_string();
            if href.is_empty() {
                continue;
            }
            let abs = if href.starts_with("http") {
                href
            } else {
                format!("{}/{}", BASE_URL.trim_end_matches('/'), href.trim_start_matches('/'))
            };
            let title: String = link.text().collect::<Vec<_>>().join(" ");
            let excerpt: String = el
                .select(&excerpt_sel)
                .next()
                .map(|e| {
                    e.text()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" | ")
                })
                .unwrap_or_default();
            let filesize: String = el
                .select(&size_sel)
                .next()
                .map(|s| s.text().collect::<Vec<_>>().join(""))
                .unwrap_or_default();
            let files: String = el
                .select(&files_sel)
                .next()
                .map(|s| s.text().collect::<Vec<_>>().join(""))
                .unwrap_or_else(|| "1".to_string());
            let magnet: String = el
                .select(&magnet_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .to_string();

            results.push(
                SearchResult::new(title, abs)
                    .with_snippet(excerpt)
                    .with_engine(self.name())
                    .with_rank(query.offset + idx + 1)
                    .with_score(1.0 - (idx as f64 * 0.05))
                    .with_result_type(ResultType::Files)
                    .with_extra("magnet", serde_json::json!(magnet))
                    .with_extra("filesize", serde_json::json!(filesize))
                    .with_extra("files", serde_json::json!(files)),
            );
            idx += 1;
        }
        results
    }
}

#[async_trait]
impl Engine for BtdiggEngine {
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
        matches!(t, ResultType::Files | ResultType::Torrents | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://btdig.com".into());
        s
    }
}
