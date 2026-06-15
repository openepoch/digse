//! DOAJ (Directory of Open Access Journals) search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// DOAJ search engine
pub struct DoajEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct DoajResponse {
    #[serde(default)]
    results: Option<Vec<DoajArticle>>,
    #[serde(default)]
    total: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct DoajArticle {
    #[serde(default)]
    title: String,
    #[serde(default)]
    bibjson: DoajBibJson,
    #[serde(default)]
    id: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DoajBibJson {
    #[serde(default)]
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    #[serde(default)]
    author: Vec<DoajAuthor>,
    #[serde(default)]
    journal: DoajJournal,
    #[serde(default)]
    year: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DoajAuthor {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DoajJournal {
    #[serde(default)]
    name: String,
    #[serde(default)]
    publisher: DoajPublisher,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct DoajPublisher {
    #[serde(default)]
    name: String,
}

impl DoajEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "doaj".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "DOAJ - Directory of Open Access Journals.".to_string(),
            website: Some("https://doaj.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create DOAJ HTTP client");

        DoajEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<DoajArticle>> {
        let url = "https://doaj.org/api/search/articles";
        let page_size = query.count.to_string();

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("pageSize", page_size.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "doaj".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let doaj_response: DoajResponse = serde_json::from_str(&text)
            .map_err(|e| Error::JsonError(e))?;

        Ok(doaj_response.results.unwrap_or_default())
    }

    fn format_authors(&self, authors: &[DoajAuthor]) -> String {
        if authors.is_empty() {
            "Unknown Authors".to_string()
        } else {
            authors.iter()
                .take(5)
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn get_abstract(&self, bibjson: &DoajBibJson) -> String {
        bibjson.abstract_text.clone().unwrap_or_default()
    }

    fn get_url(&self, article: &DoajArticle) -> String {
        article.bibjson.url.clone()
            .unwrap_or_else(|| format!("https://doaj.org/article/{}", article.id))
    }

    fn create_snippet(&self, article: &DoajArticle) -> String {
        let mut parts = Vec::new();

        if !article.bibjson.author.is_empty() {
            parts.push(format!("Authors: {}", self.format_authors(&article.bibjson.author)));
        }

        if !article.bibjson.journal.name.is_empty() {
            parts.push(format!("Journal: {}", article.bibjson.journal.name));
        }

        if !article.bibjson.journal.publisher.name.is_empty() {
            parts.push(format!("Publisher: {}", article.bibjson.journal.publisher.name));
        }

        if let Some(year) = &article.bibjson.year {
            parts.push(format!("Year: {}", year));
        }

        let abstract_text = self.get_abstract(&article.bibjson);
        if !abstract_text.is_empty() {
            let truncated = if abstract_text.len() > 150 {
                format!("{}...", &abstract_text[..150])
            } else {
                abstract_text
            };
            parts.push(format!("Abstract: {}", truncated));
        }

        parts.join(" | ")
    }
}

#[async_trait]
impl Engine for DoajEngine {
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
        let articles = self.fetch_results(query).await?;

        let mut results = Vec::new();
        for (i, article) in articles.iter().enumerate() {
            let url = self.get_url(article);
            let snippet = self.create_snippet(article);

            let search_result = SearchResult::new(&article.title, &url)
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
        settings.insert("type".to_string(), "doaj".to_string());
        settings.insert("open_access".to_string(), "true".to_string());
        settings
    }
}
