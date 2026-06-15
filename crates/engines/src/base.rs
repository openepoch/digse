//! BASE (Scholar publications) search engine implementation.
//! Parses the BASE XML API.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// BASE (Bielefeld Academic Search Engine) scholarly publications search.
pub struct BaseEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl BaseEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "base".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "BASE - Bielefeld Academic Search Engine scholarly publications."
                .to_string(),
            website: Some("https://base-search.net".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create BASE HTTP client");
        BaseEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page_size: i64 = 10;
        let offset = ((query.offset as i64) * page_size).max(0);
        let hits = page_size.to_string();
        let offset_s = offset.to_string();
        let encoded_query = urlencoding::encode(&query.query);

        let url = format!(
            "https://api.base-search.net/cgi-bin/BaseHttpSearchInterface.fcgi?func=PerformSearch&query={query}&boost=oa&hits={hits}&offset={offset}",
            query = encoded_query,
            hits = hits,
            offset = offset_s,
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
        Ok(self.parse_xml(&text, query))
    }

    /// Parse the BASE XML response, mirroring the Python xpath over ./result/doc.
    fn parse_xml(&self, xml: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let mut idx = 0;
        let mut cursor = 0usize;
        while let Some(rel) = xml[cursor..].find("<doc") {
            let doc_start = cursor + rel;
            let after = &xml[doc_start..];
            let doc_open_end = match after.find('>') {
                Some(p) => doc_start + p + 1,
                None => break,
            };
            let rest = &xml[doc_open_end..];
            let doc_end = match rest.find("</doc>") {
                Some(p) => doc_open_end + p,
                None => break,
            };
            let body = &xml[doc_open_end..doc_end];

            // Each <str name="...">value</str> within the doc maps to a field.
            let mut title = String::new();
            let mut url = String::new();
            let mut content = String::new();
            let mut date = String::new();

            let mut fc = 0usize;
            while let Some(rel) = body[fc..].find("<str") {
                let tag_start = fc + rel;
                let after_tag = &body[tag_start..];
                let open_end = match after_tag.find('>') {
                    Some(p) => tag_start + p + 1,
                    None => break,
                };
                let close_rel = match body[open_end..].find("</str>") {
                    Some(p) => p,
                    None => break,
                };
                let value = &body[open_end..open_end + close_rel];
                let open_tag = &body[tag_start..open_end];

                let name = extract_attr(open_tag, "name").unwrap_or_default();
                match name.as_str() {
                    "dctitle" => title = decode_entities(value),
                    "dclink" => url = decode_entities(value),
                    "dcdescription" => {
                        let mut s = value.to_string();
                        if s.len() > 300 {
                            s.truncate(300);
                            s.push_str("...");
                        }
                        content = s;
                    }
                    "dcdate" => date = decode_entities(value),
                    _ => {}
                }
                fc = open_end + close_rel + "</str>".len();
            }

            if !url.is_empty() {
                let snippet = if !content.is_empty() {
                    if !date.is_empty() {
                        format!("Published: {} | {}", date, content)
                    } else {
                        content
                    }
                } else if !date.is_empty() {
                    format!("Published: {}", date)
                } else {
                    "No description available".to_string()
                };

                results.push(
                    SearchResult::new(title.clone(), url.clone())
                        .with_snippet(snippet)
                        .with_engine(self.name())
                        .with_rank(query.offset + idx + 1)
                        .with_score(1.0 - (idx as f64 * 0.03))
                        .with_result_type(ResultType::Academic)
                        .with_extra("published", serde_json::json!(date)),
                );
                idx += 1;
            }

            cursor = doc_end + "</doc>".len();
        }
        results
    }
}

fn extract_attr(open_tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    let start = open_tag.find(&needle)? + needle.len();
    let rest = &open_tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .trim()
        .to_string()
}

#[async_trait]
impl Engine for BaseEngine {
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
        matches!(t, ResultType::Academic | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://api.base-search.net".into());
        s.insert("format".into(), "xml".into());
        s
    }
}
