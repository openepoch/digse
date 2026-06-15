//! HackerNews search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// HackerNews search engine
pub struct HackerNewsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct HackerNewsResponse {
    #[serde(default)]
    hits: Vec<HackerNewsHit>,
}

#[derive(Debug, Serialize, Deserialize)]
struct HackerNewsHit {
    #[serde(default)]
    object_id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    points: i64,
    #[serde(default)]
    num_comments: i64,
    #[serde(default)]
    created_at_i: i64,
    #[serde(default)]
    object_type: String,
}

impl HackerNewsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "hackernews".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "HackerNews tech news search".to_string(),
            website: Some("https://news.ycombinator.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create HackerNews HTTP client");

        HackerNewsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "http://hn.algolia.com/api/v1/search?query={}&hitsPerPage={}&tags=story",
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
                "hackernews".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let hn_response: HackerNewsResponse = serde_json::from_str(&text)
            .map_err(|e| Error::ParseError(format!("Failed to parse HackerNews response: {}", e)))?;


        let results: Vec<SearchResult> = hn_response.hits
            .into_iter()
            .enumerate()
            .map(|(i, hit)| {
                // Use HackerNews URL if no external URL
                let url = if hit.url.is_empty() || !hit.url.starts_with("http") {
                    format!("https://news.ycombinator.com/item?id={}", hit.object_id)
                } else {
                    hit.url.clone()
                };

                let content = format!(
                    "[{} points | {} comments] by {}",
                    hit.points, hit.num_comments, hit.author
                );

                let result = SearchResult::new(&hit.title, &url)
                    .with_snippet(&content)
                    .with_engine("hackernews")
                    .with_rank(query.offset + i + 1)
                    .with_score((hit.points as f64).ln().max(1.0))
                    .with_extra("points", serde_json::json!(hit.points))
                    .with_extra("num_comments", serde_json::json!(hit.num_comments))
                    .with_extra("author", serde_json::json!(hit.author))
                    .with_extra("object_id", serde_json::json!(hit.object_id))
                    .with_extra("created_at", serde_json::json!(hit.created_at_i))
                    .with_extra("object_type", serde_json::json!(hit.object_type));

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Engine for HackerNewsEngine {
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
        *result_type == ResultType::News || *result_type == ResultType::IT || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "http://hn.algolia.com".to_string());
        settings.insert("api_endpoint".to_string(), "/api/v1/search".to_string());
        settings.insert("tags".to_string(), "story".to_string());
        settings
    }
}
