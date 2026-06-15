//! Bilibili video search engine implementation.
//! Uses the Bilibili JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Bilibili video search engine.
pub struct BilibiliEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct BilibiliResponse {
    #[serde(default)]
    data: Option<BilibiliData>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BilibiliData {
    #[serde(default)]
    result: Vec<BilibiliItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BilibiliItem {
    #[serde(default, rename = "title")]
    title_raw: String,
    #[serde(default)]
    arcurl: String,
    #[serde(default)]
    pic: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    aid: serde_json::Value,
    #[serde(default)]
    pubdate: i64,
    #[serde(default)]
    duration: String,
}

impl BilibiliEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bilibili".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Bilibili - Chinese video sharing website.".to_string(),
            website: Some("https://www.bilibili.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Bilibili HTTP client");
        BilibiliEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.bilibili.com/x/web-interface/search/type";
        let page = (query.offset + 1).to_string();
        let page_size = "20".to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Referer", "https://www.bilibili.com/")
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .query(&[
                ("__refresh__", "true"),
                ("page", page.as_str()),
                ("page_size", page_size.as_str()),
                ("single_column", "0"),
                ("keyword", query.query.as_str()),
                ("search_type", "video"),
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
        let parsed: BilibiliResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let items = parsed.data.map(|d| d.result).unwrap_or_default();
        for (i, item) in items.iter().enumerate() {
            let aid_str = match &item.aid {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => String::new(),
            };
            let title = strip_html(&item.title_raw);
            let iframe_src = if !aid_str.is_empty() {
                format!(
                    "https://player.bilibili.com/player.html?aid={}&high_quality=1&autoplay=false&danmaku=0",
                    aid_str
                )
            } else {
                String::new()
            };
            let published = if item.pubdate > 0 {
                item.pubdate.to_string()
            } else {
                String::new()
            };
            results.push(
                SearchResult::new(title.clone(), item.arcurl.clone())
                    .with_snippet(item.description.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("thumbnail", serde_json::json!(item.pic))
                    .with_extra("author", serde_json::json!(item.author))
                    .with_extra("duration", serde_json::json!(item.duration))
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("iframe_src", serde_json::json!(iframe_src)),
            );
        }
        Ok(results)
    }
}

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
    out
}

#[async_trait]
impl Engine for BilibiliEngine {
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
        matches!(t, ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "base_url".into(),
            "https://api.bilibili.com/x/web-interface/search/type".into(),
        );
        s
    }
}
