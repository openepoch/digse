//! Mwmbl search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Mwmbl search engine
pub struct MwmblEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct MwmblResponse {
    results: Vec<MwmblResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MwmblResult {
    title: String,
    url: String,
    #[serde(default)]
    snippet: String,
}

impl MwmblEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "mwmbl".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 5,
            description: "Mwmbl - The free, open, and transparent search engine".to_string(),
            website: Some("https://mwmbl.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("Failed to create Mwmbl HTTP client");

        MwmblEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<MwmblResult>> {
        let url = format!(
            "https://mwmbl.org/api/search?q={}&s={}",
            urlencoding::encode(&query.query),
            query.count
        );


        let response = self.client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;


        if !response.status().is_success() {
            return Err(Error::EngineError(
                "mwmbl".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let mwmbl_response: MwmblResponse = serde_json::from_str(&text)?;

        Ok(mwmbl_response.results)
    }
}

#[async_trait]
impl Engine for MwmblEngine {
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
        let mwmbl_results = self.fetch_results(query).await?;

        let results: Vec<SearchResult> = mwmbl_results
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                let mut result = SearchResult::new(&r.title, &r.url);
                if !r.snippet.is_empty() {
                    result = result.with_snippet(&r.snippet);
                }
                result
                    .with_engine("mwmbl")
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.1))
            })
            .collect();

        Ok(results)
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        *result_type == ResultType::Web || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://mwmbl.org".to_string());
        settings.insert("api_endpoint".to_string(), "/api/search".to_string());
        settings
    }
}