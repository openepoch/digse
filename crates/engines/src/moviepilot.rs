//! Moviepilot search engine implementation
//!
//! queries the German movie database
//! moviepilot.de via its internal JSON API. Category: videos (movies).

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Moviepilot (German movie database) search engine
pub struct MoviepilotEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

const IMAGE_URL: &str =
    "https://assets.cdn.moviepilot.de/files/{image_id}/fill/155/223/{filename}";

const FILTER_TYPES: &[&str] = &[
    "fsk",
    "genre",
    "jahr",
    "jahrzehnt",
    "land",
    "online",
    "stimmung",
    "person",
];

#[derive(Debug, Deserialize)]
struct MoviepilotSearch(Vec<MoviepilotItemSuggest>);

#[derive(Debug, Deserialize, Default)]
struct MoviepilotItemSuggest {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    #[serde(rename = "class")]
    class_: String,
    #[serde(default)]
    info: String,
    #[serde(default)]
    more: String,
    #[serde(default)]
    image: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MoviepilotDiscovery {
    #[serde(default)]
    results: Vec<MoviepilotItemDiscover>,
}

#[derive(Debug, Deserialize, Default)]
struct MoviepilotItemDiscover {
    #[serde(default)]
    title: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    meta_short: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    image_filename: Option<String>,
}

impl MoviepilotEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "moviepilot".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Moviepilot - German movie database.".to_string(),
            website: Some("https://www.moviepilot.de".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Moviepilot HTTP client");

        MoviepilotEngine {
            metadata,
            client,
            base_url: "https://www.moviepilot.de".to_string(),
        }
    }

    fn parse_discovery_filters(query: &str) -> Vec<String> {
        query
            .split_whitespace()
            .filter(|part| {
                if let Some((cat, _)) = part.split_once('-') {
                    FILTER_TYPES.iter().any(|f| *f == cat)
                } else {
                    false
                }
            })
            .map(|s| s.to_string())
            .collect()
    }

    fn strip_html(s: &str) -> String {
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
        out.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string()
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let discovery_filters = Self::parse_discovery_filters(&query.query);
        let is_discovery = !discovery_filters.is_empty();
        let page = (query.offset / 10).max(0) + 1;
        let page_str = page.to_string();

        let url = if is_discovery {
            let mut u = format!(
                "{}/api/discovery?page={}&order=beste",
                self.base_url, page_str
            );
            for f in &discovery_filters {
                u.push_str(&format!("&filters[]={}", f));
            }
            u
        } else {
            format!(
                "{}/api/search?q={}&page={}&type=suggest",
                self.base_url,
                urlencoding::encode(&query.query),
                page_str
            )
        };

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

        let mut results = Vec::new();
        if is_discovery {
            let parsed: MoviepilotDiscovery = match serde_json::from_str(&text) {
                Ok(p) => p,
                Err(_) => return Ok(vec![]),
            };
            for (i, item) in parsed.results.iter().enumerate() {
                if item.path.is_empty() {
                    continue;
                }
                let url = format!("{}{}", self.base_url, item.path);
                let mut content_parts = Vec::new();
                if let Some(a) = &item.abstract_text {
                    if !a.is_empty() {
                        content_parts.push(Self::strip_html(a));
                    }
                }
                if let Some(s) = &item.summary {
                    if !s.is_empty() {
                        content_parts.push(Self::strip_html(s));
                    }
                }
                let mut result = SearchResult::new(item.title.clone(), url)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("source", serde_json::json!("moviepilot"));
                if !content_parts.is_empty() {
                    result = result.with_snippet(content_parts.join(" | "));
                }
                if let (Some(img), Some(filename)) = (&item.image, &item.image_filename) {
                    if !img.is_empty() && !filename.is_empty() {
                        let thumb = IMAGE_URL
                            .replace("{image_id}", img)
                            .replace("{filename}", filename);
                        result = result.with_extra("thumbnail", serde_json::json!(thumb));
                    }
                }
                results.push(result);
                if results.len() >= query.count {
                    break;
                }
            }
        } else {
            let parsed: MoviepilotSearch = match serde_json::from_str(&text) {
                Ok(p) => p,
                Err(_) => return Ok(vec![]),
            };
            for (i, item) in parsed.0.iter().enumerate() {
                if item.url.is_empty() {
                    continue;
                }
                let url = if item.url.starts_with("http") {
                    item.url.clone()
                } else {
                    format!("{}{}", self.base_url, item.url)
                };
                let content_parts: Vec<String> =
                    [&item.class_, &item.info, &item.more]
                        .iter()
                        .map(|s| (*s).to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                let mut result = SearchResult::new(item.title.clone(), url)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Videos)
                    .with_extra("source", serde_json::json!("moviepilot"));
                if !content_parts.is_empty() {
                    result = result.with_snippet(content_parts.join(", "));
                }
                if let Some(img) = &item.image {
                    if !img.is_empty() {
                        result = result.with_extra("thumbnail", serde_json::json!(img));
                    }
                }
                results.push(result);
                if results.len() >= query.count {
                    break;
                }
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for MoviepilotEngine {
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
        matches!(result_type, ResultType::Videos | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("language".to_string(), "de".to_string());
        settings
    }
}
