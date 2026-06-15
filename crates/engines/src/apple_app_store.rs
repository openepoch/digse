//! Apple App Store search engine implementation (iTunes JSON API)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Apple App Store (iTunes) search engine
pub struct AppleAppStoreEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ItunesResponse {
    #[serde(default)]
    results: Vec<ItunesApp>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ItunesApp {
    #[serde(default, rename = "trackViewUrl")]
    track_view_url: String,
    #[serde(default, rename = "trackName")]
    track_name: String,
    #[serde(default)]
    description: String,
    #[serde(default, rename = "artworkUrl100")]
    artwork_url_100: String,
    #[serde(default, rename = "currentVersionReleaseDate")]
    current_version_release_date: String,
    #[serde(default, rename = "sellerName")]
    seller_name: String,
}

impl AppleAppStoreEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "apple_app_store".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Apple App Store - iOS/macOS app search (iTunes API).".to_string(),
            website: Some("https://www.apple.com/app-store/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Apple App Store HTTP client");

        AppleAppStoreEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let resp = self.client
            .get("https://itunes.apple.com/search")
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("term", query.query.as_str()),
                ("media", "software"),
                ("explicit", "Yes"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: ItunesResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, app) in parsed.results.iter().enumerate() {
            if app.track_view_url.is_empty() {
                continue;
            }
            let r = SearchResult::new(app.track_name.clone(), app.track_view_url.clone())
                .with_snippet(app.description.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Files)
                .with_extra("thumbnail", serde_json::json!(app.artwork_url_100))
                .with_extra("author", serde_json::json!(app.seller_name))
                .with_extra("published", serde_json::json!(app.current_version_release_date));
            results.push(r);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for AppleAppStoreEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Files | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://itunes.apple.com".to_string());
        s
    }
}
