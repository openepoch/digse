//! Lemmy search engine implementation
//!
//! Federated social platform JSON API.
//! `lemmy_type` selects Communities / Users / Posts / Comments.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Error, Result, SearchQuery, SearchResult, ResultType,
};

/// Lemmy (federated social) search engine
pub struct LemmyEngine {
    metadata: EngineMetadata,
    client: reqwest::Client,
    base_url: String,
    lemmy_type: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyResponse {
    #[serde(default)]
    communities: Vec<LemmyCommunityView>,
    #[serde(default)]
    users: Vec<LemmyPersonView>,
    #[serde(default)]
    posts: Vec<LemmyPostView>,
    #[serde(default)]
    comments: Vec<LemmyCommentView>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyCommunityView {
    #[serde(default)]
    community: LemmyCommunity,
    #[serde(default)]
    counts: LemmyCommunityCounts,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyCommunity {
    #[serde(default)]
    actor_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    banner: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyCommunityCounts {
    #[serde(default)]
    subscribers: i64,
    #[serde(default)]
    posts: i64,
    #[serde(default)]
    users_active_half_year: i64,
    #[serde(default)]
    published: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyPersonView {
    #[serde(default)]
    person: LemmyPerson,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyPerson {
    #[serde(default)]
    actor_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    bio: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyPostView {
    #[serde(default)]
    post: LemmyPost,
    #[serde(default)]
    counts: LemmyPostCounts,
    #[serde(default)]
    creator: LemmyPerson,
    #[serde(default)]
    community: LemmyCommunity,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyPost {
    #[serde(default)]
    ap_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    thumbnail_url: Option<String>,
    #[serde(default)]
    published: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyPostCounts {
    #[serde(default)]
    upvotes: i64,
    #[serde(default)]
    downvotes: i64,
    #[serde(default)]
    comments: i64,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyCommentView {
    #[serde(default)]
    comment: LemmyComment,
    #[serde(default)]
    counts: LemmyPostCounts,
    #[serde(default)]
    creator: LemmyPerson,
    #[serde(default)]
    community: LemmyCommunity,
    #[serde(default)]
    post: LemmyPost,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct LemmyComment {
    #[serde(default)]
    ap_id: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    published: String,
}

impl LemmyEngine {
    pub fn new() -> Self {
        Self::with_type("https://lemmy.ml/", "Posts")
    }

    pub fn with_type(base_url: &str, lemmy_type: &str) -> Self {
        let lemmy_type = match lemmy_type {
            "Communities" | "Users" | "Posts" | "Comments" => lemmy_type.to_string(),
            _ => "Posts".to_string(),
        };
        let metadata = EngineMetadata {
            name: "lemmy".to_string(),
            category: EngineCategory::Social,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 15,
            description: format!("Lemmy {} - federated social platform.", lemmy_type),
            website: Some("https://lemmy.ml/".to_string()),
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create Lemmy HTTP client");
        LemmyEngine {
            metadata,
            client,
            base_url: base_url.trim_end_matches('/').to_string() + "/",
            lemmy_type,
        }
    }

    async fn fetch_results(&self, query: &SearchQuery) -> Result<Vec<SearchResult>> {
        let pageno = (query.offset / query.count.max(1)) + 1;
        let url = format!(
            "{}api/v3/search?q={}&page={}&type_={}",
            self.base_url,
            urlencoding::encode(&query.query),
            pageno,
            self.lemmy_type
        );

        let response = self
            .client
            .get(&url)
            .header("User-Agent", "digse/0.0.1")
            .header("Accept", "application/json")
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
        let parsed: LemmyResponse = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Ok(vec![]),
        };

        let mut results = Vec::new();
        match self.lemmy_type.as_str() {
            "Communities" => {
                for (i, view) in parsed.communities.iter().enumerate() {
                    if i >= query.count {
                        break;
                    }
                    let c = &view.community;
                    if c.actor_id.is_empty() || c.title.is_empty() {
                        continue;
                    }
                    let metadata = format!(
                        "subscribers: {} | posts: {} | active users: {}",
                        view.counts.subscribers, view.counts.posts, view.counts.users_active_half_year
                    );
                    let snippet = if c.description.is_empty() {
                        metadata.clone()
                    } else {
                        format!("{} | {}", strip_markdown(&c.description), metadata)
                    };
                    let thumb = c.icon.clone().or_else(|| c.banner.clone()).unwrap_or_default();
                    let mut result = SearchResult::new(c.title.clone(), c.actor_id.clone())
                        .with_snippet(snippet)
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Social);
                    if !thumb.is_empty() {
                        result = result.with_extra("thumbnail", serde_json::json!(thumb));
                    }
                    if !view.counts.published.is_empty() {
                        result = result.with_extra("published", serde_json::json!(view.counts.published));
                    }
                    results.push(result);
                }
            }
            "Users" => {
                for (i, view) in parsed.users.iter().enumerate() {
                    if i >= query.count {
                        break;
                    }
                    let p = &view.person;
                    if p.actor_id.is_empty() {
                        continue;
                    }
                    let result = SearchResult::new(p.name.clone(), p.actor_id.clone())
                        .with_snippet(strip_markdown(&p.bio))
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Social);
                    results.push(result);
                }
            }
            "Posts" => {
                for (i, view) in parsed.posts.iter().enumerate() {
                    if i >= query.count {
                        break;
                    }
                    let p = &view.post;
                    if p.ap_id.is_empty() || p.name.is_empty() {
                        continue;
                    }
                    let user = if view.creator.display_name_or_name().is_empty() {
                        view.creator.name.clone()
                    } else {
                        view.creator.display_name_or_name().to_string()
                    };
                    let metadata = format!(
                        "▲ {} ▼ {} | user: {} | comments: {} | community: {}",
                        view.counts.upvotes,
                        view.counts.downvotes,
                        user,
                        view.counts.comments,
                        view.community.title
                    );
                    let snippet = if p.body.trim().is_empty() {
                        metadata.clone()
                    } else {
                        format!("{} | {}", strip_markdown(p.body.trim()), metadata)
                    };
                    let mut result = SearchResult::new(p.name.clone(), p.ap_id.clone())
                        .with_snippet(snippet)
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Social);
                    if let Some(thumb) = &p.thumbnail_url {
                        if !thumb.is_empty() {
                            let full = format!("{}?format=webp&thumbnail=208", thumb);
                            result = result.with_extra("thumbnail", serde_json::json!(full));
                        }
                    }
                    if !p.published.is_empty() {
                        result = result.with_extra("published", serde_json::json!(p.published));
                    }
                    results.push(result);
                }
            }
            "Comments" => {
                for (i, view) in parsed.comments.iter().enumerate() {
                    if i >= query.count {
                        break;
                    }
                    let c = &view.comment;
                    if c.ap_id.is_empty() {
                        continue;
                    }
                    let user = if view.creator.display_name_or_name().is_empty() {
                        view.creator.name.clone()
                    } else {
                        view.creator.display_name_or_name().to_string()
                    };
                    let metadata = format!(
                        "▲ {} ▼ {} | user: {} | community: {}",
                        view.counts.upvotes, view.counts.downvotes, user, view.community.title
                    );
                    let snippet = format!("{} | {}", strip_markdown(&c.content), metadata);
                    let result = SearchResult::new(view.post.name.clone(), c.ap_id.clone())
                        .with_snippet(snippet)
                        .with_engine(self.name())
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.05))
                        .with_result_type(ResultType::Social)
                        .with_extra("published", serde_json::json!(c.published));
                    results.push(result);
                }
            }
            _ => {}
        }

        Ok(results)
    }
}

impl LemmyPerson {
    /// Mirrors Python: `display_name` if present, else `name`.
    fn display_name_or_name(&self) -> &str {
        if !self.display_name.is_empty() {
            &self.display_name
        } else {
            &self.name
        }
    }
}

/// Crude markdown-to-text: strip common markup characters.
fn strip_markdown(s: &str) -> String {
    s.replace("**", "")
        .replace("__", "")
        .replace('*', "")
        .replace('_', "")
        .replace('`', "")
        .replace("##", "")
        .replace("#", "")
        .trim()
        .to_string()
}

#[async_trait]
impl Engine for LemmyEngine {
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
        matches!(t, ResultType::Social | ResultType::All)
    }
    fn settings(&self) -> HashMap<String, String> {
        let mut s = HashMap::new();
        s.insert("base_url".into(), self.base_url.clone());
        s.insert("lemmy_type".into(), self.lemmy_type.clone());
        s
    }
}
