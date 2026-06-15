//! hex.pm search engine implementation
//!
//! Searches the Elixir/Erlang package
//! registry at hex.pm. JSON API; sorted by recent downloads.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// hex.pm package search engine
pub struct HexEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct HexPackage {
    #[serde(default)]
    name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    docs_html_url: String,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    meta: HexMeta,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct HexMeta {
    #[serde(default)]
    description: String,
    #[serde(default)]
    latest_version: String,
    #[serde(default)]
    maintainers: Vec<String>,
    #[serde(default)]
    licenses: Vec<String>,
    #[serde(default)]
    links: HashMap<String, String>,
}

impl HexEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "hex".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "hex.pm - Elixir/Erlang package registry.".to_string(),
            website: Some("https://hex.pm/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create hex.pm HTTP client");

        HexEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://hex.pm/api/packages/";
        let pageno = ((query.offset / query.count.max(1)) + 1).to_string();
        let per_page = query.count.to_string();

        let response = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("page", pageno.as_str()),
                ("per_page", per_page.as_str()),
                ("sort", "recent_downloads"),
                ("search", query.query.as_str()),
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
        let packages: Vec<HexPackage> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, package) in packages.iter().enumerate() {
            if package.name.is_empty() || package.html_url.is_empty() {
                continue;
            }
            let mut snippet_parts = Vec::new();
            if !package.meta.description.is_empty() {
                snippet_parts.push(package.meta.description.clone());
            }
            if !package.meta.latest_version.is_empty() {
                snippet_parts.push(format!("Version: {}", package.meta.latest_version));
            }
            if !package.meta.maintainers.is_empty() {
                snippet_parts.push(format!(
                    "Maintainers: {}",
                    package.meta.maintainers.join(", ")
                ));
            }
            if !package.meta.licenses.is_empty() {
                snippet_parts.push(format!("License: {}", package.meta.licenses.join(", ")));
            }
            if !package.updated_at.is_empty() {
                snippet_parts.push(format!("Updated: {}", package.updated_at));
            }
            let snippet = snippet_parts.join(" | ");

            let mut result = SearchResult::new(package.name.clone(), package.html_url.clone())
                .with_snippet(snippet)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT);
            if !package.meta.latest_version.is_empty() {
                result = result.with_extra("version", serde_json::json!(package.meta.latest_version));
            }
            if !package.docs_html_url.is_empty() {
                result = result.with_extra("homepage", serde_json::json!(package.docs_html_url));
            }
            if !package.updated_at.is_empty() {
                result = result.with_extra("published", serde_json::json!(package.updated_at));
            }
            results.push(result);
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for HexEngine {
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
        matches!(t, ResultType::IT | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://hex.pm".into());
        s.insert("api_endpoint".into(), "/api/packages/".into());
        s.insert("sort".into(), "recent_downloads".into());
        s
    }
}
