//! Genius search engine implementation
//!
//! Uses the Genius public API
//! `https://genius.com/api/search/multi?q=...&per_page=N&page=M` returning
//! sections of hits (lyric/song, artist, album). Category: music.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Genius lyrics/music search engine (JSON API)
pub struct GeniusEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeniusResponse {
    #[serde(default)]
    response: GeniusResponseBody,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GeniusResponseBody {
    #[serde(default)]
    sections: Vec<GeniusSection>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GeniusSection {
    #[serde(default)]
    hits: Vec<GeniusHit>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeniusHit {
    #[serde(default)]
    #[serde(rename = "type")]
    hit_type: String,
    #[serde(default)]
    result: serde_json::Value,
    #[serde(default)]
    highlights: Vec<GeniusHighlight>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GeniusHighlight {
    #[serde(default)]
    value: String,
}

impl GeniusEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "genius".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Genius - song lyrics, artists and albums.".to_string(),
            website: Some("https://genius.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Genius HTTP client");

        GeniusEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page_size = 5.min(query.count.max(1));
        let per_page = page_size.to_string();
        let page = query.offset + 1;
        let page_str = page.to_string();
        let url = "https://genius.com/api/search/multi";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("per_page", per_page.as_str()),
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

        let parsed: GeniusResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let mut i = 0usize;
        for section in &parsed.response.sections {
            for hit in &section.hits {
                if i >= query.count {
                    break;
                }
                let parsed_hit = match hit.hit_type.as_str() {
                    "lyric" | "song" => parse_lyric(hit),
                    "artist" => parse_artist(hit),
                    "album" => parse_album(hit),
                    _ => None,
                };
                if let Some(mut r) = parsed_hit {
                    r = r
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Music)
                        .with_extra("source", serde_json::json!("genius"));
                    // Add the apple_music iframe player if an api_path exists.
                    if let Some(api_path) = hit
                        .result
                        .get("api_path")
                        .and_then(|v| v.as_str())
                    {
                        r = r.with_extra(
                            "iframe_src",
                            serde_json::json!(format!(
                                "https://genius.com{}/apple_music_player",
                                api_path
                            )),
                        );
                    }
                    results.push(r);
                    i += 1;
                }
            }
        }

        Ok(results)
    }
}

fn parse_lyric(hit: &GeniusHit) -> Option<SearchResult> {
    let result = &hit.result;
    let url = result
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let full_title = result
        .get("full_title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let thumbnail = result
        .get("song_art_image_thumbnail_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let content = if let Some(h) = hit.highlights.first() {
        h.value.clone()
    } else {
        result
            .get("title_with_featured")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let title = if full_title.is_empty() {
        "Genius".to_string()
    } else {
        full_title
    };
    Some(
        SearchResult::new(title, url)
            .with_snippet(content)
            .with_extra("thumbnail", serde_json::json!(thumbnail)),
    )
}

fn parse_artist(hit: &GeniusHit) -> Option<SearchResult> {
    let result = &hit.result;
    let url = result
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let name = result
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let thumbnail = result
        .get("image_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let title = if name.is_empty() {
        "Genius artist".to_string()
    } else {
        name
    };
    Some(
        SearchResult::new(title, url)
            .with_snippet("")
            .with_extra("thumbnail", serde_json::json!(thumbnail)),
    )
}

fn parse_album(hit: &GeniusHit) -> Option<SearchResult> {
    let result = &hit.result;
    let url = result
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let full_title = result
        .get("full_title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let cover = result
        .get("cover_art_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let mut content = result
        .get("name_with_artist")
        .and_then(|v| v.as_str())
        .or_else(|| result.get("name").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    if let Some(year) = result
        .get("release_date_components")
        .and_then(|c| c.get("year"))
        .and_then(|y| y.as_i64())
    {
        content = format!("{} / {}", year, content);
    }
    let title = if full_title.is_empty() {
        "Genius album".to_string()
    } else {
        full_title
    };
    Some(
        SearchResult::new(title, url)
            .with_snippet(content.trim().to_string())
            .with_extra("thumbnail", serde_json::json!(cover)),
    )
}

#[async_trait]
impl Engine for GeniusEngine {
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
        settings.insert("base_url".to_string(), "https://genius.com".to_string());
        settings.insert(
            "search_endpoint".to_string(),
            "/api/search/multi".to_string(),
        );
        settings
    }
}
