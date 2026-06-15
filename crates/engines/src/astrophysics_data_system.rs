//! NASA Astrophysics Data System (ADS) search engine implementation
//! (paid, requires ADS_API_KEY env var)

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result,
    SearchQuery, SearchResult, ResultType,
};

/// NASA Astrophysics Data System search engine
pub struct AstrophysicsDataSystemEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AdsResponse {
    #[serde(default)]
    response: AdsResponseBody,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AdsResponseBody {
    #[serde(default)]
    docs: Vec<AdsDoc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct AdsDoc {
    #[serde(default)]
    title: Vec<String>,
    #[serde(default)]
    author: Vec<String>,
    #[serde(default, rename = "abstract")]
    abstract_text: String,
    #[serde(default)]
    bibcode: String,
    #[serde(default)]
    doi: Vec<String>,
    #[serde(default)]
    keyword: Vec<String>,
    #[serde(default)]
    page: Vec<String>,
    #[serde(default)]
    #[serde(rename = "pub")]
    pub_: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    volume: String,
    #[serde(default)]
    year: String,
    #[serde(default, rename = "read_count")]
    read_count: String,
    #[serde(default)]
    pubnote: Vec<String>,
}

impl AstrophysicsDataSystemEngine {
    pub fn new() -> Self {
        let api_key = std::env::var("ADS_API_KEY").ok().filter(|k| !k.is_empty() && k != "unset");
        let metadata = EngineMetadata {
            name: "astrophysics_data_system".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: true,
            timeout_seconds: 20,
            description: "NASA ADS - astrophysics & physics research literature.".to_string(),
            website: Some("https://ui.adsabs.harvard.edu/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .expect("Failed to create ADS HTTP client");

        AstrophysicsDataSystemEngine { metadata, client, api_key }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let api_key = match &self.api_key {
            Some(k) => k.clone(),
            None => {
                eprintln!("astrophysics_data_system requires ADS_API_KEY");
                return Ok(vec![]);
            }
        };

        let base_url = "https://api.adsabs.harvard.edu/v1/search/query";
        let rows = query.count.to_string();
        let start = query.offset.to_string();
        let fl = "abstract,author,bibcode,comment,date,doi,isbn,issn,keyword,page,page_count,page_range,pub,pubdate,pubnote,read_count,title,volume,year";

        let resp = self.client
            .get(base_url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .query(&[
                ("q", query.query.as_str()),
                ("fl", fl),
                ("rows", rows.as_str()),
                ("start", start.as_str()),
                ("sort", "read_count desc"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: AdsResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, doc) in parsed.response.docs.iter().enumerate() {
            let title = doc.title.first().cloned().unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let url = format!("https://ui.adsabs.harvard.edu/abs/{}/", doc.bibcode);
            let mut authors = doc.author.clone();
            if authors.len() > 15 {
                authors.truncate(15);
                authors.push("et al.".to_string());
            }
            let publisher = format!("{} {}", doc.pub_, doc.year);
            let pages = doc.page.join(",");

            let r = SearchResult::new(title, url)
                .with_snippet(doc.abstract_text.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.03))
                .with_result_type(ResultType::Academic)
                .with_extra("authors", serde_json::json!(authors.join(", ")))
                .with_extra("doi", serde_json::json!(doc.doi.first().cloned().unwrap_or_default()))
                .with_extra("publisher", serde_json::json!(publisher))
                .with_extra("volume", serde_json::json!(doc.volume))
                .with_extra("published", serde_json::json!(doc.date))
                .with_extra("pages", serde_json::json!(pages))
                .with_extra("tags", serde_json::json!(doc.keyword))
                .with_extra("views", serde_json::json!(doc.read_count));
            results.push(r);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for AstrophysicsDataSystemEngine {
    fn name(&self) -> &str { &self.metadata.name }
    fn category(&self) -> EngineCategory { self.metadata.category }
    fn is_enabled(&self) -> bool { self.metadata.enabled }
    fn metadata(&self) -> EngineMetadata { self.metadata.clone() }

    async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        self.fetch_results(query).await
    }

    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Academic | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".to_string(), "https://api.adsabs.harvard.edu".to_string());
        s.insert("requires_auth".to_string(), "true".to_string());
        s
    }
}
