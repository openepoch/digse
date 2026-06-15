//! Chatnoir search engine implementation.
//! Webis Chatnoir web search API (CommonCrawl index).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Chatnoir web search engine (Webis research).
pub struct ChatnoirEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct ChatnoirRequest {
    query: String,
    index: Vec<&'static str>,
    from: usize,
    size: usize,
    #[serde(rename = "_extended_meta")]
    extended_meta: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatnoirResponse {
    #[serde(default)]
    results: Vec<ChatnoirItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ChatnoirItem {
    #[serde(default, rename = "target_uri")]
    target_uri: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    snippet: String,
}

impl ChatnoirEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("CHATNOIR_API_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        let metadata = EngineMetadata {
            name: "chatnoir".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: api_key.is_some(),
            timeout_seconds: 15,
            description: "Chatnoir - Webis web search over CommonCrawl.".to_string(),
            website: Some("https://www.chatnoir.eu".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Chatnoir HTTP client");
        ChatnoirEngine {
            metadata,
            client,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://www.chatnoir.eu/api/v1/_search";
        let body = ChatnoirRequest {
            query: query.query.clone(),
            index: vec!["cw22"],
            from: query.offset * 10,
            size: 10,
            extended_meta: true,
        };

        let mut req = self
            .client
            .post(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .json(&body);
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
        let resp = req
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
        let parsed: ChatnoirResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.results.iter().enumerate() {
            if item.target_uri.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(strip_html(&item.title), item.target_uri.clone())
                    .with_snippet(strip_html(&item.snippet))
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web),
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
impl Engine for ChatnoirEngine {
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
        s.insert("base_url".into(), "https://www.chatnoir.eu".into());
        s.insert("index".into(), "cw22".into());
        s
    }
}
