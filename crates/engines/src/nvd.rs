//! NVD (National Vulnerability Database) search engine implementation (IT; JSON)
//!
//! Queries the NIST NUDP JSON service and
//! returns CVE records.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// NVD vulnerability search engine
pub struct NvdEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

const RESULTS_PER_PAGE: i64 = 10;

#[derive(Debug, Serialize, Deserialize, Default)]
struct NvdResponse {
    #[serde(default)]
    response: Vec<NvdResponseBlock>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NvdResponseBlock {
    #[serde(default)]
    grid: NvdGrid,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NvdGrid {
    #[serde(default)]
    vulnerabilities: Vec<NvdVuln>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NvdVuln {
    #[serde(default)]
    cve: NvdCve,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NvdCve {
    #[serde(default)]
    id: String,
    #[serde(default)]
    descriptions: Vec<NvdDescription>,
    #[serde(default)]
    published: String,
    #[serde(default)]
    metrics: NvdMetrics,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NvdDescription {
    #[serde(default)]
    value: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NvdMetrics {
    #[serde(default, rename = "cvssMetricV31")]
    cvss_metric_v31: Vec<NvdCvssMetric>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NvdCvssMetric {
    #[serde(default, rename = "cvssData")]
    cvss_data: NvdCvssData,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NvdCvssData {
    #[serde(default, rename = "baseSeverity")]
    base_severity: String,
    #[serde(default, rename = "baseScore")]
    base_score: Option<f64>,
}

impl NvdEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "nvd".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "NVD - National Vulnerability Database (NIST CVEs).".to_string(),
            website: Some("https://nvd.nist.gov".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create NVD HTTP client");

        NvdEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://nvd.nist.gov/extensions/nudp/services/json/nvd/cve/search/results";
        let offset = (query.offset as i64).to_string();
        let row_count = RESULTS_PER_PAGE.to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .header("Referer", "https://nvd.nist.gov/vuln/search")
            .query(&[
                ("resultType", "records"),
                ("keyword", query.query.as_str()),
                ("rowCount", row_count.as_str()),
                ("offset", offset.as_str()),
            ])
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: NvdResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        let vulnerabilities = parsed
            .response
            .into_iter()
            .next()
            .map(|b| b.grid.vulnerabilities)
            .unwrap_or_default();
        for (i, vuln) in vulnerabilities.iter().enumerate() {
            let cve = &vuln.cve;
            let cve_id = if cve.id.is_empty() {
                "CVE".to_string()
            } else {
                cve.id.clone()
            };
            let detail_url = format!("https://nvd.nist.gov/vuln/detail/{}", cve_id);

            let description = cve
                .descriptions
                .first()
                .map(|d| d.value.clone())
                .unwrap_or_default();

            let info = cve.metrics.cvss_metric_v31.first().map(|m| &m.cvss_data);
            let severity = info.map(|i| i.base_severity.clone()).unwrap_or_default();
            let cvss_score = info.and_then(|i| i.base_score);

            let published_str = format!("Published: {}", cve.published);
            let metadata_str = match (severity.as_str(), cvss_score) {
                (s, Some(score)) if !s.is_empty() => format!("Severity: {} | CVSS Score: {}", s, score),
                (_, Some(score)) => format!("CVSS Score: {}", score),
                _ => String::new(),
            };

            let mut snippet_parts: Vec<&str> = Vec::new();
            if !metadata_str.is_empty() {
                snippet_parts.push(&metadata_str);
            }
            if !description.is_empty() {
                snippet_parts.push(&description);
            }
            if !published_str.is_empty() {
                snippet_parts.push(&published_str);
            }

            results.push(
                SearchResult::new(&cve_id, &detail_url)
                    .with_snippet(snippet_parts.join(" | "))
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::IT)
                    .with_extra("published", serde_json::json!(cve.published))
                    .with_extra("severity", serde_json::json!(severity))
                    .with_extra("cvss_score", serde_json::json!(cvss_score)),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for NvdEngine {
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
        s.insert("base_url".to_string(), "https://nvd.nist.gov".to_string());
        s.insert("results_per_page".to_string(), RESULTS_PER_PAGE.to_string());
        s
    }
}
