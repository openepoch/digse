//! Deezer (music) search engine implementation
//!
//! Uses the official Deezer API to search for music tracks.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Deezer music search engine
pub struct DeezerEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeezerResponse {
    #[serde(default)]
    data: Vec<DeezerTrack>,
    #[serde(default)]
    total: i64,
    #[serde(default)]
    next: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DeezerTrack {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: String,
    #[serde(default)]
    duration: i64,
    #[serde(default)]
    artist: DeezerArtist,
    #[serde(default)]
    album: DeezerAlbum,
    #[serde(default)]
    #[serde(rename = "type")]
    item_type: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DeezerArtist {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DeezerAlbum {
    #[serde(default)]
    title: String,
}

impl DeezerEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "deezer".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Deezer music search.".to_string(),
            website: Some("https://www.deezer.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Deezer HTTP client");

        DeezerEngine { metadata, client }
    }

    /// Format duration (seconds) as "M:SS"
    fn format_duration(secs: i64) -> String {
        if secs <= 0 {
            return "0:00".to_string();
        }
        let minutes = secs / 60;
        let seconds = secs % 60;
        format!("{}:{:02}", minutes, seconds)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let offset = query.offset.to_string();
        let url = "https://api.deezer.com/search";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("index", offset.as_str()),
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

        let parsed: DeezerResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, track) in parsed.data.iter().enumerate() {
            // Only include tracks
            if !track.item_type.is_empty() && track.item_type != "track" {
                continue;
            }
            if track.link.is_empty() {
                continue;
            }
            // Force HTTPS
            let url = if track.link.starts_with("http://") {
                format!("https{}", &track.link[4..])
            } else {
                track.link.clone()
            };

            let content = format!(
                "{} - {} - {}",
                track.artist.name, track.album.title, track.title
            );
            let duration = Self::format_duration(track.duration);
            let audio_src = format!(
                "https://www.deezer.com/plugins/player?type=tracks&id={}",
                track.id
            );

            let result = SearchResult::new(track.title.clone(), url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Music)
                .with_extra("artist", serde_json::json!(track.artist.name))
                .with_extra("album", serde_json::json!(track.album.title))
                .with_extra("duration", serde_json::json!(duration))
                .with_extra("audio_src", serde_json::json!(audio_src));

            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for DeezerEngine {
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
        matches!(result_type, ResultType::Music | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://api.deezer.com".to_string());
        settings.insert("search_endpoint".to_string(), "/search".to_string());
        settings
    }
}
