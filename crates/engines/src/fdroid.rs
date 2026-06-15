//! F-Droid search engine implementation
//!
//! HTML scrape of
//! `https://search.f-droid.org/?q=...&page=N` collecting
//! `a.package-header` elements. Categories in ref: files, apps.

use async_trait::async_trait;
use scraper::{Html, Selector};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// F-Droid FOSS Android app repository search engine (HTML scrape)
pub struct FDroidEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl FDroidEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "fdroid".to_string(),
            category: EngineCategory::Files,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "F-Droid - repository of FOSS applications for Android.".to_string(),
            website: Some("https://f-droid.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create F-Droid HTTP client");

        FDroidEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://search.f-droid.org/";
        let page = query.offset + 1;
        let page_str = page.to_string();

        let response = self
            .client
            .get(base_url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "text/html,application/xhtml+xml")
            .query(&[
                ("q", query.query.as_str()),
                ("page", page_str.as_str()),
                ("lang", ""),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let html = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let doc = Html::parse_document(&html);
        let header_sel = match Selector::parse("a.package-header") {
            Ok(s) => s,
            Err(_) => return Ok(vec![]),
        };
        let name_sel = Selector::parse("div h4.package-name").unwrap();
        let summary_sel = Selector::parse("span.package-summary").unwrap();
        let license_sel = Selector::parse("span.package-license").unwrap();
        let icon_sel = Selector::parse("img.package-icon").unwrap();

        let mut results = Vec::new();
        for (i, app) in doc.select(&header_sel).enumerate() {
            if i >= query.count {
                break;
            }
            let app_url = app.value().attr("href").unwrap_or("").to_string();
            let app_title = app
                .select(&name_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if app_title.is_empty() {
                continue;
            }
            let summary = app
                .select(&summary_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let license = app
                .select(&license_sel)
                .next()
                .map(|t| t.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            let thumbnail = app
                .select(&icon_sel)
                .next()
                .and_then(|img| img.value().attr("src"))
                .unwrap_or("")
                .to_string();

            let content = if summary.is_empty() && license.is_empty() {
                String::new()
            } else if license.is_empty() {
                summary.clone()
            } else if summary.is_empty() {
                license.clone()
            } else {
                format!("{} - {}", summary, license)
            };

            let result = SearchResult::new(app_title, app_url)
                .with_snippet(content)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Files)
                .with_extra("thumbnail", serde_json::json!(thumbnail))
                .with_extra("license", serde_json::json!(license))
                .with_extra("source", serde_json::json!("fdroid"));

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for FDroidEngine {
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
        matches!(result_type, ResultType::Files | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://search.f-droid.org/".to_string());
        settings
    }
}
