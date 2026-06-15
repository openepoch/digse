//! Docker Hub (IT) search engine implementation
//!
//! queries the Docker Hub catalog
//! search API for container images.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Docker Hub search engine
pub struct DockerHubEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const PAGE_SIZE: i64 = 10;

#[derive(Debug, Serialize, Deserialize)]
struct DockerHubResponse {
    #[serde(default)]
    results: Vec<DockerHubImage>,
    #[serde(default)]
    count: i64,
    #[serde(default)]
    total: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DockerHubImage {
    #[serde(default)]
    name: String,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    short_description: String,
    #[serde(default)]
    star_count: i64,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    logo_url: DockerHubLogo,
    #[serde(default)]
    publisher: DockerHubPublisher,
    #[serde(default)]
    rate_plans: Vec<DockerHubRatePlan>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DockerHubLogo {
    #[serde(default)]
    large: String,
    #[serde(default)]
    small: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DockerHubPublisher {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DockerHubRatePlan {
    #[serde(default)]
    repositories: Vec<DockerHubRepository>,
    #[serde(default)]
    architectures: Vec<DockerHubArchitecture>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DockerHubRepository {
    #[serde(default)]
    pull_count: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DockerHubArchitecture {
    #[serde(default)]
    name: String,
}

impl DockerHubEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "docker_hub".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Docker Hub container images.".to_string(),
            website: Some("https://hub.docker.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Docker Hub HTTP client");

        DockerHubEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = ((query.offset as i64) / PAGE_SIZE) + 1;
        let from = (pageno - 1) * PAGE_SIZE;
        let size = query.count.to_string();
        let from_str = from.to_string();

        let url = "https://hub.docker.com/api/search/v3/catalog/search";
        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("query", query.query.as_str()),
                ("from", from_str.as_str()),
                ("size", size.as_str()),
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

        let parsed: DockerHubResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let base_url = "https://hub.docker.com";
        let mut results = Vec::new();
        for (i, image) in parsed.results.iter().enumerate() {
            let is_official = image.source == "store" || image.source == "official";
            let path = if is_official { "/_/" } else { "/r/" };
            let url = format!("{}{}{}", base_url, path, image.slug);
            let thumbnail = if !image.logo_url.large.is_empty() {
                image.logo_url.large.clone()
            } else {
                image.logo_url.small.clone()
            };

            let mut popularity: Vec<String> = vec![format!("{} stars", image.star_count)];
            let mut architectures: Vec<String> = Vec::new();
            for plan in image.rate_plans.iter() {
                if let Some(repo) = plan.repositories.first() {
                    if repo.pull_count > 0 {
                        popularity.insert(0, format!("{} pulls", repo.pull_count));
                    }
                }
                for arch in plan.architectures.iter() {
                    if !arch.name.is_empty() {
                        architectures.push(arch.name.clone());
                    }
                }
            }

            let title = if image.name.is_empty() {
                image.slug.clone()
            } else {
                image.name.clone()
            };

            let mut result = SearchResult::new(title, url)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT);
            if !image.short_description.is_empty() {
                result = result.with_snippet(image.short_description.clone());
            }
            result = result
                .with_extra("package_name", serde_json::json!(image.name))
                .with_extra("maintainer", serde_json::json!(image.publisher.name))
                .with_extra("popularity", serde_json::json!(popularity.join(", ")))
                .with_extra("tags", serde_json::json!(architectures));
            if !thumbnail.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
            }
            if !image.updated_at.is_empty() {
                result = result.with_extra("published", serde_json::json!(image.updated_at));
            } else if !image.created_at.is_empty() {
                result = result.with_extra("published", serde_json::json!(image.created_at));
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
impl Engine for DockerHubEngine {
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
        settings.insert(
            "base_url".to_string(),
            "https://hub.docker.com".to_string(),
        );
        settings.insert("page_size".to_string(), "10".to_string());
        settings
    }
}
