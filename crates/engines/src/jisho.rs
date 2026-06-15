//! Jisho search engine implementation
//!
//! Japanese-English dictionary JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Jisho (Japanese dictionary) search engine
pub struct JishoEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct JishoResponse {
    #[serde(default)]
    data: Vec<JishoPage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct JishoPage {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    japanese: Vec<JishoJapanese>,
    #[serde(default)]
    senses: Vec<JishoSense>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct JishoJapanese {
    #[serde(default)]
    word: String,
    #[serde(default)]
    reading: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct JishoSense {
    #[serde(default)]
    english_definitions: Vec<String>,
    #[serde(default)]
    parts_of_speech: Vec<String>,
}

impl JishoEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "jisho".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Jisho - Japanese-English dictionary.".to_string(),
            website: Some("https://jisho.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Jisho HTTP client");

        JishoEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base = "https://jisho.org";
        let url = format!("{}/api/v1/search/words", base);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[("keyword", query.query.as_str())])
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
        let parsed: JishoResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, page) in parsed.data.iter().enumerate() {
            if i >= query.count {
                break;
            }
            // alternative forms
            let alt_forms: Vec<String> = page
                .japanese
                .iter()
                .map(|j| {
                    if j.word.is_empty() {
                        j.reading.clone()
                    } else if j.reading.is_empty() {
                        j.word.clone()
                    } else {
                        format!("{} ({})", j.word, j.reading)
                    }
                })
                .collect();
            if alt_forms.is_empty() {
                continue;
            }
            let title = alt_forms.join(", ");
            let result_url = format!("https://jisho.org/word/{}", page.slug);

            // definitions
            let defs: Vec<String> = page
                .senses
                .iter()
                .filter(|s| {
                    // exclude Wikipedia-only entries
                    !(s.parts_of_speech.len() == 1
                        && s.parts_of_speech[0] == "Wikipedia definition")
                })
                .map(|s| s.english_definitions.join("; "))
                .collect();
            let mut content = defs
                .iter()
                .map(|d| format!("{}.", d))
                .collect::<Vec<_>>()
                .join(" ");
            if content.len() > 300 {
                content.truncate(300);
                content.push_str("...");
            }

            let result = SearchResult::new(title, result_url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for JishoEngine {
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
        s.insert("base_url".into(), "https://jisho.org".into());
        s.insert("api_endpoint".into(), "/api/v1/search/words".into());
        s
    }
}
