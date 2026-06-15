//! Unsplash search engine implementation
//!
//! Uses Unsplash's internal
//! `napi/search/photos` JSON endpoint, which requires no API key. Common
//! browser user agents are blocked by Anubis, so a non-browser UA is used.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Unsplash stock photo search engine
pub struct UnsplashEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize, Default)]
struct UnsplashResponse {
    #[serde(default)]
    results: Vec<UnsplashPhoto>,
}

#[derive(Debug, Deserialize, Default)]
struct UnsplashPhoto {
    #[serde(default)]
    alt_description: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    links: UnsplashLinks,
    #[serde(default)]
    urls: UnsplashUrls,
    #[serde(default)]
    user: UnsplashUser,
    #[serde(default)]
    width: Option<i64>,
    #[serde(default)]
    height: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct UnsplashLinks {
    #[serde(default)]
    html: String,
}

#[derive(Debug, Deserialize, Default)]
struct UnsplashUrls {
    #[serde(default)]
    raw: Option<String>,
    #[serde(default)]
    full: Option<String>,
    #[serde(default)]
    regular: Option<String>,
    #[serde(default)]
    small: Option<String>,
    #[serde(default)]
    thumb: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct UnsplashUser {
    #[serde(default)]
    username: String,
    #[serde(default)]
    name: String,
}

/// Strip the `ixid` query parameter (tracking) from an Unsplash URL.
fn clean_url(url: &str) -> String {
    match url.split_once('?') {
        None => url.to_string(),
        Some((path, qs)) => {
            let kept: Vec<&str> = qs
                .split('&')
                .filter(|kv| !kv.starts_with("ixid="))
                .collect();
            if kept.is_empty() {
                path.to_string()
            } else {
                format!("{}?{}", path, kept.join("&"))
            }
        }
    }
}

impl UnsplashEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "unsplash".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Unsplash stock photo search".to_string(),
            website: Some("https://unsplash.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Unsplash HTTP client");

        UnsplashEngine { metadata, client }
    }

    pub fn new_general() -> Self {
        Self::new()
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = (query.offset / query.count.max(1)) + 1;
        let page_str = pageno.to_string();
        let per_page = query.count.clamp(1, 30).to_string();
        let url = "https://unsplash.com/napi/search/photos";

        let response = self
            .client
            .get(url)
            .query(&[
                ("query", query.query.as_str()),
                ("page", page_str.as_str()),
                ("per_page", per_page.as_str()),
            ])
            // Common browser UAs are blocked by Anubis; use a non-browser UA.
            .header("User-Agent", "digse/0.1")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let parsed: UnsplashResponse = match response.json().await {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, photo) in parsed.results.iter().enumerate() {
            if results.len() >= query.count {
                break;
            }
            if photo.links.html.is_empty() {
                continue;
            }
            let page_url = clean_url(&photo.links.html);
            let title = photo
                .alt_description
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown".to_string());
            let img_src = photo.urls.regular.clone().unwrap_or_default();
            let thumb = photo.urls.thumb.clone().unwrap_or_default();
            let result = SearchResult::new(title, page_url)
                .with_snippet(photo.description.clone().unwrap_or_default())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(clean_url(&img_src)))
                .with_extra("thumbnail", serde_json::json!(clean_url(&thumb)))
                .with_extra("source", serde_json::json!("unsplash"))
                .with_extra("author", serde_json::json!(photo.user.name));
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for UnsplashEngine {
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
        matches!(t, ResultType::Images | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://unsplash.com".to_string());
        settings.insert(
            "api_endpoint".to_string(),
            "/napi/search/photos".to_string(),
        );
        settings
    }
}
