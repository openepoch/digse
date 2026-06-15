//! Destatis search engine implementation
//!
//! Scrapes the German Federal Statistical Office (destatis.de) expert search.
//! HTML scraping via the `scraper` crate.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Destatis (German statistics) search engine
pub struct DestatisEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl DestatisEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "destatis".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Destatis - German Federal Statistical Office.".to_string(),
            website: Some("https://www.destatis.de".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Destatis HTTP client");

        DestatisEngine { metadata, client }
    }

    fn parse_html(&self, html: &str) -> Vec<(String, String, String, String)> {
        let doc = Html::parse_document(html);
        let mut out = Vec::new();

        let result_sel = match Selector::parse("div.c-result") {
            Ok(s) => s,
            Err(_) => return out,
        };
        let a_sel = Selector::parse("a").unwrap();
        let date_sel = Selector::parse("span.c-result__date").unwrap();
        let doctype_sel = Selector::parse("div.c-result__doctype p").unwrap();
        let content_sel = Selector::parse("div.column p").unwrap();

        for el in doc.select(&result_sel) {
            // skip recommended results
            if let Some(cls) = el.value().attr("class") {
                if cls.contains("c-result--recommended") {
                    continue;
                }
            }
            let a = el.select(&a_sel).next();
            let (url, title) = if let Some(a) = a {
                let href = a.value().attr("href").unwrap_or("").to_string();
                let title = a
                    .text()
                    .collect::<String>()
                    .trim()
                    .to_string();
                (href, title)
            } else {
                continue;
            };
            if url.is_empty() && title.is_empty() {
                continue;
            }
            let date = el
                .select(&date_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let doctype = el
                .select(&doctype_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let content = el
                .select(&content_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            let mut metadata_parts = Vec::new();
            if !doctype.is_empty() {
                metadata_parts.push(doctype.clone());
            }
            if !date.is_empty() {
                metadata_parts.push(date.clone());
            }
            let metadata = metadata_parts.join(", ");

            let full_url = if url.starts_with("http") {
                url
            } else {
                format!("https://www.destatis.de/{}", url.trim_start_matches('/'))
            };
            out.push((title, full_url, content, metadata));
        }
        out
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = ((query.offset / 10) + 1).to_string();
        let url = "https://www.destatis.de/SiteGlobals/Forms/Suche/Expertensuche_Formular.html";
        let gtp = format!("474_list%3D{}", pageno);

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept-Language", "de")
            .query(&[
                ("templateQueryString", query.query.as_str()),
                ("gtp", gtp.as_str()),
            ])
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
        for (i, (title, url, content, metadata)) in parsed.iter().enumerate() {
            if title.is_empty() && url.is_empty() {
                continue;
            }
            let mut result = SearchResult::new(
                if title.is_empty() { "Destatis result".to_string() } else { title.clone() },
                url.clone(),
            )
            .with_engine(self.name())
            .with_rank(query.offset + i + 1)
            .with_score(1.0 - (i as f64 * 0.05))
            .with_result_type(ResultType::Web);

            if !content.is_empty() {
                result = result.with_snippet(content.clone());
            }
            if !metadata.is_empty() {
                result = result.with_extra("metadata", serde_json::json!(metadata));
            }
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for DestatisEngine {
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
            "https://www.destatis.de".to_string(),
        );
        settings.insert("language".to_string(), "de".to_string());
        settings
    }
}
