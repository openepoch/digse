//! Kickass Torrents search engine implementation
//!
//! HTML scrape of the torrent search
//! results table. Results sorted by seeder count (descending).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// Kickass Torrents search engine
pub struct KickassEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

struct KickassRow {
    result: SearchResult,
    seed: i64,
}

impl KickassEngine {
    pub fn new() -> Self {
        Self::with_base_url("https://kickasstorrents.to")
    }

    pub fn with_base_url(base_url: &str) -> Self {
        let metadata = EngineMetadata {
            name: "kickass".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Kickass Torrents - torrent search.".to_string(),
            website: Some("https://kickasstorrents.to".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Kickass HTTP client");
        KickassEngine {
            metadata,
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = (query.offset / query.count.max(1)) + 1;
        let url = format!(
            "{}/usearch/{}/{}",
            self.base_url,
            urlencoding::encode(&query.query),
            pageno
        );

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

        let text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let doc = Html::parse_document(&text);
        let row_sel = match Selector::parse("table[class*='data'] tr") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let link_sel = Selector::parse("a[class*='cellMainLink']").unwrap();
        let content_sel = Selector::parse("span.font11px.lightgrey.block").unwrap();
        let seed_sel = Selector::parse("td[class*='green']").unwrap();
        let leech_sel = Selector::parse("td[class*='red']").unwrap();
        let size_sel = Selector::parse("td[class*='nobr']").unwrap();

        let mut rows: Vec<KickassRow> = Vec::new();
        for el in doc.select(&row_sel) {
            // skip rows without a main link (header row)
            let a = match el.select(&link_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("");
            if href.is_empty() {
                continue;
            }
            let url = if href.starts_with("http") {
                href.to_string()
            } else {
                format!("{}{}", self.base_url, href)
            };
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let content = el
                .select(&content_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let seed = el
                .select(&seed_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().parse::<i64>().unwrap_or(0))
                .unwrap_or(0);
            let leech = el
                .select(&leech_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().parse::<i64>().unwrap_or(0))
                .unwrap_or(0);
            let filesize = el
                .select(&size_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_score(seed.max(1) as f64)
                .with_result_type(ResultType::Torrents)
                .with_extra("seeders", serde_json::json!(seed))
                .with_extra("leechers", serde_json::json!(leech))
                .with_extra("filesize", serde_json::json!(filesize))
                .with_extra("source", serde_json::json!("kickass"));
            rows.push(KickassRow { result, seed });
        }

        // sort by seed descending (as in Python ref)
        rows.sort_by(|a, b| b.seed.cmp(&a.seed));

        let mut results = Vec::new();
        for (i, row) in rows.into_iter().enumerate() {
            if i >= query.count {
                break;
            }
            // re-apply rank after sorting
            results.push(row.result.with_rank(query.offset + i + 1));
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for KickassEngine {
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
        s.insert("base_url".into(), self.base_url.clone());
        s
    }
}
