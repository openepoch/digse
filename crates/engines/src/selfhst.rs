//! selfh.st/icons search engine implementation
//!
//! The engine downloads a
//! static `index.json` listing of self-hosted-software icons from the jsDelivr
//! CDN and filters it client-side by the query keywords.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// selfh.st/icons search engine (IT / self-hosted software icons, JSON)
pub struct SelfhstEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl SelfhstEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "selfhst".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "selfh.st/icons - logos for self-hosted dashboards.".to_string(),
            website: Some("https://selfh.st/icons/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create selfhst HTTP client");
        SelfhstEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let cdn_base_url = "https://cdn.jsdelivr.net/gh/selfhst/icons";
        let url = format!("{}/index.json", cdn_base_url);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
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
        let items: Vec<SelfhstItem> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let query_parts: Vec<String> = query
            .query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        if query_parts.is_empty() {
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        for item in items.iter() {
            let keyword = item.reference.to_lowercase();
            if !query_parts.iter().any(|p| keyword.contains(p)) {
                continue;
            }
            // pick the first available format
            let img_format = ["SVG", "PNG", "WebP"]
                .iter()
                .find(|f| {
                    item.format_flag(f).map(|v| v == "Yes").unwrap_or(false)
                })
                .map(|s| s.to_lowercase());
            let img_format = match img_format {
                Some(f) => f,
                None => continue,
            };
            let img_src = format!("{}/{}/{}.{}", cdn_base_url, img_format, item.reference, img_format);
            results.push(
                SearchResult::new(item.name.clone(), img_src.clone())
                    .with_engine(self.name())
                    .with_result_type(ResultType::IT)
                    .with_extra("img_src", serde_json::json!(img_src))
                    .with_extra("thumbnail", serde_json::json!(img_src))
                    .with_extra("format", serde_json::json!(img_format))
                    .with_extra("source", serde_json::json!("selfhst"))
                    .with_extra("published", serde_json::json!(item.created_at)),
            );
        }
        Ok(results)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SelfhstItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    reference: String,
    #[serde(default)]
    svg: String,
    #[serde(default)]
    png: String,
    #[serde(default)]
    webp: String,
    #[serde(default, rename = "CreatedAt")]
    created_at: String,
}

impl SelfhstItem {
    fn format_flag(&self, name: &str) -> Option<&str> {
        match name {
            "SVG" => Some(&self.svg),
            "PNG" => Some(&self.png),
            "WebP" => Some(&self.webp),
            _ => None,
        }
    }
}

#[async_trait]
impl Engine for SelfhstEngine {
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
        matches!(t, ResultType::IT | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "cdn_base_url".into(),
            "https://cdn.jsdelivr.net/gh/selfhst/icons".into(),
        );
        s
    }
}
