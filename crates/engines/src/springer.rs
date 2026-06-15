//! Springer Nature search engine implementation
//!
//! Queries the Springer
//! Meta-API v2 JSON endpoint, which requires an API key (`SPRINGER_API_KEY`).
//! Returns an empty result set with an informational log line when the key is
//! not set.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Springer Nature academic search engine
pub struct SpringerEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

const BASE_URL: &str = "https://api.springernature.com/meta/v2/json";
const PAGE_SIZE: usize = 10;

#[derive(Debug, Serialize, Deserialize)]
struct SpringerResponse {
    #[serde(default)]
    records: Vec<SpringerRecord>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    message: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SpringerRecord {
    #[serde(default)]
    title: Vec<String>,
    #[serde(default)]
    #[serde(rename = "abstract")]
    abstract_text: Vec<String>,
    #[serde(default)]
    #[serde(rename = "publicationDate")]
    publication_date: String,
    #[serde(default)]
    #[serde(rename = "publicationName")]
    publication_name: Vec<String>,
    #[serde(default)]
    publisher: Vec<String>,
    #[serde(default)]
    doi: Vec<String>,
    #[serde(default)]
    volume: Vec<String>,
    #[serde(default)]
    number: Vec<String>,
    #[serde(default)]
    #[serde(rename = "startingPage")]
    starting_page: Vec<String>,
    #[serde(default)]
    #[serde(rename = "endingPage")]
    ending_page: Vec<String>,
    #[serde(default)]
    contentType: Vec<String>,
    #[serde(default)]
    keyword: Vec<String>,
    #[serde(default)]
    issn: Vec<String>,
    #[serde(default)]
    isbn: Vec<String>,
    #[serde(default)]
    creators: Vec<SpringerCreator>,
    #[serde(default)]
    url: Vec<SpringerUrl>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SpringerCreator {
    #[serde(default)]
    creator: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct SpringerUrl {
    #[serde(default)]
    platform: String,
    #[serde(default)]
    format: String,
    #[serde(default)]
    value: String,
}

fn first_or_empty(v: &[String]) -> String {
    v.first().cloned().unwrap_or_default()
}

impl SpringerEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("SPRINGER_API_KEY")
            .ok()
            .filter(|k| !k.is_empty());

        let metadata = EngineMetadata {
            name: "springer".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: api_key.is_some(),
            timeout_seconds: 20,
            description: "Springer Nature - scientific publications (Meta-API).".to_string(),
            website: Some("https://www.springernature.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create Springer HTTP client");

        SpringerEngine {
            metadata,
            client,
            api_key,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                tracing::info!("springer: SPRINGER_API_KEY not set; returning empty");
                return Ok(vec![]);
            }
        };

        let pageno = (query.offset / PAGE_SIZE) + 1;
        let s = ((pageno - 1) * PAGE_SIZE).to_string();
        let p = PAGE_SIZE.to_string();

        let resp = self
            .client
            .get(BASE_URL)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("api_key", api_key.as_str()),
                ("q", query.query.as_str()),
                ("s", s.as_str()),
                ("p", p.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let parsed: SpringerResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        // Premium-feature 403 responses should return empty, not error.
        if parsed.status.eq_ignore_ascii_case("fail")
            && parsed.message.to_lowercase().contains("premium feature")
        {
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        for (i, record) in parsed.records.iter().enumerate() {
            let title = first_or_empty(&record.title);
            if title.is_empty() {
                continue;
            }

            let authors: Vec<String> = record
                .creators
                .iter()
                .map(|c| {
                    // Reverse "Last, First" -> "First Last".
                    let parts: Vec<&str> = c.creator.split(", ").collect();
                    if parts.len() == 2 {
                        format!("{} {}", parts[1], parts[0])
                    } else {
                        c.creator.clone()
                    }
                })
                .collect();

            // pick html (landing) and pdf urls from the url list.
            let mut html_url = String::new();
            let mut pdf_url = String::new();
            for item in &record.url {
                if item.platform != "web" {
                    continue;
                }
                let val = item.value.replacen("http://", "https://", 1);
                if item.format.eq_ignore_ascii_case("html") {
                    html_url = val;
                } else if item.format.eq_ignore_ascii_case("pdf") {
                    pdf_url = val;
                }
            }
            let url = if !html_url.is_empty() {
                html_url
            } else if !pdf_url.is_empty() {
                pdf_url.clone()
            } else {
                format!("https://www.springernature.com/search?q={}", urlencoding::encode(&query.query))
            };

            let venue = first_or_empty(&record.publication_name);
            let publisher = first_or_empty(&record.publisher);
            let work_type = first_or_empty(&record.contentType);
            let doi = first_or_empty(&record.doi);
            let abstract_text = first_or_empty(&record.abstract_text);
            let year = record
                .publication_date
                .get(0..4)
                .unwrap_or("")
                .to_string();

            let mut snippet_parts = Vec::new();
            if !authors.is_empty() {
                snippet_parts.push(format!("Authors: {}", authors.join(", ")));
            }
            if !venue.is_empty() {
                snippet_parts.push(format!("Published in: {}", venue));
            }
            if !publisher.is_empty() {
                snippet_parts.push(format!("Publisher: {}", publisher));
            }
            if !work_type.is_empty() {
                snippet_parts.push(format!("Type: {}", work_type));
            }
            if !record.publication_date.is_empty() {
                snippet_parts.push(format!("Published: {}", record.publication_date));
            }
            if !abstract_text.is_empty() {
                let truncated: String = abstract_text.chars().take(200).collect();
                snippet_parts.push(format!("Abstract: {}...", truncated));
            }

            let mut result = SearchResult::new(title, url)
                .with_snippet(snippet_parts.join(" | "))
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.03))
                .with_result_type(ResultType::Academic);

            if !authors.is_empty() {
                result = result.with_extra("authors", serde_json::json!(authors.join(", ")));
            }
            if !doi.is_empty() {
                result = result.with_extra("doi", serde_json::json!(doi));
            }
            if !pdf_url.is_empty() {
                result = result.with_extra("pdf_url", serde_json::json!(pdf_url));
            }
            if !year.is_empty() {
                result = result.with_extra("year", serde_json::json!(year));
            }
            if !venue.is_empty() {
                result = result.with_extra("venue", serde_json::json!(venue));
            }
            if !record.keyword.is_empty() {
                result = result.with_extra("tags", serde_json::json!(record.keyword));
            }

            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for SpringerEngine {
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
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Academic | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), BASE_URL.into());
        s.insert("page_size".into(), PAGE_SIZE.to_string());
        s.insert(
            "requires_key".into(),
            self.api_key.is_some().to_string(),
        );
        s
    }
}
