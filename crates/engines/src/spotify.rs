//! Spotify search engine implementation
//!
//! Uses the official Web API with the
//! client-credentials OAuth flow. Requires `SPOTIFY_CLIENT_ID` and
//! `SPOTIFY_CLIENT_SECRET`; when
//! either is absent the engine degrades to returning no results. Standard
//! base64 is encoded inline because the `base64` crate is not a dependency.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Spotify music search engine
pub struct SpotifyEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SpotifySearch {
    #[serde(default)]
    tracks: Option<SpotifyTracks>,
}

#[derive(Debug, Deserialize, Default)]
struct SpotifyTracks {
    #[serde(default)]
    items: Vec<SpotifyTrack>,
}

#[derive(Debug, Deserialize, Default)]
struct SpotifyTrack {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    external_urls: SpotifyExternalUrls,
    #[serde(default)]
    artists: Vec<SpotifyArtist>,
    #[serde(default)]
    album: SpotifyAlbum,
}

#[derive(Debug, Deserialize, Default)]
struct SpotifyArtist {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct SpotifyAlbum {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct SpotifyExternalUrls {
    #[serde(default)]
    spotify: String,
}

#[derive(Debug, Deserialize)]
struct SpotifyToken {
    access_token: String,
}

/// Standard (RFC 4648) base64 encoding with padding.
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut chunks = input.chunks_exact(3);
    for c in chunks.by_ref() {
        let n = ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | (c[2] as u32);
        out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
        out.push(TABLE[(n & 0x3F) as usize] as char);
    }
    let rem = chunks.remainder();
    match rem.len() {
        1 => {
            let n = (rem[0] as u32) << 16;
            out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
            out.push(TABLE[((n >> 18) & 0x3F) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3F) as usize] as char);
            out.push(TABLE[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

impl SpotifyEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "spotify".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: true,
            timeout_seconds: 10,
            description: "Spotify music search (needs SPOTIFY_CLIENT_ID/SECRET).".to_string(),
            website: Some("https://www.spotify.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Spotify HTTP client");

        SpotifyEngine {
            metadata,
            client,
            client_id: std::env::var("SPOTIFY_CLIENT_ID").ok(),
            client_secret: std::env::var("SPOTIFY_CLIENT_SECRET").ok(),
        }
    }

    /// Fetch a bearer token via the client-credentials flow. `None` if creds
    /// are missing or the request fails.
    async fn fetch_token(&self) -> Option<String> {
        let id = self.client_id.as_ref()?;
        let secret = self.client_secret.as_ref()?;
        let auth = base64_encode(format!("{}:{}", id, secret).as_bytes());

        let resp = self
            .client
            .post("https://accounts.spotify.com/api/token")
            .header("Authorization", format!("Basic {}", auth))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body("grant_type=client_credentials")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let token: SpotifyToken = resp.json().await.ok()?;
        Some(token.access_token)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let token = match self.fetch_token().await {
            Some(t) => t,
            None => {
                tracing::info!("spotify requires SPOTIFY_CLIENT_ID/SECRET; returning empty");
                return Ok(vec![]);
            }
        };

        let pageno = (query.offset / query.count.max(1)) + 1;
        let limit = query.count.clamp(1, 50).to_string();
        let offset = ((pageno - 1) * query.count.max(1)) as usize;
        let offset_str = offset.to_string();
        let url = "https://api.spotify.com/v1/search";

        let response = self
            .client
            .get(url)
            .query(&[
                ("q", query.query.as_str()),
                ("type", "track"),
                ("limit", limit.as_str()),
                ("offset", offset_str.as_str()),
                ("market", "US"),
            ])
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let parsed: SpotifySearch = match response.json().await {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        if let Some(tracks) = parsed.tracks {
            for (i, track) in tracks.items.iter().enumerate() {
                if results.len() >= query.count {
                    break;
                }
                if track.r#type != "track" || track.external_urls.spotify.is_empty() {
                    continue;
                }
                let artist = track
                    .artists
                    .first()
                    .map(|a| a.name.clone())
                    .unwrap_or_default();
                let content = format!("{} - {} - {}", artist, track.album.name, track.name);

                let result = SearchResult::new(track.name.clone(), track.external_urls.spotify.clone())
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Music)
                    .with_extra("artist", serde_json::json!(artist))
                    .with_extra("album", serde_json::json!(track.album.name))
                    .with_extra("track_id", serde_json::json!(track.id))
                    .with_extra(
                        "audio_src",
                        serde_json::json!(format!("https://embed.spotify.com/?uri=spotify:track:{}", track.id)),
                    )
                    .with_extra("source", serde_json::json!("spotify"));
                results.push(result);
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for SpotifyEngine {
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
        s.insert("search_url".to_string(), "https://api.spotify.com/v1/search".to_string());
        s.insert("token_url".to_string(), "https://accounts.spotify.com/api/token".to_string());
        s
    }
}
