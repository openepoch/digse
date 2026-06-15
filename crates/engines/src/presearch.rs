//! Presearch search engine implementation
//!
//!
//! Presearch exposes a JSON results endpoint. The reference implementation first
//! fetches a `requestId` from an HTML page and then queries the JSON endpoint.
//! For digse we perform both steps over HTTP; any failure (captcha, missing id,
//! non-200, parse error) results in an empty result list.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Presearch search engine (general/web, JSON results)
pub struct PresearchEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl PresearchEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "presearch".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Presearch - decentralized web search.".to_string(),
            website: Some("https://presearch.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Presearch HTTP client");
        PresearchEngine { metadata, client }
    }

    /// Fetch the `window.searchId` from the HTML of the search page.
    async fn get_request_id(&self, query: &SearchQuery) -> Option<String> {
        let page = (query.offset / 10) + 1;
        let page_str = page.to_string();
        let url = "https://presearch.com/search";
        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header(
                "Cookie",
                "b=1; presearch_session=; use_local_search_results=false; use_safe_search=true",
            )
            .query(&[
                ("q", query.query.as_str()),
                ("page", page_str.as_str()),
            ])
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        for line in text.lines() {
            if let Some(idx) = line.find("window.searchId = ") {
                let rest = &line[idx + "window.searchId = ".len()..];
                // strip trailing quote characters / semicolons
                let cleaned: String = rest
                    .trim()
                    .trim_end_matches(';')
                    .trim_matches('"')
                    .to_string();
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }
        None
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let request_id = match self.get_request_id(query).await {
            Some(id) => id,
            None => return Ok(vec![]),
        };
        let url = format!("https://presearch.com/results?id={}", request_id);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
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
        let root: PresearchRoot = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };
        let mut results = Vec::new();
        let json_results = root.results.unwrap_or_default();

        // Top stories (compact)
        for item in json_results
            .special_sections
            .as_ref()
            .and_then(|s| s.top_stories_compact.as_ref())
            .and_then(|t| t.data.as_ref())
            .map(|d| d.iter())
            .into_iter()
            .flatten()
        {
            let title = strip_html(&item.title);
            let url = item.link.clone().unwrap_or_default();
            if url.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(title, url)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web),
            );
        }

        // Standard web results
        for item in json_results.standard_results.iter().flatten() {
            let title = strip_html(&item.title);
            let url = item.link.clone().unwrap_or_default();
            if url.is_empty() {
                continue;
            }
            let content = strip_html(item.description.as_deref().unwrap_or(""));
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web),
            );
        }

        // Apply rank/score
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
}

fn strip_html(s: &str) -> String {
    // minimal HTML tag stripper
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

// ---- response model --------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Default)]
struct PresearchRoot {
    #[serde(default)]
    results: Option<PresearchResults>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PresearchResults {
    #[serde(default)]
    special_sections: Option<PresearchSpecialSections>,
    #[serde(default)]
    standard_results: Option<Vec<PresearchStandardItem>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PresearchSpecialSections {
    #[serde(default)]
    #[serde(rename = "topStoriesCompact")]
    top_stories_compact: Option<PresearchTopStories>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PresearchTopStories {
    #[serde(default)]
    data: Option<Vec<PresearchTopStory>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PresearchTopStory {
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PresearchStandardItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[async_trait]
impl Engine for PresearchEngine {
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
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://presearch.com".into());
        s.insert("search_type".into(), "search".into());
        s
    }
}
