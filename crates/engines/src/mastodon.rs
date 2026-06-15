//! Mastodon search engine implementation
//!
//! queries a Mastodon instance's v2
//! search API for accounts. Category: social media.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Mastodon (social media) search engine
pub struct MastodonEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    mastodon_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MastodonResponse {
    #[serde(default)]
    accounts: Vec<MastodonAccount>,
    #[serde(default)]
    hashtags: Vec<MastodonHashtag>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MastodonAccount {
    #[serde(default)]
    uri: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    avatar: Option<String>,
    #[serde(default)]
    followers_count: i64,
    #[serde(default)]
    created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MastodonHashtag {
    #[serde(default)]
    url: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    history: Vec<MastodonHistory>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct MastodonHistory {
    #[serde(default)]
    uses: String,
    #[serde(default)]
    accounts: String,
}

impl MastodonEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "mastodon".to_string(),
            category: EngineCategory::Social,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: "Mastodon - federated social network search.".to_string(),
            website: Some("https://joinmastodon.org/".to_string()),
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Mastodon HTTP client");

        MastodonEngine {
            metadata,
            client,
            base_url: "https://mastodon.social".to_string(),
            mastodon_type: "accounts".to_string(),
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        if query.query.is_empty() {
            return Ok(vec![]);
        }
        let limit_str = "40".to_string();
        let url = format!("{}/api/v2/search", self.base_url);
        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
            .query(&[
                ("q", query.query.as_str()),
                ("resolve", "false"),
                ("type", self.mastodon_type.as_str()),
                ("limit", limit_str.as_str()),
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
        let parsed: MastodonResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        for (i, acct) in parsed.accounts.iter().enumerate() {
            if acct.uri.is_empty() {
                continue;
            }
            let title = format!("{} ({} followers)", acct.username, acct.followers_count);
            let created = if acct.created_at.len() >= 10 {
                acct.created_at[..10].to_string()
            } else {
                acct.created_at.clone()
            };
            let mut result = SearchResult::new(title, acct.uri.clone())
                .with_snippet(acct.note.clone())
                .with_engine(self.name())
                .with_rank(query.offset + i + 1)
                .with_score(1.0 - (i as f64 * 0.05))
                .with_result_type(ResultType::Social)
                .with_extra("author", serde_json::json!(acct.username));
            if let Some(av) = &acct.avatar {
                if !av.is_empty() {
                    result = result.with_extra("thumbnail", serde_json::json!(av));
                }
            }
            if !created.is_empty() {
                result = result.with_extra("published", serde_json::json!(created));
            }
            results.push(result);
            if results.len() >= query.count {
                break;
            }
        }
        if results.is_empty() {
            for (i, tag) in parsed.hashtags.iter().enumerate() {
                if tag.url.is_empty() {
                    continue;
                }
                let uses_count: i64 = tag
                    .history
                    .iter()
                    .filter_map(|h| h.uses.parse::<i64>().ok())
                    .sum();
                let user_count: i64 = tag
                    .history
                    .iter()
                    .filter_map(|h| h.accounts.parse::<i64>().ok())
                    .sum();
                let content = format!(
                    "Hashtag has been used {} times by {} different users",
                    uses_count, user_count
                );
                let result = SearchResult::new(tag.name.clone(), tag.url.clone())
                    .with_snippet(content)
                    .with_engine(self.name())
                    .with_rank(query.offset + i + 1)
                    .with_score(1.0 - (i as f64 * 0.05))
                    .with_result_type(ResultType::Social);
                results.push(result);
                if results.len() >= query.count {
                    break;
                }
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl Engine for MastodonEngine {
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
        matches!(result_type, ResultType::Social | ResultType::All)
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("base_url".to_string(), self.base_url.clone());
        settings.insert("mastodon_type".to_string(), self.mastodon_type.clone());
        settings.insert("page_size".to_string(), "40".to_string());
        settings
    }
}
