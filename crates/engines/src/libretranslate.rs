//! LibreTranslate search engine implementation
//!
//! Free and open-source machine
//! translation API. The public libretranslate.com endpoint requires an API key;
//! a self-hosted instance works without one. Returns a single translation
//! result (with alternatives in the snippet).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// LibreTranslate translation engine
pub struct LibretranslateEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct LtRequest {
    q: String,
    source: String,
    target: String,
    alternatives: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LtResponse {
    #[serde(default)]
    translatedText: String,
    #[serde(default)]
    alternatives: Vec<String>,
}

impl LibretranslateEngine {
    pub fn new() -> Self {
        Self::with_base_url("https://libretranslate.com")
    }

    pub fn with_base_url(base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        let api_key = std::env::var("LIBRETRANSLATE_API_KEY").ok().filter(|s| !s.is_empty());
        // The public libretranslate.com endpoint requires an API key.
        let metadata = EngineMetadata {
            name: "libretranslate".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: api_key.is_some(),
            timeout_seconds: 15,
            description: "LibreTranslate - free and open-source machine translation.".to_string(),
            website: Some("https://libretranslate.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create LibreTranslate HTTP client");
        LibretranslateEngine {
            metadata,
            client,
            base_url,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // The public libretranslate.com endpoint requires an API key.
        if self.base_url == "https://libretranslate.com" && self.api_key.is_none() {
            tracing::info!("libretranslate: public endpoint requires API key; returning empty");
            return Ok(vec![]);
        }

        // Source/target languages are not encoded in SearchQuery; default to
        // auto-detect -> English, which matches the dictionary engine pattern.
        let source = "auto".to_string();
        let target = "en".to_string();

        let body = LtRequest {
            q: query.query.clone(),
            source: source.clone(),
            target: target.clone(),
            alternatives: 3,
            api_key: self.api_key.clone(),
        };

        let url = format!("{}/translate", self.base_url);
        let response = self
            .client
            .post(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
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
        let parsed: LtResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        if parsed.translatedText.is_empty() {
            return Ok(vec![]);
        }

        // Build the snippet from alternatives
        let snippet = if parsed.alternatives.is_empty() {
            format!("Translation ({} -> {})", source, target)
        } else {
            format!(
                "Alternatives: {}",
                parsed.alternatives.join(" | ")
            )
        };

        let result = SearchResult::new(parsed.translatedText.clone(), self.base_url.clone())
            .with_snippet(snippet)
            .with_engine(self.name())
            .with_rank(1)
            .with_score(1.0)
            .with_result_type(ResultType::Web)
            .with_extra("source_lang", serde_json::json!(source))
            .with_extra("target_lang", serde_json::json!(target))
            .with_extra("alternatives", serde_json::json!(parsed.alternatives));

        Ok(vec![result])
    }
}

#[async_trait]
impl Engine for LibretranslateEngine {
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
        s.insert("base_url".into(), self.base_url.clone());
        s.insert("api_endpoint".into(), "/translate".into());
        s
    }
}
