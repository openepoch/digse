//! crates.io (Cargo) package search engine implementation.
//! crates.io JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// crates.io (Rust Cargo) package search engine.
pub struct CratesEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct CratesResponse {
    #[serde(default)]
    crates: Vec<CratesItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CratesItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    newest_version: Option<String>,
    #[serde(default)]
    max_version: Option<String>,
    #[serde(default)]
    max_stable_version: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    keywords: Option<Vec<String>>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    documentation: Option<String>,
    #[serde(default)]
    repository: Option<String>,
}

impl CratesEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "crates".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "crates.io - Rust Cargo package registry search.".to_string(),
            website: Some("https://crates.io/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create crates.io HTTP client");
        CratesEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://crates.io/api/v1/crates";
        let page = (query.offset + 1).to_string();
        let per_page = "10".to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1 (https://github.com/digse)")
            .header("Accept", "application/json")
            .query(&[
                ("page", page.as_str()),
                ("q", query.query.as_str()),
                ("per_page", per_page.as_str()),
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
        let parsed: CratesResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.crates.iter().enumerate() {
            let page_url = format!("https://crates.io/crates/{}", item.name);
            let version = item
                .newest_version
                .clone()
                .or_else(|| item.max_version.clone())
                .or_else(|| item.max_stable_version.clone())
                .unwrap_or_default();
            let description = item.description.clone().unwrap_or_default();
            let keywords = item.keywords.clone().unwrap_or_default();
            let updated = item.updated_at.clone().unwrap_or_default();

            // Build a links snippet like the Python linked_terms OrderedDict.
            let mut links: Vec<String> = Vec::new();
            if let Some(h) = &item.homepage {
                if !h.is_empty() {
                    links.push(format!("Project homepage: {}", h));
                }
            }
            if let Some(d) = &item.documentation {
                if !d.is_empty() {
                    links.push(format!("Documentation: {}", d));
                }
            }
            if let Some(r) = &item.repository {
                if !r.is_empty() {
                    links.push(format!("Source code: {}", r));
                }
            }
            let snippet = if links.is_empty() {
                description
            } else if description.is_empty() {
                links.join(" | ")
            } else {
                format!("{} | {}", description, links.join(" | "))
            };

            let mut result = SearchResult::new(item.name.clone(), page_url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT)
                .with_extra("version", serde_json::json!(version))
                .with_extra("published", serde_json::json!(updated))
                .with_extra("tags", serde_json::json!(keywords));
            if let Some(h) = &item.homepage {
                if !h.is_empty() {
                    result = result.with_extra("homepage", serde_json::json!(h));
                }
            }
            if let Some(d) = &item.documentation {
                if !d.is_empty() {
                    result = result.with_extra("documentation", serde_json::json!(d));
                }
            }
            if let Some(r) = &item.repository {
                if !r.is_empty() {
                    result = result.with_extra("repository", serde_json::json!(r));
                }
            }
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for CratesEngine {
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
        matches!(t, ResultType::IT | ResultType::Files | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://crates.io/api/v1/crates".into());
        s
    }
}
