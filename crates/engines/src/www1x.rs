//! 1x (1x.com) image search engine implementation (HTML via XML backend).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// 1x curated photography search engine.
pub struct Www1xEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const BASE_URL: &str = "https://1x.com";
const GALLERY_URL: &str = "https://gallery.1x.com/";

impl Www1xEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "www1x".to_string(),
            category: EngineCategory::Images,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "1x - curated photography gallery search.".to_string(),
            website: Some("https://1x.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create 1x HTTP client");
        Www1xEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "{}/backend/search.php?q={}",
            BASE_URL,
            urlencoding::encode(&query.query)
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse(&text, query))
    }

    /// The upstream response is an XML envelope whose `<data>` element holds an
    /// HTML fragment of `<a>` links. We extract the `<a>` links and their
    /// nested `<img src>`.
    fn parse(&self, body: &str, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        // Treat the whole body as HTML; scraper tolerates the mixed XML/HTML.
        let document = Html::parse_document(body);
        let mut results = Vec::new();

        let a_sel = match Selector::parse("a") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let img_sel = match Selector::parse("img") {
            Ok(s) => s,
            Err(_) => return results,
        };

        for (i, link) in document.select(&a_sel).enumerate() {
            if i >= query.count {
                break;
            }
            let href = link.value().attr("href").unwrap_or("").to_string();
            if href.is_empty() {
                continue;
            }
            let url = join(BASE_URL, &href);
            let title = link.text().collect::<String>().trim().to_string();
            let img_src = link
                .select(&img_sel)
                .next()
                .and_then(|im| im.value().attr("src").map(|s| s.to_string()))
                .unwrap_or_default();
            // upstream replaces the base_url prefix and joins with gallery_url
            let thumb = if !img_src.is_empty() {
                let stripped = img_src.replace(BASE_URL, "");
                join(GALLERY_URL, &stripped)
            } else {
                String::new()
            };

            let r = SearchResult::new(title, url)
                .with_snippet(String::new())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Images)
                .with_extra("img_src", serde_json::json!(thumb))
                .with_extra("thumbnail", serde_json::json!(thumb))
                .with_extra("source", serde_json::json!("1x"));
            results.push(r);
        }
        results
    }
}

/// Minimal URL joiner: if `href` is absolute (starts with http), keep it;
/// otherwise concatenate base + href (normalising a single separating slash).
fn join(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with('/') {
        let scheme_end = base.find("://").map(|i| i + 3).unwrap_or(0);
        let origin_end = base[scheme_end..].find('/').map(|i| i + scheme_end).unwrap_or(base.len());
        return format!("{}{}", &base[..origin_end], href);
    }
    if base.ends_with('/') {
        format!("{}{}", base, href)
    } else {
        format!("{}/{}", base, href)
    }
}

#[async_trait]
impl Engine for Www1xEngine {
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
        s.insert("base_url".to_string(), BASE_URL.to_string());
        s.insert("gallery_url".to_string(), GALLERY_URL.to_string());
        s.insert("results".to_string(), "HTML".to_string());
        s
    }
}
