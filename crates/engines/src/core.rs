//! CORE (core.ac.uk) academic search engine implementation.
//! Paid scholarly literature API (requires API key).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// CORE (Connecting Repositories) academic search engine - paid API.
pub struct CoreEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CoreResponse {
    #[serde(default)]
    results: Vec<CoreWork>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CoreWork {
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    title: String,
    #[serde(default)]
    doi: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    #[serde(rename = "sourceFulltextUrls")]
    source_fulltext_urls: serde_json::Value,
    #[serde(default)]
    full_text: Option<String>,
    #[serde(default)]
    #[serde(rename = "publishedDate")]
    published_date: Option<String>,
    #[serde(default)]
    #[serde(rename = "depositedDate")]
    deposited_date: Option<String>,
    #[serde(default)]
    document_type: Option<String>,
    #[serde(default)]
    field_of_study: Option<Vec<String>>,
    #[serde(default)]
    authors: Option<Vec<CoreAuthor>>,
    #[serde(default)]
    contributors: Option<Vec<String>>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    journals: Option<Vec<CoreJournal>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CoreAuthor {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CoreJournal {
    #[serde(default)]
    title: String,
}

impl CoreEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("CORE_API_KEY")
            .ok()
            .filter(|s| !s.is_empty() && s != "unset" && s != "unknown" && s != "...");
        let metadata = EngineMetadata {
            name: "core".to_string(),
            category: EngineCategory::Science,
            enabled: api_key.is_some(),
            requires_auth: true,
            timeout_seconds: 20,
            description: "CORE - world scholarly literature aggregator.".to_string(),
            website: Some("https://core.ac.uk".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create CORE HTTP client");
        CoreEngine {
            metadata,
            client,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                tracing::warn!("core requires CORE_API_KEY");
                return Ok(vec![]);
            }
        };
        let url = "https://api.core.ac.uk/v3/search/works/";
        let offset = (query.offset * 10).to_string();
        let limit = "10".to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .query(&[
                ("q", query.query.as_str()),
                ("offset", offset.as_str()),
                ("limit", limit.as_str()),
                ("sort", "relevance"),
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
        let parsed: CoreResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let mut idx = 0;
        for work in parsed.results.iter() {
            if work.title.is_empty() {
                continue;
            }
            // Resolve a URL: DOI -> core work id -> downloadUrl -> sourceFulltextUrls.
            let url = if let Some(doi) = &work.doi {
                if !doi.is_empty() {
                    Some(format!("https://doi.org/{}", doi))
                } else {
                    None
                }
            } else {
                None
            };
            let url = url.or_else(|| {
                work.id.as_str().map(|i| format!("https://core.ac.uk/works/{}", i))
            });
            let url = url
                .or_else(|| work.download_url.clone())
                .or_else(|| match &work.source_fulltext_urls {
                    serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
                    serde_json::Value::Array(a) if !a.is_empty() => {
                        a.first().and_then(|v| v.as_str()).map(|s| s.to_string())
                    }
                    _ => None,
                });
            let url = match url {
                Some(u) if !u.is_empty() => u,
                _ => continue,
            };

            let authors: Vec<String> = work
                .authors
                .as_ref()
                .map(|a| a.iter().map(|x| x.name.clone()).collect())
                .unwrap_or_default();
            let journals: Vec<String> = work
                .journals
                .as_ref()
                .map(|j| j.iter().map(|x| x.title.clone()).collect())
                .unwrap_or_default();
            let tags = work.field_of_study.clone().unwrap_or_default();
            let contributors = work.contributors.clone().unwrap_or_default();
            let publisher = work.publisher.clone().unwrap_or_default();
            let document_type = work.document_type.clone().unwrap_or_default();
            let published = work
                .published_date
                .clone()
                .or_else(|| work.deposited_date.clone())
                .unwrap_or_default();
            let snippet = format!(
                "{}{}{}{}{}",
                authors.join(", "),
                if journals.is_empty() {
                    String::new()
                } else {
                    format!(" | Journal: {}", journals.join(", "))
                },
                if publisher.is_empty() {
                    String::new()
                } else {
                    format!(" | Publisher: {}", publisher)
                },
                if document_type.is_empty() {
                    String::new()
                } else {
                    format!(" | Type: {}", document_type)
                },
                if contributors.is_empty() {
                    String::new()
                } else {
                    format!(" | Contributors: {}", contributors.join(", "))
                },
            );
            let pdf_url = work.download_url.clone().unwrap_or_default();

            results.push(
                SearchResult::new(work.title.clone(), url)
                    .with_snippet(snippet)
                    .with_engine(self.name())
                    .with_rank(query.offset + idx + 1)
                    .with_score(1.0 - (idx as f64 * 0.03))
                    .with_result_type(ResultType::Academic)
                    .with_extra("authors", serde_json::json!(authors))
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("doi", serde_json::json!(work.doi))
                    .with_extra("pdf_url", serde_json::json!(pdf_url))
                    .with_extra("tags", serde_json::json!(tags)),
            );
            idx += 1;
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for CoreEngine {
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
        matches!(t, ResultType::Academic | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://api.core.ac.uk/v3/search/works/".into());
        s
    }
}
