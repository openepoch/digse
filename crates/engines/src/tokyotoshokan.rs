//! Tokyo Toshokan search engine implementation
//!
//! Scrapes the `search.php` results table.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Tokyo Toshokan torrent search engine
pub struct TokyoToshokanEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://www.tokyotosho.info";

impl TokyoToshokanEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "tokyotoshokan".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Tokyo Toshokan - Japanese media torrent search.".to_string(),
            website: Some("https://www.tokyotosho.info/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Tokyo Toshokan HTTP client");

        TokyoToshokanEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = ((query.offset / 30) + 1).to_string();
        let resp = self
            .client
            .get(format!("{}/search.php", BASE_URL))
            .header("User-Agent", "digse/0.1.0")
            .query(&[("page", page.as_str()), ("terms", query.query.as_str())])
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

        let row_sel = match Selector::parse("table.listing tr.category_0") {
            Ok(s) => s,
            Err(_) => return results,
        };

        // collect rows; the page lays out two rows per result.
        let rows: Vec<_> = document.select(&row_sel).collect();
        if rows.is_empty() || rows.len() % 2 != 0 {
            return results;
        }

        let desc_top_a_sel = Selector::parse("td.desc-top a").unwrap();
        let desc_bot_sel = Selector::parse("td.desc-bot").unwrap();
        let stats_span_sel = Selector::parse("td.stats span").unwrap();

        let mut idx = 0usize;
        let mut i = 0usize;
        while i + 1 < rows.len() {
            let name_row = rows[i];
            let info_row = rows[i + 1];
            i += 2;

            let links: Vec<_> = name_row.select(&desc_top_a_sel).collect();
            if links.is_empty() {
                continue;
            }
            let last = links.last().unwrap();
            let url = last.value().attr("href").unwrap_or("").to_string();
            let title = last.text().collect::<String>().trim().to_string();
            if url.is_empty() || title.is_empty() {
                continue;
            }

            let mut magnet = String::new();
            if links.len() == 2 {
                let href = links[0].value().attr("href").unwrap_or("").to_string();
                if href.starts_with("magnet") {
                    magnet = href;
                }
            }

            // second row: description + stats
            let desc = info_row
                .select(&desc_bot_sel)
                .next()
                .map(|d| d.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let mut filesize = String::new();
            let mut published = String::new();
            let mut content = String::new();
            for segment in desc.split('|') {
                let seg = segment.trim();
                if let Some(rest) = seg.strip_prefix("Size:") {
                    if let Some(m) = parse_size(rest) {
                        filesize = m;
                    }
                } else if let Some(rest) = seg.strip_prefix("Date:") {
                    published = rest.trim().to_string();
                } else if let Some(rest) = seg.strip_prefix("Comment:") {
                    content = rest.trim().to_string();
                }
            }

            let stats: Vec<String> = info_row
                .select(&stats_span_sel)
                .map(|s| s.text().collect::<String>().trim().to_string())
                .collect();
            let mut seed = String::new();
            let mut leech = String::new();
            if stats.len() >= 2 {
                seed = stats[0].clone();
                leech = stats[1].clone();
            }

            let snippet = if content.is_empty() {
                format!("Size: {} | Seeders: {} | Leechers: {}", filesize, seed, leech)
            } else {
                format!(
                    "{} | Size: {} | Seeders: {} | Leechers: {}",
                    content, filesize, seed, leech
                )
            };

            let mut result = SearchResult::new(title, url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + idx + 1)
                .with_score(1.0 - (idx as f64 * 0.05))
                .with_result_type(ResultType::Torrents);
            idx += 1;

            if !magnet.is_empty() {
                result = result.with_extra("magnet", serde_json::json!(magnet));
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

            results.push(result);
        }

        results
    }
}

/// Parse a size string like " 1.4GB" and return the canonicalized "1.4GB".
fn parse_size(s: &str) -> Option<String> {
    let trimmed = s.trim();
    let bytes = trimmed.as_bytes();
    let mut end = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        if b.is_ascii_digit() || b == b'.' {
            end = i + 1;
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let number = &trimmed[..end];
    // optional unit (T/G/M)B
    let rest = trimmed[end..].trim();
    let unit = if rest.len() >= 2 {
        let u = &rest[..2];
        if u.eq_ignore_ascii_case("tb")
            || u.eq_ignore_ascii_case("gb")
            || u.eq_ignore_ascii_case("mb")
        {
            u.to_uppercase()
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    if unit.is_empty() {
        Some(format!("{}B", number))
    } else {
        Some(format!("{}{}", number, unit))
    }
}

#[async_trait]
impl Engine for TokyoToshokanEngine {
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
        s.insert("search_url".into(), format!("{}/search.php", BASE_URL));
        s.insert("results".into(), "HTML".into());
        s
    }
}
