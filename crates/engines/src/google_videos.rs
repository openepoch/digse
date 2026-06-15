//! Google Videos search engine implementation
//!
//! Scrapes Google's `tbm=vid`
//! video search results. Extras: thumbnail, duration, iframe_src, author.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};
use scraper::{Html, Selector};

/// Google Videos search engine
pub struct GoogleVideosEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl GoogleVideosEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "google_videos".to_string(),
            category: EngineCategory::Videos,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Google Videos - video search.".to_string(),
            website: Some("https://www.google.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Google Videos HTTP client");

        GoogleVideosEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let start = query.offset;
        let url = format!(
            "https://www.google.com/search?q={}&tbm=vid&start={}",
            urlencoding::encode(&query.query),
            start
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0 (digse/0.0.1)")
            .header("Accept", "text/html,application/xhtml+xml")
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

        Ok(self.parse(&text, query))
    }

    fn parse(&self, html_text: &str, query: &SearchQuery) -> Vec<SearchResult> {
        let doc = Html::parse_document(html_text);
        let mut results = Vec::new();

        // Google frequently wraps results in div.MjjYud
        let result_sel = match Selector::parse("div.MjjYud") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let heading_sel = Selector::parse("h3.LC20lb, div[role='heading']").unwrap();
        let a_sel = Selector::parse("a[jsname='UWckNb'], a[href*='/url?q=']").unwrap();
        let content_sel = Selector::parse("div.ITZIwc").unwrap();
        let pub_info_sel = Selector::parse("div.gqF9jc, div.WRu9Cd").unwrap();
        let img_sel = Selector::parse("img").unwrap();
        let duration_sel = Selector::parse("span.k1U36b").unwrap();
        let vid_div_sel = Selector::parse("div[jscontroller='rTuANe']").unwrap();

        for (i, el) in doc.select(&result_sel).enumerate() {
            let title = el
                .select(&heading_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }

            let a = match el.select(&a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let raw_url = a.value().attr("href").unwrap_or("").to_string();
            let url = if raw_url.starts_with("/url?q=") {
                let after = &raw_url[7..];
                let end = after.find("&sa=U").unwrap_or(after.len());
                urlencoding::decode(&after[..end])
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| after[..end].to_string())
            } else {
                raw_url
            };
            if url.is_empty() || !url.starts_with("http") {
                continue;
            }

            let content = el
                .select(&content_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let author = el
                .select(&pub_info_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let duration = el
                .select(&duration_sel)
                .next()
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = el
                .select(&img_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .map(|s| {
                    if s.starts_with("data:image") {
                        String::new()
                    } else {
                        s.to_string()
                    }
                })
                .unwrap_or_default();
            let video_id = el
                .select(&vid_div_sel)
                .next()
                .and_then(|d| d.value().attr("data-vid"))
                .unwrap_or("")
                .to_string();

            // Fallback thumbnail from youtube
            let thumbnail = if thumbnail.is_empty() && !video_id.is_empty() {
                format!("https://img.youtube.com/vi/{}/hqdefault.jpg", video_id)
            } else {
                thumbnail
            };
            // Embed URL
            let iframe_src = if !video_id.is_empty() {
                Some(format!("https://www.youtube.com/embed/{}", video_id))
            } else {
                None
            };

            let mut result = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Videos);
            if !thumbnail.is_empty() {
                result = result.with_extra("thumbnail", serde_json::json!(thumbnail));
            }
            if !duration.is_empty() {
                result = result.with_extra("duration", serde_json::json!(duration));
            }
            if !author.is_empty() {
                result = result.with_extra("author", serde_json::json!(author));
            }
            if let Some(src) = iframe_src {
                result = result.with_extra("iframe_src", serde_json::json!(src));
            }
            results.push(result);
        }

        results
    }
}

#[async_trait]
impl Engine for GoogleVideosEngine {
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
        s.insert("base_url".into(), "https://www.google.com".into());
        s
    }
}
