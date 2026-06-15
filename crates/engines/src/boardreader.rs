//! Boardreader (forum search) engine implementation.
//! JSON search via return.php.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Boardreader forum search engine.
pub struct BoardreaderEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize)]
struct BoardreaderResponse {
    #[serde(default)]
    #[serde(rename = "SearchResults")]
    search_results: Vec<BoardreaderItem>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct BoardreaderItem {
    #[serde(default)]
    #[serde(rename = "Subject")]
    subject: String,
    #[serde(default)]
    #[serde(rename = "Text")]
    text: String,
    #[serde(default)]
    #[serde(rename = "Url")]
    url: String,
    #[serde(default)]
    #[serde(rename = "Published")]
    published: String,
    #[serde(default)]
    #[serde(rename = "Author")]
    author: String,
}

impl BoardreaderEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "boardreader".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Boardreader - forum and social media search.".to_string(),
            website: Some("https://boardreader.com".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Boardreader HTTP client");
        BoardreaderEngine { metadata, client }
    }

    /// Fetch the boardreader session id by scraping the home page JS variable.
    async fn get_session_id(&self) -> Option<String> {
        let resp = self
            .client
            .get("https://boardreader.com")
            .header("User-Agent", "digse/0.0.1")
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let text = resp.text().await.ok()?;
        extract_between(&text, "'currentSessionId', '", "'").map(|s| s.to_string())
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let session_id = match self.get_session_id().await {
            Some(s) => s,
            None => return Ok(vec![]),
        };
        let url = "https://boardreader.com/return.php";
        let page = (query.offset + 1).to_string();

        let resp = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("query", query.query.as_str()),
                ("page", page.as_str()),
                ("language", "All"),
                ("session_id", session_id.as_str()),
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
        let parsed: BoardreaderResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.search_results.iter().enumerate() {
            if item.url.is_empty() {
                continue;
            }
            let snippet = format!(
                "{}{}",
                if item.author.is_empty() {
                    String::new()
                } else {
                    format!("Posted by {} | ", item.author)
                },
                strip_keyword_markers(&item.text)
            );
            results.push(
                SearchResult::new(strip_keyword_markers(&item.subject), item.url.clone())
                    .with_snippet(snippet)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Web)
                    .with_extra("published", serde_json::json!(item.published))
                    .with_extra("author", serde_json::json!(item.author))
                    .with_extra("source", serde_json::json!("boardreader")),
            );
        }
        Ok(results)
    }
}

fn extract_between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let s_pos = s.find(start)? + start.len();
    let rest = &s[s_pos..];
    let end_pos = rest.find(end)?;
    Some(&rest[..end_pos])
}

fn strip_keyword_markers(s: &str) -> String {
    s.replace("[Keyword]", "").replace("[/Keyword]", "")
}

#[async_trait]
impl Engine for BoardreaderEngine {
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
        matches!(t, ResultType::Web | ResultType::Social | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), "https://boardreader.com".into());
        s
    }
}
