//! Pixiv search engine implementation (images; JSON)
//!
//! Queries the Pixiv AJAX search API.
//! Image URLs use the `i.pximg.net` host which requires a `Referer` header from
//! pixiv.net to load; the reference remaps to a configurable image proxy.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Pixiv image search engine
pub struct PixivEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    /// Optional list of pixiv image-proxy URLs (comma-separated via env var).
    proxies: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PixivResponse {
    #[serde(default)]
    body: PixivBody,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PixivBody {
    #[serde(default)]
    illust: PixivIllustContainer,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PixivIllustContainer {
    #[serde(default)]
    data: Vec<PixivIllust>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PixivIllust {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    alt: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    userName: String,
    #[serde(default)]
    userId: String,
}

impl PixivEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "pixiv".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Pixiv - Japanese illustration community.".to_string(),
            website: Some("https://www.pixiv.net/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Pixiv HTTP client");

        let proxies: Vec<String> = std::env::var("PIXIV_IMAGE_PROXIES")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        PixivEngine {
            metadata,
            client,
            proxies,
        }
    }

    /// Remap an `i.pximg.net` thumbnail URL to a proxy URL when a proxy is
    /// configured, and produce the higher-resolution master URL.
    fn proxy_urls(&self, image_url: &str) -> (String, String) {
        let proxy_root = if self.proxies.is_empty() {
            "https://i.pximg.net".to_string()
        } else {
            self.proxies[0].clone()
        };
        let thumb = image_url.replace("https://i.pximg.net", &proxy_root);
        let full = thumb
            .replace("/c/250x250_80_a2/", "/")
            .replace("_square1200.jpg", "_master1200.jpg")
            .replace("custom-thumb", "img-master")
            .replace("_custom1200.jpg", "_master1200.jpg");
        (thumb, full)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.pixiv.net/ajax/search/illustrations";
        let page = ((query.offset / 10) + 1).to_string();

        let resp = self
            .client
            .get(format!("{}/{}", base_url, query.query))
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .header("Referer", "https://www.pixiv.net/")
            .query(&[
                ("word", query.query.as_str()),
                ("order", "date_d"),
                ("mode", "all"),
                ("p", page.as_str()),
                ("s_mode", "s_tag_full"),
                ("type", "illust_and_ugoira"),
                ("lang", "en"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: PixivResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.body.illust.data.iter().enumerate() {
            if item.url.is_empty() {
                continue;
            }
            let (thumbnail, full) = self.proxy_urls(&item.url);
            let title = if item.title.is_empty() {
                format!("Illustration {}", item.id)
            } else {
                item.title.clone()
            };
            let author = format!("{} (ID: {})", item.userName, item.userId);

            results.push(
                SearchResult::new(&title, &full)
                    .with_snippet(item.alt.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(full))
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("source", serde_json::json!("pixiv.net"))
                    .with_extra("author", serde_json::json!(author)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for PixivEngine {
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
        s.insert(
            "base_url".to_string(),
            "https://www.pixiv.net/ajax/search/illustrations".to_string(),
        );
        if let Some(p) = self.proxies.first() {
            s.insert("image_proxy".to_string(), p.clone());
        }
        s
    }
}
