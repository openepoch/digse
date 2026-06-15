//! GitHub search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// GitHub search engine for repositories
pub struct GitHubEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubResponse {
    #[serde(default)]
    items: Vec<GitHubItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubItem {
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    stargazers_count: i64,
    #[serde(alias = "owner")]
    #[serde(default)]
    owner_info: GitHubOwner,
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    clone_url: String,
    #[serde(default)]
    license: Option<GitHubLicense>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GitHubOwner {
    #[serde(default)]
    login: String,
    #[serde(default)]
    avatar_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubLicense {
    #[serde(default)]
    spdx_id: String,
    #[serde(default)]
    name: String,
}

impl GitHubEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "github".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "GitHub repository search".to_string(),
            website: Some("https://github.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create GitHub HTTP client");

        GitHubEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "https://api.github.com/search/repositories?sort=stars&order=desc&q={}",
            urlencoding::encode(&query.query)
        );


        let response = self.client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/vnd.github.preview.text-match+json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;


        if !response.status().is_success() {
            return Err(Error::EngineError(
                "github".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let github_response: GitHubResponse = serde_json::from_str(&text)
            .map_err(|e| Error::ParseError(format!("Failed to parse GitHub response: {}", e)))?;


        let results: Vec<SearchResult> = github_response.items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let mut content = Vec::new();
                if !item.language.is_empty() {
                    content.push(item.language.clone());
                }
                if !item.description.is_empty() {
                    content.push(item.description.clone());
                }
                let content_str = content.join(" / ");

                let mut result = SearchResult::new(&item.full_name, &item.html_url)
                    .with_snippet(&content_str)
                    .with_engine("github")
                    .with_rank(query.offset + i + 1)
                    .with_score((item.stargazers_count as f64).ln().max(1.0))
                    .with_extra("stars", serde_json::json!(item.stargazers_count))
                    .with_extra("language", serde_json::json!(item.language))
                    .with_extra("owner", serde_json::json!(item.owner_info.login))
                    .with_extra("avatar", serde_json::json!(item.owner_info.avatar_url));

                if let Some(license) = item.license {
                    if !license.spdx_id.is_empty() {
                        result = result.with_extra("license", serde_json::json!(license.name));
                    }
                }

                if !item.topics.is_empty() {
                    result = result.with_extra("topics", serde_json::json!(item.topics.join(", ")));
                }

                if !item.homepage.is_empty() {
                    result = result.with_extra("homepage", serde_json::json!(item.homepage));
                }

                if !item.clone_url.is_empty() {
                    result = result.with_extra("clone_url", serde_json::json!(item.clone_url));
                }

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Engine for GitHubEngine {
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
        *result_type == ResultType::IT || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://api.github.com".to_string());
        settings.insert("search_endpoint".to_string(), "/search/repositories".to_string());
        settings.insert("sort".to_string(), "stars".to_string());
        settings.insert("order".to_string(), "desc".to_string());
        settings
    }
}
