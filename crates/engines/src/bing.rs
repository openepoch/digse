//! Bing search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Bing search engine
pub struct BingEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct BingResponse {
    #[serde(default, rename = "webPages")]
    web_pages: Option<BingWebPages>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BingWebPages {
    #[serde(default)]
    value: Vec<BingWebResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BingWebResult {
    #[serde(default)]
    name: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    snippet: String,
    #[serde(default, rename = "displayUrl")]
    display_url: String,
}

impl BingEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bing".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Bing - Microsoft's search engine.".to_string(),
            website: Some("https://www.bing.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Bing HTTP client");

        BingEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<BingWebResult>> {
        let url = "https://api.bing.microsoft.com/v7.0/search";

        let count = query.count.to_string();
        let offset = query.offset.to_string();

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("q", query.query.as_str()),
                ("count", count.as_str()),
                ("offset", offset.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            // Return empty results for now - would need API key in production
            return Ok(Vec::new());
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        // Try JSON parsing
        if let Ok(bing_response) = serde_json::from_str::<BingResponse>(&text) {
            if let Some(web_pages) = bing_response.web_pages {
                return Ok(web_pages.value);
            }
        }

        Ok(Vec::new())
    }
}

#[async_trait]
impl Engine for BingEngine {
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
        let bing_results = self.fetch_results(query).await?;

        let mut results = Vec::new();
        for (i, result) in bing_results.iter().enumerate() {
            let search_result = SearchResult::new(&result.name, &result.url)
                .with_snippet(&result.snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05));

            results.push(search_result);
        }

        Ok(results)
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> std::collections::HashMap<String, String> {
        let mut settings = std::collections::HashMap::new();
        settings.insert("type".to_string(), "bing".to_string());
        settings.insert("api_required".to_string(), "true".to_string());
        settings
    }
}
