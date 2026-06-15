//! Yahoo News search engine implementation (HTML).

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Yahoo News search engine.
pub struct YahooNewsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl YahooNewsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "yahoo_news".to_string(),
            category: EngineCategory::News,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Yahoo News - English-language news search.".to_string(),
            website: Some("https://news.yahoo.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Yahoo News HTTP client");
        YahooNewsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let offset = (query.offset + 1).to_string();
        let url = format!(
            "https://news.search.yahoo.com/search?p={}&b={}",
            urlencoding::encode(&query.query),
            offset
        );

        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html, query))
    }

    fn parse_html(&self, html: &str, query: &SearchQuery) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        // upstream: //ol[contains(@class,"searchCenterMiddle")]//li
        let li_sel = match Selector::parse("ol.searchCenterMiddle li, ol[class*='searchCenterMiddle'] li")
        {
            Ok(s) => s,
            Err(_) => return results,
        };
        let h4_a_sel = match Selector::parse("h4 a") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let p_sel = match Selector::parse("p").ok() {
            Some(s) => s,
            None => return results,
        };
        let img_sel = match Selector::parse("img") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let time_sel = match Selector::parse("span.s-time, span[class*='s-time']") {
            Ok(s) => s,
            Err(_) => return results,
        };

        for (i, li) in document.select(&li_sel).enumerate() {
            if i >= query.count {
                break;
            }
            let a = match li.select(&h4_a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let raw_href = a.value().attr("href").unwrap_or("").to_string();
            if raw_href.is_empty() {
                continue;
            }
            let url = extract_yahoo_redirect(&raw_href).unwrap_or(raw_href.clone());
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let content = li
                .select(&p_sel)
                .next()
                .map(|p| p.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = li
                .select(&img_sel)
                .next()
                .and_then(|im| {
                    im.value()
                        .attr("data-src")
                        .or_else(|| im.value().attr("src"))
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();
            let pub_date_raw = li
                .select(&time_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let pub_date = parse_pub_date(&pub_date_raw);

            let r = SearchResult::new(title, url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::News)
                .with_extra("published", serde_json::json!(pub_date))
                .with_extra("source", serde_json::json!("yahoo_news"))
                .with_extra("img_src", serde_json::json!(thumbnail));
            results.push(r);
        }
        results
    }
}

/// Yahoo wraps result URLs in a redirect like
/// `https://r.search.yahoo.com/...;_ylu=.../RU=<percent-encoded>/RK=.../RS=...`
/// Extract the RU= value when present.
fn extract_yahoo_redirect(href: &str) -> Option<String> {
    let idx = href.find("/RU=")?;
    let rest = &href[idx + "/RU=".len()..];
    let raw = rest.split('/').next().unwrap_or(rest);
    let decoded = urlencoding::decode(raw).ok()?.into_owned();
    if decoded.starts_with("http") {
        Some(decoded)
    } else {
        None
    }
}

/// Parse a Yahoo News "time ago" or absolute date string. We keep the textual
/// label (best-effort), since upstream converts "N <unit> ago" into a relative
/// datetime.
fn parse_pub_date(raw: &str) -> String {
    raw.trim().to_string()
}

#[async_trait]
impl Engine for YahooNewsEngine {
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
        s.insert("base_url".to_string(), "https://news.search.yahoo.com".to_string());
        s.insert("results".to_string(), "HTML".to_string());
        s.insert("language_support".to_string(), "false".to_string());
        s
    }
}
