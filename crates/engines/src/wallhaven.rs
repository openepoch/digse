//! Wallhaven wallpaper search engine implementation (JSON).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Wallhaven wallpaper search engine (JSON API; API key optional).
pub struct WallhavenEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

const BASE_URL: &str = "https://wallhaven.cc";

#[derive(Debug, Serialize, Deserialize)]
struct WallhavenResponse {
    #[serde(default)]
    data: Vec<WallhavenItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WallhavenItem {
    #[serde(default)]
    id: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    category: String,
    #[serde(default)]
    purity: String,
    #[serde(default)]
    resolution: String,
    #[serde(default)]
    file_type: String,
    #[serde(default)]
    file_size: i64,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    thumbs: WallhavenThumbs,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WallhavenThumbs {
    #[serde(default)]
    small: String,
    #[serde(default)]
    large: String,
    #[serde(default)]
    original: String,
}

impl WallhavenEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("WALLHAVEN_API_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        let metadata = EngineMetadata {
            name: "wallhaven".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: api_key.is_some(),
            timeout_seconds: 15,
            description: "Wallhaven - community wallpapers (JSON API).".to_string(),
            website: Some(BASE_URL.to_string() + "/"),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Wallhaven HTTP client");
        WallhavenEngine {
            metadata,
            client,
            api_key,
        }
    }

    /// Map digse safe_search (false=off, true=on) to wallhaven purity.
    /// purity bits: SFW / Sketchy / NSFW. NSFW requires a valid API key.
    fn purity(safe: bool) -> &'static str {
        if safe {
            "100"
        } else {
            "111"
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = (query.offset / 10) + 1;
        let page_str = page.to_string();
        let purity = Self::purity(query.safe_search);

        let mut req = self
            .client
            .get(format!("{}/api/v1/search", BASE_URL))
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page_str.as_str()),
                ("purity", purity),
            ]);
        if let Some(key) = &self.api_key {
            req = req.header("X-API-Key", key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: WallhavenResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.data.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let content = format!("{} / {}", item.category, item.purity);
            let resolution = item.resolution.replace('x', " x ");
            let r = SearchResult::new(item.id.clone(), item.url.clone())
                .with_snippet(content.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(item.path))
                .with_extra("thumbnail", serde_json::json!(item.thumbs.small))
                .with_extra("source", serde_json::json!("wallhaven"))
                .with_extra("resolution", serde_json::json!(resolution))
                .with_extra("width", serde_json::json!(resolution.clone()))
                .with_extra("format", serde_json::json!(item.file_type))
                .with_extra("filesize", serde_json::json!(humanize_bytes(item.file_size)))
                .with_extra("published", serde_json::json!(item.created_at));
            results.push(r);
        }
        Ok(results)
    }
}

fn humanize_bytes(bytes: i64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size.abs() >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

#[async_trait]
impl Engine for WallhavenEngine {
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
        matches!(t, ResultType::Images | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), BASE_URL.to_string());
        s.insert("results".to_string(), "JSON".to_string());
        if let Some(k) = &self.api_key {
            s.insert("api_key".to_string(), k.clone());
        }
        s
    }
}
