//! Kagi search engine implementation
//!
//! Paid, privacy-focused search engine
//! requiring an API key. Without `KAGI_API_KEY`, gracefully returns empty.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Kagi search engine (paid; requires API key)
pub struct KagiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
    kagi_categ: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct KagiResponse {
    #[serde(default)]
    data: KagiData,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct KagiData {
    #[serde(default)]
    search: Vec<KagiResult>,
    #[serde(default)]
    news: Vec<KagiResult>,
    #[serde(default)]
    images: Vec<KagiImage>,
    #[serde(default)]
    video: Vec<KagiVideo>,
    #[serde(default, rename = "related_search")]
    related_search: Vec<KagiRelated>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct KagiResult {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    image: Option<KagiImage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct KagiImage {
    #[serde(default)]
    url: String,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct KagiVideo {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    image: Option<KagiImage>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    props: KagiVideoProps,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct KagiVideoProps {
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    creator_name: Option<String>,
    #[serde(default)]
    thumbnail: Option<KagiImage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct KagiRelated {
    #[serde(default)]
    title: String,
}

impl KagiEngine {
    pub fn new() -> Self {
        Self::with_categ("search")
    }

    pub fn with_categ(categ: &str) -> Self {
        let categ = match categ {
            "images" | "news" | "videos" => categ.to_string(),
            _ => "search".to_string(),
        };
        let api_key = std::env::var("KAGI_API_KEY").ok().filter(|s| !s.is_empty());
        let (category, description) = match categ.as_str() {
            "images" => (
                EngineCategory::Images,
                "Kagi Images - paid, privacy-focused image search.".to_string(),
            ),
            "news" => (
                EngineCategory::News,
                "Kagi News - paid, privacy-focused news search.".to_string(),
            ),
            "videos" => (
                EngineCategory::Videos,
                "Kagi Videos - paid, privacy-focused video search.".to_string(),
            ),
            _ => (
                EngineCategory::General,
                "Kagi - paid, privacy-focused web search.".to_string(),
            ),
        };
        let metadata = EngineMetadata {
            name: "kagi".to_string(),
            category,
            enabled: true,
            requires_auth: true,
            timeout_seconds: 15,
            description,
            website: Some("https://kagi.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Kagi HTTP client");
        KagiEngine {
            metadata,
            client,
            api_key,
            kagi_categ: categ,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                tracing::info!("kagi: KAGI_API_KEY not set; returning empty");
                return Ok(vec![]);
            }
        };

        let pageno = (query.offset / query.count.max(1)) + 1;
        // Kagi supports at most page 10
        if pageno > 10 {
            return Ok(vec![]);
        }

        let body = serde_json::json!({
            "query": query.query,
            "page": pageno,
            "workflow": self.kagi_categ,
            "safe_search": false,
            "filters": {
                "region": "no_region",
            }
        });

        let response = self
            .client
            .post("https://kagi.com/api/v1/search")
            .header("Authorization", format!("Bearer {}", api_key))
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
        let parsed: KagiResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        match self.kagi_categ.as_str() {
            "search" | "news" => {
                let items = if self.kagi_categ == "search" {
                    &parsed.data.search
                } else {
                    &parsed.data.news
                };
                for (i, r) in items.iter().enumerate() {
                    if i >= query.count {
                        break;
                    }
                    if r.url.is_empty() || r.title.is_empty() {
                        continue;
                    }
                    let thumbnail = r
                        .image
                        .as_ref()
                        .map(|im| im.url.clone())
                        .unwrap_or_default();
                    let mut result = SearchResult::new(r.title.clone(), r.url.clone())
                        .with_snippet(r.snippet.clone())
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(if self.kagi_categ == "news" {
                            ResultType::News
                        } else {
                            ResultType::Web
                        });
                    if !thumbnail.is_empty() {
                        result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
                    }
                    if let Some(t) = &r.time {
                        if !t.is_empty() {
                            result = result.with_extra("published", serde_json::json!(t));
                        }
                    }
                    results.push(result);
                }
            }
            "images" => {
                for (i, r) in parsed.data.images.iter().enumerate() {
                    if i >= query.count {
                        break;
                    }
                    if r.url.is_empty() {
                        continue;
                    }
                    let result = SearchResult::new(String::new(), r.url.clone())
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Images)
                        .with_extra("img_src", serde_json::json!(r.url))
                        .with_extra(
                            "format",
                            serde_json::json!(format!("{}x{}", r.width, r.height)),
                        );
                    results.push(result);
                }
            }
            "videos" => {
                for (i, r) in parsed.data.video.iter().enumerate() {
                    if i >= query.count {
                        break;
                    }
                    if r.url.is_empty() {
                        continue;
                    }
                    let thumbnail = r
                        .image
                        .as_ref()
                        .map(|im| im.url.clone())
                        .unwrap_or_default();
                    let mut result = SearchResult::new(r.title.clone(), r.url.clone())
                        .with_snippet(r.snippet.clone())
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Videos)
                        .with_extra("thumbnail", serde_json::json!(thumbnail));
                    if let Some(d) = &r.props.duration {
                        result = result.with_extra("duration", serde_json::json!(d));
                    }
                    if let Some(a) = &r.props.creator_name {
                        result = result.with_extra("author", serde_json::json!(a));
                    }
                    if let Some(t) = &r.time {
                        result = result.with_extra("published", serde_json::json!(t));
                    }
                    results.push(result);
                }
            }
            _ => {}
        }

        // related searches ignored as results
        let _ = &parsed.data.related_search;
        Ok(results)
    }
}

#[async_trait]
impl Engine for KagiEngine {
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
        match self.kagi_categ.as_str() {
            "images" => matches!(t, ResultType::Images | ResultType::All),
            "news" => matches!(t, ResultType::News | ResultType::All),
            "videos" => matches!(t, ResultType::Videos | ResultType::All),
            _ => matches!(t, ResultType::Web | ResultType::All),
        }
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://kagi.com".into());
        s.insert("api_endpoint".into(), "/api/v1/search".into());
        s.insert("kagi_categ".into(), self.kagi_categ.clone());
        s
    }
}
