//! Open Semantic Search engine implementation (general; JSON via local Solr)
//!
//! Open Semantic Search is a
//! self-hosted Solr-based search appliance. The default backend is
//! `http://localhost:8983/solr/opensemanticsearch/`. If the backend is not
//! reachable, the engine returns an empty result set gracefully.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Open Semantic Search engine
pub struct OpenSemanticEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SolrResponse {
    #[serde(default)]
    response: SolrResponseBody,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SolrResponseBody {
    #[serde(default)]
    docs: Vec<SolrDoc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SolrDoc {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title_txt_txt_en: Option<String>,
    #[serde(default)]
    content_txt: Vec<String>,
    #[serde(default)]
    file_modified_dt: Vec<String>,
}

impl OpenSemanticEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "opensemantic".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Open Semantic Search - Self-hosted Solr-based search.".to_string(),
            website: Some("https://www.opensemanticsearch.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Open Semantic Search HTTP client");

        OpenSemanticEngine {
            metadata,
            client,
            base_url: "http://localhost:8983/solr/opensemanticsearch/".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!("{}query?q={}", self.base_url, urlencoding::encode(&query.query));

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .send()
            .await;

        // The Open Semantic backend is a local service that is usually not
        // available in this environment; fall back to an empty result set.
        let resp = match resp {
            Ok(r) => r,
            Err(e) => {
                tracing::info!(
                    "opensemantic backend unreachable ({}); returning empty",
                    e
                );
                return Ok(vec![]);
            }
        };

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: SolrResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, doc) in parsed.response.docs.iter().enumerate() {
            let title = doc
                .title_txt_txt_en
                .clone()
                .unwrap_or_else(|| "Open Semantic Search result".to_string());
            let content = doc.content_txt.first().cloned().unwrap_or_default();
            let published = doc.file_modified_dt.first().cloned().unwrap_or_default();
            results.push(
                SearchResult::new(&title, &doc.id)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web)
                    .with_extra("published", serde_json::json!(published)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for OpenSemanticEngine {
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
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), self.base_url.clone());
        s
    }
}
