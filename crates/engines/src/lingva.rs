//! Lingva Translate search engine implementation
//!
//! Lingva is an alternative front-end for Google
//! Translate). Category: general / translate. Since digse's SearchQuery does
//! not carry source/target language parameters, this implementation translates
//! the query from auto-detect to English by default.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Lingva Translate engine
pub struct LingvaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LingvaResponse {
    #[serde(default)]
    translation: String,
    #[serde(default)]
    info: Option<LingvaInfo>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LingvaInfo {
    #[serde(default)]
    typo: Option<String>,
    #[serde(default)]
    definitions: Vec<LingvaDefinition>,
    #[serde(default, rename = "extraTranslations")]
    extra_translations: Vec<LingvaExtraTranslation>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LingvaDefinition {
    #[serde(default)]
    #[serde(rename = "type")]
    def_type: String,
    #[serde(default)]
    list: Vec<LingvaDefinitionItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LingvaDefinitionItem {
    #[serde(default)]
    definition: Option<String>,
    #[serde(default)]
    example: Option<String>,
    #[serde(default)]
    synonyms: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LingvaExtraTranslation {
    #[serde(default)]
    #[serde(rename = "type")]
    word_type: String,
    #[serde(default)]
    list: Vec<LingvaExtraWord>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LingvaExtraWord {
    #[serde(default)]
    word: String,
    #[serde(default)]
    meanings: Vec<String>,
}

impl LingvaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "lingva".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Lingva Translate - alternative front-end for Google Translate.".to_string(),
            website: Some("https://lingva.ml".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Lingva HTTP client");

        LingvaEngine {
            metadata,
            client,
            base_url: "https://lingva.ml".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let from_lang = "auto";
        let to_lang = "en";
        let encoded = urlencoding::encode(&query.query);
        let url = format!(
            "{}/api/v1/{}/{}/{}",
            self.base_url, from_lang, to_lang, encoded
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
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
        let parsed: LingvaResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        if parsed.translation.is_empty() {
            return Ok(vec![]);
        }

        let result_url = format!("{}/{}/{}/{}", self.base_url, from_lang, to_lang, encoded);
        let mut snippet_parts = vec![format!("{} -> {}: {}", from_lang, to_lang, parsed.translation)];

        let mut definitions: Vec<String> = Vec::new();
        let mut synonyms: Vec<String> = Vec::new();
        if let Some(info) = &parsed.info {
            if let Some(typo) = &info.typo {
                snippet_parts.push(format!("Did you mean: {}", typo));
            }
            for def in &info.definitions {
                for item in &def.list {
                    if let Some(d) = &item.definition {
                        definitions.push(d.clone());
                    }
                    for s in &item.synonyms {
                        synonyms.push(s.clone());
                    }
                }
            }
            for extra in &info.extra_translations {
                for word in &extra.list {
                    snippet_parts.push(format!("{}: {}", word.word, word.meanings.join(", ")));
                }
            }
        }
        if !definitions.is_empty() {
            snippet_parts.push(format!("Definitions: {}", definitions.join("; ")));
        }
        if !synonyms.is_empty() {
            snippet_parts.push(format!("Synonyms: {}", synonyms.join(", ")));
        }

        let title = format!("Translate '{}'", query.query);
        let result = SearchResult::new(title, result_url)
            .with_snippet(snippet_parts.join(" | "))
            .with_engine(self.name())
            .with_rank(query.offset + 1)
            .with_score(1.0)
            .with_result_type(ResultType::Web)
            .with_extra("translation", serde_json::json!(parsed.translation))
            .with_extra("from_lang", serde_json::json!(from_lang))
            .with_extra("to_lang", serde_json::json!(to_lang))
            .with_extra(
                "definitions",
                serde_json::json!(definitions),
            )
            .with_extra("synonyms", serde_json::json!(synonyms));

        Ok(vec![result])
    }
}

#[async_trait]
impl Engine for LingvaEngine {
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
        settings.insert("from_lang".to_string(), "auto".to_string());
        settings.insert("to_lang".to_string(), "en".to_string());
        settings
    }
}
