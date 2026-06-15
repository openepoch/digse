//! Search result types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Result type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResultType {
    Web,
    Images,
    Videos,
    Music,
    News,
    Files,
    Torrents,
    Academic,
    IT,
    Social,
    Maps,
    Shopping,
    Weather,
    All,
}

impl ResultType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResultType::Web => "web",
            ResultType::Images => "images",
            ResultType::Videos => "videos",
            ResultType::Music => "music",
            ResultType::News => "news",
            ResultType::Files => "files",
            ResultType::Torrents => "torrents",
            ResultType::Academic => "academic",
            ResultType::IT => "it",
            ResultType::Social => "social",
            ResultType::Maps => "maps",
            ResultType::Shopping => "shopping",
            ResultType::Weather => "weather",
            ResultType::All => "all",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "web" => Some(ResultType::Web),
            "images" => Some(ResultType::Images),
            "videos" => Some(ResultType::Videos),
            "music" => Some(ResultType::Music),
            "news" => Some(ResultType::News),
            "files" => Some(ResultType::Files),
            "torrents" => Some(ResultType::Torrents),
            "academic" => Some(ResultType::Academic),
            "it" => Some(ResultType::IT),
            "social" => Some(ResultType::Social),
            "maps" => Some(ResultType::Maps),
            "shopping" => Some(ResultType::Shopping),
            "weather" => Some(ResultType::Weather),
            "all" => Some(ResultType::All),
            _ => None,
        }
    }
}

/// Search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: Option<String>,
    pub engine: String,
    pub score: f64,
    pub rank: usize,
    pub result_type: ResultType,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl SearchResult {
    /// Create a new web search result
    pub fn new(title: impl Into<String>, url: impl Into<String>) -> Self {
        SearchResult {
            title: title.into(),
            url: url.into(),
            snippet: None,
            engine: String::new(),
            score: 1.0,
            rank: 0,
            result_type: ResultType::Web,
            extra: HashMap::new(),
        }
    }

    /// Set snippet
    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = Some(snippet.into());
        self
    }

    /// Set engine
    pub fn with_engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = engine.into();
        self
    }

    /// Set score
    pub fn with_score(mut self, score: f64) -> Self {
        self.score = score;
        self
    }

    /// Set rank
    pub fn with_rank(mut self, rank: usize) -> Self {
        self.rank = rank;
        self
    }

    /// Set result type (images, videos, files, etc.)
    pub fn with_result_type(mut self, result_type: ResultType) -> Self {
        self.result_type = result_type;
        self
    }

    /// Add extra field
    pub fn with_extra(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra.insert(key.into(), value);
        self
    }
}

/// Result builder
pub struct SearchResultBuilder {
    title: Option<String>,
    url: Option<String>,
    snippet: Option<String>,
    engine: String,
    score: f64,
    rank: usize,
    result_type: ResultType,
    extra: HashMap<String, serde_json::Value>,
}

impl SearchResultBuilder {
    pub fn new() -> Self {
        SearchResultBuilder {
            title: None,
            url: None,
            snippet: None,
            engine: String::new(),
            score: 1.0,
            rank: 0,
            result_type: ResultType::Web,
            extra: HashMap::new(),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = Some(snippet.into());
        self
    }

    pub fn engine(mut self, engine: impl Into<String>) -> Self {
        self.engine = engine.into();
        self
    }

    pub fn score(mut self, score: f64) -> Self {
        self.score = score;
        self
    }

    pub fn rank(mut self, rank: usize) -> Self {
        self.rank = rank;
        self
    }

    pub fn result_type(mut self, result_type: ResultType) -> Self {
        self.result_type = result_type;
        self
    }

    pub fn extra(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.extra.insert(key.into(), value);
        self
    }

    pub fn build(self) -> Result<SearchResult, crate::Error> {
        let title = self.title.ok_or_else(|| {
            crate::Error::ParseError("Title is required".to_string())
        })?;
        let url = self.url.ok_or_else(|| {
            crate::Error::ParseError("URL is required".to_string())
        })?;

        Ok(SearchResult {
            title,
            url,
            snippet: self.snippet,
            engine: self.engine,
            score: self.score,
            rank: self.rank,
            result_type: self.result_type,
            extra: self.extra,
        })
    }
}

impl Default for SearchResultBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub result_type: ResultType,
    pub total_results: usize,
    pub engines_used: Vec<String>,
    pub search_duration_ms: Option<u64>,
    pub engine_stats: Vec<EngineStat>,
    pub results: Vec<SearchResult>,
}

/// Engine statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStat {
    pub engine: String,
    pub results_count: usize,
    pub duration_ms: u64,
    pub status: EngineStatus,
}

/// Engine status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineStatus {
    Success,
    Partial,
    Failed,
    Timeout,
    RateLimited,
}

impl EngineStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EngineStatus::Success => "success",
            EngineStatus::Partial => "partial",
            EngineStatus::Failed => "failed",
            EngineStatus::Timeout => "timeout",
            EngineStatus::RateLimited => "rate_limited",
        }
    }
}