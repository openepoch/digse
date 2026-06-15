//! YouTube (no API) search engine implementation
//!
//! Scrapes the results page,
//! which embeds a `ytInitialData = {...};</script>` JSON blob, and walks the
//! renderer tree to extract video results. Navigation is dynamic, so the JSON
//! is handled via serde_json::Value.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// YouTube video search engine (no API key)
pub struct YouTubeNoApiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

/// Extract the substring between `start` and `end` after the first `start`.
fn extr<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = text.find(start)? + start.len();
    let rest = &text[i..];
    let j = rest.find(end)?;
    Some(&rest[..j])
}

/// Concatenate text from a YouTube renderer field: `runs[].text` joined, else
/// `simpleText`.
fn yt_text(element: &serde_json::Value) -> String {
    if let Some(runs) = element.get("runs").and_then(|r| r.as_array()) {
        runs.iter()
            .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("")
    } else {
        element
            .get("simpleText")
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string()
    }
}

impl YouTubeNoApiEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "youtube".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "YouTube video search (no API key).".to_string(),
            website: Some("https://www.youtube.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create YouTube HTTP client");

        YouTubeNoApiEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let encoded = urlencoding::encode(&query.query);
        let pageno = (query.offset / query.count.max(1)) + 1;
        let url = format!(
            "https://www.youtube.com/results?search_query={}&page={}",
            encoded, pageno
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.0.1)")
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Cookie", "CONSENT=YES+")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let text = match response.text().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        let blob = match extr(&text, "ytInitialData = ", ";</script>") {
            Some(s) => s,
            None => return Ok(vec![]),
        };
        let json: serde_json::Value = match serde_json::from_str(blob) {
            Ok(v) => v,
            Err(_) => return Ok(vec![]),
        };

        let sections = json
            .get("contents")
            .and_then(|c| c.get("twoColumnSearchResultsRenderer"))
            .and_then(|t| t.get("primaryContents"))
            .and_then(|p| p.get("sectionListRenderer"))
            .and_then(|s| s.get("contents"))
            .and_then(|c| c.as_array());

        let mut results = Vec::new();
        if let Some(sections) = sections {
            for section in sections {
                let contents = section
                    .get("itemSectionRenderer")
                    .and_then(|i| i.get("contents"))
                    .and_then(|c| c.as_array());
                if let Some(contents) = contents {
                    for video_container in contents {
                        let video = match video_container.get("videoRenderer") {
                            Some(v) => v,
                            None => continue,
                        };
                        if results.len() >= query.count {
                            break;
                        }
                        let videoid = match video.get("videoId").and_then(|v| v.as_str()) {
                            Some(s) => s.to_string(),
                            None => continue,
                        };
                        let page_url = format!("https://www.youtube.com/watch?v={}", videoid);
                        let title = yt_text(video.get("title").unwrap_or(&serde_json::Value::Null));
                        if title.is_empty() {
                            continue;
                        }
                        let content =
                            yt_text(video.get("descriptionSnippet").unwrap_or(&serde_json::Value::Null));
                        let author = yt_text(video.get("ownerText").unwrap_or(&serde_json::Value::Null));
                        let length = yt_text(video.get("lengthText").unwrap_or(&serde_json::Value::Null));
                        let thumbnail = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", videoid);

                        let result = SearchResult::new(title, page_url)
                            .with_snippet(content)
                            .with_engine(self.name())
                            .with_rank(query.offset + results.len() + 1)
                            .with_score(1.0 - (results.len() as f64 * 0.05))
                            .with_result_type(ResultType::Videos)
                            .with_extra("author", serde_json::json!(author))
                            .with_extra("duration", serde_json::json!(length))
                            .with_extra("thumbnail", serde_json::json!(thumbnail))
                            .with_extra(
                                "iframe_src",
                                serde_json::json!(format!(
                                    "https://www.youtube-nocookie.com/embed/{}",
                                    videoid
                                )),
                            )
                            .with_extra("source", serde_json::json!("youtube"));
                        results.push(result);
                    }
                }
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for YouTubeNoApiEngine {
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
        matches!(t, ResultType::Videos | ResultType::Music | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "search_url".to_string(),
            "https://www.youtube.com/results".to_string(),
        );
        s
    }
}
