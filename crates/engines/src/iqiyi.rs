//! iQiyi search engine implementation
//!
//! Chinese video search via the iQiyi
//! JSON API. Extras: thumbnail, duration, published.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// iQiyi video search engine
pub struct IqiyiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IqiyiResponse {
    #[serde(default)]
    data: Option<IqiyiData>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IqiyiData {
    #[serde(default)]
    templates: Vec<IqiyiTemplate>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IqiyiTemplate {
    #[serde(default)]
    albumInfo: IqiyiAlbum,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IqiyiAlbum {
    #[serde(default)]
    videos: Vec<IqiyiVideo>,
    #[serde(default)]
    pageUrl: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    brief: IqiyiBrief,
    #[serde(default)]
    img: String,
    #[serde(default)]
    duration: i64,
    #[serde(default)]
    releaseTime: Option<IqiyiValue>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IqiyiVideo {
    #[serde(default)]
    pageUrl: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    duration: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IqiyiBrief {
    #[serde(default)]
    value: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct IqiyiValue {
    #[serde(default)]
    value: String,
}

impl IqiyiEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "iqiyi".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "iQiyi - Chinese video streaming.".to_string(),
            website: Some("https://www.iqiyi.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create iQiyi HTTP client");

        IqiyiEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://mesh.if.iqiyi.com";
        let url = format!(
            "{}/portal/lw/search/homePageV3?key={}&pageNum={}&pageSize=25",
            base_url,
            urlencoding::encode(&query.query),
            (query.offset / 25) + 1
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
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
        let parsed: IqiyiResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let data = match parsed.data {
            Some(d) => d,
            None => return Ok(results),
        };
        for template in &data.templates {
            let album = &template.albumInfo;
            if !album.videos.is_empty() {
                for video in &album.videos {
                    if results.len() >= query.count {
                        break;
                    }
                    let idx = results.len();
                    results.push(self.make_result(
                        &video.pageUrl,
                        &video.title,
                        &album.brief.value,
                        video.duration,
                        &album.img,
                        album.releaseTime.as_ref().map(|v| &v.value),
                        query,
                        idx,
                    ));
                }
            } else {
                // album-only single video
                if results.len() >= query.count {
                    break;
                }
                let idx = results.len();
                results.push(self.make_result(
                    &album.pageUrl,
                    &album.title,
                    &album.brief.value,
                    album.duration,
                    &album.img,
                    album.releaseTime.as_ref().map(|v| &v.value),
                    query,
                    idx,
                ));
            }
        }
        Ok(results)
    }

    #[allow(clippy::too_many_arguments)]
    fn make_result(
        &self,
        page_url: &str,
        title: &str,
        brief: &str,
        duration_ms: i64,
        img: &str,
        release_time: Option<&String>,
        query: &SearchQuery,
        idx: usize,
    ) -> SearchResult {
        let url = page_url.replace("http://", "https://");
        let duration = format_duration_ms(duration_ms);
        let mut result = SearchResult::new(title.to_string(), url)
            .with_snippet(brief.to_string())
            .with_engine(self.name())
            .with_rank(query.offset + idx + 1)
            .with_score(1.0 - (idx as f64 * 0.05))
            .with_result_type(ResultType::Videos)
            .with_extra("thumbnail", serde_json::json!(img));
        if !duration.is_empty() {
            result = result.with_extra("duration", serde_json::json!(duration));
        }
        if let Some(rt) = release_time {
            if !rt.is_empty() {
                result = result.with_extra("published", serde_json::json!(rt));
            }
        }
        result
    }
}

/// Format milliseconds as H:MM:SS or MM:SS (mirrors Python's timedelta).
fn format_duration_ms(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{}:{:02}:{:02}", hours, minutes, secs)
    } else {
        format!("{:02}:{:02}", minutes, secs)
    }
}

#[async_trait]
impl Engine for IqiyiEngine {
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
        s.insert("base_url".into(), "https://mesh.if.iqiyi.com".into());
        s
    }
}
