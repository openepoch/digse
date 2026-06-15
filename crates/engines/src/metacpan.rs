//! MetaCPAN search engine implementation
//!
//! queries the MetaCPAN Elasticsearch
//! API for Perl module documentation. Category: it / packages.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// MetaCPAN (Perl modules) search engine
pub struct MetacpanEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    page_size: usize,
}

const SEARCH_URL: &str = "https://fastapi.metacpan.org/v1/file/_search";

#[derive(Debug, Deserialize)]
struct MetacpanResponse {
    #[serde(default)]
    hits: MetacpanHits,
}

#[derive(Debug, Deserialize, Default)]
struct MetacpanHits {
    #[serde(default)]
    hits: Vec<MetacpanHit>,
}

#[derive(Debug, Deserialize, Default)]
struct MetacpanHit {
    #[serde(default)]
    #[serde(rename = "_source")]
    source: MetacpanSource,
}

#[derive(Debug, Deserialize, Default)]
struct MetacpanSource {
    #[serde(default)]
    documentation: String,
    #[serde(default, rename = "abstract")]
    abstract_text: Option<String>,
}

impl MetacpanEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "metacpan".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "MetaCPAN - Perl module search.".to_string(),
            website: Some("https://metacpan.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create MetaCPAN HTTP client");

        MetacpanEngine {
            metadata,
            client,
            page_size: 20,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let from = query.offset;
        // Build the JSON body manually to mirror the exact ES DSL shape.
        let body = serde_json::json!({
            "query": {
                "multi_match": {
                    "type": "most_fields",
                    "fields": ["documentation", "documentation.*"],
                    "analyzer": "camelcase",
                    "query": query.query
                }
            },
            "filter": {
                "bool": {
                    "must": [
                        {"exists": {"field": "documentation"}},
                        {"term": {"status": "latest"}},
                        {"term": {"indexed": 1}},
                        {"term": {"authorized": 1}}
                    ]
                }
            },
            "sort": [
                {"_score": {"order": "desc"}},
                {"date": {"order": "desc"}}
            ],
            "_source": ["documentation", "abstract"],
            "size": self.page_size.min(query.count.max(self.page_size)),
            "from": from
        });

        let response = self
            .client
            .post(SEARCH_URL)
            .header("User-Agent", "digse/0.0.1")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
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
        let parsed: MetacpanResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, hit) in parsed.hits.hits.iter().enumerate() {
            let module = hit.source.documentation.clone();
            if module.is_empty() {
                continue;
            }
            let url = format!("https://metacpan.org/pod/{}", module);
            let abstract_text = hit.source.abstract_text.clone().unwrap_or_default();
            let mut result = SearchResult::new(module.clone(), url)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT)
                .with_extra("package_name", serde_json::json!(module))
                .with_extra("source", serde_json::json!("metacpan"));
            if !abstract_text.is_empty() {
                result = result.with_snippet(abstract_text);
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
impl Engine for MetacpanEngine {
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
        settings.insert("search_url".to_string(), SEARCH_URL.to_string());
        settings.insert("page_size".to_string(), self.page_size.to_string());
        settings
    }
}
