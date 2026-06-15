//! IMDB search engine implementation
//!
//! Uses IMDB's undocumented suggestion
//! JSON API. Categories: name, title, keyword, company, episode.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// IMDB search engine
pub struct ImdbEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImdbResponse {
    #[serde(default)]
    d: Vec<ImdbEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ImdbEntry {
    #[serde(default)]
    id: String,
    #[serde(default)]
    l: String,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    rank: Option<i64>,
    #[serde(default)]
    y: Option<i64>,
    #[serde(default)]
    s: Option<String>,
    #[serde(default)]
    i: Option<ImdbImage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ImdbImage {
    #[serde(default)]
    imageUrl: String,
}

fn category_for(id: &str) -> Option<&'static str> {
    match id.get(..2) {
        Some("nm") => Some("name"),
        Some("tt") => Some("title"),
        Some("kw") => Some("keyword"),
        Some("co") => Some("company"),
        Some("ep") => Some("episode"),
        _ => None,
    }
}

/// Apply IMDB's magic thumbnail resize recipe: insert `QL75_UX280_CR0,0,280,414_`
/// after `_V1_` (prepending `_V1_` if absent).
fn make_thumbnail(image_url: &str) -> String {
    if let Some(dot_idx) = image_url.rfind('.') {
        let (name, ext) = image_url.split_at(dot_idx);
        let ext = &ext[1..]; // strip the dot
        let magic = "QL75_UX280_CR0,0,280,414_";
        if name.ends_with("_V1_") {
            format!("{}{}.{}", name, magic, ext)
        } else {
            format!("{}_V1_{}.{}", name, magic, ext)
        }
    } else {
        image_url.to_string()
    }
}

impl ImdbEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "imdb".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "IMDB - Internet Movie Database.".to_string(),
            website: Some("https://imdb.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create IMDB HTTP client");

        ImdbEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let normalized = query.query.replace(' ', "_").to_lowercase();
        if normalized.is_empty() {
            return Ok(vec![]);
        }
        let letter = normalized.chars().next().unwrap_or('a');
        let url = format!(
            "https://v2.sg.media-imdb.com/suggestion/{}/{}.json",
            letter,
            urlencoding::encode(&normalized)
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
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
        let parsed: ImdbResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, entry) in parsed.d.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let categ = match category_for(&entry.id) {
                Some(c) => c,
                None => continue,
            };
            let mut title = entry.l.clone();
            if let Some(q) = &entry.q {
                if !q.is_empty() {
                    title.push_str(&format!(" ({})", q));
                }
            }
            if title.is_empty() {
                continue;
            }
            let url = format!("https://imdb.com/{}/{}", categ, entry.id);

            let mut content_parts = Vec::new();
            if let Some(rank) = entry.rank {
                content_parts.push(format!("({})", rank));
            }
            if let Some(y) = entry.y {
                content_parts.push(format!("{} -", y));
            }
            if let Some(s) = &entry.s {
                content_parts.push(s.clone());
            }
            let content = content_parts.join(" ");

            let thumbnail = entry
                .i
                .as_ref()
                .map(|img| make_thumbnail(&img.imageUrl))
                .unwrap_or_default();

            let mut result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            if !thumbnail.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
            }
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for ImdbEngine {
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
        s.insert("base_url".into(), "https://imdb.com".into());
        s.insert(
            "suggestion_url".into(),
            "https://v2.sg.media-imdb.com/suggestion".into(),
        );
        s
    }
}
