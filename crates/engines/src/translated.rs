//! MyMemory Translated search engine implementation
//!
//! Calls the
//! MyMemory API at `api.mymemory.translated.net/get?q=&langpair=from|to`. The
//! primary translation is `responseData.translatedText`; `matches[]` provides
//! additional example translations. No API key required (one may be supplied
//! via `TRANSLATED_API_KEY`).
//!
//! Query convention (mirrors `dictzone.rs`): `"<text> <from> <to>"` where the
//! last two tokens are the source/target language codes (e.g. `"hello en fr"`).
//! With fewer tokens the query defaults to English → German.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

const API_URL: &str = "https://api.mymemory.translated.net";
const WEB_URL: &str = "https://mymemory.translated.net";

/// MyMemory translated.net dictionary/translation engine
pub struct TranslatedEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TranslatedResponse {
    #[serde(default)]
    response_data: ResponseData,
    #[serde(default)]
    matches: Vec<TranslatedMatch>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct ResponseData {
    #[serde(default, rename = "translatedText")]
    translated_text: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TranslatedMatch {
    #[serde(default)]
    translation: String,
    #[serde(default)]
    segment: String,
}

impl TranslatedEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "translated".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "MyMemory translated.net dictionary/translation.".to_string(),
            website: Some("https://mymemory.translated.net/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Translated HTTP client");

        TranslatedEngine {
            metadata,
            client,
            api_key: std::env::var("TRANSLATED_API_KEY").ok(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // Parse "<text> <from> <to>" — last two tokens are language codes.
        let tokens: Vec<&str> = query.query.split_whitespace().collect();
        let (from_lang, to_lang, text) = if tokens.len() >= 3 {
            (
                tokens[tokens.len() - 2].to_string(),
                tokens[tokens.len() - 1].to_string(),
                tokens[..tokens.len() - 2].join(" "),
            )
        } else {
            ("en".to_string(), "de".to_string(), query.query.clone())
        };
        if text.is_empty() {
            return Ok(vec![]);
        }

        let langpair = format!("{}|{}", from_lang, to_lang);
        let encoded_q = urlencoding::encode(&text);
        let encoded_lp = urlencoding::encode(&langpair);
        let mut request_url = format!(
            "{}/get?q={}&langpair={}",
            API_URL, encoded_q, encoded_lp
        );
        if let Some(key) = &self.api_key {
            if !key.is_empty() {
                request_url.push_str(&format!("&key={}", urlencoding::encode(key)));
            }
        }

        let response = self
            .client
            .get(&request_url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let data: TranslatedResponse = match response.json().await {
            Ok(d) => d,
            Err(_) => return Ok(vec![]),
        };

        let translated = data.response_data.translated_text;
        if translated.is_empty() {
            return Ok(vec![]);
        }

        // Collect distinct example translations.
        let mut examples: Vec<String> = Vec::new();
        for m in &data.matches {
            if m.translation == translated || m.translation.is_empty() {
                continue;
            }
            let seg = html_to_text(&m.segment);
            let tr = html_to_text(&m.translation);
            if seg.is_empty() {
                examples.push(tr);
            } else {
                examples.push(format!("{} : {}", seg, tr));
            }
        }

        // Build the web link back into MyMemory's search UI.
        let locale = query.language.as_deref().unwrap_or("en");
        let link = format!(
            "{}/search.php?q={}&lang={}&sl={}&tl={}",
            WEB_URL,
            urlencoding::encode(&text),
            urlencoding::encode(locale),
            urlencoding::encode(&from_lang),
            urlencoding::encode(&to_lang),
        );

        let snippet = if examples.is_empty() {
            translated.clone()
        } else {
            format!("{} | examples: {}", translated, examples.join(" / "))
        };

        let result = SearchResult::new(translated.clone(), link)
            .with_snippet(snippet)
            .with_engine(self.name())
            .with_rank(query.offset + 1)
            .with_score(1.0)
            .with_result_type(ResultType::Web)
            .with_extra("translation", serde_json::json!(translated))
            .with_extra("from_lang", serde_json::json!(from_lang))
            .with_extra("to_lang", serde_json::json!(to_lang))
            .with_extra("examples", serde_json::json!(examples))
            .with_extra("source", serde_json::json!("translated"));

        Ok(vec![result])
    }
}

/// Minimal HTML-to-text: strip tags, decode common entities, collapse whitespace.
fn html_to_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[async_trait]
impl Engine for TranslatedEngine {
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
        s.insert("api_url".to_string(), API_URL.to_string());
        s.insert("web_url".to_string(), WEB_URL.to_string());
        s.insert("engine_type".to_string(), "online_dictionary".to_string());
        s
    }
}
