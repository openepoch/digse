//! Ahmia search engine implementation (HTML, onion hidden services)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Ahmia onion-services search engine
pub struct AhmiaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl AhmiaEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "ahmia".to_string(),
            category: EngineCategory::General, // onions -> general
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "Ahmia - search engine for Tor onion hidden services.".to_string(),
            website: Some("http://juhanurmihxlp77nkq76byazcldy2hlmovfu2epvl5ankdibsot4csyd.onion".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create Ahmia HTTP client");

        AhmiaEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let search_url = "http://juhanurmihxlp77nkq76byazcldy2hlmovfu2epvl5ankdibsot4csyd.onion/search/";

        let resp = self.client
            .get(search_url)
            .header("User-Agent", "digse/0.0.1")
            .query(&[("q", query.query.as_str())])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let html = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        Ok(self.parse_html(&html))
    }

    fn parse_html(&self, html: &str) -> Vec<SearchResult> {
        use scraper::{Html, Selector};
        let document = Html::parse_document(html);
        let mut results = Vec::new();

        let li_sel = match Selector::parse("li.result") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let a_sel = Selector::parse("h4 a").unwrap();
        let p_sel = Selector::parse("p").unwrap();

        for el in document.select(&li_sel) {
            let a = match el.select(&a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            // href is typically ahmia's own redirect: .../?redirect_url=<encoded>
            let href = a.value().attr("href").unwrap_or("").to_string();
            let cleaned = extract_redirect_url(&href).unwrap_or_else(|| href.clone());
            let title = a.text().collect::<String>().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let content = el.select(&p_sel).next()
                .map(|p| p.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let mut r = SearchResult::new(title, cleaned)
                .with_snippet(content)
                .with_engine(self.name())
                .with_result_type(ResultType::Web);
            r = r.with_extra("is_onion", serde_json::json!(true));
            results.push(r);
        }
        results
    }
}

/// Extract the real destination URL from an Ahmia redirect link of the form
/// `...?redirect_url=<percent-encoded>`.
fn extract_redirect_url(href: &str) -> Option<String> {
    if let Some(idx) = href.find("redirect_url=") {
        let rest = &href[idx + "redirect_url=".len()..];
        let raw = rest.split('&').next().unwrap_or(rest);
        return Some(urlencoding::decode(raw).map(|c| c.into_owned()).unwrap_or_default());
    }
    None
}

#[async_trait]
impl Engine for AhmiaEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

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
        s.insert("base_url".to_string(),
            "http://juhanurmihxlp77nkq76byazcldy2hlmovfu2epvl5ankdibsot4csyd.onion".to_string());
        s
    }
}
