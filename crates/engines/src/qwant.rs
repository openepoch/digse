//! Qwant search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Qwant search engine
pub struct QwantEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct QwantResponse {
    #[serde(default)]
    data: Option<QwantData>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct QwantData {
    #[serde(default)]
    result: Option<QwantResult>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct QwantResult {
    #[serde(default)]
    items: Vec<QwantItem>,
    #[serde(default)]
    web: Vec<QwantItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct QwantItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    desc: String,
    #[serde(alias = "description")]
    #[serde(default)]
    description: String,
}

impl QwantEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "qwant".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: "Qwant - Privacy-focused search engine from Europe.".to_string(),
            website: Some("https://www.qwant.com".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Qwant HTTP client");

        QwantEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<QwantItem>> {
        let url = "https://api.qwant.com/v3/search/web";

        let count = query.count.to_string();

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept-Language", "en-US,en;q=0.9")
            .query(&[
                ("q", query.query.as_str()),
                ("count", count.as_str()),
                ("locale", "en_US"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "qwant".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        // Try JSON parsing first
        if let Ok(qwant_response) = serde_json::from_str::<QwantResponse>(&text) {
            if let Some(data) = qwant_response.data {
                if let Some(result) = data.result {
                    if !result.items.is_empty() {
                        return Ok(result.items);
                    }
                    if !result.web.is_empty() {
                        return Ok(result.web);
                    }
                }
            }
        }

        // Fallback: HTML parsing
        self.parse_html_results(&text)
    }

    fn parse_html_results(&self, html: &str) -> Result<Vec<QwantItem>> {
        let mut results = Vec::new();
        let document = scraper::Html::parse_document(html);
        let a_selector = scraper::Selector::parse("a").unwrap();

        // Qwant specific selectors
        let title_selectors = vec![".result__title", ".web-result__title", "h3"];
        for selector_str in title_selectors {
            if let Ok(selector) = scraper::Selector::parse(selector_str) {
                for element in document.select(&selector) {
                    let title = element.text().collect::<Vec<_>>().join(" ");
                    if title.is_empty() {
                        continue;
                    }

                    // Try to find URL
                    if let Some(link) = element.select(&a_selector).next() {
                        if let Some(url) = link.value().attr("href") {
                            let description = String::new();
                            results.push(QwantItem {
                                title: title.clone(),
                                url: url.to_string(),
                                desc: description.clone(),
                                description: description,
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
impl Engine for QwantEngine {
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
        let qwant_results = self.fetch_results(query).await?;

        let mut results = Vec::new();
        for (i, result) in qwant_results.iter().enumerate() {
            let description = if !result.desc.is_empty() {
                &result.desc
            } else {
                &result.description
            };

            let search_result = SearchResult::new(&result.title, &result.url)
                .with_snippet(description)
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
        settings.insert("type".to_string(), "qwant".to_string());
        settings.insert("privacy_focused".to_string(), "true".to_string());
        settings.insert("locale".to_string(), "en_US".to_string());
        settings
    }
}
