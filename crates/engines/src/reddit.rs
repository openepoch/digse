//! Reddit search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Reddit search engine
pub struct RedditEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct RedditResponse {
    #[serde(default)]
    data: RedditData,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RedditData {
    #[serde(default)]
    children: Vec<RedditChild>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RedditChild {
    #[serde(default)]
    data: RedditPost,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RedditPost {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    permalink: String,
    #[serde(default)]
    selftext: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    subreddit: String,
    #[serde(default)]
    score: i64,
    #[serde(default)]
    num_comments: i64,
    #[serde(default)]
    created_utc: f64,
    #[serde(default)]
    over_18: bool,
}

impl RedditEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "reddit".to_string(),
            category: EngineCategory::Social,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Reddit social media search".to_string(),
            website: Some("https://reddit.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Reddit HTTP client");

        RedditEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "https://www.reddit.com/search.json?q={}&limit={}&sort=relevance&t=all",
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
                "reddit".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let reddit_response: RedditResponse = serde_json::from_str(&text)
            .map_err(|e| Error::ParseError(format!("Failed to parse Reddit response: {}", e)))?;


        let results: Vec<SearchResult> = reddit_response.data.children
            .into_iter()
            .enumerate()
            .filter(|(_, _child)| {
                // Filter out NSFW content if needed (for now allow all)
                true
            })
            .map(|(i, child)| {
                let post = &child.data;

                // Use the permalink for the Reddit URL, and the post url for external content
                let reddit_url = format!("https://reddit.com{}", post.permalink);
                let display_url = if post.url.starts_with("http") && !post.url.contains("reddit.com") {
                    &post.url
                } else {
                    &reddit_url
                };

                // Create content from selftext or use a placeholder
                let content = if !post.selftext.is_empty() {
                    post.selftext.chars().take(500).collect()
                } else {
                    format!("Posted by u/{} in r/{} - {} points, {} comments",
                        post.author, post.subreddit, post.score, post.num_comments)
                };

                let mut result = SearchResult::new(&post.title, display_url)
                    .with_snippet(&content)
                    .with_engine("reddit")
                    .with_rank(query.offset + i + 1)
                    .with_score((post.score as f64).ln().max(1.0))
                    .with_extra("subreddit", serde_json::json!(post.subreddit.clone()))
                    .with_extra("author", serde_json::json!(post.author.clone()))
                    .with_extra("score", serde_json::json!(post.score))
                    .with_extra("comments", serde_json::json!(post.num_comments))
                    .with_extra("permalink", serde_json::json!(reddit_url));

                if post.url != reddit_url {
                    result = result.with_extra("external_url", serde_json::json!(post.url.clone()));
                }

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Engine for RedditEngine {
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
        *result_type == ResultType::Social || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://reddit.com".to_string());
        settings.insert("search_endpoint".to_string(), "/search.json".to_string());
        settings.insert("sort".to_string(), "relevance".to_string());
        settings.insert("time_range".to_string(), "all".to_string());
        settings
    }
}
