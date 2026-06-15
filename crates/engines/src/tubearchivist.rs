//! Tube Archivist search engine implementation
//!
//! Queries a self-hosted Tube
//! Archivist instance at `{base_url}/api/search/?query=` with a `Token {ta_token}`
//! Authorization header. The response carries channel results and video
//! results; each becomes a URL-centric `SearchResult` tagged `Videos`.
//!
//! Configuration mirrors the reference `settings.yml` fields, supplied via
//! environment: `TUBEARCHIVIST_BASE_URL` (required) and
//! `TUBEARCHIVIST_TOKEN` / `TUBEARCHIVIST_TA_TOKEN` (required). When either is
//! unset the engine degrades to returning no results.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Tube Archivist self-hosted video search engine
pub struct TubearchivistEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    ta_token: String,
    ta_link_to_mp4: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TaResponse {
    #[serde(default)]
    results: Option<TaResults>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TaResults {
    #[serde(default)]
    channel_results: Vec<TaChannel>,
    #[serde(default)]
    video_results: Vec<TaVideo>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TaChannel {
    #[serde(default)]
    channel_id: String,
    #[serde(default)]
    channel_name: String,
    #[serde(default)]
    channel_description: String,
    #[serde(default)]
    channel_subs: serde_json::Value,
    #[serde(default)]
    channel_thumb_url: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TaVideo {
    #[serde(default)]
    youtube_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    channel: TaChannel,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    player: TaPlayer,
    #[serde(default)]
    stats: TaStats,
    #[serde(default)]
    published: String,
    #[serde(default)]
    vid_thumb_url: String,
    #[serde(default)]
    media_url: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TaPlayer {
    #[serde(default)]
    duration_str: String,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct TaStats {
    #[serde(default)]
    view_count: serde_json::Value,
}

impl TubearchivistEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "tubearchivist".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Tube Archivist self-hosted video search.".to_string(),
            website: Some("https://www.tubearchivist.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Tube Archivist HTTP client");

        let ta_token = std::env::var("TUBEARCHIVIST_TOKEN")
            .or_else(|_| std::env::var("TUBEARCHIVIST_TA_TOKEN"))
            .unwrap_or_default();

        TubearchivistEngine {
            metadata,
            client,
            base_url: std::env::var("TUBEARCHIVIST_BASE_URL").unwrap_or_default(),
            ta_token,
            ta_link_to_mp4: std::env::var("TUBEARCHIVIST_LINK_TO_MP4")
                .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
                .unwrap_or(false),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // Graceful degradation when the backend is unconfigured (ref: init()).
        if self.base_url.is_empty() || self.ta_token.is_empty() {
            tracing::info!(
                "tubearchivist requires TUBEARCHIVIST_BASE_URL and token; returning empty"
            );
            return Ok(vec![]);
        }

        let encoded = urlencoding::encode(&query.query);
        let url = format!(
            "{}/api/search/?query={}",
            self.base_url.trim_end_matches('/'),
            encoded
        );

        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Token {}", self.ta_token))
            .header("Accept", "application/json")
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let parsed: TaResponse = match response.json().await {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let results = match parsed.results {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        let mut out = Vec::new();
        let base = self.base_url.trim_end_matches('/');

        // Channel results.
        for (i, ch) in results.channel_results.iter().enumerate() {
            if out.len() >= query.count {
                break;
            }
            if ch.channel_id.is_empty() {
                continue;
            }
            let channel_url = format!("{}/channel/{}", base, ch.channel_id);
            let thumbnail = format!("{}{}?auth={}", base, ch.channel_thumb_url, self.ta_token);
            let subs = humanize_number(&ch.channel_subs);

            let result = SearchResult::new(ch.channel_name.clone(), channel_url)
                .with_snippet(html_to_text(&ch.channel_description))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("author", serde_json::json!(ch.channel_name))
                .with_extra("views", serde_json::json!(subs))
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("source", serde_json::json!("tubearchivist"));
            out.push(result);
        }

        // Video results.
        for video in results.video_results.iter() {
            if out.len() >= query.count {
                break;
            }
            let url = if self.ta_link_to_mp4 {
                format!("{}{}", base, video.media_url)
            } else {
                format!("{}/?videoId={}", base, video.youtube_id)
            };
            let thumbnail =
                format!("{}{}?auth={}", base, video.vid_thumb_url, self.ta_token);
            let views = humanize_number(&video.stats.view_count);

            // metadata: channel name + up to 5 tags.
            let mut meta: Vec<String> = Vec::new();
            if !video.channel.channel_name.is_empty() {
                meta.push(video.channel.channel_name.clone());
            }
            for t in &video.tags {
                if meta.len() >= 5 {
                    break;
                }
                if !t.is_empty() {
                    meta.push(t.clone());
                }
            }

            let result = SearchResult::new(video.title.clone(), url)
                .with_snippet(html_to_text(&video.description))
                .with_engine(self.name())
                .with_rank(query.offset + out.len() + 1)
                .with_score(1.0 - (out.len() as f64 * 0.05))
                .with_result_type(ResultType::Videos)
                .with_extra("author", serde_json::json!(video.channel.channel_name))
                .with_extra("duration", serde_json::json!(video.player.duration_str))
                .with_extra("views", serde_json::json!(views))
                .with_extra("published", serde_json::json!(video.published))
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("iframe_src", serde_json::json!(format!("https://www.youtube.com/embed/{}", video.youtube_id)))
                .with_extra("metadata", serde_json::json!(meta.join(" | ")))
                .with_extra("source", serde_json::json!("tubearchivist"));
            out.push(result);
        }
        Ok(out)
    }
}

/// Render a numeric Value (int or string) compactly, e.g. `1.2K`. Falls back to
/// the raw string representation for non-numeric input.
fn humanize_number(v: &serde_json::Value) -> String {
    let n: Option<i64> = v.as_i64().or_else(|| {
        v.as_str()
            .and_then(|s| s.trim().parse::<i64>().ok())
    });
    match n {
        Some(num) if num >= 0 => {
            const UNITS: &[&str] = &["", "K", "M", "B", "T"];
            let mut size = num as f64;
            let mut unit = 0;
            while size >= 1000.0 && unit < UNITS.len() - 1 {
                size /= 1000.0;
                unit += 1;
            }
            if unit == 0 {
                num.to_string()
            } else {
                format!("{:.1}{}", size, UNITS[unit])
            }
        }
        _ => v.to_string(),
    }
}

/// Minimal HTML-to-text: strip tags, decode common entities, collapse whitespace.
fn html_to_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[async_trait]
impl Engine for TubearchivistEngine {
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
        matches!(t, ResultType::Videos | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), self.base_url.clone());
        s.insert("ta_link_to_mp4".to_string(), self.ta_link_to_mp4.to_string());
        s
    }
}
