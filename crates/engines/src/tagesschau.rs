//! Tagesschau search engine implementation
//!
//! Queries the ARD
//! Tagesschau `/api2u/search` JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Tagesschau (ARD) news search engine
pub struct TagesschauEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://www.tagesschau.de";
const RESULTS_PER_PAGE: usize = 10;

#[derive(Debug, Serialize, Deserialize)]
struct TagesschauResponse {
    #[serde(default)]
    #[serde(rename = "searchResults")]
    search_results: Vec<TagesschauItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TagesschauItem {
    #[serde(default)]
    #[serde(rename = "type")]
    item_type: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    #[serde(rename = "shareURL")]
    share_url: String,
    #[serde(default)]
    detailsweb: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    #[serde(rename = "firstSentence")]
    first_sentence: String,
    #[serde(default)]
    #[serde(rename = "teaserImage")]
    teaser_image: TagesschauTeaser,
    #[serde(default)]
    streams: TagesschauStreams,
    #[serde(default)]
    #[serde(rename = "sophoraId")]
    sophora_id: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TagesschauTeaser {
    #[serde(default)]
    #[serde(rename = "imageVariants")]
    image_variants: TagesschauImageVariants,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TagesschauImageVariants {
    #[serde(default)]
    #[serde(rename = "16x9-256")]
    variant_16x9_256: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct TagesschauStreams {
    #[serde(default)]
    h264s: String,
    #[serde(default)]
    h264m: String,
    #[serde(default)]
    h264l: String,
    #[serde(default)]
    h264xl: String,
}

impl TagesschauEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "tagesschau".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Tagesschau (ARD) - German news search.".to_string(),
            website: Some("https://tagesschau.de".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Tagesschau HTTP client");

        TagesschauEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = ((query.offset / RESULTS_PER_PAGE)).to_string();
        let page_size = RESULTS_PER_PAGE.to_string();

        let resp = self
            .client
            .get(format!("{}/api2u/search", BASE_URL))
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("searchText", query.query.as_str()),
                ("pageSize", page_size.as_str()),
                ("resultPage", page.as_str()),
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

        let parsed: TagesschauResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.search_results.iter().enumerate() {
            match item.item_type.as_str() {
                "story" | "webview" => {
                    let url = if !item.share_url.is_empty() {
                        item.share_url.clone()
                    } else {
                        item.detailsweb.clone()
                    };
                    if url.is_empty() {
                        continue;
                    }
                    let thumbnail = item.teaser_image.image_variants.variant_16x9_256.clone();
                    let mut result = SearchResult::new(item.title.clone(), url)
                        .with_snippet(item.first_sentence.clone())
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::News);
                    if !thumbnail.is_empty() {
                        result =
                            result.with_extra("thumbnail", serde_json::json!(thumbnail));
                    }
                    if !item.date.is_empty() {
                        result = result.with_extra("published", serde_json::json!(item.date));
                    }
                    result = result.with_extra("source", serde_json::json!("tagesschau"));
                    results.push(result);
                }
                "video" => {
                    let video_url = if !item.streams.h264s.is_empty() {
                        item.streams.h264s.clone()
                    } else if !item.streams.h264m.is_empty() {
                        item.streams.h264m.clone()
                    } else if !item.streams.h264l.is_empty() {
                        item.streams.h264l.clone()
                    } else {
                        item.streams.h264xl.clone()
                    };
                    let url = if !video_url.is_empty() {
                        video_url.clone()
                    } else if !item.sophora_id.is_empty() {
                        format!("{}/multimedia/video/{}.html", BASE_URL, item.sophora_id)
                    } else {
                        continue;
                    };
                    let thumbnail = item.teaser_image.image_variants.variant_16x9_256.clone();
                    let mut result = SearchResult::new(item.title.clone(), url)
                        .with_snippet(item.first_sentence.clone())
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::News);
                    if !thumbnail.is_empty() {
                        result =
                            result.with_extra("thumbnail", serde_json::json!(thumbnail));
                    }
                    if !item.date.is_empty() {
                        result = result.with_extra("published", serde_json::json!(item.date));
                    }
                    if !video_url.is_empty() {
                        result =
                            result.with_extra("iframe_src", serde_json::json!(video_url));
                    }
                    result = result.with_extra("source", serde_json::json!("tagesschau"));
                    results.push(result);
                }
                _ => continue,
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for TagesschauEngine {
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
        matches!(t, ResultType::News | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("api_endpoint".into(), "/api2u/search".into());
        s.insert("language".into(), "de".into());
        s.insert("results".into(), "JSON".into());
        s
    }
}
