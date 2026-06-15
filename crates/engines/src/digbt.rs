//! DigBT (torrents) search engine implementation
//!
//! Scrapes digbt.org search results for
//! torrent magnet links. Categories: videos/music/files.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// DigBT torrent search engine
pub struct DigbtEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl DigbtEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "digbt".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "DigBT - torrent search.".to_string(),
            website: Some("https://digbt.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create DigBT HTTP client");

        DigbtEngine { metadata, client }
    }

    /// Join a relative href onto the digbt base URL.
    fn join_url(href: &str) -> String {
        if href.starts_with("http") {
            href.to_string()
        } else if href.starts_with('/') {
            format!("https://digbt.org{}", href)
        } else {
            format!("https://digbt.org/{}", href)
        }
    }

    fn parse_html(&self, html: &str) -> Vec<DigbtRow> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();

        let item_sel = match Selector::parse("td.x-item") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let a_title_sel = Selector::parse("a[title]").unwrap();
        let files_sel = Selector::parse("div.files").unwrap();
        let tail_sel = Selector::parse("div.tail").unwrap();
        let magnet_sel = Selector::parse("a.title").unwrap();

        for el in doc.select(&item_sel) {
            let a = el.select(&a_title_sel).next();
            let (url, title) = if let Some(a) = a {
                let href = a.value().attr("href").unwrap_or("").to_string();
                let title = a.value().attr("title").unwrap_or("").to_string();
                if title.is_empty() {
                    let title_text = a.text().collect::<String>().trim().to_string();
                    (Self::join_url(&href), title_text)
                } else {
                    (Self::join_url(&href), title)
                }
            } else {
                continue;
            };
            if title.is_empty() && url.is_empty() {
                continue;
            }
            let content = el
                .select(&files_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let tail_text = el
                .select(&tail_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            // Parse filesize: tokens[3] and tokens[4] (e.g. "... size 1.5 GB ...")
            let tokens: Vec<&str> = tail_text.split_whitespace().collect();
            let filesize = if tokens.len() >= 5 {
                format!("{} {}", tokens[3], tokens[4])
            } else {
                String::new()
            };
            let magnet = el
                .select(&magnet_sel)
                .next()
                .and_then(|a| a.value().attr("href").map(|s| s.to_string()))
                .unwrap_or_default();

            out.push(DigbtRow {
                url,
                title,
                content,
                filesize,
                magnet,
            });
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = ((query.offset / 10) + 1).to_string();
        // Reference URL: https://digbt.org/search/{query}-time-{pageno}
        let encoded = urlencoding::encode(&query.query);
        let url = format!(
            "https://digbt.org/search/{}-time-{}",
            encoded, pageno
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "text/html")
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
        for (i, row) in parsed.iter().enumerate() {
            let title = if row.title.is_empty() {
                "DigBT result".to_string()
            } else {
                row.title.clone()
            };
            let mut result = SearchResult::new(title, row.url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Torrents);
            if !row.content.is_empty() {
                result = result.with_snippet(row.content.clone());
            }
            if !row.filesize.is_empty() {
                result = result.with_extra("filesize", serde_json::json!(row.filesize));
            }
            if !row.magnet.is_empty() {
                result = result.with_extra("magnet", serde_json::json!(row.magnet));
            }
            result = result
                .with_extra("seeders", serde_json::json!("N/A"))
                .with_extra("leechers", serde_json::json!("N/A"));
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

struct DigbtRow {
    url: String,
    title: String,
    content: String,
    filesize: String,
    magnet: String,
}

#[async_trait]
impl Engine for DigbtEngine {
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
        matches!(
            result_type,
            ResultType::Torrents | ResultType::Videos | ResultType::Music | ResultType::Files | ResultType::All
        )
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://digbt.org".to_string());
        settings
    }
}
