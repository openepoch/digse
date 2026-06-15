//! DeepL translation engine implementation
//!
//! DeepL is a paid translation API. The API key is read from the `DEEPL_API_KEY`
//! environment variable. Without a key the engine returns an empty list gracefully.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// DeepL translation engine
pub struct DeeplEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

impl DeeplEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "deepl".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: true,
            timeout_seconds: 15,
            description: "DeepL translation (paid API).".to_string(),
            website: Some("https://www.deepl.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create DeepL HTTP client");

        let api_key = std::env::var("DEEPL_API_KEY").ok().filter(|k| !k.is_empty());

        DeeplEngine {
            metadata,
            client,
            api_key,
        }
    }

    /// Heuristically extract (text, target_lang) from a query like
    /// "hello world -> de" or "hello to german". Falls back to whole query as
    /// text and a default target language (EN).
    fn parse_query(&self, q: &str) -> (String, String) {
        // Look for "-> XX" or "to XX" 2-letter lang at the end.
        let lower = q.to_lowercase();
        let mut target = "EN".to_string();
        let mut text = q.to_string();

        for sep in ["->", "=>", " to ", " in "] {
            if let Some(idx) = lower.rfind(sep) {
                let candidate = lower[idx + sep.len()..].trim();
                let token = candidate
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c: char| !c.is_alphabetic());
                if (2..=5).contains(&token.len()) && token.chars().all(|c| c.is_alphabetic()) {
                    target = token.to_uppercase();
                    text = q[..idx].trim().to_string();
                    break;
                }
            }
        }
        (text, target)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                // No API key configured -> graceful no-op
                eprintln!("deepl: no DEEPL_API_KEY configured, returning empty");
                return Ok(vec![]);
            }
        };

        let (text, target_lang) = self.parse_query(&query.query);
        if text.is_empty() {
            return Ok(vec![]);
        }

        let url = "https://api-free.deepl.com/v2/translate";

        let response = self
            .client
            .post(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .form(&[
                ("auth_key", api_key.as_str()),
                ("text", text.as_str()),
                ("target_lang", target_lang.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let body_text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let v: serde_json::Value = match serde_json::from_str(&body_text) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        let translations = match v.get("translations").and_then(|t| t.as_array()) {
            Some(arr) => arr,
            None => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, t) in translations.iter().enumerate() {
            let translated = t
                .get("text")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if translated.is_empty() {
                continue;
            }
            let detected = t
                .get("detected_source_language")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let result = SearchResult::new(translated.clone(), "https://www.deepl.com/translator".to_string())
                .with_snippet(format!(
                    "{} -> {} (source: {})",
                    text, translated, detected
                ))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("translation", serde_json::json!(translated))
                .with_extra("source_lang", serde_json::json!(detected))
                .with_extra("target_lang", serde_json::json!(target_lang));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for DeeplEngine {
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
        settings.insert(
            "base_url".to_string(),
            "https://api-free.deepl.com/v2/translate".to_string(),
        );
        settings.insert(
            "api_key_env".to_string(),
            "DEEPL_API_KEY".to_string(),
        );
        settings.insert("engine_type".to_string(), "online_dictionary".to_string());
        settings
    }
}
