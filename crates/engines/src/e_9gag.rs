//! 9GAG search engine implementation (JSON API, social media)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// 9GAG social media search engine
pub struct Gag9Engine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Gag9Response {
    #[serde(default)]
    data: Gag9Data,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Gag9Data {
    #[serde(default)]
    posts: Vec<Gag9Post>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Gag9Post {
    #[serde(default, rename = "type")]
    post_type: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "creationTs")]
    creation_ts: serde_json::Value,
    #[serde(default)]
    images: Gag9Images,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Gag9Images {
    #[serde(default)]
    #[serde(rename = "image700")]
    image700: Gag9Img,
    #[serde(default)]
    #[serde(rename = "imageFbThumbnail")]
    image_fb_thumbnail: Gag9Img,
    #[serde(default)]
    #[serde(rename = "image460sv")]
    image460sv: Gag9Img,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct Gag9Img {
    #[serde(default)]
    url: String,
    #[serde(default)]
    height: i64,
}

impl Gag9Engine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "9gag".to_string(),
            category: EngineCategory::Social,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "9GAG - social media meme & content search.".to_string(),
            website: Some("https://9gag.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create 9GAG HTTP client");

        Gag9Engine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let c = (query.offset * 10).to_string();

        let resp = self.client
            .get("https://9gag.com/v1/search-posts")
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("query", query.query.as_str()),
                ("c", c.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: Gag9Response = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, post) in parsed.data.posts.iter().enumerate() {
            if post.url.is_empty() {
                continue;
            }
            let thumbnail = if post.images.image700.height > 400 {
                post.images.image_fb_thumbnail.url.clone()
            } else {
                post.images.image700.url.clone()
            };
            let is_animated = post.post_type == "Animated";

            let mut r = SearchResult::new(post.title.clone(), post.url.clone())
                .with_snippet(post.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(if is_animated { ResultType::Videos } else { ResultType::Images })
                .with_extra("thumbnail", serde_json::json!(thumbnail));

            if let Some(ts) = post.creation_ts.as_i64() {
                r = r.with_extra("published", serde_json::json!(ts));
            }

            if is_animated {
                r = r.with_extra("iframe_src", serde_json::json!(post.images.image460sv.url));
            } else {
                r = r.with_extra("img_src", serde_json::json!(post.images.image700.url));
            }
            results.push(r);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for Gag9Engine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Social | ResultType::Images | ResultType::Videos | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://9gag.com".to_string());
        s.insert("api_url".to_string(), "https://9gag.com/v1/search-posts".to_string());
        s
    }
}
