//! SoundCloud search engine implementation
//!
//! The reference engine
//! queries SoundCloud's internal v2 search API, which requires a `client_id`
//! scraped from the SoundCloud web app. This implementation queries the same
//! endpoint and degrades gracefully (empty results) when no client id is
//! available.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// SoundCloud music search engine
pub struct SoundCloudEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    cached_client_id: std::sync::Mutex<Option<String>>,
}

const SEARCH_URL: &str = "https://api-v2.soundcloud.com/search";
const RESULTS_PER_PAGE: usize = 10;

#[derive(Debug, Serialize, Deserialize, Default)]
struct SoundCloudResponse {
    #[serde(default)]
    collection: Vec<SoundCloudTrack>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SoundCloudTrack {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    permalink_url: Option<String>,
    #[serde(default)]
    uri: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    label_name: Option<String>,
    #[serde(default)]
    artwork_url: Option<String>,
    #[serde(default)]
    duration: Option<i64>,
    #[serde(default)]
    likes_count: Option<i64>,
    #[serde(default)]
    playback_count: Option<i64>,
    #[serde(default)]
    last_modified: Option<String>,
    #[serde(default)]
    user: SoundCloudUser,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SoundCloudUser {
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    avatar_url: Option<String>,
}

/// Extract a guest client_id from a JS bundle body by locating the
/// `client_id:"..."` substring.
fn extract_client_id(body: &str) -> Option<String> {
    let needle = "client_id:\"";
    let mut start = 0usize;
    while let Some(rel) = body[start..].find(needle) {
        let id_start = start + rel + needle.len();
        if let Some(end_rel) = body[id_start..].find('"') {
            let id = &body[id_start..id_start + end_rel];
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
        start = id_start + 1;
        if start >= body.len() {
            break;
        }
    }
    None
}

/// Collect `/assets/...js` script URLs from the SoundCloud homepage HTML.
fn extract_asset_js_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut search_from = 0usize;
    while let Some(rel) = html[search_from..].find("/assets/") {
        let abs = search_from + rel;
        // find the end of the path token (next quote or whitespace)
        let rest = &html[abs..];
        let end = rest
            .find(|c: char| c == '"' || c == '\'' || c.is_whitespace())
            .map(|e| abs + e)
            .unwrap_or(html.len());
        let path = &html[abs..end];
        if path.ends_with(".js") {
            let full = if path.starts_with("//") {
                format!("https:{}", path)
            } else if path.starts_with("http") {
                path.to_string()
            } else {
                format!("https://soundcloud.com{}", path)
            };
            urls.push(full);
        }
        search_from = end + 1;
        if search_from >= html.len() {
            break;
        }
    }
    // Iterate in reverse order.
    urls.reverse();
    urls
}

impl SoundCloudEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "soundcloud".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "SoundCloud - audio streaming & music discovery.".to_string(),
            website: Some("https://soundcloud.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create SoundCloud HTTP client");

        SoundCloudEngine {
            metadata,
            client,
            cached_client_id: std::sync::Mutex::new(None),
        }
    }

    /// Fetch (and cache) a guest client_id from the SoundCloud web app.
    /// Returns `None` on any failure.
    async fn get_client_id(&self) -> Option<String> {
        {
            let guard = self.cached_client_id.lock().ok()?;
            if let Some(id) = guard.as_ref() {
                return Some(id.clone());
            }
        }

        let home = self
            .client
            .get("https://soundcloud.com")
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .ok()?;
        if !home.status().is_success() {
            return None;
        }
        let html = home.text().await.ok()?;

        for url in extract_asset_js_urls(&html) {
            let resp = match self
                .client
                .get(&url)
                .header("User-Agent", "digse/0.1.0")
                .send()
                .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };
            if !resp.status().is_success() {
                continue;
            }
            let body = match resp.text().await {
                Ok(t) => t,
                Err(_) => continue,
            };
            if let Some(id) = extract_client_id(&body) {
                if let Ok(mut guard) = self.cached_client_id.lock() {
                    *guard = Some(id.clone());
                }
                return Some(id);
            }
        }
        None
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let client_id = match self.get_client_id().await {
            Some(id) => id,
            None => {
                tracing::info!("soundcloud: no client_id available; returning empty");
                return Ok(vec![]);
            }
        };

        let offset = query.offset.to_string();
        let limit = RESULTS_PER_PAGE.to_string();
        let locale = "en".to_string();

        let resp = self
            .client
            .get(SEARCH_URL)
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("q", query.query.as_str()),
                ("offset", offset.as_str()),
                ("limit", limit.as_str()),
                ("facet", "model"),
                ("client_id", client_id.as_str()),
                ("app_locale", locale.as_str()),
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

        let parsed: SoundCloudResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, track) in parsed.collection.iter().enumerate() {
            if track.kind != "track" && track.kind != "playlist" {
                continue;
            }
            let url = match &track.permalink_url {
                Some(u) if !u.is_empty() => u.clone(),
                _ => continue,
            };

            let mut content_parts = Vec::new();
            if let Some(d) = &track.description {
                if !d.is_empty() {
                    content_parts.push(d.clone());
                }
            }
            if let Some(l) = &track.label_name {
                if !l.is_empty() {
                    content_parts.push(l.clone());
                }
            }

            let duration_secs = track.duration.unwrap_or(0) / 1000;
            let author = if !track.user.full_name.is_empty() {
                track.user.full_name.clone()
            } else {
                String::new()
            };

            let iframe_src = track
                .uri
                .as_ref()
                .map(|u| {
                    format!(
                        "https://w.soundcloud.com/player/?url={}",
                        urlencoding::encode(u)
                    )
                })
                .unwrap_or_default();

            let thumbnail = track
                .artwork_url
                .clone()
                .or_else(|| track.user.avatar_url.clone())
                .unwrap_or_default();

            let mut result = SearchResult::new(track.title.clone(), url)
                .with_snippet(content_parts.join(" / "))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Music);

            if !author.is_empty() {
                result = result.with_extra("artist", serde_json::json!(author));
            }
            if duration_secs > 0 {
                result = result.with_extra("duration", serde_json::json!(duration_secs));
            }
            if !iframe_src.is_empty() {
                result = result.with_extra("audio_src", serde_json::json!(iframe_src));
                result = result.with_extra("iframe_src", serde_json::json!(iframe_src));
            }
            if !thumbnail.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
            }
            if let Some(views) = track.playback_count {
                result = result.with_extra("views", serde_json::json!(views));
            }
            if let Some(published) = &track.last_modified {
                if !published.is_empty() {
                    result = result.with_extra("published", serde_json::json!(published));
                }
            }

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for SoundCloudEngine {
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
        s.insert("search_url".into(), SEARCH_URL.into());
        s.insert(
            "results_per_page".into(),
            RESULTS_PER_PAGE.to_string(),
        );
        s
    }
}
