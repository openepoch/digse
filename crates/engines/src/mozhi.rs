//! Mozhi search engine implementation
//!
//! an alternative frontend for popular
//! translation engines (Google, etc.). Category: general / translate. Since
//! digse's SearchQuery does not carry source/target language parameters, this
//! implementation translates the query from auto-detect to English by default.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Mozhi (translation proxy) engine
pub struct MozhiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    mozhi_engine: String,
}

#[derive(Debug, Deserialize)]
struct MozhiResponse {
    #[serde(default, rename = "translated-text")]
    translated_text: String,
    #[serde(default)]
    target_transliteration: Option<String>,
    #[serde(default)]
    word_choices: Vec<MozhiWordChoice>,
    #[serde(default)]
    source_synonyms: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
struct MozhiWordChoice {
    #[serde(default)]
    definition: Option<String>,
    #[serde(default, rename = "examples_target")]
    examples_target: Vec<String>,
}

impl MozhiEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "mozhi".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Mozhi - alternative frontend for popular translation engines.".to_string(),
            website: Some("https://codeberg.org/aryak/mozhi".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Mozhi HTTP client");

        MozhiEngine {
            metadata,
            client,
            base_url: std::env::var("MOZHI_BASE_URL")
                .unwrap_or_else(|_| "https://mozhi.aryak.me".to_string()),
            mozhi_engine: "google".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let from_lang = "auto";
        let to_lang = "en";
        let url = format!("{}/api/translate", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("from", from_lang),
                ("to", to_lang),
                ("text", query.query.as_str()),
                ("engine", self.mozhi_engine.as_str()),
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
        let parsed: MozhiResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        if parsed.translated_text.is_empty() {
            return Ok(vec![]);
        }

        let result_url = self.base_url.clone();
        let mut snippet_parts = vec![format!(
            "{} -> {}: {}",
            from_lang, to_lang, parsed.translated_text
        )];

        let mut definitions: Vec<String> = Vec::new();
        let mut examples: Vec<String> = Vec::new();
        for word in &parsed.word_choices {
            if let Some(d) = &word.definition {
                if !d.is_empty() {
                    definitions.push(d.clone());
                }
            }
            for ex in &word.examples_target {
                let cleaned = ex.replace('<', "").replace('>', "").trim_start_matches(['-', ' ']).to_string();
                if !cleaned.is_empty() {
                    examples.push(cleaned);
                }
            }
        }
        let transliteration = parsed
            .target_transliteration
            .as_deref()
            .filter(|t| !t.is_empty() && !t.starts_with("Direction '"))
            .map(|s| s.to_string());

        if let Some(tr) = &transliteration {
            snippet_parts.push(format!("Transliteration: {}", tr));
        }
        if !definitions.is_empty() {
            snippet_parts.push(format!("Definitions: {}", definitions.join("; ")));
        }
        if !examples.is_empty() {
            snippet_parts.push(format!("Examples: {}", examples.join("; ")));
        }

        let title = format!("Translate '{}'", query.query);
        let result = SearchResult::new(title, result_url)
            .with_snippet(snippet_parts.join(" | "))
            .with_engine(self.name())
            .with_rank(query.offset + 1)
            .with_score(1.0)
            .with_result_type(ResultType::Web)
            .with_extra("translation", serde_json::json!(parsed.translated_text))
            .with_extra("from_lang", serde_json::json!(from_lang))
            .with_extra("to_lang", serde_json::json!(to_lang))
            .with_extra(
                "transliteration",
                serde_json::json!(transliteration.unwrap_or_default()),
            )
            .with_extra("definitions", serde_json::json!(definitions))
            .with_extra("examples", serde_json::json!(examples));

        Ok(vec![result])
    }
}

#[async_trait]
impl Engine for MozhiEngine {
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
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("mozhi_engine".to_string(), self.mozhi_engine.clone());
        settings.insert("from_lang".to_string(), "auto".to_string());
        settings.insert("to_lang".to_string(), "en".to_string());
        settings
    }
}
