//! Wolfram|Alpha search engine implementation (XML API, paid).
//!
//! Requires `WOLFRAMALPHA_API_KEY`. Without a key, the engine returns an
//! empty result set gracefully. The upstream API returns XML; this port parses
//! the query result pods for plaintext/image content and synthesizes a single
//! infobox-style result plus a link to the Wolfram|Alpha site.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Wolfram|Alpha (official API) engine.
pub struct WolframalphaApiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

const SITE_URL: &str = "https://www.wolframalpha.com/input/?";

impl WolframalphaApiEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("WOLFRAMALPHA_API_KEY")
            .ok()
            .filter(|s| !s.is_empty() && s != "unset");
        let metadata = EngineMetadata {
            name: "wolframalpha_api".to_string(),
            category: EngineCategory::General,
            enabled: api_key.is_some(),
            requires_auth: true,
            timeout_seconds: 20,
            description: "Wolfram|Alpha - computational knowledge (official API).".to_string(),
            website: Some("https://www.wolframalpha.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create Wolfram|Alpha HTTP client");
        WolframalphaApiEngine {
            metadata,
            client,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                tracing::info!("wolframalpha_api requires WOLFRAMALPHA_API_KEY; returning empty");
                return Ok(vec![]);
            }
        };

        let api_url = format!(
            "https://api.wolframalpha.com/v2/query?appid={}&input={}",
            key,
            urlencoding::encode(&query.query)
        );
        let resp = self
            .client
            .get(&api_url)
            .header("User-Agent", "digse/0.0.1")
            .header("Referer", format!("{}i={}", SITE_URL, urlencoding::encode(&query.query)))
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        self.parse_xml(&text, &query.query)
    }

    fn parse_xml(&self, xml: &str, query: &str) -> Result<Vec<SearchResult>> {
        // Lightweight XML walk: the WA response is small and shallow. We avoid a
        // full XML dependency by scanning for <pod ...> ... </pod> blocks.
        if xml.contains(r#"success="false"#) {
            return Ok(vec![]);
        }

        let mut infobox_title = String::new();
        let mut result_content = String::new();
        let mut attributes: Vec<(String, String)> = Vec::new();

        let image_pods = ["VisualRepresentation", "Illustration"];

        for pod in split_pods(xml) {
            let pod_id = attr(&pod, "id").unwrap_or_default();
            let pod_title = attr(&pod, "title").unwrap_or_default();
            let pod_is_result = attr(&pod, "primary").is_some();

            // first plaintext from a subpod
            for subpod_plain in extract_all(&pod, "plaintext") {
                let content = decode_entities(&subpod_plain);
                if content.is_empty() {
                    continue;
                }
                if pod_id == "Input" && infobox_title.is_empty() {
                    infobox_title = content.clone();
                }
                if !image_pods.contains(&pod_id.as_str()) {
                    if content != "(requires interactivity)" {
                        attributes.push((pod_title.clone(), content.clone()));
                    }
                    if pod_is_result || result_content.is_empty() {
                        if pod_id != "Input" {
                            result_content = format!("{}: {}", pod_title, content);
                        }
                    }
                }
            }
            // images
            if !image_pods.contains(&pod_id.as_str()) {
                // already handled as text above
            } else {
                for img_src in extract_attr_all(&pod, "img", "src") {
                    attributes.push((pod_title.clone(), img_src));
                }
            }
        }

        if attributes.is_empty() {
            return Ok(vec![]);
        }

        let title = if infobox_title.is_empty() {
            query.to_string()
        } else {
            infobox_title.clone()
        };
        let referer = format!("{}i={}", SITE_URL, urlencoding::encode(query));

        let attrs_json: Vec<serde_json::Value> = attributes
            .iter()
            .map(|(k, v)| serde_json::json!({"label": k, "value": v}))
            .collect();

        let r = SearchResult::new(format!("Wolfram|Alpha ({})", title), referer)
            .with_snippet(if result_content.is_empty() {
                title.clone()
            } else {
                result_content
            })
            .with_engine(self.name())
            .with_rank(1)
            .with_score(1.0)
            .with_result_type(ResultType::Web)
            .with_extra("infobox", serde_json::json!(title))
            .with_extra("attributes", serde_json::json!(attrs_json));
        Ok(vec![r])
    }
}

fn split_pods(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<pod ") {
        // find matching </pod>
        let after = &rest[start..];
        if let Some(end) = after.find("</pod>") {
            out.push(after[..end + "</pod>".len()].to_string());
            rest = &after[end + "</pod>".len()..];
        } else {
            break;
        }
    }
    out
}

fn attr(block: &str, name: &str) -> Option<String> {
    let needle = format!("{}=\"", name);
    let idx = block.find(&needle)?;
    let rest = &block[idx + needle.len()..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn extract_all(block: &str, tag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut rest = block;
    while let Some(s) = rest.find(&open) {
        let after_open = &rest[s..];
        // skip attributes to '>'
        let gt = match after_open.find('>') {
            Some(g) => g,
            None => break,
        };
        let inner_start = &after_open[gt + 1..];
        if let Some(e) = inner_start.find(&close) {
            let inner = &inner_start[..e];
            out.push(inner.trim().to_string());
            rest = &inner_start[e + close.len()..];
        } else {
            break;
        }
    }
    out
}

fn extract_attr_all(block: &str, tag: &str, attr_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let open = format!("<{}", tag);
    let mut rest = block;
    while let Some(s) = rest.find(&open) {
        let after_open = &rest[s..];
        let gt = match after_open.find('>') {
            Some(g) => g,
            None => break,
        };
        let tag_block = &after_open[..gt];
        let needle = format!("{}=\"", attr_name);
        if let Some(idx) = tag_block.find(&needle) {
            let val_rest = &tag_block[idx + needle.len()..];
            if let Some(end) = val_rest.find('"') {
                out.push(val_rest[..end].to_string());
            }
        }
        rest = &after_open[gt + 1..];
    }
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[async_trait]
impl Engine for WolframalphaApiEngine {
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
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("results".to_string(), "XML".to_string());
        s.insert("api_endpoint".to_string(), "https://api.wolframalpha.com/v2/query".to_string());
        s
    }
}
