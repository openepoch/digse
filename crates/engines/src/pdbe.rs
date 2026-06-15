//! PDBe search engine implementation (science; JSON via POST)
//!
//! PDBe (Protein Data Bank in Europe)
//! provides structural biology data via a Solr search endpoint.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// PDBe (Protein Data Bank in Europe) search engine
pub struct PdbeEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

/// status codes of unpublished entries (skipped in the reference impl)
const PDB_UNPUBLISHED_CODES: &[&str] = &[
    "HPUB", "HOLD", "PROC", "WAIT", "AUTH", "AUCO", "REPL", "POLC", "REFI", "TRSF", "WDRN",
];

#[derive(Debug, Serialize, Deserialize)]
struct PdbeResponse {
    #[serde(default)]
    response: PdbeResponseBody,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PdbeResponseBody {
    #[serde(default)]
    docs: Vec<PdbeDoc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PdbeDoc {
    #[serde(default)]
    pdb_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    superseded_by: String,
    #[serde(default)]
    citation_title: String,
    #[serde(default)]
    entry_author_list: Vec<String>,
    #[serde(default)]
    journal: String,
    #[serde(default)]
    journal_volume: String,
    #[serde(default)]
    journal_page: String,
    #[serde(default)]
    citation_year: Option<i32>,
    #[serde(default)]
    release_year: Option<i32>,
}

impl PdbeEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "pdbe".to_string(),
            category: EngineCategory::Science,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "PDBe - Protein Data Bank in Europe.".to_string(),
            website: Some("https://www.ebi.ac.uk/pdbe".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create PDBe HTTP client");

        PdbeEngine { metadata, client }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://www.ebi.ac.uk/pdbe/search/pdb/select?";
        let form = [("q", query.query.as_str()), ("wt", "json")];

        let resp = self
            .client
            .post(url)
            .header("User-Agent", "digse/0.1.0")
            .header("Accept", "application/json")
            .form(&form)
            .send()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        if !resp.status().is_success() {
            return Ok(vec![]);
        }
        let text = resp.text().await.map_err(|e| Error::HttpError(e.to_string()))?;
        let parsed: PdbeResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, doc) in parsed.response.docs.iter().enumerate() {
            // skip unpublished entries
            if PDB_UNPUBLISHED_CODES.contains(&doc.status.as_str()) {
                continue;
            }

            let entry_url = format!("https://www.ebi.ac.uk/pdbe/entry/pdb/{}", doc.pdb_id);
            let thumbnail = format!(
                "https://www.ebi.ac.uk/pdbe/static/entry/{}_deposited_chain_front_image-200x200.png",
                doc.pdb_id
            );

            let title = if doc.status == "OBS" {
                format!("{} (OBSOLETE)", doc.title)
            } else {
                doc.title.clone()
            };

            let content = if doc.status == "OBS" {
                if doc.superseded_by.is_empty() {
                    String::new()
                } else {
                    format!(
                        "This entry has been superseded by: https://www.ebi.ac.uk/pdbe/entry/pdb/{} ({})",
                        doc.superseded_by, doc.superseded_by
                    )
                }
            } else {
                let authors = doc.entry_author_list.first().cloned().unwrap_or_default();
                let year = doc
                    .citation_year
                    .map(|y| y.to_string())
                    .or_else(|| doc.release_year.map(|y| y.to_string()))
                    .unwrap_or_default();
                if !doc.journal.is_empty() {
                    format!(
                        "{} - {} {} ({}) ({})",
                        doc.citation_title, authors, doc.journal, doc.journal_volume, year
                    )
                } else {
                    format!("{} - {} ({})", doc.citation_title, authors, year)
                }
            };

            let year_val = doc
                .citation_year
                .or(doc.release_year);

            results.push(
                SearchResult::new(&title, &entry_url)
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.03))
                    .with_result_type(ResultType::Academic)
                    .with_extra("thumbnail", serde_json::json!(thumbnail))
                    .with_extra("pdb_id", serde_json::json!(doc.pdb_id))
                    .with_extra("year", serde_json::json!(year_val))
                    .with_extra(
                        "authors",
                        serde_json::json!(doc.entry_author_list),
                    ),
            );
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for PdbeEngine {
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
        s.insert(
            "solr_url".to_string(),
            "https://www.ebi.ac.uk/pdbe/search/pdb/select?".to_string(),
        );
        s
    }
}
