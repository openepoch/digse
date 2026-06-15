//! AOL search engine implementation (HTML scrape; proxy for Bing index)

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// AOL search engine (HTML scraping)
pub struct AolEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl AolEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "aol".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "AOL - Web search (uses Bing index).".to_string(),
            website: Some("https://www.aol.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create AOL HTTP client");

        AolEngine { metadata, client }
    }

    /// De-obfuscate an AOL redirect URL of the form
    /// `.../RU=<urlencoded>/RK=...`
    fn deobfuscate_url(obfuscated: &str) -> Option<String> {
        if obfuscated.is_empty() {
            return None;
        }
        for part in obfuscated.split('/') {
            if let Some(rest) = part.strip_prefix("RU=") {
                return Some(urlencoding::decode(rest).map(|c| c.into_owned()).unwrap_or_default());
            }
        }
        Some(obfuscated.to_string())
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://search.aol.com";
        let page = (query.offset / 10) + 1;
        let b = (page * 10 + 1).to_string();
        let pz = "10".to_string();

        let resp = self.client
            .get(format!("{}/aol/search", base_url))
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("q", query.query.as_str()),
                ("b", b.as_str()),
                ("pz", pz.as_str()),
                ("fr2", "sb-top-search"),
            ])
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

        let li_sel = match Selector::parse("#web ol li") {
            Ok(s) => s,
            Err(_) => return results,
        };
        let h3_sel = Selector::parse("h3 a").unwrap();
        let content_sel = Selector::parse("div.compText").unwrap();

        for el in document.select(&li_sel) {
            let href = match el.select(&h3_sel).next() {
                Some(a) => a.value().attr("href").unwrap_or("").to_string(),
                None => continue,
            };
            let url = match Self::deobfuscate_url(&href) {
                Some(u) if !u.is_empty() => u,
                _ => continue,
            };
            let title = el.select(&h3_sel).next()
                .map(|a| a.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let content = el.select(&content_sel).next()
                .map(|c| c.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_result_type(ResultType::Web),
            );
        }
        results
    }
}

#[async_trait]
impl Engine for AolEngine {
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
        s.insert("base_url".to_string(), "https://search.aol.com".to_string());
        s.insert("search_type".to_string(), "search".to_string());
        s
    }
}
