//! Google News search engine implementation
//!
//! Google News requires a `ceid`
//! region/language argument; results are scraped from HTML. The real article
//! URL is base64-encoded inside the `jslog` attribute of each result link.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// Google News search engine
pub struct GoogleNewsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl GoogleNewsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "google_news".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Google News - aggregated headlines and news search.".to_string(),
            website: Some("https://news.google.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Google News HTTP client");

        GoogleNewsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // Default ceid region/language; could be driven by query language.
        let ceid = "US:en";
        let region = "US";
        let lang = "en";

        let url = format!(
            "https://news.google.com/search?q={}&hl={}&gl={}&lr=lang_{}&ceid={}",
            urlencoding::encode(&query.query),
            lang,
            region,
            lang,
            ceid
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.0.1)")
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

        Ok(self.parse(&text))
    }

    fn parse(&self, html_text: &str) -> Vec<SearchResult> {
        let doc = Html::parse_document(html_text);
        let mut results = Vec::new();

        // Result containers: div[jslog][data-n-tid][jsdata]
        let result_sel = match Selector::parse("div[jslog][data-n-tid][jsdata]") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let link_sel = Selector::parse("a[target='_blank']").unwrap();
        let h4_sel = Selector::parse("h4").unwrap();
        let time_sel = Selector::parse("time").unwrap();
        let source_sel = Selector::parse("div.vr1PYe").unwrap();
        let img_sel = Selector::parse("figure img").unwrap();

        for (i, el) in doc.select(&result_sel).enumerate() {
            // Find the target link
            let a = el.select(&link_sel).next();
            let href = a.and_then(|a| a.value().attr("href")).unwrap_or("");
            if href.is_empty() {
                continue;
            }

            // Normalize relative href
            let mut url = if href.starts_with("./") {
                format!("https://news.google.com{}", &href[1..])
            } else if href.starts_with('/') {
                format!("https://news.google.com{}", href)
            } else {
                href.to_string()
            };

            // Decode real URL from jslog base64 segment
            if let Some(a) = a {
                if let Some(jslog) = a.value().attr("jslog") {
                    let parts: Vec<&str> = jslog.split(';').collect();
                    if parts.len() > 1 {
                        let b64_data = parts[1].split(':').last().unwrap_or("").trim();
                        if !b64_data.is_empty() {
                            let padded = match b64_data.len() % 4 {
                                0 => b64_data.to_string(),
                                n => format!("{}{}", b64_data, "=".repeat(4 - n)),
                            };
                            if let Ok(decoded_bytes) = b64_decode(&padded) {
                                if let Ok(decoded_str) = String::from_utf8(decoded_bytes) {
                                    if let Ok(arr) =
                                        serde_json::from_str::<serde_json::Value>(&decoded_str)
                                    {
                                        if let Some(arr) = arr.as_array() {
                                            if let Some(last) = arr.last() {
                                                if let Some(s) = last.as_str() {
                                                    if s.starts_with("http") {
                                                        url = s.to_string();
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if url.is_empty() || !url.starts_with("http") {
                continue;
            }

            let title = el
                .select(&h4_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }

            let pub_date = el
                .select(&time_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let pub_origin = el
                .select(&source_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let mut content_parts = Vec::new();
            if !pub_origin.is_empty() {
                content_parts.push(pub_origin.clone());
            }
            if !pub_date.is_empty() {
                content_parts.push(pub_date.clone());
            }
            let content = content_parts.join(" / ");

            let thumbnail = el
                .select(&img_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .map(|s| {
                    if s.starts_with('/') {
                        format!("https://news.google.com{}", s)
                    } else {
                        s.to_string()
                    }
                })
                .unwrap_or_default();

            let mut result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(0)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::News);
            if !pub_date.is_empty() {
                result = result.with_extra("published", serde_json::json!(pub_date));
            }
            if !pub_origin.is_empty() {
                result = result.with_extra("source", serde_json::json!(pub_origin));
            }
            if !thumbnail.is_empty() {
                result = result.with_extra("img_src", serde_json::json!(thumbnail));
            }
            results.push(result);
        }

        results
    }
}

/// Self-contained standard-alphabet base64 decoder (URL-safe variant is not
/// needed here; Google News jslog uses standard base64). Returns Err on any
/// invalid input so callers can fall back gracefully.
fn b64_decode(input: &str) -> std::result::Result<Vec<u8>, ()> {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    fn val(c: u8) -> Option<u8> {
        TABLE.iter().position(|&t| t == c).map(|p| p as u8)
    }
    let trimmed: Vec<u8> = input.bytes().filter(|&b| b != b'\n' && b != b'\r' && b != b' ').collect();
    let mut out = Vec::with_capacity(trimmed.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in &trimmed {
        if b == b'=' {
            break;
        }
        let v = val(b).ok_or(())? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[async_trait]
impl Engine for GoogleNewsEngine {
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
        matches!(t, ResultType::News | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://news.google.com".into());
        s.insert("default_ceid".into(), "US:en".into());
        s
    }
}
