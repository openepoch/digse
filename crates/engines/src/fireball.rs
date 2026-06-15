//! Fireball search engine implementation
//!
//! Fireball is a Germany-based
//! privacy-focused search engine. The reference engine obtains a settings
//! cookie via a POST to `/settings`, then GETs `/getResults/?f=web&q=...`.
//! Category: general.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Fireball web search engine (JSON getResults API)
pub struct FireballEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct FireballResult {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    page_age: Option<String>,
}

impl FireballEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "fireball".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Fireball - Germany-based privacy-focused web search.".to_string(),
            website: Some("https://fireball.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Fireball HTTP client");

        FireballEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://fireball.com";
        let referer = format!("{}/search?f=web&q={}", base_url, urlencoding::encode(&query.query));
        let url = format!(
            "{}/getResults/?f=web&q={}",
            base_url,
            urlencoding::encode(&query.query)
        );

        // Obtain a settings cookie so the request is treated as configured.
        let _ = self
            .client
            .post(format!("{}/settings", base_url))
            .header("User-Agent", "digse/0.1.0")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&[
                ("action", "save"),
                ("language", "en"),
                ("market", "US"),
                ("adprovider", "automatic"),
                ("target", "_blank"),
                ("tiles", "on"),
                (
                    "safesearch",
                    if query.safe_search { "moderate" } else { "off" },
                ),
            ])
            .send()
            .await;

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .header("Referer", &referer)
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

        let v: serde_json::Value = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };
        let web_results = v
            .get("web")
            .and_then(|w| w.get("results"))
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));

        let parsed: Vec<FireballResult> = match serde_json::from_value(web_results) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, r) in parsed.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let mut result = SearchResult::new(r.title.clone(), r.url.clone())
                .with_snippet(r.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("source", serde_json::json!("fireball"));
            if let Some(pa) = &r.page_age {
                result = result.with_extra("published", serde_json::json!(pa));
            }
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for FireballEngine {
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
        settings.insert("base_url".to_string(), "https://fireball.com".to_string());
        settings.insert("fireball_category".to_string(), "web".to_string());
        settings
    }
}
