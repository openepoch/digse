//! Dogpile search engine implementation
//!
//! a metasearch engine by System1. The
//! reference hits the JSON POST API at `https://www.dogpile.com/api/search`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Dogpile metasearch engine
pub struct DogpileEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct DogpileResponse {
    #[serde(default)]
    results: Vec<DogpileResult>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DogpileResult {
    #[serde(default)]
    #[serde(rename = "clickUrl")]
    click_url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
}

impl DogpileEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "dogpile".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Dogpile metasearch.".to_string(),
            website: Some("https://www.dogpile.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Dogpile HTTP client");

        DogpileEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://www.dogpile.com/api/search";
        let page = ((query.offset / 10) + 1).to_string();
        let qadf = if query.safe_search { "heavy" } else { "none" };

        let body = serde_json::json!({
            "q": query.query,
            "qadf": qadf,
            "page": page,
        });

        let response = self
            .client
            .post(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&body)
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

        let parsed: DogpileResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.results.iter().enumerate() {
            if item.click_url.is_empty() {
                continue;
            }
            let title = if item.title.is_empty() {
                "Dogpile result".to_string()
            } else {
                Self::html_to_text(&item.title)
            };
            let result = SearchResult::new(title, item.click_url.clone())
                .with_snippet(Self::html_to_text(&item.description))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
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
        out.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[async_trait]
impl Engine for DogpileEngine {
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
        settings.insert(
            "base_url".to_string(),
            "https://www.dogpile.com".to_string(),
        );
        settings.insert("dogpile_categ".to_string(), "search".to_string());
        settings
    }
}
