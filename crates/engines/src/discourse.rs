//! Discourse forum search engine implementation
//!
//! searches an arbitrary Discourse forum
//! via its `/search.json` endpoint. The forum base URL is configurable; the
//! default points to the official Discourse meta forum.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Discourse forum search engine
pub struct DiscourseEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    api_order: String,
    api_key: Option<String>,
    api_username: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DiscourseResponse {
    #[serde(default)]
    posts: Vec<DiscoursePost>,
    #[serde(default)]
    topics: Vec<DiscourseTopic>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DiscoursePost {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    topic_id: i64,
    #[serde(default)]
    username: String,
    #[serde(default)]
    blurb: String,
    #[serde(default)]
    avatar_template: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DiscourseTopic {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    posts_count: i64,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    closed: bool,
    #[serde(default)]
    has_accepted_answer: bool,
}

impl DiscourseEngine {
    pub fn new() -> Self {
        let base_url = std::env::var("DISCOURSE_BASE_URL")
            .unwrap_or_else(|_| "https://meta.discourse.org".to_string());
        let metadata = EngineMetadata {
            name: "discourse".to_string(),
            category: EngineCategory::Social,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Discourse forum search.".to_string(),
            website: Some("https://discourse.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Discourse HTTP client");

        let api_key = std::env::var("DISCOURSE_API_KEY").ok().filter(|k| !k.is_empty());
        let api_username = std::env::var("DISCOURSE_API_USERNAME").ok().filter(|k| !k.is_empty());

        DiscourseEngine {
            metadata,
            client,
            base_url,
            api_order: "likes".to_string(),
            api_key,
            api_username,
        }
    }

    /// Minimal HTML-unescape for common entities.
    fn html_unescape(s: &str) -> String {
        s.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&#x27;", "'")
            .replace("&nbsp;", " ")
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.trim().len() <= 2 {
            return Ok(vec![]);
        }

        let url = format!("{}/search.json", self.base_url);
        let q = format!("{} order:{}", query.query, self.api_order);
        let page = ((query.offset / 10) + 1).to_string();

        let mut req = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json, text/javascript, */*; q=0.01")
            .header("X-Requested-With", "XMLHttpRequest")
            .query(&[("q", q.as_str()), ("page", page.as_str())]);

        if let Some(key) = &self.api_key {
            req = req.header("Api-Key", key);
        }
        if let Some(u) = &self.api_username {
            req = req.header("Api-Username", u);
        }

        let response = req
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

        let parsed: DiscourseResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let topics_by_id: HashMap<i64, &DiscourseTopic> = parsed
            .topics
            .iter()
            .map(|t| (t.id, t))
            .collect();

        let mut results = Vec::new();
        for (i, post) in parsed.posts.iter().enumerate() {
            let post_url = format!("{}/p/{}", self.base_url, post.id);
            let topic = topics_by_id.get(&post.topic_id);
            let title = topic
                .map(|t| Self::html_unescape(&t.title))
                .unwrap_or_else(|| "Discourse post".to_string());
            let blurb = Self::html_unescape(&post.blurb);
            let comments = topic.map(|t| t.posts_count).unwrap_or(0);

            let mut metadata: Vec<String> = Vec::new();
            if !post.username.is_empty() {
                metadata.push(format!("@{}", post.username));
            }
            if comments > 1 {
                metadata.push(format!("comments: {}", comments));
            }
            if let Some(t) = topic {
                if t.has_accepted_answer {
                    metadata.push("answered".to_string());
                } else if comments > 1 {
                    metadata.push(if t.closed { "closed" } else { "open" }.to_string());
                }
            }

            let mut result = SearchResult::new(title, post_url)
                .with_snippet(blurb.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Social)
                .with_extra("author", serde_json::json!(post.username));
            if !metadata.is_empty() {
                result = result.with_extra("metadata", serde_json::json!(metadata.join(" | ")));
            }
            if let Some(t) = topic {
                if !t.created_at.is_empty() {
                    result = result.with_extra("published", serde_json::json!(t.created_at));
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
impl Engine for DiscourseEngine {
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
        settings.insert("search_endpoint".to_string(), "/search.json".to_string());
        settings.insert("api_order".to_string(), self.api_order.clone());
        settings
    }
}
