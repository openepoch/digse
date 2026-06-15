//! ScanR Structures search engine implementation
//!
//! ScanR is a
//! French research-structures search service exposing a POST JSON endpoint.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// ScanR Structures search engine (science / academic, POST JSON)
pub struct ScanrStructuresEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

impl ScanrStructuresEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "scanr_structures".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "ScanR - French research structures search.".to_string(),
            website: Some("https://scanr.enseignementsup-recherche.gouv.fr".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create ScanR HTTP client");
        ScanrStructuresEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let base_url = "https://scanr.enseignementsup-recherche.gouv.fr";
        let url = format!("{}/api/structures/search", base_url);
        let page = (query.offset / 20) + 1;
        let body = serde_json::json!({
            "query": query.query.as_str(),
            "searchField": "ALL",
            "sortDirection": "ASC",
            "sortOrder": "RELEVANCY",
            "page": page,
            "pageSize": 20,
        });
        let resp = self
            .client
            .post(&url)
            .header("User-Agent", "digse/0.1.0")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
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
        let root: ScanrResponse = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(_) => return Ok(vec![]),
        };
        let total = root.total.unwrap_or(0);
        if total < 1 {
            return Ok(vec![]);
        }
        let mut results = Vec::new();
        for result in root.results.iter().flatten() {
            let id = match &result.id {
                Some(i) if !i.is_empty() => i.clone(),
                _ => continue,
            };
            let url = format!("{}/structure/{}", base_url, id);
            let title = result.label.clone().unwrap_or_default();
            // thumbnail from optional logo
            let thumbnail = result.logo.as_ref().map(|l| {
                if l.starts_with('/') {
                    format!("{}{}", base_url, l)
                } else {
                    l.clone()
                }
            });
            // snippet from first highlight value
            let content = result
                .highlights
                .as_ref()
                .and_then(|h| h.first())
                .and_then(|h| h.value.clone())
                .unwrap_or_default();
            let mut r = SearchResult::new(title, url)
                .with_snippet(strip_html(&content))
                .with_engine(self.name())
                .with_result_type(ResultType::Academic);
            if let Some(t) = thumbnail {
                r = r.with_extra("thumbnail", serde_json::json!(t));
            }
            results.push(r);
        }
        Ok(results)
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[derive(Debug, Serialize, Deserialize)]
struct ScanrResponse {
    #[serde(default)]
    total: Option<i64>,
    #[serde(default)]
    results: Option<Vec<ScanrStructure>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ScanrStructure {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    logo: Option<String>,
    #[serde(default)]
    highlights: Option<Vec<ScanrHighlight>>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ScanrHighlight {
    #[serde(default)]
    value: Option<String>,
}

#[async_trait]
impl Engine for ScanrStructuresEngine {
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
            r.score = 1.0 - (i as f64 * 0.03);
        }
        Ok(results)
    }
    fn supports_result_type(&self, t: &ResultType) -> bool {
        matches!(t, ResultType::Academic | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert(
            "base_url".into(),
            "https://scanr.enseignementsup-recherche.gouv.fr".into(),
        );
        s.insert("page_size".into(), "20".into());
        s
    }
}
