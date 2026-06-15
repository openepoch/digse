//! StackOverflow search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// StackOverflow search engine
pub struct StackOverflowEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct StackOverflowResponse {
    #[serde(default)]
    items: Vec<StackOverflowItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct StackOverflowItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    score: i64,
    #[serde(default)]
    answer_count: i64,
    #[serde(default)]
    view_count: i64,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    owner: StackOverflowOwner,
    #[serde(default)]
    creation_date: f64,
    #[serde(default)]
    question_id: i64,
    #[serde(default)]
    is_answered: bool,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct StackOverflowOwner {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    reputation: i64,
}

impl StackOverflowEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "stackoverflow".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "StackOverflow Q&A search".to_string(),
            website: Some("https://stackoverflow.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create StackOverflow HTTP client");

        StackOverflowEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "https://api.stackexchange.com/2.3/search/advanced?order=desc&sort=activity&accepted=True&answers=1&title={}&site=stackoverflow&pagesize={}",
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
                "stackoverflow".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let so_response: StackOverflowResponse = serde_json::from_str(&text)
            .map_err(|e| Error::ParseError(format!("Failed to parse StackOverflow response: {}", e)))?;


        let results: Vec<SearchResult> = so_response.items
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                // Clean up HTML tags from body
                let clean_body = item.body
                    .replace("<p>", "")
                    .replace("</p>", " ")
                    .replace("<code>", "")
                    .replace("</code>", "")
                    .replace("<pre>", "")
                    .replace("</pre>", " ")
                    .chars()
                    .take(300)
                    .collect::<String>()
                    .trim()
                    .to_string();

                let content = format!(
                    "[Score: {} | Answers: {} | Views: {}] {}",
                    item.score, item.answer_count, item.view_count, clean_body
                );

                let mut result = SearchResult::new(&item.title, &item.link)
                    .with_snippet(&content)
                    .with_engine("stackoverflow")
                    .with_rank(query.offset + i + 1)
                    .with_score((item.score as f64).max(1.0))
                    .with_extra("score", serde_json::json!(item.score))
                    .with_extra("answer_count", serde_json::json!(item.answer_count))
                    .with_extra("view_count", serde_json::json!(item.view_count))
                    .with_extra("tags", serde_json::json!(item.tags.join(", ")))
                    .with_extra("question_id", serde_json::json!(item.question_id))
                    .with_extra("is_answered", serde_json::json!(item.is_answered))
                    .with_extra("author", serde_json::json!(item.owner.display_name))
                    .with_extra("author_reputation", serde_json::json!(item.owner.reputation));

                if !item.tags.is_empty() {
                    result = result.with_extra("primary_tag", serde_json::json!(item.tags.first().unwrap_or(&String::new())));
                }

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Engine for StackOverflowEngine {
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
        settings.insert("base_url".to_string(), "https://api.stackexchange.com".to_string());
        settings.insert("api_version".to_string(), "2.3".to_string());
        settings.insert("site".to_string(), "stackoverflow".to_string());
        settings.insert("order".to_string(), "desc".to_string());
        settings.insert("sort".to_string(), "activity".to_string());
        settings
    }
}
