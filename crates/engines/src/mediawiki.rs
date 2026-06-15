//! MediaWiki (generic) search engine implementation
//!
//! queries any MediaWiki wiki via the
//! MediaWiki Action API (`action=query&list=search`). The base URL is
//! configurable (defaults to English Wikipedia). Category: general.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// MediaWiki (generic) search engine
pub struct MediaWikiEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    api_path: String,
    language: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MediaWikiResponse {
    #[serde(default)]
    query: MediaWikiQuery,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MediaWikiQuery {
    #[serde(default)]
    search: Vec<MediaWikiArticle>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MediaWikiArticle {
    #[serde(default)]
    title: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    sectiontitle: Option<String>,
    #[serde(default)]
    categorysnippet: Option<String>,
    #[serde(default)]
    timestamp: String,
}

impl MediaWikiEngine {
    pub fn new() -> Self {
        let language = std::env::var("MEDIAWIKI_LANGUAGE").unwrap_or_else(|_| "en".to_string());
        let base_url = std::env::var("MEDIAWIKI_BASE_URL")
            .unwrap_or_else(|_| format!("https://{}.wikipedia.org/", language));
        let metadata = EngineMetadata {
            name: "mediawiki".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "MediaWiki - generic wiki search via the Action API.".to_string(),
            website: Some(base_url.clone()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create MediaWiki HTTP client");

        MediaWikiEngine {
            metadata,
            client,
            base_url,
            api_path: "w/api.php".to_string(),
            language,
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
        out.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string()
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let lang = query
            .language
            .as_ref()
            .map(|l| l.split('-').next().unwrap_or(l).to_string())
            .unwrap_or_else(|| self.language.clone());
        let lang_for_url = if lang.is_empty() {
            self.language.clone()
        } else {
            lang
        };

        let base = self
            .base_url
            .replace("{language}", &lang_for_url)
            .trim_end_matches('/')
            .to_string();
        let api_url = format!("{}/{}?", base, self.api_path);

        let srlimit = query.count.to_string();
        let sroffset = query.offset.to_string();
        let response = self
            .client
            .get(&api_url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("action", "query"),
                ("list", "search"),
                ("format", "json"),
                ("srsearch", query.query.as_str()),
                ("sroffset", sroffset.as_str()),
                ("srlimit", srlimit.as_str()),
                ("srwhat", "nearmatch"),
                (
                    "srprop",
                    "sectiontitle|snippet|timestamp|categorysnippet",
                ),
                ("srsort", "relevance"),
                ("srenablerewrites", "1"),
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
        let parsed: MediaWikiResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, article) in parsed.query.search.iter().enumerate() {
            if article.snippet.starts_with("#REDIRECT") {
                continue;
            }
            let mut title = article.title.clone();
            let mut url = format!(
                "{}/wiki/{}",
                base,
                urlencoding::encode(&article.title.replace(' ', "_"))
            );
            if let Some(section) = &article.sectiontitle {
                if !section.is_empty() {
                    url.push('#');
                    url.push_str(&urlencoding::encode(&section.replace(' ', "_")));
                    title.push_str(" / ");
                    title.push_str(section);
                }
            }
            let content = Self::strip_html(&article.snippet);
            let metadata = article
                .categorysnippet
                .as_deref()
                .map(Self::strip_html)
                .unwrap_or_default();

            let mut snippet_parts = Vec::new();
            if !content.is_empty() {
                snippet_parts.push(content);
            }
            if !metadata.is_empty() {
                snippet_parts.push(metadata);
            }

            let mut result = SearchResult::new(title, url)
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Web);
            if !snippet_parts.is_empty() {
                result = result.with_snippet(snippet_parts.join(" | "));
            }
            if !article.timestamp.is_empty() {
                result = result.with_extra("published", serde_json::json!(article.timestamp));
            }
            result = result.with_extra("language", serde_json::json!(lang_for_url));
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for MediaWikiEngine {
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
        matches!(result_type, ResultType::Web | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("api_path".to_string(), self.api_path.clone());
        settings.insert("language".to_string(), self.language.clone());
        settings
    }
}
