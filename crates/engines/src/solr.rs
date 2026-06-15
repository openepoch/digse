//! Solr search engine implementation
//!
//! Solr is a Lucene-based search platform that is queried via an HTTP REST
//! endpoint (`/solr/<collection>/select`). The engine requires a
//! configured `base_url` + `collection`. Without a configured collection the
//! engine cannot run, so this implementation returns an empty result set with
//! an informational log line when the backend is not configured.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Solr index-backed search engine
pub struct SolrEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    collection: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct SolrResponse {
    #[serde(default)]
    response: SolrResponseBody,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SolrResponseBody {
    #[serde(default)]
    docs: Vec<serde_json::Map<String, serde_json::Value>>,
}

impl SolrEngine {
    pub fn new() -> Self {
        let base_url =
            std::env::var("SOLR_BASE_URL").unwrap_or_else(|_| "http://localhost:8983".to_string());
        let collection = std::env::var("SOLR_COLLECTION").unwrap_or_default();

        let metadata = EngineMetadata {
            name: "solr".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Solr - Lucene-based search platform (HTTP select endpoint).".to_string(),
            website: Some("https://solr.apache.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Solr HTTP client");

        SolrEngine {
            metadata,
            client,
            base_url,
            collection,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if self.collection.is_empty() {
            tracing::info!("solr: SOLR_COLLECTION not set; returning empty");
            return Ok(vec![]);
        }

        let rows = query.count.to_string();
        let start = query.offset.to_string();
        let url = format!(
            "{}/solr/{}/select",
            self.base_url.trim_end_matches('/'),
            self.collection
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("rows", rows.as_str()),
                ("start", start.as_str()),
                ("wt", "json"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let parsed: SolrResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, doc) in parsed.response.docs.iter().enumerate() {
            // Derive a title/url from common field names if present.
            let title = doc
                .get("name")
                .or_else(|| doc.get("title"))
                .map(|v| value_to_string(v))
                .unwrap_or_else(|| "Solr document".to_string());
            let url = doc
                .get("url")
                .map(|v| value_to_string(v))
                .unwrap_or_else(|| format!("{}/doc/{}", url, i + 1));
            let snippet = doc
                .get("description")
                .or_else(|| doc.get("content"))
                .map(|v| value_to_string(v))
                .unwrap_or_default();

            results.push(
                SearchResult::new(title, url)
                    .with_snippet(snippet)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web),
            );
        }

        Ok(results)
    }
}

fn value_to_string(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(value_to_string)
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

#[async_trait]
impl Engine for SolrEngine {
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
        matches!(t, ResultType::Web | ResultType::Files | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), self.base_url.clone());
        s.insert("collection".into(), self.collection.clone());
        s.insert("field_list".into(), "name".into());
        s
    }
}
