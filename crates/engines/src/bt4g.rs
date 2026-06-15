//! BT4G (bt4gprx.com) torrent search engine implementation.
//! RSS/XML torrent metadata search.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// BT4G torrent metadata search engine.
pub struct Bt4gEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl Bt4gEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bt4g".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "BT4G - torrent metadata and magnet links.".to_string(),
            website: Some("https://bt4gprx.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create BT4G HTTP client");
        Bt4gEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let encoded = urlencoding::encode(&query.query);
        let pageno = (query.offset + 1).to_string();
        let url = format!(
            "https://bt4gprx.com/search?q={q}&orderby=relevance&category=all&p={p}&page=rss",
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
        Ok(self.parse_rss(&text, query))
    }

    fn parse_rss(&self, xml: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let mut idx = 0;
        let mut cursor = 0usize;
        while let Some(rel) = xml[cursor..].find("<item>") {
            let start = cursor + rel + "<item>".len();
            let end_rel = match xml[start..].find("</item>") {
                Some(p) => p,
                None => break,
            };
            let body = &xml[start..start + end_rel];

            let title = extract_tag(body, "title").unwrap_or_default();
            let guid = extract_tag(body, "guid").unwrap_or_default();
            let link = extract_tag(body, "link").unwrap_or_default();
            let desc = extract_tag(body, "description").unwrap_or_default();
            let pub_date = extract_tag(body, "pubDate").unwrap_or_default();

            if guid.is_empty() {
                cursor = start + end_rel + "</item>".len();
                continue;
            }
            // description is HTML: "Title<br>Size<br>..."
            let parts: Vec<&str> = desc.split("<br>").collect();
            let filesize = parts.get(1).map(|s| decode_entities(s)).unwrap_or_default();

            results.push(
                SearchResult::new(title.clone(), guid.clone())
                    .with_snippet(desc.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + idx + 1)
                    .with_score(1.0 - (idx as f64 * 0.05))
                    .with_result_type(ResultType::Files)
                    .with_extra("magnet", serde_json::json!(link))
                    .with_extra("filesize", serde_json::json!(filesize))
                    .with_extra("published", serde_json::json!(pub_date))
                    .with_extra("seeders", serde_json::json!("N/A"))
                    .with_extra("leechers", serde_json::json!("N/A")),
            );
            idx += 1;
            cursor = start + end_rel + "</item>".len();
        }
        results
    }
}

fn extract_tag(content: &str, tag: &str) -> Option<String> {
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    // CDATA-safe: just take inner text between tags.
    let s = content.find(&start_tag)? + start_tag.len();
    let rest = &content[s..];
    let e = rest.find(&end_tag)?;
    Some(decode_entities(rest[..e].trim()))
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("<![CDATA[", "")
        .replace("]]>", "")
        .trim()
        .to_string()
}

#[async_trait]
impl Engine for Bt4gEngine {
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
        s.insert("base_url".into(), "https://bt4gprx.com".into());
        s.insert("format".into(), "rss".into());
        s
    }
}
