//! Flickr (no API) search engine implementation
//!
//! HTML scrape of
//! `https://www.flickr.com/search?text=...&page=N`. The reference engine
//! extracts a `modelExport` JSON blob from the page JS and indexes into its
//! `legend` to recover photo metadata. This Rust port scrapes the page and
//! attempts the modelExport extraction; on any parse miss it returns empty.
//! Category: images.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Flickr image search engine (no API key; HTML scrape)
pub struct FlickrNoApiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl FlickrNoApiEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "flickr_noapi".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Flickr - image search (no API key, HTML scrape).".to_string(),
            website: Some("https://www.flickr.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Flickr (noapi) HTTP client");

        FlickrNoApiEngine { metadata, client }
    }

    fn build_flickr_url(user_id: &str, photo_id: &str) -> String {
        format!("https://www.flickr.com/photos/{}/{}", user_id, photo_id)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = query.offset + 1;
        let page_str = page.to_string();
        let url = "https://www.flickr.com/search";

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html,application/xhtml+xml")
            .query(&[
                ("text", query.query.as_str()),
                ("page", page_str.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let html = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        // Extract the modelExport JSON object without regex. The ref pattern is:
        //   modelExport: { ... },
        // We find the start, balance braces, then parse.
        let model_export_str = match extract_model_export(&html) {
            Some(s) => s,
            None => return Ok(vec![]),
        };

        let model: serde_json::Value = match serde_json::from_str(&model_export_str) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        // The legend is a deeply nested structure of photo indices. Walking it
        // exactly as the Python ref does requires recursive indexing across many
        // heterogeneous levels. We instead scan the parsed model for any
        // photo-bearing object and harvest fields defensively.
        let photos = collect_photos(&model);
        let mut results = Vec::new();
        for (i, photo) in photos.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let img_src = photo.img_src.clone();
            if img_src.is_empty() {
                continue;
            }
            let thumbnail = if !photo.thumbnail_src.is_empty() {
                photo.thumbnail_src.clone()
            } else {
                img_src.clone()
            };
            let url = if !photo.owner_nsid.is_empty() && !photo.id.is_empty() {
                Self::build_flickr_url(&photo.owner_nsid, &photo.id)
            } else {
                img_src.clone()
            };
            let title = if photo.title.is_empty() {
                "Flickr photo".to_string()
            } else {
                photo.title.clone()
            };

            let result = SearchResult::new(title, url)
                .with_snippet(photo.content.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("author", serde_json::json!(photo.author))
                .with_extra("source", serde_json::json!("flickr"))
                .with_extra("resolution", serde_json::json!(photo.resolution));
            results.push(result);
        }

        Ok(results)
    }
}

#[derive(Debug, Default, Clone)]
struct FlickrPhotoInfo {
    id: String,
    owner_nsid: String,
    author: String,
    title: String,
    content: String,
    img_src: String,
    thumbnail_src: String,
    resolution: String,
}

// Extract the `modelExport` object literal from the page HTML by locating the
// marker `modelExport:` and brace-balancing forward, respecting string literals
// so that braces inside strings do not affect nesting. Returns None if the
// marker is absent or no balanced object is found.
fn extract_model_export(text: &str) -> Option<String> {
    let marker = "modelExport:";
    let mpos = text.find(marker)?;
    let after_marker = &text[mpos + marker.len()..];
    // skip whitespace
    let bytes = after_marker.as_bytes();
    let mut idx = 0;
    while idx < bytes.len() && (bytes[idx] as char).is_whitespace() {
        idx += 1;
    }
    if idx >= bytes.len() || bytes[idx] != b'{' {
        return None;
    }
    let start = idx;
    let mut depth: i64 = 0;
    let mut in_string = false;
    let mut escape = false;
    while idx < bytes.len() {
        let c = bytes[idx] as char;
        if in_string {
            if escape {
                escape = false;
            } else if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                let obj = &after_marker[start..=idx];
                return Some(obj.to_string());
            }
        }
        idx += 1;
    }
    None
}

// Recursively walk the modelExport JSON collecting any object that looks like a
// photo entry (has a `sizes.data` map with at least one size carrying a `url`).
fn collect_photos(model: &serde_json::Value) -> Vec<FlickrPhotoInfo> {
    let mut out = Vec::new();
    walk(model, &mut out);
    out
}

fn walk(v: &serde_json::Value, out: &mut Vec<FlickrPhotoInfo>) {
    match v {
        serde_json::Value::Object(_) => {
            if let Some(info) = try_photo(v) {
                out.push(info);
            }
            if let serde_json::Value::Object(map) = v {
                for (_, child) in map {
                    walk(child, out);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for child in arr {
                walk(child, out);
            }
        }
        _ => {}
    }
}

fn try_photo(v: &serde_json::Value) -> Option<FlickrPhotoInfo> {
    let sizes = v.get("sizes").and_then(|s| s.get("data"))?;
    let map = sizes.as_object()?;
    if map.is_empty() {
        return None;
    }
    // image_sizes priority order from ref
    let priority = ["o", "k", "h", "b", "c", "z", "m", "n", "t", "q", "s"];
    let mut img_src = String::new();
    let mut resolution = String::new();
    for key in priority.iter() {
        if let Some(size_data) = map.get(*key).and_then(|d| d.get("data")) {
            if let Some(u) = size_data.get("url").and_then(|u| u.as_str()) {
                img_src = u.to_string();
                let w = size_data.get("width").and_then(|w| w.as_i64()).unwrap_or(0);
                let h = size_data
                    .get("height")
                    .and_then(|h| h.as_i64())
                    .unwrap_or(0);
                resolution = format!("{} x {}", w, h);
                break;
            }
        }
    }
    if img_src.is_empty() {
        return None;
    }
    let thumbnail_src = map
        .get("n")
        .or_else(|| map.get("z"))
        .and_then(|d| d.get("data"))
        .and_then(|d| d.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or(&img_src)
        .to_string();

    let id = v
        .get("id")
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .to_string();
    let owner_nsid = v
        .get("ownerNsid")
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .to_string();
    let author = v
        .get("realname")
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .to_string();
    let title = v
        .get("title")
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .to_string();
    let content = v
        .get("description")
        .and_then(|i| i.as_str())
        .unwrap_or("")
        .to_string();

    Some(FlickrPhotoInfo {
        id,
        owner_nsid,
        author,
        title,
        content,
        img_src,
        thumbnail_src,
        resolution,
    })
}

#[async_trait]
impl Engine for FlickrNoApiEngine {
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
        settings.insert("base_url".to_string(), "https://www.flickr.com".to_string());
        settings.insert("requires_api_key".to_string(), "false".to_string());
        settings
    }
}
