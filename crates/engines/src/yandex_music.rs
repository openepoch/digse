//! Yandex Music search engine implementation (JSON).

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Yandex Music search engine.
pub struct YandexMusicEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://music.yandex.ru";

impl YandexMusicEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "yandex_music".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Yandex Music - track search.".to_string(),
            website: Some(BASE_URL.to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Yandex Music HTTP client");
        YandexMusicEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = (query.offset / 10).to_string();
        let url = format!(
            "{}/handlers/music-search.jsx?text={}&page={}",
            BASE_URL,
            urlencoding::encode(&query.query),
            page
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: YmResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let items = parsed.tracks.map(|t| t.items).unwrap_or_default();
        for (i, item) in items.iter().enumerate() {
            if i >= query.count {
                break;
            }
            if item.type_field.as_deref() != Some("music") {
                continue;
            }
            let track_id = item.id.clone();
            let album_id = item.albums.first().map(|a| a.id.clone()).unwrap_or_default();
            if track_id.is_empty() || album_id.is_empty() {
                continue;
            }
            let url = format!("{}/album/{}/track/{}", BASE_URL, album_id, track_id);
            let title = item.title.clone();
            let album_title = item
                .albums
                .first()
                .map(|a| a.title.clone())
                .unwrap_or_default();
            let artist = item
                .artists
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Unknown Artist".to_string());
            let content = format!("[{}] {} - {}", album_title, artist, title);
            let iframe_src = format!("{}/iframe/track/{}/{}", BASE_URL, track_id, album_id);

            let r = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Music)
                .with_extra("artist", serde_json::json!(artist))
                .with_extra("album", serde_json::json!(album_title))
                .with_extra("audio_src", serde_json::json!(iframe_src))
                .with_extra("iframe_src", serde_json::json!(iframe_src))
                .with_extra("track_id", serde_json::json!(track_id));
            results.push(r);
        }
        Ok(results)
    }
}

#[derive(Debug, Deserialize)]
struct YmResponse {
    #[serde(default)]
    tracks: Option<YmTracks>,
}

#[derive(Debug, Deserialize)]
struct YmTracks {
    #[serde(default)]
    items: Vec<YmTrack>,
}

#[derive(Debug, Deserialize)]
struct YmTrack {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "type")]
    type_field: Option<String>,
    #[serde(default)]
    albums: Vec<YmAlbum>,
    #[serde(default)]
    artists: Vec<YmArtist>,
}

#[derive(Debug, Deserialize)]
struct YmAlbum {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug, Deserialize)]
struct YmArtist {
    #[serde(default)]
    name: String,
}

#[async_trait]
impl Engine for YandexMusicEngine {
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
        matches!(t, ResultType::Music | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), BASE_URL.to_string());
        s.insert("results".to_string(), "JSON".to_string());
        s
    }
}
