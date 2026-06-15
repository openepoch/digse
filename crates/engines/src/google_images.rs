//! Google Images search engine implementation
//!
//! The reference hits Google's
//! internal `/search?tbm=isch&...&async=_fmt:json,...` endpoint and parses a
//! JSON document whose root key is `ischj.metadata[]`. Each metadata item has
// `result.referrer_url`, `result.page_title`, `text_in_grid.snippet`,
// `original_image.url`, `thumbnail.url`, etc. Category: images.

use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Google Images search engine (JSON via internal ischj endpoint)
pub struct GoogleImagesEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct IschjRoot {
    #[serde(default)]
    ischj: Ischj,
}

#[derive(Debug, Deserialize, Default)]
struct Ischj {
    #[serde(default)]
    metadata: Vec<MetadataItem>,
}

#[derive(Debug, Deserialize, Default)]
struct MetadataItem {
    #[serde(default)]
    result: ImageResult,
    #[serde(default)]
    text_in_grid: TextInGrid,
    #[serde(default)]
    original_image: OriginalImage,
    #[serde(default)]
    thumbnail: Thumbnail,
    #[serde(default)]
    gsa: Option<Gsa>,
}

#[derive(Debug, Deserialize, Default)]
struct ImageResult {
    #[serde(default)]
    referrer_url: String,
    #[serde(default)]
    page_title: String,
    #[serde(default)]
    site_title: String,
    #[serde(default)]
    freshness_date: Option<String>,
    #[serde(default)]
    iptc: Option<Iptc>,
}

#[derive(Debug, Deserialize, Default)]
struct Iptc {
    #[serde(default)]
    creator: Vec<String>,
    #[serde(default)]
    copyright_notice: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TextInGrid {
    #[serde(default)]
    snippet: String,
}

#[derive(Debug, Deserialize, Default)]
struct OriginalImage {
    #[serde(default)]
    url: String,
    #[serde(default)]
    width: i64,
    #[serde(default)]
    height: i64,
}

#[derive(Debug, Deserialize, Default)]
struct Thumbnail {
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize, Default)]
struct Gsa {
    #[serde(default)]
    file_size: Option<String>,
}

impl GoogleImagesEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "google_images".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Google Images - image search.".to_string(),
            website: Some("https://images.google.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Google Images HTTP client");

        GoogleImagesEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        // ref: ijn is zero-based page index; uses NSTN Android UA.
        let ijn = query.offset;
        let async_str = format!("_fmt:json,p:1,ijn:{}", ijn);
        let url = "https://www.google.com/search";

        let response = self
            .client
            .get(url)
            .header(
                "User-Agent",
                "NSTN/3.60.474802233.release Dalvik/2.1.0 (Linux; U; Android 12; US) gzip",
            )
            .header("Accept", "*/*")
            .header("Accept-Language", "en-US,en;q=0.9")
            .query(&[
                ("q", query.query.as_str()),
                ("tbm", "isch"),
                ("hl", "en-US"),
                ("ie", "utf8"),
                ("oe", "utf8"),
                ("asearch", "isch"),
            ])
            .query(&[("async", async_str.as_str())])
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

        // ref: json_start = resp.text.find('{"ischj":')
        let json_str = match text.find("{\"ischj\":") {
            Some(pos) => &text[pos..],
            None => return Ok(vec![]),
        };

        let parsed: IschjRoot = match serde_json::from_str(json_str) {
            Ok(p) => p,
            Err(_) => {
                // Try to recover a balanced prefix if the document has trailing
                // non-JSON content.
                let trimmed = balance_json(json_str);
                match serde_json::from_str(&trimmed) {
                    Ok(p) => p,
                    Err(_) => return Ok(vec![]),
                }
            }
        };

        let mut results = Vec::new();
        for (i, item) in parsed.ischj.metadata.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let url = item.result.referrer_url.clone();
            let title = item.result.page_title.clone();
            let img_src = item.original_image.url.clone();
            if url.is_empty() && img_src.is_empty() {
                continue;
            }
            let page_url = if url.is_empty() {
                img_src.clone()
            } else {
                url
            };
            let title = if title.is_empty() {
                "Google Image".to_string()
            } else {
                title
            };
            let resolution = format!(
                "{} x {}",
                item.original_image.width, item.original_image.height
            );
            let snippet = item.text_in_grid.snippet.clone();

            let result = SearchResult::new(title, page_url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(img_src))
                .with_extra("thumbnail", serde_json::json!(item.thumbnail.url))
                .with_extra("resolution", serde_json::json!(resolution))
                .with_extra("source", serde_json::json!(item.result.site_title))
                .with_extra("author", serde_json::json!(item.result.iptc.as_ref().map(|i| i.creator.clone()).unwrap_or_default()));
            results.push(result);
        }

        Ok(results)
    }
}

// Balance braces on the `{"ischj":...}` prefix when the document trails off
// with non-JSON content. Walks from the first `{` and stops once depth returns
// to zero, respecting string literals.
fn balance_json(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut depth: i64 = 0;
    let mut in_string = false;
    let mut escape = false;
    let mut end = bytes.len();
    for (idx, &b) in bytes.iter().enumerate() {
        let c = b as char;
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
                end = idx + 1;
                break;
            }
        }
    }
    text[..end].to_string()
}

#[async_trait]
impl Engine for GoogleImagesEngine {
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
        settings.insert("base_url".to_string(), "https://images.google.com".to_string());
        settings.insert("search_endpoint".to_string(), "/search".to_string());
        settings.insert("tbm".to_string(), "isch".to_string());
        settings
    }
}
