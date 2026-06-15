//! Brave Search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Brave Search engine
pub struct BraveEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct BraveResponse {
    #[serde(default)]
    web: Option<BraveWebResults>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BraveWebResults {
    #[serde(default)]
    results: Vec<BraveResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BraveResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
}

impl BraveEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "brave".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Brave Search - Private, independent search engine.".to_string(),
            website: Some("https://search.brave.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Brave HTTP client");

        BraveEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<BraveResult>> {
        let url = "https://search.brave.com/api/search";

        let count = query.count.to_string();

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("count", count.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "brave".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        // Try to parse as JSON
        if let Ok(brave_response) = serde_json::from_str::<BraveResponse>(&text) {
            if let Some(web) = brave_response.web {
                return Ok(web.results);
            }
        }

        // Fallback: parse HTML if JSON fails
        self.parse_html_results(&text)
    }

    fn parse_html_results(&self, html: &str) -> Result<Vec<BraveResult>> {
        let mut results = Vec::new();

        // Simple HTML parsing for Brave search results
        // This is a basic implementation that would need to be enhanced
        let document = scraper::Html::parse_document(html);
        let a_selector = scraper::Selector::parse("a").unwrap();

        // Try to find result elements
        let title_selectors = vec![".web-title", ".result-title", "h3"];
        for selector_str in title_selectors {
            if let Ok(selector) = scraper::Selector::parse(selector_str) {
                for element in document.select(&selector) {
                    let title = element.text().collect::<Vec<_>>().join(" ");
                    if title.is_empty() {
                        continue;
                    }

                    // Try to find URL within the element
                    if let Some(url_element) = element.select(&a_selector).next() {
                        if let Some(url) = url_element.value().attr("href") {
                            results.push(BraveResult {
                                title: title.clone(),
                                url: url.to_string(),
                                description: String::new(),
                            });
                        }
                    }
                }
                if !results.is_empty() {
                    break;
                }
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for BraveEngine {
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
        let brave_results = self.fetch_results(query).await?;

        let mut results = Vec::new();
        for (i, result) in brave_results.iter().enumerate() {
            let search_result = SearchResult::new(&result.title, &result.url)
                .with_snippet(&result.description)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05));

            results.push(search_result);
        }

        Ok(results)
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> std::collections::HashMap<String, String> {
        let mut settings = std::collections::HashMap::new();
        settings.insert("type".to_string(), "brave".to_string());
        settings.insert("privacy_focused".to_string(), "true".to_string());
        settings
    }
}
