//! Public Domain Image Archive search engine implementation
//!
//! The reference first scrapes the Algolia API URL out of a JS bundle, then
//! POSTs a JSON search request to it. digse caches the API URL after the first
//! resolution and falls back gracefully on any failure.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Public Domain Image Archive search engine (images, Algolia-style POST API)
pub struct PublicDomainImageArchiveEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    cached_api_url: Mutex<Option<String>>,
}

impl PublicDomainImageArchiveEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "public_domain_image_archive".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Public Domain Image Archive - curated public-domain artwork.".to_string(),
            website: Some("https://pdimagearchive.org".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Public Domain Image Archive HTTP client");
        PublicDomainImageArchiveEngine {
            metadata,
            client,
            cached_api_url: Mutex::new(None),
        }
    }

    /// Resolve (and cache) the Algolia API URL by scraping the site's JS bundle.
    async fn get_api_url(&self) -> Option<String> {
        {
            let cached = self.cached_api_url.lock().ok()?;
            if let Some(u) = cached.as_ref() {
                return Some(u.clone());
            }
        }
        let base_url = "https://pdimagearchive.org";
        // fetch the search page to discover the JS bundle filename
        let resp = self
            .client
            .get(format!("{}/search/?q=", base_url))
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let page_text = resp.text().await.ok()?;
        let start_marker = "/_astro/InfiniteSearch.";
        let end_marker = ".js";
        let s_idx = page_text.find(start_marker)?;
        let after = &page_text[s_idx + start_marker.len()..];
        let e_idx = after.find(end_marker)?;
        let filepart = &after[..e_idx];
        let config_url = format!("{}{}{}{}", base_url, start_marker, filepart, end_marker);

        let resp2 = self
            .client
            .get(&config_url)
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .ok()?;
        if !resp2.status().is_success() {
            return None;
        }
        let js_text = resp2.text().await.ok()?;
        // find a URL of the form "https://.../search-proxy"
        let api_url = extract_quoted_url(&js_text, "https://", "/search-proxy")?;
        if let Ok(mut cache) = self.cached_api_url.lock() {
            *cache = Some(api_url.clone());
        }
        Some(api_url)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_url = match self.get_api_url().await {
            Some(u) => u,
            None => return Ok(vec![]),
        };
        let base_url = "https://pdimagearchive.org";
        let page = (query.offset / 20) as i64; // zero-based page
        let body = serde_json::json!({
            "page": page,
            "query": query.query.as_str(),
            "hitsPerPage": 20,
            "indexName": "prod_all-images",
        });
        let resp = self
            .client
            .post(&api_url)
            .header("User-Agent", "digse/0.0.1")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        let status = resp.status();
        if status == reqwest::StatusCode::FORBIDDEN {
            // clear cached API url (it may have rotated)
            if let Ok(mut cache) = self.cached_api_url.lock() {
                *cache = None;
            }
            return Ok(vec![]);
        }
        if !status.is_success() {
            return Ok(vec![]);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        let root: PdiaResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(_) => return Ok(vec![]),
        };
        // hits live under results[0].hits
        let hits = root
            .results
            .first()
            .map(|r| r.hits.clone())
            .unwrap_or_default();
        let mut results = Vec::new();
        for hit in hits.iter() {
            let thumbnail = hit.thumbnail.clone().unwrap_or_default();
            if thumbnail.is_empty() {
                continue;
            }
            let base_image_url = thumbnail.split('?').next().unwrap_or(&thumbnail).to_string();
            let object_id = hit.object_id.clone().unwrap_or_default();
            let url = format!("{}/images/{}", base_url, object_id);
            let title = format!(
                "{} by {} {}",
                hit.title.clone().unwrap_or_default().trim(),
                hit.artist.clone().unwrap_or_default(),
                hit.display_year.clone().unwrap_or_default()
            )
            .trim()
            .to_string();
            let mut content_parts = Vec::new();
            if let Some(themes) = &hit.themes {
                content_parts.push(format!("Themes: {}", themes));
            }
            if let Some(work) = &hit.encompassing_work {
                content_parts.push(format!("Encompassing work: {}", work));
            }
            results.push(
                SearchResult::new(title, clean_url(&url))
                    .with_snippet(content_parts.join("\n"))
                    .with_engine(self.name())
                    .with_result_type(ResultType::Images)
                    .with_extra("img_src", serde_json::json!(clean_url(&base_image_url)))
                    .with_extra(
                        "thumbnail",
                        serde_json::json!(clean_url(&format!(
                            "{}?fit=max&h=360&w=360",
                            base_image_url
                        ))),
                    )
                    .with_extra("source", serde_json::json!("public_domain_image_archive"))
                    .with_extra("format", serde_json::json!("image")),
            );
        }
        Ok(results)
    }
}

/// Find a double-quoted substring starting with `prefix` and ending with `suffix`.
fn extract_quoted_url(text: &str, prefix: &str, suffix: &str) -> Option<String> {
    let mut search_from = 0;
    while let Some(p_idx) = text[search_from..].find(prefix) {
        let abs = search_from + p_idx;
        // ensure the char before prefix is a quote
        let bytes = text.as_bytes();
        if abs > 0 && (bytes[abs - 1] == b'"' || bytes[abs - 1] == b'\'') {
            if let Some(s_idx) = text[abs..].find(suffix) {
                return Some(format!("{}{}", &text[abs..abs + s_idx + suffix.len()], ""));
            }
        }
        search_from = abs + prefix.len();
    }
    None
}

/// Strip the `ixid` and `s` query params (per reference `_clean_url`).
fn clean_url(url: &str) -> String {
    if let Some(q_idx) = url.find('?') {
        let (path, query) = url.split_at(q_idx);
        let query = &query[1..]; // drop '?'
        let kept: Vec<&str> = query
            .split('&')
            .filter(|kv| {
                let key = kv.split('=').next().unwrap_or("");
                key != "ixid" && key != "s"
            })
            .collect();
        if kept.is_empty() {
            path.to_string()
        } else {
            format!("{}?{}", path, kept.join("&"))
        }
    } else {
        url.to_string()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PdiaResponse {
    #[serde(default)]
    results: Vec<PdiaResultGroup>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PdiaResultGroup {
    #[serde(default)]
    hits: Vec<PdiaHit>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct PdiaHit {
    #[serde(default)]
    object_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default)]
    display_year: Option<String>,
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(default)]
    themes: Option<String>,
    #[serde(default)]
    encompassing_work: Option<String>,
}

#[async_trait]
impl Engine for PublicDomainImageArchiveEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Images | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://pdimagearchive.org".into());
        s.insert("page_size".into(), "20".into());
        s
    }
}
