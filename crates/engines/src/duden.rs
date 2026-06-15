//! Duden (German dictionary) search engine implementation
//!
//! scrapes duden.de search results.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Duden German dictionary search engine
pub struct DudenEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl DudenEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "duden".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Duden German dictionary.".to_string(),
            website: Some("https://www.duden.de".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Duden HTTP client");

        DudenEngine { metadata, client }
    }

    fn join_url(href: &str) -> String {
        if href.starts_with("http") {
            href.to_string()
        } else if href.starts_with('/') {
            format!("https://www.duden.de{}", href)
        } else {
            format!("https://www.duden.de/{}", href)
        }
    }

    fn parse_html(&self, html: &str) -> Vec<(String, String, String)> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();

        // Reference: //section[not(contains(@class, "essay"))]
        let section_sel = match Selector::parse("section") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let h2_a_sel = Selector::parse("h2 a").unwrap();
        let p_sel = Selector::parse("p").unwrap();

        for section in doc.select(&section_sel) {
            if let Some(cls) = section.value().attr("class") {
                if cls.contains("essay") {
                    continue;
                }
            }
            let a = match section.select(&h2_a_sel).next() {
                Some(a) => a,
                None => continue,
            };
            let href = a.value().attr("href").unwrap_or("").to_string();
            if href.is_empty() {
                continue;
            }
            let url = Self::join_url(&href);
            let title = a.text().collect::<String>().trim().to_string();
            let content = section
                .select(&p_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            out.push((title, url, content));
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.trim().is_empty() {
            return Ok(vec![]);
        }
        let encoded = urlencoding::encode(&query.query);
        let pageno = (query.offset / 10) + 1;
        let url = if pageno <= 1 {
            format!("https://www.duden.de/suchen/dudenonline/{}", encoded)
        } else {
            format!(
                "https://www.duden.de/suchen/dudenonline/{}?search_api_fulltext=&page={}",
                encoded,
                pageno - 1
            )
        };

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html")
            .header("Accept-Language", "de")
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

        let parsed = self.parse_html(&text);
        let mut results = Vec::new();
        for (i, (title, url, content)) in parsed.iter().enumerate() {
            let title = if title.is_empty() {
                "Duden result".to_string()
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
impl Engine for DudenEngine {
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
        settings.insert("base_url".to_string(), "https://www.duden.de".to_string());
        settings.insert("language".to_string(), "de".to_string());
        settings
    }
}
