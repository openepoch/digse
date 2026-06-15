//! Startpage search engine implementation
//!
//! Startpage requires a well-formed HTTP POST with a per-request `sc` form
//! token scraped from its homepage, plus a preferences cookie; otherwise it
//! serves a CAPTCHA. This implementation issues the POST and parses the
//! embedded JSON SERP payload, degrading gracefully (empty results) on any
//! failure (non-200, CAPTCHA redirect, or malformed payload).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Startpage general web search engine
pub struct StartpageEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://www.startpage.com";
const SEARCH_URL: &str = "https://www.startpage.com/sp/search";

impl StartpageEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "startpage".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "Startpage - private general web search.".to_string(),
            website: Some("https://startpage.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create Startpage HTTP client");

        StartpageEngine { metadata, client }
    }

    /// Fetch a fresh `sc` token from Startpage's homepage form.
    /// Returns `None` on any failure.
    async fn get_sc_code(&self) -> Option<String> {
        let resp = self
            .client
            .get(format!("{}/", BASE_URL))
            .header("User-Agent", "digse/0.0.1")
            .header("Accept-Language", "en-US,en;q=0.5")
            .send()
            .await
            .ok()?;

        // A captcha redirect means we cannot proceed.
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        if location.starts_with("https://www.startpage.com/sp/captcha") {
            return None;
        }

        if !resp.status().is_success() {
            return None;
        }
        let html = resp.text().await.ok()?;

        // Look for: <input ... name="sc" value="...">
        let needle = "name=\"sc\"";
        if let Some(rel) = html.find(needle) {
            let after = &html[rel..];
            if let Some(vrel) = after.find("value=\"") {
                let start = vrel + "value=\"".len();
                if let Some(end) = html[start..].find('"') {
                    let val = &html[start..start + end];
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
            }
        }
        None
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let sc_code = match self.get_sc_code().await {
            Some(c) => c,
            None => {
                tracing::info!("startpage: could not obtain sc token; returning empty");
                return Ok(vec![]);
            }
        };

        let pageno = (query.offset / 10) + 1;
        let pageno_str = pageno.to_string();

        let mut form: Vec<(&str, &str)> = vec![
            ("query", query.query.as_str()),
            ("cat", "web"),
            ("t", "device"),
            ("sc", sc_code.as_str()),
            ("withdate", ""),
            ("abd", "1"),
            ("abe", "1"),
            ("qsr", "all"),
            ("qadf", "none"),
        ];
        if pageno > 1 {
            form.push(("page", pageno_str.as_str()));
            form.push(("segment", "startpage.udog"));
        }

        let resp = self
            .client
            .post(SEARCH_URL)
            .header("User-Agent", "digse/0.0.1")
            .header("Origin", BASE_URL)
            .header("Referer", format!("{}/", BASE_URL))
            .form(&form)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        // A captcha redirect means no results.
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        if location.starts_with("https://www.startpage.com/sp/captcha") {
            return Ok(vec![]);
        }

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        Ok(self.parse_results(&text, query))
    }

    /// Extract the embedded SERP JSON payload and pull out web results.
    /// The payload follows `React.createElement(UIStartpage.AppSerpWeb, {` ... `}})`.
    fn parse_results(&self, html: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let mut results = Vec::new();

        let prefix = "React.createElement(UIStartpage.AppSerpWeb, {";
        let payload = match html.find(prefix) {
            Some(start) => {
                let body_start = start + prefix.len();
                // Find the matching closing `}})` from body_start.
                match find_serp_end(&html[body_start..]) {
                    Some(end) => &html[body_start..body_start + end],
                    None => return results,
                }
            }
            None => return results,
        };

        // Re-wrap into a JSON object for parsing.
        let json_str = format!("{{{}}}", payload);
        let value: serde_json::Value = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(_) => return results,
        };

        let regions = value
            .pointer("/render/presenter/regions")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let mainline = regions
            .get("mainline")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for results_categ in mainline {
            let display_type = results_categ
                .get("display_type")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let items = match results_categ.get("results").and_then(|v| v.as_array()) {
                Some(a) => a,
                None => continue,
            };
            if display_type != "web-google" {
                continue;
            }
            for (i, item) in items.iter().enumerate() {
                let url = item
                    .get("clickUrl")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let description = item
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if url.is_empty() || title.is_empty() {
                    continue;
                }
                results.push(
                    SearchResult::new(title, url)
                        .with_snippet(strip_html(&description))
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Web),
                );
            }
        }

        results
    }
}

/// Find the index of the matching `}})` that closes the SERP payload, scanning
/// from the start of the payload body.
fn find_serp_end(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        let c = bytes[i];
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 && bytes[i + 1] == b'}' && bytes[i + 2] == b')' {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Strip simple HTML tags from a description string.
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[async_trait]
impl Engine for StartpageEngine {
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
        matches!(t, ResultType::Web | ResultType::All)
            || matches!(t, ResultType::News | ResultType::Images)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("search_url".into(), SEARCH_URL.into());
        s.insert("startpage_categ".into(), "web".into());
        s.insert("results".into(), "HTML".into());
        s
    }
}
