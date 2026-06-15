//! PubMed search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// PubMed search engine
pub struct PubMedEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct PubMedResponse {
    #[serde(default)]
    esearchresult: PubMedResult,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PubMedResult {
    #[serde(default)]
    idlist: Vec<String>,
    #[serde(default)]
    count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct PubMedSummary {
    #[serde(default)]
    result: Vec<(String, PubMedArticle)>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PubMedArticle {
    #[serde(default)]
    title: Vec<String>,
    #[serde(default)]
    #[serde(rename = "abstract")]
    abstract_text: Vec<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    journal: Vec<String>,
    #[serde(default)]
    pubdate: Vec<String>,
    #[serde(default)]
    source: Vec<String>,
    #[serde(rename = "url")]
    #[serde(default)]
    urls: Vec<String>,
}

impl PubMedEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "pubmed".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "PubMed - Biomedical and life sciences literature.".to_string(),
            website: Some("https://pubmed.ncbi.nlm.nih.gov".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create PubMed HTTP client");

        PubMedEngine { metadata, client }
    }

    async fn search_ids(&self, query: &SearchQuery) -> Result<Vec<String>> {
        let url = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi";

        let retmax = query.count.to_string();

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("db", "pubmed"),
                ("term", query.query.as_str()),
                ("retmax", retmax.as_str()),
                ("retmode", "json"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "pubmed".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let pubmed_response: PubMedResponse = serde_json::from_str(&text)
            .map_err(|e| Error::JsonError(e))?;

        Ok(pubmed_response.esearchresult.idlist)
    }

    async fn fetch_summaries(&self, ids: &[String]) -> Result<Vec<PubMedArticle>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let url = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi";
        let id_string = ids.join(",");

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .query(&[
                ("db", "pubmed"),
                ("id", id_string.as_str()),
                ("retmode", "json"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "pubmed".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let summary: PubMedSummary = serde_json::from_str(&text)
            .map_err(|e| Error::JsonError(e))?;

        Ok(summary.result.into_iter().map(|(_, article)| article).collect())
    }

    fn get_title(&self, article: &PubMedArticle) -> String {
        if article.title.is_empty() {
            "Untitled".to_string()
        } else {
            article.title[0].clone()
        }
    }

    fn get_abstract(&self, article: &PubMedArticle) -> String {
        if article.abstract_text.is_empty() {
            String::new()
        } else {
            article.abstract_text.join(" ")
        }
    }

    fn format_authors(&self, authors: &[String]) -> String {
        if authors.is_empty() {
            "Unknown Authors".to_string()
        } else {
            authors.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
        }
    }

    fn create_url(&self, pmid: &str) -> String {
        format!("https://pubmed.ncbi.nlm.nih.gov/{}", pmid)
    }

    fn create_snippet(&self, article: &PubMedArticle) -> String {
        let mut parts = Vec::new();

        if !article.authors.is_empty() {
            parts.push(format!("Authors: {}", self.format_authors(&article.authors)));
        }

        if !article.journal.is_empty() {
            parts.push(format!("Journal: {}", article.journal[0]));
        }

        if !article.pubdate.is_empty() {
            parts.push(format!("Published: {}", article.pubdate[0]));
        }

        let abstract_text = self.get_abstract(article);
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
impl Engine for PubMedEngine {
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
        let ids = self.search_ids(query).await?;
        let articles = self.fetch_summaries(&ids).await?;

        let mut results = Vec::new();
        for (i, article) in articles.iter().enumerate() {
            let title = self.get_title(article);
            let url = if !article.urls.is_empty() {
                article.urls[0].clone()
            } else {
                // Use a generic PubMed URL if no specific URL available
                format!("https://pubmed.ncbi.nlm.nih.gov/#search/{}", &query.query)
            };

            let snippet = self.create_snippet(article);

            let search_result = SearchResult::new(&title, &url)
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
        settings.insert("type".to_string(), "pubmed".to_string());
        settings.insert("database".to_string(), "pubmed".to_string());
        settings
    }
}
