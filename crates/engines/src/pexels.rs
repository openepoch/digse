//! Pexels search engine implementation (images; JSON)
//!
//! Pexels is a free stock photo site.
//! The reference scrapes a secret API key from the Pexels website then queries
//! the internal v3 search API. Here we read an API key from the
//! `PEXELS_API_KEY` environment variable (falling back to the hardcoded key in
//! the reference) and query the same v3 endpoint. If the request fails or the
//! response cannot be parsed, we return an empty result set.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Pexels image search engine
pub struct PexelsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: String,
}

const RESULTS_PER_PAGE: i64 = 20;
/// Fallback API key.
const FALLBACK_KEY: &str = "H2jk9uKnhRmL6WPwh89zBezWvr";

#[derive(Debug, Serialize, Deserialize)]
struct PexelsResponse {
    #[serde(default)]
    data: Vec<PexelsPhoto>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PexelsPhoto {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    attributes: PexelsAttributes,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PexelsAttributes {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
    #[serde(default)]
    image: PexelsImage,
    #[serde(default)]
    user: PexelsUser,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PexelsImage {
    #[serde(default)]
    small: String,
    #[serde(default)]
    download_link: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PexelsUser {
    #[serde(default)]
    username: String,
}

impl PexelsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "pexels".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Pexels - Free stock photos.".to_string(),
            website: Some("https://www.pexels.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Pexels HTTP client");

        let api_key = std::env::var("PEXELS_API_KEY").unwrap_or_else(|_| FALLBACK_KEY.to_string());

        PexelsEngine {
            metadata,
            client,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.pexels.com";
        let page = ((query.offset / RESULTS_PER_PAGE as usize) + 1).to_string();
        let per_page = RESULTS_PER_PAGE.to_string();

        let resp = self
            .client
            .get(format!("{}/en-us/api/v3/search/photos", base_url))
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("secret-key", &self.api_key)
            .query(&[
                ("query", query.query.as_str()),
                ("page", page.as_str()),
                ("per_page", per_page.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: PexelsResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, photo) in parsed.data.iter().enumerate() {
            let a = &photo.attributes;
            let url = format!("{}/photo/{}-{}/", base_url, a.slug, photo.id);
            let title = if a.title.is_empty() {
                format!("Photo by {}", if a.user.username.is_empty() { "Pexels" } else { &a.user.username })
            } else {
                a.title.clone()
            };
            let resolution = format!("{}x{}", a.width, a.height);

            results.push(
                SearchResult::new(&title, &url)
                    .with_snippet(a.description.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(a.image.download_link))
                    .with_extra("thumbnail", serde_json::json!(a.image.small))
                    .with_extra("source", serde_json::json!("pexels"))
                    .with_extra("author", serde_json::json!(a.user.username))
                    .with_extra("width", serde_json::json!(a.width))
                    .with_extra("height", serde_json::json!(a.height))
                    .with_extra("resolution", serde_json::json!(resolution)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for PexelsEngine {
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
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://www.pexels.com".to_string());
        s.insert("results_per_page".to_string(), RESULTS_PER_PAGE.to_string());
        s
    }
}
