//! BPB (Bundeszentrale für politische Bildung) engine implementation.
//! German government educational resources, JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// BPB (German Federal Agency for Civic Education) search engine.
pub struct BpbEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://www.bpb.de";

#[derive(Debug, Serialize, Deserialize)]
struct BpbResponse {
    #[serde(default)]
    teaser: Vec<BpbTeaserEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BpbTeaserEntry {
    #[serde(default)]
    teaser: BpbTeaser,
    #[serde(default)]
    extension: BpbExtension,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BpbTeaser {
    #[serde(default)]
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    link: BpbLink,
    #[serde(default)]
    image: Option<BpbImage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BpbLink {
    #[serde(default)]
    url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BpbImage {
    #[serde(default)]
    sources: Vec<BpbImageSource>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BpbImageSource {
    #[serde(default)]
    url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BpbExtension {
    #[serde(default)]
    overline: String,
    #[serde(default)]
    authors: Vec<BpbAuthor>,
    #[serde(default)]
    #[serde(rename = "publishingDate")]
    publishing_date: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BpbAuthor {
    #[serde(default)]
    name: String,
}

impl BpbEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "bpb".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "BPB - Bundeszentrale für politische Bildung (German civic education)."
                .to_string(),
            website: Some("https://www.bpb.de".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create BPB HTTP client");
        BpbEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://www.bpb.de/bpbapi/filter/search";
        let page = query.offset.to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("query[term]", query.query.as_str()),
                ("page", page.as_str()),
                ("sort[direction]", "descending"),
                ("payload[nid]", "65350"),
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
        let parsed: BpbResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, entry) in parsed.teaser.iter().enumerate() {
            let path = entry.teaser.link.url.clone();
            if path.is_empty() {
                continue;
            }
            let url = format!("{}{}", BASE_URL, path);
            let mut thumbnail = String::new();
            if let Some(img) = &entry.teaser.image {
                if let Some(src) = img.sources.last() {
                    thumbnail = format!("{}{}", BASE_URL, src.url);
                }
            }
            let mut metadata = entry.extension.overline.clone();
            let authors: Vec<&str> = entry
                .extension
                .authors
                .iter()
                .map(|a| a.name.as_str())
                .collect();
            if !authors.is_empty() {
                if !metadata.is_empty() {
                    metadata.push_str(" | ");
                }
                metadata.push_str(&authors.join(", "));
            }
            let published = match &entry.extension.publishing_date {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => String::new(),
            };
            results.push(
                SearchResult::new(entry.teaser.title.clone(), url)
                    .with_snippet(entry.teaser.text.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("source", serde_json::json!("bpb")),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for BpbEngine {
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
        s.insert("base_url".into(), "https://www.bpb.de".into());
        s
    }
}
