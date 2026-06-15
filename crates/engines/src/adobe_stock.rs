//! Adobe Stock search engine implementation (JSON, images)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Adobe Stock media search engine
pub struct AdobeStockEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

// The Adobe Stock search API returns items as either a list (errors) or an
// object keyed by numeric id; we capture both and only walk the object form.
#[derive(Debug, Serialize, Deserialize)]
struct AdobeResponse {
    #[serde(default)]
    items: serde_json::Value,
}

impl AdobeStockEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "adobe_stock".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Adobe Stock - royalty-free photos, vectors, video and audio.".to_string(),
            website: Some("https://stock.adobe.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Adobe Stock HTTP client");

        AdobeStockEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://stock.adobe.com";
        let limit = query.count.min(10).to_string();
        let page = ((query.offset / 10) + 1).to_string();

        let mut q = vec![
            ("k".to_string(), query.query.clone()),
            ("limit".to_string(), limit.clone()),
            ("order".to_string(), "relevance".to_string()),
            ("search_page".to_string(), page.clone()),
            ("search_type".to_string(), "pagination".to_string()),
        ];
        // filters[content_type:*]
        for t in &["photo", "illustration", "zip_vector", "template", "3d", "image"] {
            q.push((
                format!("filters[content_type:{}]", t),
                "1".to_string(),
            ));
        }
        for t in &["video", "audio"] {
            q.push((
                format!("filters[content_type:{}]", t),
                "0".to_string(),
            ));
        }

        let resp = self.client
            .get(format!("{}/de/Ajax/Search", base_url))
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("Accept-Language", "en-US,en;q=0.5")
            .query(&q)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: AdobeResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let items = match &parsed.items {
            serde_json::Value::Object(map) => map,
            _ => return Ok(vec![]), // list form means an error response
        };

        let mut results = Vec::new();
        for (i, (_id, item)) in items.iter().enumerate() {
            let get = |k: &str| item.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let asset_type = get("asset_type").to_lowercase();
            let url = get("content_url");
            let title = get("title");
            if url.is_empty() {
                continue;
            }

            let is_video = asset_type == "video";
            let result_type = if is_video { ResultType::Videos } else { ResultType::Images };

            let mut r = SearchResult::new(title.clone(), url.clone())
                .with_snippet(asset_type.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(result_type);

            if is_video {
                let thumb = get("thumbnail_url");
                let iframe = get("video_small_preview_url");
                r = r
                    .with_extra("thumbnail", serde_json::json!(thumb))
                    .with_extra("iframe_src", serde_json::json!(iframe));
            } else {
                let img_src = get("content_thumb_extra_large_url");
                let thumbnail = get("thumbnail_url");
                let width = item.get("content_original_width").and_then(|v| v.as_i64()).unwrap_or(0);
                let height = item.get("content_original_height").and_then(|v| v.as_i64()).unwrap_or(0);
                r = r
                    .with_extra("img_src", serde_json::json!(img_src))
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("format", serde_json::json!(get("format")))
                    .with_extra("author", serde_json::json!(get("author")))
                    .with_extra("width", serde_json::json!(width))
                    .with_extra("height", serde_json::json!(height));
            }
            results.push(r);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for AdobeStockEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Images | ResultType::Videos | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://stock.adobe.com".to_string());
        s.insert("adobe_order".to_string(), "relevance".to_string());
        s
    }
}
