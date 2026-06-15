//! Cara (art social platform) image search engine implementation.
//! JSON portfolio-posts search API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Cara art portfolio image search engine.
pub struct CaraEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://cara.app";
const IMAGES_URL: &str = "https://images.cara.app";

#[derive(Debug, Serialize, Deserialize)]
struct CaraPost {
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    title: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    images: Vec<CaraImage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CaraImage {
    #[serde(default)]
    src: String,
    #[serde(default)]
    is_cover_img: bool,
}

impl CaraEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "cara".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Cara - social platform for artists (filters out AI imagery).".to_string(),
            website: Some("https://cara.app".to_string()),
        };
        // ref notes: HTTP/2 gets blocked immediately, so disable it.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Cara HTTP client");
        CaraEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!("{}/api/search/portfolio-posts", BASE_URL);
        let take = "24".to_string();
        let skip = (query.offset * 24).to_string();

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("sortBy", "Top"),
                ("take", take.as_str()),
                ("skip", skip.as_str()),
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
        let posts: Vec<CaraPost> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let mut idx = 0;
        for post in posts.iter() {
            // Mirror the Python: pick a thumbnail (first or cover) and a main image
            // (first non-cover).
            let mut thumbnail: Option<&CaraImage> = None;
            let mut main_img: Option<&CaraImage> = None;
            for img in &post.images {
                if thumbnail.is_none() || img.is_cover_img {
                    thumbnail = Some(img);
                }
                if main_img.is_none() || !img.is_cover_img {
                    main_img = Some(img);
                }
            }
            let (thumb, main) = match (thumbnail, main_img) {
                (Some(t), Some(m)) => (t, m),
                _ => continue,
            };
            let id_str = match &post.id {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => continue,
            };
            let page_url = format!("{}/post/{}", BASE_URL, id_str);
            let thumb_url = format!("{}/{}?height=256", IMAGES_URL, thumb.src);
            let img_url = format!("{}/{}", IMAGES_URL, main.src);

            results.push(
                SearchResult::new(post.title.clone(), page_url)
                    .with_snippet(post.content.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + idx + 1)
                    .with_score(1.0 - (idx as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(img_url))
                    .with_extra("thumbnail", serde_json::json!(thumb_url))
                    .with_extra("author", serde_json::json!(post.name))
                    .with_extra("source", serde_json::json!("cara")),
            );
            idx += 1;
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for CaraEngine {
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
        s.insert("base_url".into(), "https://cara.app".into());
        s.insert("images_url".into(), "https://images.cara.app".into());
        s
    }
}
