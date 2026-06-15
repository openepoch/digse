//! Sogou search engine implementation
//!
//! Sogou is a Chinese web
//! search engine; the reference scrapes the HTML results page. Sogou may
//! redirect to an anti-spider page (HTTP 302 to `/antispider`) which we treat
//! as a graceful empty result list.

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Sogou search engine (general/web, Chinese, HTML scrape)
pub struct SogouEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl SogouEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "sogou".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Sogou - Chinese web search engine.".to_string(),
            website: Some("https://www.sogou.com/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to create Sogou HTTP client");
        SogouEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://www.sogou.com";
        let page = (query.offset / 10) + 1;
        let page_str = page.to_string();
        let resp = self
            .client
            .get(format!("{}/web", base_url))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("query", query.query.as_str()),
                ("page", page_str.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        let status = resp.status().as_u16();
        // anti-spider redirect → graceful empty
        if status == 302 || status == 301 {
            if let Some(loc) = resp.headers().get(reqwest::header::LOCATION) {
                if loc.to_str().unwrap_or("").contains("/antispider") {
                    return Ok(vec![]);
                }
            }
        }
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse(&html, base_url))
    }

    fn parse(&self, html: &str, base_url: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // Result blocks: divs with class containing "rb" or "vrwrap"
        let block_sel = match Selector::parse("div.rb, div.vrwrap") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let pt_anchor_sel = Selector::parse("h3.pt a").ok();
        let vr_anchor_sel = Selector::parse("h3.vr-title a").ok();
        let ft_sel = Selector::parse("div.ft").unwrap();
        let attr_sel = Selector::parse("div.attribute-centent, div.fz-mid.space-txt").unwrap();
        let img_sel = Selector::parse("div.img-layout img").unwrap();

        for block in document.select(&block_sel) {
            // Determine title/url source
            let (title, url, is_image_variant): (String, String, bool) = if let Some(sel) = &pt_anchor_sel {
                if let Some(a) = block.select(sel).next() {
                    (
                        a.text().collect::<String>().trim().to_string(),
                        a.value().attr("href").unwrap_or("").to_string(),
                        false,
                    )
                } else if let Some(sel) = &vr_anchor_sel {
                    if let Some(a) = block.select(sel).next() {
                        (
                            a.text().collect::<String>().trim().to_string(),
                            a.value().attr("href").unwrap_or("").to_string(),
                            true,
                        )
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            } else {
                continue;
            };
            if title.is_empty() || url.is_empty() {
                continue;
            }
            let final_url = extract_url(&url, base_url);
            let content = if is_image_variant {
                block
                    .select(&attr_sel)
                    .next()
                    .map(|c| c.text().collect::<String>().trim().to_string())
                    .unwrap_or_default()
            } else {
                block
                    .select(&ft_sel)
                    .next()
                    .map(|c| c.text().collect::<String>().trim().to_string())
                    .unwrap_or_default()
            };
            let thumbnail = block
                .select(&img_sel)
                .next()
                .and_then(|i| i.value().attr("src"))
                .map(|s| s.replacen("http://", "https://", 1));
            let mut r = SearchResult::new(title, final_url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_result_type(ResultType::Web);
            if let Some(t) = thumbnail {
                r = r.with_extra("thumbnail", serde_json::json!(t));
            }
            results.push(r);
        }
        results
    }
}

/// De-obfuscate Sogou redirect URLs (`/link?url=...`) using an inline
/// `data-url` attribute when present.
fn extract_url(href: &str, base_url: &str) -> String {
    if href.starts_with("/link?url=") {
        // Without the HTML block we cannot reliably read data-url; fall back to
        // the raw redirect link on the base host.
        format!("{}{}", base_url, href)
    } else {
        href.to_string()
    }
}

#[async_trait]
impl Engine for SogouEngine {
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
        matches!(t, ResultType::Web | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://www.sogou.com".into());
        s
    }
}
