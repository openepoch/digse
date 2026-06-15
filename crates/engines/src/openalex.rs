//! OpenAlex search engine implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// OpenAlex search engine
pub struct OpenAlexEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAlexResponse {
    #[serde(default)]
    meta: OpenAlexMeta,
    #[serde(default)]
    results: Vec<OpenAlexWork>,
    #[serde(default)]
    group_by: Vec<OpenAlexGroup>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct OpenAlexMeta {
    #[serde(default)]
    count: i32,
    #[serde(default)]
    page: i32,
    #[serde(default)]
    per_page: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAlexGroup {
    #[serde(default)]
    key: String,
    #[serde(default)]
    count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAlexWork {
    #[serde(default)]
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    publication_year: Option<i32>,
    #[serde(default)]
    primary_location: Option<OpenAlexLocation>,
    #[serde(default)]
    authorships: Vec<OpenAlexAuthorship>,
    #[serde(default)]
    #[serde(rename = "type")]
    work_type: String,
    #[serde(default)]
    concepts: Vec<OpenAlexConcept>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAlexLocation {
    #[serde(default)]
    source: Option<OpenAlexSource>,
    #[serde(default)]
    pdf_url: Option<String>,
    #[serde(default)]
    landing_page_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAlexSource {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAlexAuthorship {
    #[serde(default)]
    author: Option<OpenAlexAuthor>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAlexAuthor {
    #[serde(default)]
    display_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAlexConcept {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    score: f64,
}

impl OpenAlexEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "openalex".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 20,
            description: "OpenAlex - Open index of scholarly works.".to_string(),
            website: Some("https://openalex.org".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create OpenAlex HTTP client");

        OpenAlexEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<OpenAlexWork>> {
        let url = "https://api.openalex.org/works";
        let per_page = query.count.to_string();

        let params = [
            ("search", query.query.as_str()),
            ("per_page", per_page.as_str()),
        ];

        let response = self.client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&params)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(Error::EngineError(
                "openalex".to_string(),
                format!("HTTP error: {}", response.status()),
            ));
        }

        let text = response.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let openalex_response: OpenAlexResponse = serde_json::from_str(&text)
            .map_err(|e| Error::JsonError(e))?;

        Ok(openalex_response.results)
    }

    fn get_url(&self, work: &OpenAlexWork) -> String {
        if let Some(location) = &work.primary_location {
            if let Some(pdf_url) = &location.pdf_url {
                return pdf_url.clone();
            }
            if let Some(landing_url) = &location.landing_page_url {
                return landing_url.clone();
            }
        }
        work.id.clone()
    }

    fn get_authors(&self, work: &OpenAlexWork) -> String {
        let authors: Vec<String> = work.authorships
            .iter()
            .filter_map(|a| a.author.as_ref())
            .map(|a| a.display_name.clone())
            .take(5)
            .collect();

        if authors.is_empty() {
            "Unknown Authors".to_string()
        } else {
            authors.join(", ")
        }
    }

    fn get_venue(&self, work: &OpenAlexWork) -> String {
        if let Some(location) = &work.primary_location {
            if let Some(source) = &location.source {
                return source.display_name.clone();
            }
        }
        "Unknown Venue".to_string()
    }

    fn create_snippet(&self, work: &OpenAlexWork) -> String {
        let mut parts = Vec::new();

        let authors = self.get_authors(work);
        if !authors.is_empty() {
            parts.push(format!("Authors: {}", authors));
        }

        let venue = self.get_venue(work);
        if venue != "Unknown Venue" {
            parts.push(format!("Published in: {}", venue));
        }

        if let Some(year) = work.publication_year {
            parts.push(format!("Year: {}", year));
        }

        if !work.work_type.is_empty() {
            parts.push(format!("Type: {}", work.work_type));
        }

        if !work.concepts.is_empty() {
            let top_concepts: Vec<String> = work.concepts
                .iter()
                .take(3)
                .map(|c| c.display_name.clone())
                .collect();
            parts.push(format!("Topics: {}", top_concepts.join(", ")));
        }

        parts.join(" | ")
    }
}

#[async_trait]
impl Engine for OpenAlexEngine {
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
            let url = self.get_url(work);
            let snippet = self.create_snippet(work);

            let search_result = SearchResult::new(&work.title, &url)
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
        settings.insert("type".to_string(), "openalex".to_string());
        settings.insert("api_version".to_string(), "v1".to_string());
        settings
    }
}
