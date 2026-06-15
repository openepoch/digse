//! Il Post search engine implementation
//!
//! Italian newspaper JSON search API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Il Post (Italian news) search engine
pub struct IlPostEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IlPostResponse {
    #[serde(default)]
    docs: Vec<IlPostDoc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IlPostDoc {
    #[serde(default)]
    link: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    image: Option<String>,
}

impl IlPostEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "il_post".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Il Post - Italian online newspaper.".to_string(),
            website: Some("https://www.ilpost.it".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Il Post HTTP client");

        IlPostEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.ilpost.org/search/api/site_search/";
        let pageno = ((query.offset / query.count.max(1)) + 1).to_string();

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("qs", query.query.as_str()),
                ("pg", pageno.as_str()),
                ("sort", "date_d"),
                ("filters", "ctype:articoli"),
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
        let parsed: IlPostResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, doc) in parsed.docs.iter().enumerate() {
            if doc.title.is_empty() || doc.link.is_empty() {
                continue;
            }
            let mut result = SearchResult::new(doc.title.clone(), doc.link.clone())
                .with_snippet(doc.summary.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::News)
                .with_extra("source", serde_json::json!("Il Post"));
            if let Some(img) = &doc.image {
                if !img.is_empty() {
                    result = result.with_extra("img_src", serde_json::json!(img));
                }
            }
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for IlPostEngine {
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
        matches!(t, ResultType::News | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://api.ilpost.org".into());
        s.insert("api_endpoint".into(), "/search/api/site_search/".into());
        s
    }
}
