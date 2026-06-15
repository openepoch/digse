//! Luxxle search engine implementation
//!
//! Luxxle is an American search engine focusing on
//! "unbiased" results. It performs a two-step flow (fetch a
//! page to scrape telemetry tokens, then POST to load_search.php). This port
//! performs the general-results HTML scrape. Category: general.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Luxxle general web search engine
pub struct LuxxleEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
}

impl LuxxleEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "luxxle".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Luxxle - American search engine.".to_string(),
            website: Some("https://luxxle.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Luxxle HTTP client");

        LuxxleEngine {
            metadata,
            client,
            base_url: "https://luxxle.com".to_string(),
        }
    }

    // Extract a JS string variable value of the form  var name = "...";  (double or single quotes)
    fn extr_js_var(text: &str, name: &str) -> String {
        let mut needle = String::new();
        needle.push_str("var ");
        needle.push_str(name);
        needle.push_str(" = \"");
        if let Some(start) = text.find(&needle) {
            let val_start = start + needle.len();
            if let Some(end) = text[val_start..].find("\";") {
                return text[val_start..val_start + end].to_string();
            }
        }
        // try single quotes
        let mut needle2 = String::new();
        needle2.push_str("var ");
        needle2.push_str(name);
        needle2.push_str(" = '");
        if let Some(start) = text.find(&needle2) {
            let val_start = start + needle2.len();
            if let Some(end) = text[val_start..].find("';") {
                return text[val_start..val_start + end].to_string();
            }
        }
        String::new()
    }

    fn extract_url_from_redirect(href: &str) -> String {
        // urls look like "/redirect?url=<url>"
        if let Some(idx) = href.find("?url=") {
            let raw = &href[idx + "?url=".len()..];
            return urlencoding::decode(raw)
                .map(|c| c.to_string())
                .unwrap_or_else(|_| raw.to_string());
        }
        href.to_string()
    }

    fn parse_general(&self, html: &str) -> Vec<(String, String, String)> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();
        let container_sel = match Selector::parse(
            "div#mainResults div.resultsContainer",
        ) {
            Ok(s) => s,
            Err(_) => return out,
        };
        let url_a_sel = Selector::parse("div.urlAddressLink a").unwrap();
        let urlname_sel = Selector::parse("div.urlname").unwrap();
        let snippet_sel = Selector::parse("div.urlSnippet").unwrap();
        for c in doc.select(&container_sel) {
            let href = c
                .select(&url_a_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("")
                .to_string();
            let url = Self::extract_url_from_redirect(&href);
            let title = c
                .select(&urlname_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let content = c
                .select(&snippet_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if !url.is_empty() {
                out.push((title, url, content));
            }
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let encoded = urlencoding::encode(&query.query);

        // Step 1: fetch the luxsearch page to obtain telemetry tokens.
        let page_url = format!("{}/luxsearch?q={}", self.base_url, encoded);
        let resp = self
            .client
            .get(&page_url)
            .header("User-Agent", "digse/0.1.0")
            .header("Sec-GPC", "1")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let page_text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let ip = Self::extr_js_var(&page_text, "ip");
        let time_of = Self::extr_js_var(&page_text, "timeOf");
        let authorization = Self::extr_js_var(&page_text, "authorization");
        let preferences_cookie = Self::extr_js_var(&page_text, "preferencesCookie");

        // Step 2: POST to load_search.php
        let post_url = format!("{}/load_search.php", self.base_url);
        let safe = if query.safe_search { "Moderate" } else { "Off" };
        let search_data = serde_json::json!({
            "ip": ip,
            "timeOf": time_of,
            "authorization": authorization,
            "preferencesCookie": preferences_cookie,
            "query": query.query,
            "market": "en-US",
            "safeSearch": safe,
            "freshness": "",
            "language": "english",
        });
        let body = serde_json::json!({ "searchData": search_data });

        let resp = self
            .client
            .post(&post_url)
            .header("User-Agent", "digse/0.1.0")
            .header("Content-Type", "application/json")
            .header("Accept", "text/html,application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        // The response may be HTML or JSON. Try HTML scraping first.
        let parsed = self.parse_general(&text);
        let mut results = Vec::new();
        for (i, (title, url, content)) in parsed.iter().enumerate() {
            if url.is_empty() {
                continue;
            }
            let title = if title.is_empty() {
                "Luxxle result".to_string()
            } else {
                title.clone()
            };
            let mut result = SearchResult::new(title, url.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            if !content.is_empty() {
                result = result.with_snippet(content.clone());
            }
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for LuxxleEngine {
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
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("luxxle_categ".to_string(), "search".to_string());
        settings
    }
}
