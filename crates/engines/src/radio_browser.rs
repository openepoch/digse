//! Radio Browser search engine implementation
//!
//! Radio Browser
//! exposes a JSON "advanced station search" API mirrored across several hosts.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Radio Browser search engine (music / internet radio, JSON API)
pub struct RadioBrowserEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl RadioBrowserEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "radio_browser".to_string(),
            category: EngineCategory::Music,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Radio Browser - community-driven internet radio directory.".to_string(),
            website: Some("https://www.radio-browser.info/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Radio Browser HTTP client");
        RadioBrowserEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // The reference resolves the server pool via DNS; we use a fixed mirror
        // for simplicity, falling back to a second mirror on failure.
        let page_size = 10;
        let offset = query.offset;
        let offset_str = offset.to_string();
        let limit_str = page_size.to_string();
        let servers = [
            "https://de1.api.radio-browser.info",
            "https://nl1.api.radio-browser.info",
        ];
        let mut text: Option<String> = None;
        for server in servers.iter() {
            let url = format!("{}/json/stations/search", server);
            let resp = self
                .client
                .get(&url)
                .header("User-Agent", "digse/0.1.0")
                .header("Accept", "application/json")
                .query(&[
                    ("name", query.query.as_str()),
                    ("order", "votes"),
                    ("offset", offset_str.as_str()),
                    ("limit", limit_str.as_str()),
                    ("hidebroken", "true"),
                    ("reverse", "true"),
                ])
                .send()
                .await;
            match resp {
                Ok(r) if r.status().is_success() => {
                    text = Some(r.text().await.map_err(|e| Error::HttpError(e.to_string()))?);
                    break;
                }
                _ => continue,
            }
        }
        let text = match text {
            Some(t) => t,
            None => return Ok(vec![]),
        };
        let stations: Vec<Station> = match serde_json::from_str(&text) {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let mut results = Vec::new();
        for station in stations.iter() {
            let url = if !station.homepage.is_empty() {
                station.homepage.clone()
            } else {
                station.url_resolved.clone()
            };
            if url.is_empty() {
                continue;
            }
            let mut content_parts = Vec::new();
            let tags = station.tags.split(',').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
            if !tags.is_empty() {
                content_parts.push(tags);
            }
            for field in [&station.state, &station.country] {
                let v = field.trim();
                if !v.is_empty() {
                    content_parts.push(v.to_string());
                }
            }
            let thumbnail = station
                .favicon
                .replacen("http://", "https://", 1);
            let iframe_src = station
                .url_resolved
                .replacen("http://", "https://", 1);
            let mut metadata = Vec::new();
            let codec = station.codec.trim();
            if !codec.is_empty() && codec.to_lowercase() != "unknown" {
                metadata.push(format!("{} radio", codec));
            }
            if station.bitrate > 0 {
                metadata.push(format!("bitrate {}", station.bitrate));
            }
            if station.votes != 0 {
                metadata.push(format!("votes {}", station.votes));
            }
            if station.clickcount != 0 {
                metadata.push(format!("clicks {}", station.clickcount));
            }
            let snippet = if metadata.is_empty() {
                content_parts.join(" | ")
            } else if content_parts.is_empty() {
                metadata.join(" | ")
            } else {
                format!("{} | {}", content_parts.join(" | "), metadata.join(" | "))
            };
            results.push(
                SearchResult::new(station.name.clone(), url)
                    .with_snippet(snippet)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Music)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("iframe_src", serde_json::json!(iframe_src))
                    .with_extra("audio_src", serde_json::json!(iframe_src))
                    .with_extra("source", serde_json::json!("radio_browser")),
            );
        }
        Ok(results)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Station {
    #[serde(default)]
    name: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    url_resolved: String,
    #[serde(default)]
    favicon: String,
    #[serde(default)]
    tags: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    codec: String,
    #[serde(default)]
    bitrate: i64,
    #[serde(default)]
    votes: i64,
    #[serde(default)]
    clickcount: i64,
}

#[async_trait]
impl Engine for RadioBrowserEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Music | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://de1.api.radio-browser.info".into());
        s.insert("page_size".into(), "10".into());
        s
    }
}
