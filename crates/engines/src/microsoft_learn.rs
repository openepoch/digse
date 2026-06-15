//! Microsoft Learn search engine implementation
//!
//! queries Microsoft Learn's
//! internal search API. Category: it.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Microsoft Learn (technical docs) search engine
pub struct MicrosoftLearnEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    page_size: usize,
}

const SEARCH_API: &str = "https://learn.microsoft.com/api/search";

#[derive(Debug, Deserialize)]
struct MsLearnResponse {
    #[serde(default)]
    results: Vec<MsLearnResult>,
}

#[derive(Debug, Deserialize, Default)]
struct MsLearnResult {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
}

impl MicrosoftLearnEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "microsoft_learn".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Microsoft Learn - Microsoft technical knowledge base.".to_string(),
            website: Some("https://learn.microsoft.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Microsoft Learn HTTP client");

        MicrosoftLearnEngine {
            metadata,
            client,
            page_size: 10,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let locale = query
            .language
            .as_ref()
            .map(|l| l.clone())
            .unwrap_or_else(|| "en-us".to_string());
        let top = self.page_size.to_string();
        let skip = query.offset.to_string();

        let response = self
            .client
            .get(SEARCH_API)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("search", query.query.as_str()),
                ("locale", locale.as_str()),
                ("scoringprofile", "semantic-answers"),
                ("facet", "category"),
                ("facet", "products"),
                ("facet", "tags"),
                ("$top", top.as_str()),
                ("$skip", skip.as_str()),
                ("expandScope", "true"),
                ("includeQuestion", "false"),
                ("applyOperator", "false"),
                ("partnerId", "LearnSite"),
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
        let parsed: MsLearnResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.results.iter().enumerate() {
            if item.url.is_empty() {
                continue;
            }
            let mut result = SearchResult::new(item.title.clone(), item.url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT)
                .with_extra("source", serde_json::json!("learn.microsoft.com"));
            if !item.description.is_empty() {
                result = result.with_snippet(item.description.clone());
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
impl Engine for MicrosoftLearnEngine {
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
        settings.insert("search_api".to_string(), SEARCH_API.to_string());
        settings.insert("page_size".to_string(), self.page_size.to_string());
        settings
    }
}
