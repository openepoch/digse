//! Wikidata search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Wikidata search engine
pub struct WikidataEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct WikidataResponse {
    #[serde(default)]
    search: Vec<WikidataItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WikidataItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    url: String,
    #[serde(alias = "repository")]
    #[serde(default)]
    repository: String,
}

impl WikidataEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "wikidata".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Wikidata knowledge graph search".to_string(),
            website: Some("https://wikidata.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Wikidata HTTP client");

        WikidataEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "https://www.wikidata.org/w/api.php?action=wbsearchentities&search={}&language=en&format=json&limit={}",
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
                "wikidata".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let wikidata_response: WikidataResponse = serde_json::from_str(&text)
            .map_err(|e| Error::ParseError(format!("Failed to parse Wikidata response: {}", e)))?;


        let results: Vec<SearchResult> = wikidata_response.search
            .into_iter()
            .enumerate()
            .map(|(i, item)| {
                let url = if !item.url.is_empty() {
                    item.url.clone()
                } else {
                    format!("https://www.wikidata.org/wiki/{}", item.id)
                };

                let mut result = SearchResult::new(&item.label, &url)
                    .with_snippet(&item.description)
                    .with_engine("wikidata")
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.1))
                    .with_extra("wikidata_id", serde_json::json!(item.id));

                if !item.repository.is_empty() {
                    result = result.with_extra("repository", serde_json::json!(item.repository));
                }

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Engine for WikidataEngine {
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
        *result_type == ResultType::Web || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://wikidata.org".to_string());
        settings.insert("api_endpoint".to_string(), "/w/api.php".to_string());
        settings.insert("language".to_string(), "en".to_string());
        settings
    }
}
