//! Heexy search engine implementation
//!
//! Privacy-focused JSON search engine.
//! `heexy_categ` selects between "web" and "image" results.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Heexy search engine
pub struct HeexyEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    heexy_categ: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct HeexyResponse {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    results: Vec<HeexyItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct HeexyItem {
    #[serde(default)]
    url: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    rawImage: String,
}

impl HeexyEngine {
    pub fn new() -> Self {
        Self::with_categ("web")
    }

    pub fn with_categ(categ: &str) -> Self {
        let categ = if categ == "image" { "image" } else { "web" };
        let (category, description) = if categ == "image" {
            (
                EngineCategory::Images,
                "Heexy - privacy-focused image search.".to_string(),
            )
        } else {
            (
                EngineCategory::General,
                "Heexy - privacy-focused web search.".to_string(),
            )
        };
        let metadata = EngineMetadata {
            name: "heexy".to_string(),
            category,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description,
            website: Some("https://heexy.org".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Heexy HTTP client");
        HeexyEngine {
            metadata,
            client,
            heexy_categ: categ.to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://seapi.heexy.org";
        let pageno = ((query.offset / query.count.max(1)) + 1).to_string();
        let url = format!("{}/search/{}", base_url, self.heexy_categ);

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("Origin", base_url)
            .query(&[
                ("q", query.query.as_str()),
                ("page", pageno.as_str()),
                ("safe", "off"),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: HeexyResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };
        if !parsed.success {
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        for (i, item) in parsed.results.iter().enumerate() {
            if item.url.is_empty() {
                continue;
            }
            if self.heexy_categ == "image" {
                let title = if item.description.is_empty() {
                    "Image".to_string()
                } else {
                    item.description.clone()
                };
                results.push(
                    SearchResult::new(title, item.url.clone())
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Images)
                        .with_extra("img_src", serde_json::json!(item.rawImage))
                        .with_extra("thumbnail", serde_json::json!(item.image)),
                );
            } else {
                results.push(
                    SearchResult::new(item.title.clone(), item.url.clone())
                        .with_snippet(item.description.clone())
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Web),
                );
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for HeexyEngine {
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
        if self.heexy_categ == "image" {
            matches!(t, ResultType::Images | ResultType::All)
        } else {
            matches!(t, ResultType::Web | ResultType::All)
        }
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://seapi.heexy.org".into());
        s.insert("heexy_categ".into(), self.heexy_categ.clone());
        s
    }
}
