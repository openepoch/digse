//! Ipernity search engine implementation
//!
//! Image search via HTML scrape; the
//! per-photo metadata is embedded in inline `<script>` blocks as JS objects.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// Ipernity image search engine
pub struct IpernityEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl IpernityEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "ipernity".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Ipernity - photo sharing.".to_string(),
            website: Some("https://www.ipernity.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Ipernity HTTP client");

        IpernityEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.ipernity.com";
        let pageno = (query.offset / query.count.max(1)) + 1;
        let url = format!(
            "{}/search/photo/@/page:{}:{}?q={}",
            base_url,
            pageno,
            10,
            urlencoding::encode(&query.query)
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

        let doc = Html::parse_document(&text);
        let mut results = Vec::new();

        // collect <a href="/doc/..."> thumbnails
        let img_a_sel = match Selector::parse("a[href^='/doc']") {
            Ok(s) => s,
            Err(_) => return Ok(results),
        };
        let mut thumbnails: Vec<String> = Vec::new();
        for a in doc.select(&img_a_sel) {
            // find nested img
            let img_sel = Selector::parse("img").unwrap();
            if let Some(im) = a.select(&img_sel).next() {
                if let Some(src) = im.value().attr("src") {
                    thumbnails.push(src.to_string());
                }
            }
        }

        // collect per-photo metadata from <script> tags
        let script_sel = Selector::parse("script[type='text/javascript']").unwrap();
        let mut idx = 0usize;
        for script in doc.select(&script_sel) {
            if results.len() >= query.count {
                break;
            }
            let raw = script.text().collect::<String>();
            // Look for `] = {...};` style info objects
            let info = extract_info_object(&raw);
            if let Some(info) = info {
                if info.get("mediakey").is_none() {
                    continue;
                }
                let user_id = info.get("user_id").cloned().unwrap_or_default();
                let doc_id = info.get("doc_id").cloned().unwrap_or_default();
                if user_id.is_empty() || doc_id.is_empty() {
                    continue;
                }
                let url = format!("{}/doc/{}/{}", base_url, user_id, doc_id);
                let title = info.get("title").cloned().unwrap_or_default();
                let content = info.get("content").cloned().unwrap_or_default();
                let thumbnail_src = thumbnails.get(idx).cloned().unwrap_or_default();
                let img_src = thumbnail_src.replace("240.jpg", "640.jpg");
                let resolution = match (info.get("width"), info.get("height")) {
                    (Some(w), Some(h)) => format!("{}x{}", w, h),
                    _ => String::new(),
                };

                let mut result = SearchResult::new(title, url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + results.len() + 1)
                    .with_score(1.0 - (results.len() as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(img_src))
                    .with_extra("thumbnail", serde_json::json!(thumbnail_src))
                    .with_extra("source", serde_json::json!("ipernity"));
                if !resolution.is_empty() {
                    result = result.with_extra("format", serde_json::json!(resolution));
                }
                results.push(result);
                idx += 1;
            }
        }
        Ok(results)
    }
}

/// Extract the first `] = {...};` JS object literal from a script body and
/// parse its string keys into a HashMap. Very tolerant — keys without quotes,
/// single-quoted strings, trailing semicolons all handled.
fn extract_info_object(raw: &str) -> Option<HashMap<String, String>> {
    // find `] = ` then balance braces up to the matching `};`
    let marker = "] = ";
    let start = raw.find(marker)? + marker.len();
    let rest = &raw[start..];
    if !rest.starts_with('{') {
        return None;
    }
    // balance braces
    let mut depth = 0i32;
    let mut end = 0usize;
    for (i, ch) in rest.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    if end == 0 {
        return None;
    }
    let body = &rest[..end + 1];

    // Convert to valid JSON: quote bare keys, convert single quotes to double.
    let mut jsonified = String::with_capacity(body.len());
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' => {
                jsonified.push('"');
                i += 1;
                continue;
            }
            '{' | ',' => {
                jsonified.push(c);
                i += 1;
                // skip whitespace
                while i < chars.len() && chars[i].is_whitespace() {
                    jsonified.push(chars[i]);
                    i += 1;
                }
                // if next is a bare-word key, quote it
                if i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '_') {
                    jsonified.push('"');
                    while i < chars.len()
                        && (chars[i].is_alphanumeric() || chars[i] == '_')
                    {
                        jsonified.push(chars[i]);
                        i += 1;
                    }
                    jsonified.push('"');
                }
                continue;
            }
            _ => {
                jsonified.push(c);
                i += 1;
            }
        }
    }

    let parsed: serde_json::Value = serde_json::from_str(&jsonified).ok()?;
    let map = parsed.as_object()?;
    let mut out = HashMap::new();
    for (k, v) in map {
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            _ => continue,
        };
        out.insert(k.clone(), s);
    }
    Some(out)
}

#[async_trait]
impl Engine for IpernityEngine {
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
        matches!(t, ResultType::Images | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://www.ipernity.com".into());
        s
    }
}
