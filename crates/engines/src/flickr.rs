//! Flickr (API) search engine implementation
//!
//! Uses the Flickr REST API
//! `flickr.photos.search` with an API key from `FLICKR_API_KEY`. Category:
//! images. Graceful empty when no key is configured.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Flickr image search engine (REST API; requires API key)
pub struct FlickrEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FlickrResponse {
    #[serde(default)]
    photos: Option<FlickrPhotos>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct FlickrPhotos {
    #[serde(default)]
    photo: Vec<FlickrPhoto>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct FlickrPhoto {
    #[serde(default)]
    id: String,
    #[serde(default)]
    owner: String,
    #[serde(default)]
    ownername: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: FlickrDescription,
    #[serde(default)]
    url_o: String,
    #[serde(default)]
    url_z: String,
    #[serde(default)]
    url_n: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct FlickrDescription {
    #[serde(default, rename = "_content")]
    content: String,
}

impl FlickrEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("FLICKR_API_KEY").ok().filter(|k| !k.is_empty());
        let metadata = EngineMetadata {
            name: "flickr".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Flickr - image search (API key required).".to_string(),
            website: Some("https://www.flickr.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Flickr HTTP client");

        FlickrEngine {
            metadata,
            client,
            api_key,
        }
    }

    fn build_flickr_url(user_id: &str, photo_id: &str) -> String {
        format!("https://www.flickr.com/photos/{}/{}", user_id, photo_id)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                tracing::info!("flickr: FLICKR_API_KEY not set; returning empty");
                return Ok(vec![]);
            }
        };
        let page = query.offset + 1;
        let page_str = page.to_string();
        let per_page = query.count.to_string();
        let url = "https://api.flickr.com/services/rest/";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("method", "flickr.photos.search"),
                ("api_key", api_key.as_str()),
                ("text", query.query.as_str()),
                ("sort", "relevance"),
                (
                    "extras",
                    "description, owner_name, url_o, url_n, url_z",
                ),
                ("per_page", per_page.as_str()),
                ("format", "json"),
                ("nojsoncallback", "1"),
                ("page", page_str.as_str()),
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

        let parsed: FlickrResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let photos = parsed.photos.map(|p| p.photo).unwrap_or_default();

        let mut results = Vec::new();
        for (i, photo) in photos.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let img_src = if !photo.url_o.is_empty() {
                photo.url_o.clone()
            } else if !photo.url_z.is_empty() {
                photo.url_z.clone()
            } else {
                continue;
            };
            let thumbnail = if !photo.url_n.is_empty() {
                photo.url_n.clone()
            } else if !photo.url_z.is_empty() {
                photo.url_z.clone()
            } else {
                img_src.clone()
            };
            let url = Self::build_flickr_url(&photo.owner, &photo.id);
            let title = if photo.title.is_empty() {
                "Flickr photo".to_string()
            } else {
                photo.title.clone()
            };

            let result = SearchResult::new(title, url)
                .with_snippet(photo.description.content.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("author", serde_json::json!(photo.ownername))
                .with_extra("source", serde_json::json!("flickr"));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for FlickrEngine {
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
        matches!(result_type, ResultType::Images | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://www.flickr.com".to_string());
        settings.insert(
            "api_endpoint".to_string(),
            "https://api.flickr.com/services/rest/".to_string(),
        );
        if self.api_key.is_some() {
            settings.insert("requires_api_key".to_string(), "true".to_string());
        }
        settings
    }
}
