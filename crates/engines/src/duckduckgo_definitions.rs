//! DuckDuckGo Instant Answer (definitions) engine implementation
//!
//! queries the DDG instant
//! answer API and surfaces Answer/AbstractURL/RelatedTopics entries.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// DuckDuckGo instant-answer definitions engine
pub struct DuckDuckGoDefinitionsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DdgInstantAnswer {
    #[serde(default, rename = "Heading")]
    heading: String,
    #[serde(default, rename = "Abstract")]
    abstract_text: String,
    #[serde(default, rename = "AbstractText")]
    abstract_text_plain: String,
    #[serde(default, rename = "AbstractURL")]
    abstract_url: String,
    #[serde(default, rename = "Answer")]
    answer: String,
    #[serde(default, rename = "AnswerType")]
    answer_type: String,
    #[serde(default, rename = "Definition")]
    definition: String,
    #[serde(default, rename = "DefinitionURL")]
    definition_url: String,
    #[serde(default, rename = "Image")]
    image: String,
    #[serde(default, rename = "Results")]
    results: Vec<DdgResultItem>,
    #[serde(default, rename = "RelatedTopics")]
    related_topics: Vec<DdgRelatedTopic>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DdgResultItem {
    #[serde(default, rename = "FirstURL")]
    first_url: String,
    #[serde(default, rename = "Text")]
    text: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DdgRelatedTopic {
    #[serde(default, rename = "FirstURL")]
    first_url: String,
    #[serde(default, rename = "Text")]
    text: String,
    #[serde(default, rename = "Name")]
    name: String,
    #[serde(default, rename = "Topics")]
    topics: Vec<DdgResultItem>,
}

impl DuckDuckGoDefinitionsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "duckduckgo_definitions".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "DuckDuckGo instant answers.".to_string(),
            website: Some("https://duckduckgo.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create DDG definitions HTTP client");

        DuckDuckGoDefinitionsEngine { metadata, client }
    }

    /// Minimal HTML tag stripper.
    fn html_to_text(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let mut in_tag = false;
        for ch in s.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => out.push(ch),
                _ => {}
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Is the text a "broken" link text like "http://... words"
    fn is_broken_text(text: &str) -> bool {
        text.starts_with("http") && text.contains(' ')
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.duckduckgo.com/";
        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("format", "json"),
                ("pretty", "0"),
                ("no_redirect", "1"),
                ("d", "1"),
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

        let parsed: DdgInstantAnswer = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results: Vec<SearchResult> = Vec::new();

        // Answer
        if !parsed.answer.is_empty() && parsed.answer_type != "calc" && parsed.answer_type != "ip" {
            let answer = Self::html_to_text(&parsed.answer);
            let url = if parsed.abstract_url.is_empty() {
                format!("https://duckduckgo.com/?q={}", urlencoding::encode(&query.query))
            } else {
                parsed.abstract_url.clone()
            };
            results.push(
                SearchResult::new(answer.clone(), url)
                    .with_snippet(answer.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + results.len() + 1)
                    .with_score(1.0)
                    .with_result_type(ResultType::Web)
                    .with_extra("answer", serde_json::json!(answer)),
            );
        }

        // Results array (FirstURL)
        for item in parsed.results.iter() {
            if item.first_url.is_empty() {
                continue;
            }
            let title = if parsed.heading.is_empty() {
                item.text.clone()
            } else {
                parsed.heading.clone()
            };
            results.push(
                SearchResult::new(title, item.first_url.clone())
                    .with_snippet(item.text.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + results.len() + 1)
                    .with_score(0.9)
                    .with_result_type(ResultType::Web),
            );
        }

        // RelatedTopics
        for topic in parsed.related_topics.iter() {
            if !topic.first_url.is_empty() {
                if Self::is_broken_text(&topic.text) {
                    continue;
                }
                let title = if !topic.text.is_empty() {
                    topic.text.clone()
                } else {
                    parsed.heading.clone()
                };
                results.push(
                    SearchResult::new(title, topic.first_url.clone())
                        .with_snippet(topic.text.clone())
                        .with_engine(self.name())
                        .with_rank(query.offset + results.len() + 1)
                        .with_score(0.8)
                        .with_result_type(ResultType::Web),
                );
            } else if !topic.topics.is_empty() {
                for sub in topic.topics.iter() {
                    if sub.first_url.is_empty() || Self::is_broken_text(&sub.text) {
                        continue;
                    }
                    results.push(
                        SearchResult::new(sub.text.clone(), sub.first_url.clone())
                            .with_snippet(sub.text.clone())
                            .with_engine(self.name())
                            .with_rank(query.offset + results.len() + 1)
                            .with_score(0.7)
                            .with_result_type(ResultType::Web),
                    );
                }
            }
        }

        // Abstract URL as a primary result
        if !parsed.abstract_url.is_empty() {
            let mut content = String::new();
            if !parsed.abstract_text.is_empty() {
                content.push_str(&parsed.abstract_text);
            } else if !parsed.abstract_text_plain.is_empty() {
                content.push_str(&parsed.abstract_text_plain);
            }
            let title = if parsed.heading.is_empty() {
                query.query.clone()
            } else {
                parsed.heading.clone()
            };
            let mut result = SearchResult::new(title, parsed.abstract_url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + results.len() + 1)
                .with_score(0.95)
                .with_result_type(ResultType::Web);
            if !content.is_empty() {
                result = result.with_snippet(content);
            }
            if !parsed.image.is_empty() {
                let img = if parsed.image.starts_with("http") {
                    parsed.image.clone()
                } else {
                    format!("https://duckduckgo.com{}", parsed.image)
                };
                result = result.with_extra("img_src", serde_json::json!(img));
            }
            results.push(result);
        }

        // Truncate to requested count
        results.truncate(query.count);
        Ok(results)
    }
}

#[async_trait]
impl Engine for DuckDuckGoDefinitionsEngine {
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
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://api.duckduckgo.com/".to_string());
        settings.insert("format".to_string(), "json".to_string());
        settings
    }
}
