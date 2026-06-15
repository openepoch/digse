//! Hugging Face search engine implementation
//!
//! JSON API for models, datasets,
//! and spaces. `huggingface_endpoint` selects which to search.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Hugging Face search engine
pub struct HuggingfaceEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    endpoint: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct HfEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    createdAt: String,
    #[serde(default)]
    likes: i64,
    #[serde(default)]
    downloads: i64,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    description: String,
}

impl HuggingfaceEngine {
    pub fn new() -> Self {
        Self::with_endpoint("models")
    }

    pub fn with_endpoint(endpoint: &str) -> Self {
        let endpoint = match endpoint {
            "datasets" | "models" | "spaces" => endpoint.to_string(),
            _ => "models".to_string(),
        };
        let metadata = EngineMetadata {
            name: "huggingface".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: format!(
                "Hugging Face {} - ML models, datasets, and spaces.",
                endpoint
            ),
            website: Some("https://huggingface.co/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Hugging Face HTTP client");
        HuggingfaceEngine {
            metadata,
            client,
            endpoint,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://huggingface.co";
        let url = format!("{}/api/{}", base_url, self.endpoint);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("direction", "-1"),
                ("search", query.query.as_str()),
            ])
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
        let entries: Vec<HfEntry> = match serde_json::from_str(&text) {
            Ok(e) => e,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, entry) in entries.iter().enumerate() {
            if entry.id.is_empty() {
                continue;
            }
            if i >= query.count {
                break;
            }
            let url = if self.endpoint != "models" {
                format!("{}/{}/{}", base_url, self.endpoint, entry.id)
            } else {
                format!("{}/{}", base_url, entry.id)
            };

            let mut contents = Vec::new();
            if entry.likes > 0 {
                contents.push(format!("Likes: {}", entry.likes));
            }
            if entry.downloads > 0 {
                contents.push(format!("Downloads: {}", entry.downloads));
            }
            if !entry.tags.is_empty() {
                contents.push(format!("Tags: {}", entry.tags.join(", ")));
            }
            if !entry.description.is_empty() {
                contents.push(format!("Description: {}", entry.description));
            }
            let snippet = contents.join(" | ");

            let mut result = SearchResult::new(entry.id.clone(), url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT);
            if !entry.createdAt.is_empty() {
                result = result.with_extra("published", serde_json::json!(entry.createdAt));
            }
            if entry.likes > 0 {
                result = result.with_extra("likes", serde_json::json!(entry.likes));
            }
            if entry.downloads > 0 {
                result = result.with_extra("downloads", serde_json::json!(entry.downloads));
            }
            if !entry.tags.is_empty() {
                result = result.with_extra("tags", serde_json::json!(entry.tags));
            }
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for HuggingfaceEngine {
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
        matches!(t, ResultType::IT | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://huggingface.co".into());
        s.insert("huggingface_endpoint".into(), self.endpoint.clone());
        s
    }
}
