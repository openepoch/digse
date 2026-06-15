//! CachyOS packages search engine implementation.
//! Arch Linux (CachyOS) package search, JSON API.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// CachyOS package search engine.
pub struct CachyOsEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachyResponse {
    #[serde(default)]
    packages: Vec<CachyPackage>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct CachyPackage {
    #[serde(default)]
    pkg_name: String,
    #[serde(default)]
    pkg_arch: String,
    #[serde(default)]
    repo_name: String,
    #[serde(default)]
    pkg_version: String,
    #[serde(default)]
    pkg_desc: String,
    #[serde(default)]
    pkg_builddate: i64,
}

impl CachyOsEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "cachy_os".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "CachyOS - Arch Linux package search.".to_string(),
            website: Some("https://cachyos.org".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create CachyOS HTTP client");
        CachyOsEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://packages.cachyos.org/api/search";
        let page_size = "15".to_string();
        let current_page = (query.offset + 1).to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .query(&[
                ("search", query.query.as_str()),
                ("page_size", page_size.as_str()),
                ("current_page", current_page.as_str()),
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
        let parsed: CachyResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.packages.iter().enumerate() {
            let url = format!(
                "https://packages.cachyos.org/package/{repo}/{arch}/{name}",
                repo = item.repo_name,
                arch = item.pkg_arch,
                name = item.pkg_name,
            );
            let title = format!("{} ({})", item.pkg_name, item.repo_name);
            let published = item.pkg_builddate.to_string();
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(item.pkg_desc.clone())
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::IT)
                    .with_extra("version", serde_json::json!(item.pkg_version))
                    .with_extra("published", serde_json::json!(published))
                    .with_extra("arch", serde_json::json!(item.pkg_arch))
                    .with_extra("repository", serde_json::json!(item.repo_name)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for CachyOsEngine {
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
        matches!(t, ResultType::IT | ResultType::Files | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "base_url".into(),
            "https://packages.cachyos.org/api/search".into(),
        );
        s
    }
}
