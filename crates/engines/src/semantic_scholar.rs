//! Semantic Scholar search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Semantic Scholar search engine
pub struct SemanticScholarEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct SemanticScholarResponse {
    #[serde(default)]
    data: Vec<SemanticScholarPaper>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SemanticScholarPaper {
    #[serde(default, rename = "paperId")]
    paper_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    authors: Vec<SemanticScholarAuthor>,
    #[serde(default, rename = "citationCount")]
    citation_count: Option<i32>,
    #[serde(default, rename = "publicationVenue")]
    publication_venue: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SemanticScholarAuthor {
    #[serde(default)]
    name: String,
}

impl SemanticScholarEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "semantic_scholar".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Semantic Scholar - AI-powered research paper search.".to_string(),
            website: Some("https://www.semanticscholar.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Semantic Scholar HTTP client");

        SemanticScholarEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SemanticScholarPaper>> {
        let url = "https://api.semanticscholar.org/graph/v1/paper/search";

        let limit = query.count.to_string();
        let fields = "title,abstract,url,year,authors,citationCount,publicationVenue";

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("query", query.query.as_str()),
                ("limit", limit.as_str()),
                ("fields", fields),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "semantic_scholar".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let semantic_response: SemanticScholarResponse = serde_json::from_str(&text)
            .map_err(|e| Error::JsonError(e))?;

        Ok(semantic_response.data)
    }

    fn format_paper_url(&self, paper_id: &str) -> String {
        format!("https://www.semanticscholar.org/paper/{}", paper_id)
    }

    fn format_authors(&self, authors: &[SemanticScholarAuthor]) -> String {
        if authors.is_empty() {
            "Unknown Authors".to_string()
        } else {
            authors.iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        }
    }

    fn create_snippet(&self, paper: &SemanticScholarPaper) -> String {
        let mut parts = Vec::new();

        if let Some(abstract_text) = &paper.abstract_text {
            // Truncate abstract if too long
            if abstract_text.len() > 200 {
                parts.push(format!("{}...", &abstract_text[..200]));
            } else {
                parts.push(abstract_text.clone());
            }
        }

        if let Some(year) = paper.year {
            parts.push(format!("Year: {}", year));
        }

        if let Some(venue) = &paper.publication_venue {
            parts.push(format!("Published in: {}", venue));
        }

        if let Some(citations) = paper.citation_count {
            parts.push(format!("Citations: {}", citations));
        }

        if !paper.authors.is_empty() {
            parts.push(format!("Authors: {}", self.format_authors(&paper.authors)));
        }

        parts.join(" | ")
    }
}

#[async_trait]
impl Engine for SemanticScholarEngine {
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
        let papers = self.fetch_results(query).await?;

        let mut results = Vec::new();
        for (i, paper) in papers.iter().enumerate() {
            let url = paper.url.clone()
                .unwrap_or_else(|| self.format_paper_url(&paper.paper_id));

            let snippet = self.create_snippet(paper);

            let search_result = SearchResult::new(&paper.title, &url)
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
        settings.insert("type".to_string(), "semantic_scholar".to_string());
        settings.insert("api_version".to_string(), "v1".to_string());
        settings
    }
}
