//! MRS (Matrix Rooms Search) engine implementation
//!
//! queries a Matrix Rooms Search service.
//! Category: social media. Requires MRS_BASE_URL (defaults to the public
//! matrixrooms.info-style endpoint).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// MRS (Matrix Rooms Search) engine
pub struct MrsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    matrix_url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MrsResult {
    #[serde(default)]
    alias: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    topic: String,
    #[serde(default)]
    members: serde_json::Value,
    #[serde(default)]
    server: String,
    #[serde(default)]
    avatar_url: Option<String>,
}

impl MrsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "mrs".to_string(),
            category: EngineCategory::Social,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "MRS - Matrix Rooms Search.".to_string(),
            website: Some("https://matrixrooms.info".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create MRS HTTP client");

        MrsEngine {
            metadata,
            client,
            base_url: std::env::var("MRS_BASE_URL")
                .unwrap_or_else(|_| "https://matrixrooms.info".to_string()),
            matrix_url: "https://matrix.to".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let encoded = urlencoding::encode(&query.query);
        let page_size = 20usize;
        let offset = query.offset;
        let url = format!(
            "{}/search/{}/{}/{}",
            self.base_url, encoded, page_size, offset
        );

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
        let parsed: Vec<MrsResult> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.iter().enumerate() {
            if item.alias.is_empty() {
                continue;
            }
            let url = format!("{}/#/{}", self.matrix_url, item.alias);
            let content = format!(
                "{} // {} members // {} // {}",
                item.topic,
                item.members,
                item.alias,
                item.server
            );
            let mut result = SearchResult::new(item.name.clone(), url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Social)
                .with_extra("members", serde_json::json!(item.members))
                .with_extra("server", serde_json::json!(item.server));
            if let Some(av) = &item.avatar_url {
                if !av.is_empty() {
                    result = result.with_extra("thumbnail", serde_json::json!(av));
                }
            }
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for MrsEngine {
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

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Social | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("matrix_url".to_string(), self.matrix_url.clone());
        settings.insert("page_size".to_string(), "20".to_string());
        settings
    }
}
