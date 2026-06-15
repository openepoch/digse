//! 1337x search engine implementation
//!
//! HTML scrape of the torrent index.
//! Each result row yields url/title/seeders/leechers/filesize.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// 1337x torrent search engine
pub struct X1337Engine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl X1337Engine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "1337x".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "1337x - torrent search.".to_string(),
            website: Some("https://1337x.to/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create 1337x HTTP client");

        X1337Engine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://1337x.to";
        let pageno = (query.offset / query.count.max(1)) + 1;
        let encoded = urlencoding::encode(&query.query);
        let url = format!("{}/search/{}/{}/", base_url, encoded, pageno);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.1.0)")
            .header("Accept", "text/html,application/xhtml+xml")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        let doc = Html::parse_document(&text);
        let row_sel = match Selector::parse("table.table-list tbody tr") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let name_cell_sel = Selector::parse("td.name").unwrap();
        let a_sel = Selector::parse("a").unwrap();
        let seeds_sel = Selector::parse("td.seeds").unwrap();
        let leeches_sel = Selector::parse("td.leeches").unwrap();
        let size_sel = Selector::parse("td.size").unwrap();

        let mut results = Vec::new();
        for (i, row) in doc.select(&row_sel).enumerate() {
            if results.len() >= query.count {
                break;
            }
            // 2nd anchor inside td.name holds the detail link + title.
            let name_cell = match row.select(&name_cell_sel).next() {
                Some(c) => c,
                None => continue,
            };
            let second_a = match name_cell.select(&a_sel).nth(1) {
                Some(a) => a,
                None => continue,
            };
            let href = second_a.value().attr("href").unwrap_or("");
            if href.is_empty() {
                continue;
            }
            let detail_url = urljoin(base_url, href);
            let title = second_a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let seeds = text_of(row.select(&seeds_sel).next());
            let leech = text_of(row.select(&leeches_sel).next());
            let filesize = text_of(row.select(&size_sel).next());

            let result = SearchResult::new(title, detail_url)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Torrents)
                .with_extra("seeders", serde_json::json!(seeds))
                .with_extra("leechers", serde_json::json!(leech))
                .with_extra("filesize", serde_json::json!(filesize))
                .with_extra("source", serde_json::json!("1337x"));
            results.push(result);
        }
        Ok(results)
    }
}

/// Join a base URL with a possibly-relative href, handling absolute hrefs.
fn urljoin(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else if let Some(rest) = href.strip_prefix("//") {
        format!("https:{}", rest)
    } else if let Some(path) = href.strip_prefix('/') {
        format!("{}/{}", base.trim_end_matches('/'), path)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), href)
    }
}

/// Concatenate the direct text content of an optional element.
fn text_of(el: Option<scraper::ElementRef>) -> String {
    match el {
        Some(e) => e.text().collect::<String>().trim().to_string(),
        None => String::new(),
    }
}

#[async_trait]
impl Engine for X1337Engine {
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
        s.insert("base_url".to_string(), "https://1337x.to".to_string());
        s.insert("search_url".to_string(), "/search/{q}/{pageno}/".to_string());
        s
    }
}
