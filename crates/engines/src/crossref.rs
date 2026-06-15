//! Crossref search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Crossref search engine
pub struct CrossrefEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct CrossrefResponse {
    #[serde(default)]
    message: Option<CrossrefMessage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CrossrefMessage {
    #[serde(default)]
    items: Vec<CrossrefWork>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CrossrefWork {
    #[serde(default)]
    title: Vec<String>,
    #[serde(default)]
    #[serde(rename = "URL")]
    url: String,
    #[serde(default)]
    author: Vec<CrossrefAuthor>,
    #[serde(default)]
    published: Option<CrossrefDate>,
    #[serde(default)]
    #[serde(rename = "type")]
    work_type: Option<String>,
    #[serde(default)]
    publisher: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CrossrefAuthor {
    #[serde(default)]
    given: String,
    #[serde(default)]
    family: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct CrossrefDate {
    #[serde(default)]
    parts: Vec<CrossrefDatePart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CrossrefDatePart {
    #[serde(default)]
    year: i32,
}

impl CrossrefEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "crossref".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Crossref - Metadata for scholarly works.".to_string(),
            website: Some("https://www.crossref.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Crossref HTTP client");

        CrossrefEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<CrossrefWork>> {
        let url = "https://api.crossref.org/works";
        let rows = query.count.to_string();

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .query(&[
                ("query", query.query.as_str()),
                ("rows", rows.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "crossref".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let crossref_response: CrossrefResponse = serde_json::from_str(&text)
            .map_err(|e| Error::JsonError(e))?;

        Ok(crossref_response.message.map(|m| m.items).unwrap_or_default())
    }

    fn get_title(&self, work: &CrossrefWork) -> String {
        if work.title.is_empty() {
            "Untitled".to_string()
        } else {
            work.title[0].clone()
        }
    }

    fn format_authors(&self, authors: &[CrossrefAuthor]) -> String {
        if authors.is_empty() {
            "Unknown Authors".to_string()
        } else {
            authors.iter()
                .map(|a| format!("{} {}", a.given, a.family))
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn get_year(&self, work: &CrossrefWork) -> Option<i32> {
        work.published.as_ref()?.parts.first().map(|p| p.year)
    }

    fn create_snippet(&self, work: &CrossrefWork) -> String {
        let mut parts = Vec::new();

        if !work.author.is_empty() {
            parts.push(format!("Authors: {}", self.format_authors(&work.author)));
        }

        if let Some(year) = self.get_year(work) {
            parts.push(format!("Year: {}", year));
        }

        if let Some(work_type) = &work.work_type {
            parts.push(format!("Type: {}", work_type));
        }

        if !work.publisher.is_empty() {
            parts.push(format!("Publisher: {}", work.publisher));
        }

        parts.join(" | ")
    }
}

#[async_trait]
impl Engine for CrossrefEngine {
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
        let works = self.fetch_results(query).await?;

        let mut results = Vec::new();
        for (i, work) in works.iter().enumerate() {
            let title = self.get_title(work);
            let snippet = self.create_snippet(work);

            let search_result = SearchResult::new(&title, &work.url)
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.03));

            results.push(search_result);
        }

        Ok(results)
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::Academic | ResultType::All)
    }

    fn settings(&self) -> std::collections::HashMap<String, String> {
        let mut settings = std::collections::HashMap::new();
        settings.insert("type".to_string(), "crossref".to_string());
        settings.insert("api_version".to_string(), "v1".to_string());
        settings
    }
}
