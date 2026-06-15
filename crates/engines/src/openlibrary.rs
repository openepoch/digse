//! Open Library search engine implementation (general/books; JSON)
//!
//! Queries the Open Library
//! search API at `https://openlibrary.org/search.json`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Open Library book search engine
pub struct OpenLibraryEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const RESULTS_PER_PAGE: i64 = 10;

#[derive(Debug, Serialize, Deserialize)]
struct OpenLibraryResponse {
    #[serde(default)]
    docs: Vec<OpenLibraryDoc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenLibraryDoc {
    #[serde(default)]
    key: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    author_name: Vec<String>,
    #[serde(default)]
    first_sentence: Vec<String>,
    #[serde(default)]
    isbn: Vec<String>,
    #[serde(default)]
    subject: Vec<String>,
    #[serde(default)]
    place: Vec<String>,
    #[serde(default)]
    publish_date: Vec<String>,
    #[serde(default)]
    first_publish_year: Option<i32>,
    #[serde(default)]
    lending_identifier_s: Vec<String>,
}

impl OpenLibraryEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "openlibrary".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Open Library - Open catalog of every book ever published.".to_string(),
            website: Some("https://openlibrary.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Open Library HTTP client");

        OpenLibraryEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://openlibrary.org/search.json";
        let page = ((query.offset / RESULTS_PER_PAGE as usize) + 1).to_string();
        let limit = RESULTS_PER_PAGE.to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page.as_str()),
                ("limit", limit.as_str()),
                ("fields", "*"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: OpenLibraryResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, doc) in parsed.docs.iter().enumerate() {
            let url = format!("https://openlibrary.org{}", doc.key);

            let authors = if doc.author_name.is_empty() {
                "Unknown Author".to_string()
            } else {
                doc.author_name.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
            };

            let content = doc.first_sentence.join(" / ");

            let thumbnail = doc
                .lending_identifier_s
                .first()
                .map(|id| format!("https://archive.org/services/img/{}", id))
                .unwrap_or_default();

            let published = doc
                .publish_date
                .first()
                .cloned()
                .or_else(|| doc.first_publish_year.map(|y| y.to_string()))
                .unwrap_or_default();

            let mut snippet_parts = vec![format!("Authors: {}", authors)];
            if !published.is_empty() {
                snippet_parts.push(format!("Published: {}", published));
            }
            if !content.is_empty() {
                snippet_parts.push(content.clone());
            }

            let isbns: Vec<String> = doc.isbn.iter().take(5).cloned().collect();

            results.push(
                SearchResult::new(&doc.title, &url)
                    .with_snippet(snippet_parts.join(" | "))
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web)
                    .with_extra("authors", serde_json::json!(authors))
                    .with_extra("isbn", serde_json::json!(isbns))
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("year", serde_json::json!(doc.first_publish_year))
                    .with_extra("published", serde_json::json!(published)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for OpenLibraryEngine {
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
        s.insert("base_url".to_string(), "https://openlibrary.org".to_string());
        s.insert("results_per_page".to_string(), RESULTS_PER_PAGE.to_string());
        s
    }
}
