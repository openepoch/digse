//! GMX search engine implementation
//!
//! GMX is a Germany-based general web search.
//! The reference does a two-step fetch: first a GET to
//! `{base}/web/result?q=...&page=N` (HTML) to extract a page hash `h`, then a
//! GET to `{base}/desk?lang=en&q=...&page=N&h=...&t=...` returning JSON with a
//! `results.hits` array. Category: general.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// GMX general web search engine (JSON desk API)
pub struct GmxEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct GmxResultBody {
    #[serde(default)]
    results: GmxResults,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GmxResults {
    #[serde(default)]
    hits: Vec<GmxHit>,
    #[serde(default)]
    rs: Vec<GmxSuggestion>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GmxHit {
    #[serde(default)]
    u: String,
    #[serde(default)]
    t: String,
    #[serde(default)]
    s: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GmxSuggestion {
    #[serde(default)]
    t: String,
}

impl GmxEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "gmx".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "GMX - Germany-based general web search.".to_string(),
            website: Some("https://search.gmx.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create GMX HTTP client");

        GmxEngine { metadata, client }
    }

    // ref _get_page_hash: GET {base}/web/result?q=...&page=N, then extract the
    // substring between "&h=" and "&t=".
    async fn get_page_hash(&self, query: &str, page: usize) -> Option<String> {
        let url = format!(
            "https://search.gmx.com/web/result?q={}&page={}",
            urlencoding::encode(query),
            page
        );
        let resp = self
            .client
            .get(&url)
            .header(
                "User-Agent",
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .header("Accept", "text/html,application/xhtml+xml")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", "https://search.gmx.com")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        extract_between(&text, "&h=", "&t=")
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let page = query.offset + 1;
        // ref: now = int(time.time() / 10) -- the t param
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
            / 10;
        let now_str = now_secs.to_string();

        let page_hash = match self.get_page_hash(&query.query, page).await {
            Some(h) => h,
            None => return Ok(vec![]),
        };

        let mut args: Vec<(&str, String)> = vec![
            ("lang", "en".to_string()),
            ("q", query.query.clone()),
            ("page", page.to_string()),
            ("h", page_hash),
            ("t", now_str),
        ];
        if query.safe_search {
            args.push(("family", "true".to_string()));
        }

        let response = self
            .client
            .get("https://search.gmx.com/desk")
            .header(
                "User-Agent",
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0 Safari/537.36",
            )
            .header("Accept", "application/json")
            .header("Accept-Language", "en-US,en;q=0.9")
            .header("Referer", "https://search.gmx.com")
            .query(&args)
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

        let parsed: GmxResultBody = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, hit) in parsed.results.hits.iter().enumerate() {
            if i >= query.count {
                break;
            }
            let title = if hit.t.is_empty() {
                "GMX result".to_string()
            } else {
                hit.t.clone()
            };
            let result = SearchResult::new(title, hit.u.clone())
                .with_snippet(hit.s.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web)
                .with_extra("source", serde_json::json!("gmx"));
            results.push(result);
        }

        Ok(results)
    }
}

// Extract the substring of `text` between `start_marker` and `end_marker`,
// matching the ref `extr(resp.text, "&h=", "&t=")` helper.
fn extract_between(text: &str, start_marker: &str, end_marker: &str) -> Option<String> {
    let s = text.find(start_marker)? + start_marker.len();
    let rest = &text[s..];
    let e = rest.find(end_marker)?;
    Some(rest[..e].to_string())
}

#[async_trait]
impl Engine for GmxEngine {
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
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert(
            "base_url".to_string(),
            "https://search.gmx.com".to_string(),
        );
        settings.insert("result_endpoint".to_string(), "/desk".to_string());
        settings
    }
}
