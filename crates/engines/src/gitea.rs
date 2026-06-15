//! Gitea search engine implementation
//!
//! Searches a Gitea/Forgejo instance via
//! its REST API `{base_url}/api/v1/repos/search?q=...`. Category: it.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Gitea/Forgejo repository search engine (JSON API)
pub struct GiteaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GiteaResponse {
    #[serde(default)]
    data: Vec<GiteaRepo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GiteaRepo {
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    avatar_url: String,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    website: String,
    #[serde(default)]
    clone_url: String,
    #[serde(default)]
    stars_count: i64,
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default)]
    owner: GiteaOwner,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GiteaOwner {
    #[serde(default)]
    username: String,
    #[serde(default)]
    avatar_url: String,
}

impl GiteaEngine {
    pub fn new() -> Self {
        Self::with_base_url("https://gitea.com".to_string())
    }

    pub fn with_base_url(base_url: String) -> Self {
        let metadata = EngineMetadata {
            name: "gitea".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Gitea/Forgejo - self-hosted Git service repository search.".to_string(),
            website: Some(base_url.clone()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Gitea HTTP client");

        GiteaEngine {
            metadata,
            client,
            base_url,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!("{}/api/v1/repos/search", self.base_url.trim_end_matches('/'));
        let page = query.offset + 1;
        let page_str = page.to_string();
        let limit = query.count.to_string();

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("limit", limit.as_str()),
                ("sort", "updated"),
                ("order", "desc"),
                ("page", page_str.as_str()),
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

        let parsed: GiteaResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, repo) in parsed.data.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let mut content_parts = Vec::new();
            if !repo.language.is_empty() {
                content_parts.push(repo.language.clone());
            }
            if !repo.description.is_empty() {
                content_parts.push(repo.description.clone());
            }
            let title = if repo.full_name.is_empty() {
                repo.name.clone()
            } else {
                repo.full_name.clone()
            };
            let thumbnail = if !repo.avatar_url.is_empty() {
                repo.avatar_url.clone()
            } else {
                repo.owner.avatar_url.clone()
            };

            let result = SearchResult::new(title, repo.html_url.clone())
                .with_snippet(content_parts.join(" / "))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("package_name", serde_json::json!(repo.name))
                .with_extra("maintainer", serde_json::json!(repo.owner.username))
                .with_extra("published", serde_json::json!(if !repo.updated_at.is_empty() { repo.updated_at.clone() } else { repo.created_at.clone() }))
                .with_extra("tags", serde_json::json!(repo.topics))
                .with_extra("popularity", serde_json::json!(repo.stars_count))
                .with_extra("homepage", serde_json::json!(repo.website))
                .with_extra("source_code_url", serde_json::json!(repo.clone_url))
                .with_extra("source", serde_json::json!("gitea"));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for GiteaEngine {
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
        matches!(result_type, ResultType::IT | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert(
            "search_endpoint".to_string(),
            "/api/v1/repos/search".to_string(),
        );
        settings.insert("sort".to_string(), "updated".to_string());
        settings.insert("order".to_string(), "desc".to_string());
        settings
    }
}
