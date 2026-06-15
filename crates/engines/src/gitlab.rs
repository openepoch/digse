//! GitLab search engine implementation
//!
//! Uses the public GitLab REST API
//! (`/api/v4/projects?search=...&page=...`) which requires no authentication.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// GitLab repository search engine
pub struct GitLabEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Deserialize, Default)]
struct GitLabProject {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    web_url: String,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    namespace: GitLabNamespace,
    #[serde(default)]
    last_activity_at: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    tag_list: Vec<String>,
    #[serde(default)]
    star_count: i64,
    #[serde(default)]
    forks_count: i64,
    #[serde(default)]
    readme_url: Option<String>,
    #[serde(default)]
    http_url_to_repo: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GitLabNamespace {
    #[serde(default)]
    name: String,
}

impl GitLabEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "gitlab".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "GitLab repository search".to_string(),
            website: Some("https://gitlab.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create GitLab HTTP client");

        GitLabEngine {
            metadata,
            client,
            base_url: "https://gitlab.com".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = (query.offset / query.count.max(1)) + 1;
        let page_str = pageno.to_string();
        let url = format!("{}/api/v4/projects", self.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[
                ("search", query.query.as_str()),
                ("page", page_str.as_str()),
            ])
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let items: Vec<GitLabProject> = match response.json().await {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in items.iter().enumerate() {
            if results.len() >= query.count {
                break;
            }
            if item.web_url.is_empty() {
                continue;
            }
            let title = if item.name.is_empty() {
                item.web_url.clone()
            } else {
                item.name.clone()
            };
            let score = if item.star_count > 0 {
                (item.star_count as f64).ln()
            } else {
                1.0
            };
            let mut result = SearchResult::new(title, item.web_url.clone())
                .with_snippet(item.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(score)
                .with_result_type(ResultType::IT)
                .with_extra("stars", serde_json::json!(item.star_count))
                .with_extra("forks", serde_json::json!(item.forks_count))
                .with_extra("maintainer", serde_json::json!(item.namespace.name))
                .with_extra("published", serde_json::json!(item.last_activity_at));

            if let Some(thumb) = &item.avatar_url {
                result = result.with_extra("thumbnail", serde_json::json!(thumb));
            }
            if let Some(src) = &item.http_url_to_repo {
                result = result.with_extra("source_code_url", serde_json::json!(src));
            }
            if !item.tag_list.is_empty() {
                result = result.with_extra("tags", serde_json::json!(item.tag_list));
            }
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for GitLabEngine {
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
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("api_path".to_string(), "api/v4/projects".to_string());
        settings
    }
}
