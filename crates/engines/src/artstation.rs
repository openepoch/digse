//! ArtStation search engine implementation (JSON, images)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// ArtStation artwork search engine
pub struct ArtstationEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ArtstationResponse {
    #[serde(default)]
    data: Vec<ArtstationItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ArtstationItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default, rename = "smaller_square_cover_url")]
    smaller_square_cover_url: String,
    #[serde(default)]
    user: ArtstationUser,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ArtstationUser {
    #[serde(default)]
    username: String,
    #[serde(default, rename = "full_name")]
    full_name: String,
}

impl ArtstationEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "artstation".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "ArtStation - digital artwork showcase search.".to_string(),
            website: Some("https://www.artstation.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create ArtStation HTTP client");

        ArtstationEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_url = "https://www.artstation.com/api/v2/search/projects.json";
        let page = ((query.offset / 20) + 1).to_string();
        let per_page = query.count.to_string();

        let body = serde_json::json!({
            "query": query.query,
            "page": page,
            "per_page": per_page,
            "sorting": "relevance",
            "pro_first": 1,
        });

        let resp = self.client
            .post(api_url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: ArtstationResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.data.iter().enumerate() {
            if item.url.is_empty() {
                continue;
            }
            let thumb = item.smaller_square_cover_url.clone();
            // The reference derives a large image URL from the smaller-square cover
            // by stripping the leading `/<digits>/` segment and renaming the size.
            let fullsize = to_large_image_url(&thumb);
            let author = format!("{} ({})", item.user.username, item.user.full_name);

            let r = SearchResult::new(item.title.clone(), item.url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(fullsize))
                .with_extra("thumbnail", serde_json::json!(thumb))
                .with_extra("author", serde_json::json!(author))
                .with_extra("source", serde_json::json!("artstation"));
            results.push(r);
        }
        Ok(results)
    }
}

/// Rewrite an ArtStation `smaller_square` cover URL into a large image URL by
/// stripping the leading `/<digits>/` path segment and replacing
/// `smaller_square` with `large`. Mirrors the reference's
/// `re.sub(r'/\d{6,}/', '/', thumb).replace("smaller_square", "large")`.
fn to_large_image_url(thumb: &str) -> String {
    let mut parts: Vec<&str> = thumb.split('/').collect();
    // Drop the first long all-digit path segment (6+ digits), if any.
    if let Some(idx) = parts.iter().position(|p| p.len() >= 6 && p.chars().all(|c| c.is_ascii_digit())) {
        parts.remove(idx);
    }
    let mut joined = parts.join("/");
    if joined.contains("smaller_square") {
        joined = joined.replace("smaller_square", "large");
    }
    joined
}

#[async_trait]
impl Engine for ArtstationEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Images | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://www.artstation.com".to_string());
        s.insert("api_url".to_string(), "https://www.artstation.com/api/v2/search/projects.json".to_string());
        s
    }
}
