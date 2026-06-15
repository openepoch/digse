//! Art Institute of Chicago search engine implementation (JSON, images)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Art Institute of Chicago artwork search engine
pub struct ArticEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ArticResponse {
    #[serde(default)]
    data: Vec<ArticArtwork>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ArticArtwork {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "artist_display")]
    artist_display: String,
    #[serde(default, rename = "medium_display")]
    medium_display: String,
    #[serde(default, rename = "image_id")]
    image_id: String,
    #[serde(default, rename = "date_display")]
    date_display: String,
    #[serde(default)]
    dimensions: String,
    #[serde(default, rename = "artist_titles")]
    artist_titles: Vec<String>,
}

impl ArticEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "artic".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Art Institute of Chicago - artwork search.".to_string(),
            website: Some("https://www.artic.edu".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Artic HTTP client");

        ArticEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let search_api = "https://api.artic.edu/api/v1/artworks/search";
        let page = ((query.offset / 20) + 1).to_string();
        let limit = query.count.to_string();

        let resp = self.client
            .get(search_api)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page.as_str()),
                ("limit", limit.as_str()),
                ("fields", "id,title,artist_display,medium_display,image_id,date_display,dimensions,artist_titles"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: ArticResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let image_api = "https://www.artic.edu/iiif/2/";
        let mut results = Vec::new();
        for (i, art) in parsed.data.iter().enumerate() {
            if art.image_id.is_empty() {
                continue;
            }
            let url = format!("https://artic.edu/artworks/{}", art.id);
            let title = format!("{} ({}) // {}", art.title, art.date_display, art.artist_display);
            let content = format!("{} // {}", art.medium_display, art.dimensions);
            let img_src = format!("{}/{}/full/843,/0/default.jpg", image_api, art.image_id);
            let author = art.artist_titles.join(", ");

            let r = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(img_src))
                .with_extra("author", serde_json::json!(author))
                .with_extra("source", serde_json::json!("artic"));
            results.push(r);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for ArticEngine {
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
        s.insert("base_url".to_string(), "https://api.artic.edu".to_string());
        s
    }
}
