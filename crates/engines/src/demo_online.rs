//! Demo online engine implementation
//!
//! The reference implementation queries
//! The Art Institute of Chicago's artwork API and returns image results. This
//! port hits the same API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Demo online engine (Art Institute of Chicago)
pub struct DemoOnlineEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct ArticResponse {
    #[serde(default)]
    data: Vec<ArticArtwork>,
    #[serde(default)]
    pagination: ArticPagination,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ArticArtwork {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist_display: String,
    #[serde(default)]
    medium_display: String,
    #[serde(default)]
    date_display: String,
    #[serde(default)]
    dimensions: String,
    #[serde(default)]
    image_id: String,
    #[serde(default)]
    artist_titles: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ArticPagination {
    #[serde(default)]
    total: i64,
    #[serde(default)]
    limit: i64,
    #[serde(default)]
    current_page: i64,
}

impl DemoOnlineEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "demo_online".to_string(),
            category: EngineCategory::Images,
            enabled: false,
            requires_auth: false,
            timeout_seconds: 4,
            description: "Demo online engine (Art Institute of Chicago).".to_string(),
            website: Some("https://www.artic.edu".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(4))
            .build()
            .expect("Failed to create demo_online HTTP client");

        DemoOnlineEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.artic.edu/api/v1/artworks/search";
        let page = ((query.offset / 20) + 1).to_string();
        let limit = query.count.to_string();

        let fields = "id,title,artist_display,medium_display,image_id,date_display,dimensions,artist_titles";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page.as_str()),
                ("limit", limit.as_str()),
                ("fields", fields),
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

        let parsed: ArticResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let image_api = "https://www.artic.edu/iiif/2/";
        let mut results = Vec::new();
        for (i, artwork) in parsed.data.iter().enumerate() {
            if artwork.image_id.is_empty() {
                continue;
            }
            let page_url = format!("https://artic.edu/artworks/{}", artwork.id);
            let title = format!(
                "{} ({}) // {}",
                artwork.title, artwork.date_display, artwork.artist_display
            );
            let content = format!("{} // {}", artwork.medium_display, artwork.dimensions);
            let author = artwork.artist_titles.join(", ");
            let img_src = format!(
                "{}/{}/full/843,/0/default.jpg",
                image_api, artwork.image_id
            );

            let result = SearchResult::new(title, page_url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(img_src))
                .with_extra("source", serde_json::json!("artic.edu"))
                .with_extra("author", serde_json::json!(author));

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for DemoOnlineEngine {
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
        settings.insert(
            "search_api".to_string(),
            "https://api.artic.edu/api/v1/artworks/search".to_string(),
        );
        settings.insert("image_api".to_string(), "https://www.artic.edu/iiif/2/".to_string());
        settings
    }
}
