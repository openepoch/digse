//! Wikimedia Commons media search engine implementation (MediaWiki JSON API).
//! Mirrors the `wikipedia.rs` engine pattern.
//!
//! The upstream engine can be configured for image/video/audio/file search
//! types. This port targets the default `image` search type and returns
//! Images results with media URLs, thumbnails, resolution and format.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Wikimedia Commons media search engine.
pub struct WikicommonsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    search_type: String,
}

const WC_API_URL: &str = "https://commons.wikimedia.org/w/api.php";

// Maps the configured search type to a MediaWiki filetype filter.
fn filetype_filter(search_type: &str) -> &'static str {
    match search_type {
        "video" => "video",
        "audio" => "audio",
        "file" => "multimedia|office|archive|3d",
        _ => "bitmap|drawing", // image
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WcResponse {
    #[serde(default)]
    query: Option<WcQuery>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WcQuery {
    #[serde(default)]
    pages: std::collections::BTreeMap<String, WcPage>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WcPage {
    #[serde(default)]
    title: String,
    #[serde(default)]
    imageinfo: Vec<WcImageInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WcImageInfo {
    #[serde(default)]
    descriptionurl: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    mime: String,
    #[serde(default)]
    thumburl: String,
    #[serde(default)]
    size: i64,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
}

impl WikicommonsEngine {
    pub fn new() -> Self {
        Self::with_search_type("image")
    }

    pub fn with_search_type(search_type: &str) -> Self {
        let metadata = EngineMetadata {
            name: format!("wikicommons_{}", search_type),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: format!(
                "Wikimedia Commons - freely usable media files ({})",
                search_type
            ),
            website: Some("https://commons.wikimedia.org/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Wikimedia Commons HTTP client");
        WikicommonsEngine {
            metadata,
            client,
            search_type: search_type.to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let filetype = filetype_filter(&self.search_type);
        let limit = query.count.min(10);
        let limit_str = limit.to_string();
        let offset_str = (query.offset).to_string();
        let gsrsearch = format!("filetype:{} {}", filetype, query.query);

        let url = format!(
            "{}/?format=json&action=query&prop=info|imageinfo&generator=search&gsrnamespace=6&gsrprop=snippet&gsrlimit={}&gsroffset={}&gsrsearch={}&iiprop=url|size|mime&iiurlheight=180",
            WC_API_URL,
            limit_str,
            offset_str,
            urlencoding::encode(&gsrsearch)
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1 (digse)")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: WcResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        if let Some(query_obj) = parsed.query {
            for (i, (_pageid, page)) in query_obj.pages.into_iter().enumerate() {
                if i >= query.count {
                    break;
                }
                let imageinfo = match page.imageinfo.into_iter().next() {
                    Some(ii) => ii,
                    None => continue,
                };
                let title = page
                    .title
                    .strip_prefix("File:")
                    .unwrap_or(&page.title)
                    .rsplit('.')
                    .last()
                    .unwrap_or(&page.title)
                    .to_string();
                let url = if imageinfo.descriptionurl.is_empty() {
                    imageinfo.url.clone()
                } else {
                    imageinfo.descriptionurl
                };
                if url.is_empty() {
                    continue;
                }
                let resolution = format!("{} x {}", imageinfo.width, imageinfo.height);
                let r = SearchResult::new(title, url)
                    .with_snippet(resolution.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(imageinfo.url))
                    .with_extra("thumbnail", serde_json::json!(imageinfo.thumburl))
                    .with_extra("source", serde_json::json!("wikicommons"))
                    .with_extra("resolution", serde_json::json!(resolution))
                    .with_extra("format", serde_json::json!(imageinfo.mime))
                    .with_extra(
                        "filesize",
                        serde_json::json!(humanize_bytes(imageinfo.size)),
                    )
                    .with_extra("mimetype", serde_json::json!(imageinfo.mime));
                results.push(r);
            }
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
impl Engine for WikicommonsEngine {
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
        s.insert("base_url".to_string(), WC_API_URL.to_string());
        s.insert("wc_search_type".to_string(), self.search_type.clone());
        s.insert("page_size".to_string(), "10".to_string());
        s.insert("results".to_string(), "JSON".to_string());
        s
    }
}
