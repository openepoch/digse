//! Tootfinder search engine implementation
//!
//! Queries the Tootfinder REST API
//! at `tootfinder.ch/rest/api/search/{query}`. The API sometimes appends
//! server-side HTML errors to the body, so only the line beginning with `[{`
//! (the JSON array) is parsed. Each toot yields url/title/content/thumbnail/
//! publishedDate; the first image attachment's `preview_url` becomes the
//! thumbnail.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Tootfinder fediverse/Mastodon search engine
pub struct TootfinderEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl TootfinderEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "tootfinder".to_string(),
            category: EngineCategory::Social,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Tootfinder Mastodon/fediverse search.".to_string(),
            website: Some("https://www.tootfinder.ch".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Tootfinder HTTP client");

        TootfinderEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let encoded = urlencoding::encode(&query.query);
        let url = format!("https://www.tootfinder.ch/rest/api/search/{}", encoded);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
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

        // The API appends server-side HTML errors; take only the JSON array line.
        let json_str = text
            .lines()
            .find(|l| l.trim_start().starts_with("[{"))
            .unwrap_or("");

        let parsed: Vec<Value> = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.iter().enumerate() {
            if results.len() >= query.count {
                break;
            }
            let toot_url = item.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if toot_url.is_empty() {
                continue;
            }
            let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let plain = html_to_text(content);

            // title: card.title, else first 75 chars of the content.
            let title = item
                .get("card")
                .and_then(|c| c.get("title"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| plain.chars().take(75).collect());

            // thumbnail: first image attachment preview_url
            let thumbnail = item
                .get("media_attachments")
                .and_then(|v| v.as_array())
                .and_then(|arr| {
                    arr.iter().find_map(|a| {
                        let t = a.get("type").and_then(|v| v.as_str())?;
                        if t == "image" {
                            a.get("preview_url")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default();

            let published = item.get("created_at").and_then(|v| v.as_str()).unwrap_or("");

            let mut result = SearchResult::new(title, toot_url.to_string())
                .with_snippet(plain)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Social)
                .with_extra("published", serde_json::json!(published))
                .with_extra("source", serde_json::json!("tootfinder"));
            if !thumbnail.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
            }
            results.push(result);
        }
        Ok(results)
    }
}

/// Minimal HTML-to-text: strip tags, decode common entities, collapse whitespace.
fn html_to_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[async_trait]
impl Engine for TootfinderEngine {
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
        matches!(t, ResultType::Social | ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "base_url".to_string(),
            "https://www.tootfinder.ch".to_string(),
        );
        s.insert(
            "search_url".to_string(),
            "/rest/api/search/{query}".to_string(),
        );
        s
    }
}
