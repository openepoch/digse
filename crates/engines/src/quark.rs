//! Quark (Shenma) search engine implementation
//!
//! The
//! reference parses JSON blobs embedded in `<script type="application/json">`
//! tags and dispatches on the `sc` (source-category) field. Quark may also
//! return an Alibaba CAPTCHA page; we treat that as a graceful empty result.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Quark (Shenma) search engine (general/web, embedded JSON)
pub struct QuarkEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl QuarkEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "quark".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Quark (Shenma) - Chinese web search.".to_string(),
            website: Some("https://quark.sm.cn/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Quark HTTP client");
        QuarkEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = (query.offset / 10) + 1;
        let page_str = page.to_string();
        let endpoint = "https://quark.sm.cn/s";
        let resp = self
            .client
            .get(endpoint)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("q", query.query.as_str()),
                ("layout", "html"),
                ("page", page_str.as_str()),
            ])
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
        if is_captcha(&text) {
            return Ok(vec![]);
        }
        Ok(self.parse(&text))
    }

    fn parse(&self, html: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        // extract JSON from <script type="application/json" id="s-data-..." data-used-by="hydrate">...</script>
        let pattern = "data-used-by=\"hydrate\">";
        let mut rest = html;
        while let Some(start_idx) = rest.find(pattern) {
            rest = &rest[start_idx + pattern.len()..];
            let end_idx = match rest.find("</script>") {
                Some(e) => e,
                None => break,
            };
            let blob = &rest[..end_idx];
            rest = &rest[end_idx..];
            if let Ok(data) = serde_json::from_str::<QuarkBlob>(blob) {
                let sc = data.extra_data.sc.clone().unwrap_or_default();
                let initial = data.data.initial_data.clone();
                match sc.as_str() {
                    "nature_result" | "ss_doc" | "ss_kv" | "ss_pic" | "ss_text"
                    | "ss_video" | "baike" | "structure_web_novel" => {
                        if let Some(r) = parse_ss_doc(&initial) {
                            results.push(r);
                        }
                    }
                    "addition" => {
                        if let Some(r) = parse_addition(&initial) {
                            results.push(r);
                        }
                    }
                    "ai_page" => {
                        for r in parse_ai_page(&initial) {
                            results.push(r);
                        }
                    }
                    "news_uchq" => {
                        for r in parse_news_uchq(&initial) {
                            results.push(r);
                        }
                    }
                    _ => {}
                }
            }
        }
        for r in results.iter_mut() {
            r.engine = self.name().to_string();
            r.result_type = ResultType::Web;
        }
        results
    }
}

fn is_captcha(html: &str) -> bool {
    // Alibaba X5SEC CAPTCHA signature
    html.contains("\"action\":\"captcha\"") || html.contains(" action: \"captcha\"")
}

fn strip_html(s: &str) -> String {
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
    out.trim().to_string()
}

// ----- embedded JSON shape -------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Default)]
struct QuarkBlob {
    #[serde(default)]
    data: QuarkData,
    #[serde(default)]
    extra_data: QuarkExtra,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct QuarkData {
    #[serde(default)]
    initial_data: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct QuarkExtra {
    #[serde(default)]
    sc: Option<String>,
}

fn parse_ss_doc(v: &serde_json::Value) -> Option<SearchResult> {
    let title = v
        .get("titleProps")
        .and_then(|t| t.get("content"))
        .and_then(|c| c.as_str())
        .or_else(|| v.get("title").and_then(|t| t.as_str()))
        .unwrap_or("");
    let url = v
        .get("sourceProps")
        .and_then(|s| s.get("dest_url"))
        .and_then(|u| u.as_str())
        .or_else(|| v.get("normal_url").and_then(|u| u.as_str()))
        .or_else(|| v.get("url").and_then(|u| u.as_str()))
        .unwrap_or("");
    let content = v
        .get("summaryProps")
        .and_then(|s| s.get("content"))
        .and_then(|c| c.as_str())
        .or_else(|| {
            v.get("message")
                .and_then(|m| m.get("replyContent"))
                .and_then(|c| c.as_str())
        })
        .or_else(|| v.get("show_body").and_then(|c| c.as_str()))
        .or_else(|| v.get("desc").and_then(|c| c.as_str()))
        .unwrap_or("");
    let title = strip_html(title);
    let url = url.to_string();
    if title.is_empty() || url.is_empty() {
        return None;
    }
    Some(
        SearchResult::new(title, url)
            .with_snippet(strip_html(content))
            .with_result_type(ResultType::Web),
    )
}

fn parse_addition(v: &serde_json::Value) -> Option<SearchResult> {
    let title = v
        .get("title")
        .and_then(|t| t.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let url = v
        .get("source")
        .and_then(|s| s.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or("");
    let content = v
        .get("summary")
        .and_then(|s| s.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let title = strip_html(title);
    if title.is_empty() || url.is_empty() {
        return None;
    }
    Some(
        SearchResult::new(title, url.to_string())
            .with_snippet(strip_html(content))
            .with_result_type(ResultType::Web),
    )
}

fn parse_ai_page(v: &serde_json::Value) -> Vec<SearchResult> {
    let mut out = Vec::new();
    if let Some(list) = v.get("list").and_then(|l| l.as_array()) {
        for item in list {
            let title = item
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("");
            let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let content = match item.get("content") {
                Some(serde_json::Value::Array(arr)) => arr
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                Some(serde_json::Value::String(s)) => s.clone(),
                _ => String::new(),
            };
            let title = strip_html(title);
            if !title.is_empty() && !url.is_empty() {
                out.push(
                    SearchResult::new(title, url.to_string())
                        .with_snippet(strip_html(&content))
                        .with_result_type(ResultType::Web),
                );
            }
        }
    }
    out
}

fn parse_news_uchq(v: &serde_json::Value) -> Vec<SearchResult> {
    let mut out = Vec::new();
    if let Some(feed) = v.get("feed").and_then(|f| f.as_array()) {
        for item in feed {
            let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let summary = item.get("summary").and_then(|s| s.as_str()).unwrap_or("");
            let image = item
                .get("image")
                .and_then(|i| i.as_str())
                .map(|s| s.replacen("http://", "https://", 1));
            let title = strip_html(title);
            if !title.is_empty() && !url.is_empty() {
                let mut r = SearchResult::new(title, url.to_string())
                    .with_snippet(strip_html(summary))
                    .with_result_type(ResultType::Web);
                if let Some(img) = image {
                    r = r.with_extra("thumbnail", serde_json::json!(img));
                }
                out.push(r);
            }
        }
    }
    out
}

#[async_trait]
impl Engine for QuarkEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://quark.sm.cn".into());
        s.insert("category".into(), "general".into());
        s
    }
}
