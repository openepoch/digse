//! Wikipedia search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Wikipedia search engine
pub struct WikipediaEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    language: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WikipediaResponse {
    #[serde(default)]
    query: WikipediaQuery,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WikipediaQuery {
    #[serde(default)]
    search: Vec<WikipediaArticle>,
    #[serde(default)]
    searchinfo: WikipediaSearchInfo,
}

#[derive(Debug, Serialize, Deserialize)]
struct WikipediaArticle {
    #[serde(default)]
    title: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    pageid: i64,
    #[serde(default)]
    wordcount: i64,
    #[serde(default)]
    timestamp: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct WikipediaSearchInfo {
    #[serde(default)]
    totalhits: i64,
}

impl WikipediaEngine {
    pub fn new(language: Option<String>) -> Self {
        let lang = language.unwrap_or_else(|| "en".to_string());

        let metadata = EngineMetadata {
            name: format!("wikipedia_{}", lang),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 10,
            description: format!("Wikipedia {} language search", lang),
            website: Some(format!("https://{}.wikipedia.org", lang)),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to create Wikipedia HTTP client");

        WikipediaEngine { metadata, client, language: lang }
    }

    pub fn new_general() -> Self {
        Self::new(None)
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = format!(
            "https://{}.wikipedia.org/w/api.php?action=query&list=search&srsearch={}&format=json&srlimit={}&sroffset={}",
            self.language,
            urlencoding::encode(&query.query),
            query.count,
            query.offset
        );


        let response = self.client
            .get(&url)
            .header("User-Agent", "digse/0.1.0")
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;


        if !response.status().is_success() {
            return Err(Error::EngineError(
                "wikipedia".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;

        let wiki_response: WikipediaResponse = serde_json::from_str(&text)
            .map_err(|e| Error::ParseError(format!("Failed to parse Wikipedia response: {}", e)))?;


        let results: Vec<SearchResult> = wiki_response.query.search
            .into_iter()
            .enumerate()
            .map(|(i, article)| {
                let url = format!(
                    "https://{}.wikipedia.org/wiki/{}",
                    self.language,
                    urlencoding::encode(&article.title.replace(' ', "_"))
                );

                // Clean up snippet (remove HTML tags)
                let snippet = article.snippet
                    .replace("<span class=\"searchmatch\">", "")
                    .replace("</span>", "");

                let result = SearchResult::new(&article.title, &url)
                    .with_snippet(&snippet)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_extra("page_id", serde_json::json!(article.pageid))
                    .with_extra("word_count", serde_json::json!(article.wordcount))
                    .with_extra("timestamp", serde_json::json!(article.timestamp))
                    .with_extra("language", serde_json::json!(self.language.clone()));

                result
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Engine for WikipediaEngine {
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
        *result_type == ResultType::Web || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), format!("https://{}.wikipedia.org", self.language));
        settings.insert("api_endpoint".to_string(), "/w/api.php".to_string());
        settings.insert("language".to_string(), self.language.clone());
        settings
    }
}
