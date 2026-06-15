//! Repology search engine implementation
//!
//! The Repology API
//! returns a map of project-name -> list of repository entries; we collapse
//! each project to one result using the most-common fields.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Repology search engine (IT / package repositories, JSON API)
pub struct RepologyEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl RepologyEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "repology".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Repology - cross-distro package repository monitor.".to_string(),
            website: Some("https://repology.org".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Repology HTTP client");
        RepologyEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://repology.org";
        let url = format!("{}/api/v1/projects/", base_url);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[("search", query.query.as_str())])
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
        // The response is a JSON object: { pkgname: [repo_entries...] }
        let projects: HashMap<String, Vec<RepoEntry>> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };
        let mut results = Vec::new();
        for (pkgname, repos) in projects.into_iter() {
            if repos.is_empty() {
                continue;
            }
            let latest_version = repos
                .iter()
                .find(|r| r.status.as_deref() == Some("newest"))
                .and_then(|r| r.version.clone())
                .or_else(|| most_common(repos.iter().filter_map(|r| r.version.as_ref())));
            let summary = most_common(repos.iter().filter_map(|r| r.summary.as_ref()));
            let visiblename = most_common(repos.iter().filter_map(|r| r.visiblename.as_ref()));
            let licenses = most_common(repos.iter().flat_map(|r| r.licenses.iter().flatten()));
            let tags: Vec<String> = repos
                .iter()
                .filter_map(|r| r.repo.clone())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let url = format!("{}/project/{}/versions", base_url, pkgname);
            let title = visiblename.clone().unwrap_or_else(|| pkgname.clone());
            let mut snippet_parts = Vec::new();
            if let Some(v) = &latest_version {
                snippet_parts.push(format!("Version: {}", v));
            }
            if let Some(s) = &summary {
                snippet_parts.push(s.clone());
            }
            if !tags.is_empty() {
                snippet_parts.push(format!("Repos: {}", tags.join(", ")));
            }
            results.push(
                SearchResult::new(title, url)
                    .with_snippet(snippet_parts.join(" | "))
                    .with_engine(self.name())
                    .with_result_type(ResultType::IT)
                    .with_extra("package_name", serde_json::json!(pkgname))
                    .with_extra("version", serde_json::json!(latest_version.unwrap_or_default()))
                    .with_extra("license", serde_json::json!(licenses.unwrap_or_default()))
                    .with_extra("tags", serde_json::json!(tags)),
            );
        }
        Ok(results)
    }
}

fn most_common<'a, I: Iterator<Item = &'a String>>(mut iter: I) -> Option<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut best: Option<(String, usize)> = None;
    for item in iter.by_ref() {
        let c = counts.entry(item.clone()).or_insert(0);
        *c += 1;
        match &best {
            Some((_, n)) if *c <= *n => {}
            _ => best = Some((item.clone(), *c)),
        }
    }
    best.map(|(s, _)| s)
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RepoEntry {
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    visiblename: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    licenses: Option<Vec<String>>,
}

#[async_trait]
impl Engine for RepologyEngine {
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
        let mut results = self.fetch_results(query).await?;
        for (i, r) in results.iter_mut().enumerate() {
            r.rank = query.offset + i + 1;
            r.score = 1.0 - (i as f64 * 0.05);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::IT | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://repology.org".into());
        s.insert("api_version".into(), "v1".into());
        s
    }
}
