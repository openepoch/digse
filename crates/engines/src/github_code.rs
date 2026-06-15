//! GitHub Code search engine implementation
//!
//! Uses the GitHub REST API
//! `https://api.github.com/search/code?sort=indexed&q=...&page=N`. The API
//! supports anonymous requests (heavily rate-limited) by sending a placeholder
//! Authorization header; a personal access token / bearer token may be supplied
//! via `GITHUB_TOKEN` for higher limits. Category: it (code search).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// GitHub code search engine (REST API)
pub struct GitHubCodeEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubCodeResponse {
    #[serde(default)]
    items: Vec<GitHubCodeItem>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubCodeItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    repository: GitHubCodeRepo,
    #[serde(default)]
    text_matches: Vec<GitHubTextMatch>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct GitHubCodeRepo {
    #[serde(default)]
    full_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct GitHubTextMatch {
    #[serde(default)]
    fragment: String,
}

impl GitHubCodeEngine {
    pub fn new() -> Self {
        let token = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty());
        let metadata = EngineMetadata {
            name: "github_code".to_string(),
            category: EngineCategory::IT,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "GitHub code search across repositories.".to_string(),
            website: Some("https://github.com/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create GitHub Code HTTP client");

        GitHubCodeEngine {
            metadata,
            client,
            token,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let url = "https://api.github.com/search/code";
        let page = query.offset + 1;
        let page_str = page.to_string();

        let mut req = self
            .client
            .get(url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/vnd.github.text-match+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .query(&[
                ("sort", "indexed"),
                ("q", query.query.as_str()),
                ("page", page_str.as_str()),
            ]);

        // ref: without auth a placeholder Authorization is sent (anonymous,
        // rate-limited). With a token, use `token <pat>`; bearer tokens use
        // `Bearer <token>`. We treat a single env var as a PAT-style token.
        if let Some(tok) = &self.token {
            req = req.header("Authorization", format!("token {}", tok));
        } else {
            req = req.header("Authorization", "placeholder");
        }

        let response = match req.send().await {
            Ok(r) => r,
            Err(e) => return Err(Error::HttpError(e.to_string())),
        };

        let status = response.status();
        // ref: 422 (invalid search term) -> empty; other non-success -> empty
        if status.as_u16() == 422 || !status.is_success() {
            return Ok(vec![]);
        }

        let text = response
            .text()
            .await
            .map_err(|e| Error::HttpError(e.to_string()))?;

        let parsed: GitHubCodeResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, item) in parsed.items.iter().enumerate() {
            if i >= query.count {
                break;
            }
            // ref extracts code fragments and relabels line numbers from 1
            let mut code_lines: Vec<String> = Vec::new();
            for (mi, m) in item.text_matches.iter().enumerate() {
                if mi > 0 {
                    code_lines.push("...".to_string());
                }
                for line in m.fragment.lines() {
                    code_lines.push(line.to_string());
                }
            }
            let numbered: Vec<String> = code_lines
                .iter()
                .enumerate()
                .map(|(n, line)| format!("{:>4} | {}", n + 1, line))
                .collect();
            let snippet = numbered.join("\n");
            let title = format!("{} · {}", item.repository.full_name, item.name);

            let result = SearchResult::new(title, item.html_url.clone())
                .with_snippet(if snippet.is_empty() {
                    item.repository.description.clone()
                } else {
                    snippet
                })
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::IT)
                .with_extra("filename", serde_json::json!(item.name))
                .with_extra("path", serde_json::json!(item.path))
                .with_extra("repository", serde_json::json!(item.repository.html_url))
                .with_extra("source", serde_json::json!("github_code"));
            results.push(result);
        }

        Ok(results)
    }
}

#[async_trait]
impl Engine for GitHubCodeEngine {
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

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        matches!(result_type, ResultType::IT | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), "https://api.github.com".to_string());
        settings.insert(
            "search_endpoint".to_string(),
            "/search/code".to_string(),
        );
        settings.insert("sort".to_string(), "indexed".to_string());
        settings.insert("api_version".to_string(), "2022-11-28".to_string());
        settings
    }
}
