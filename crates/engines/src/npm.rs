//! NPM search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// NPM search engine
pub struct NpmEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct NpmResponse {
    #[serde(default)]
    objects: Vec<NpmObject>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NpmObject {
    #[serde(default)]
    package: NpmPackage,
    #[serde(default)]
    score: NpmScore,
    #[serde(default)]
    search_score: f64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NpmPackage {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    author: NpmAuthor,
    #[serde(default)]
    links: NpmLinks,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NpmAuthor {
    #[serde(default)]
    username: String,
    #[serde(default)]
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NpmLinks {
    #[serde(default)]
    npm: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    repository: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NpmScore {
    #[serde(default)]
    final_score: f64,
    #[serde(default)]
    detail: NpmScoreDetail,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NpmScoreDetail {
    #[serde(default)]
    quality: f64,
    #[serde(default)]
    popularity: f64,
    #[serde(default)]
    maintenance: f64,
}

impl NpmEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "npm".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "NPM package search".to_string(),
            website: Some("https://npmjs.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create NPM HTTP client");

        NpmEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "https://registry.npmjs.org/-/v1/search?text={}&size={}",
            urlencoding::encode(&query.query),
            query.count
        );


        let response = self.client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;


        if !response.status().is_success() {
            return Err(Error::EngineError(
                "npm".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let npm_response: NpmResponse = serde_json::from_str(&text)
            .map_err(|e| Error::ParseError(format!("Failed to parse NPM response: {}", e)))?;


        let results: Vec<SearchResult> = npm_response.objects
            .into_iter()
            .enumerate()
            .map(|(i, obj)| {
                let package = &obj.package;
                let url = format!("https://npmjs.com/package/{}", package.name);

                let author_name = package.author.name.as_str();
                let content = format!(
                    "[v{}] {} | Author: {} | Score: {:.2}",
                    package.version,
                    package.description.chars().take(200).collect::<String>(),
                    author_name,
                    obj.score.final_score
                );

                let mut result = SearchResult::new(&package.name, &url)
                    .with_snippet(&content)
                    .with_engine("npm")
                    .with_rank(query.offset + i + 1)
                    .with_score(obj.search_score)
                    .with_extra("version", serde_json::json!(package.version))
                    .with_extra("author", serde_json::json!(author_name))
                    .with_extra("quality_score", serde_json::json!(obj.score.detail.quality))
                    .with_extra("popularity_score", serde_json::json!(obj.score.detail.popularity))
                    .with_extra("maintenance_score", serde_json::json!(obj.score.detail.maintenance))
                    .with_extra("keywords", serde_json::json!(package.keywords.join(", ")));

                if !package.links.homepage.is_empty() {
                    result = result.with_extra("homepage", serde_json::json!(package.links.homepage));
                }

                if !package.links.repository.is_empty() {
                    result = result.with_extra("repository", serde_json::json!(package.links.repository));
                }

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Engine for NpmEngine {
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
        settings.insert("base_url".to_string(), "https://registry.npmjs.org".to_string());
        settings.insert("api_endpoint".to_string(), "/-/v1/search".to_string());
        settings
    }
}
