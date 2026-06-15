//! Search query types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::result::ResultType;

/// Search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub result_type: ResultType,
    pub count: usize,
    pub offset: usize,
    pub timeout_seconds: u64,
    pub language: Option<String>,
    pub time_range: Option<TimeRange>,
    pub safe_search: bool,
    pub engine_params: HashMap<String, String>,
}

impl SearchQuery {
    /// Create a new search query
    pub fn new(query: impl Into<String>) -> Self {
        SearchQuery {
            query: query.into(),
            result_type: ResultType::Web,
            count: 10,
            offset: 0,
            timeout_seconds: 5,
            language: None,
            time_range: None,
            safe_search: false,
            engine_params: HashMap::new(),
        }
    }

    /// Set result type
    pub fn with_result_type(mut self, result_type: ResultType) -> Self {
        self.result_type = result_type;
        self
    }

    /// Set result count
    pub fn with_count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    /// Set offset
    pub fn with_offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    /// Set timeout
    pub fn with_timeout(mut self, timeout: u64) -> Self {
        self.timeout_seconds = timeout;
        self
    }

    /// Set language
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Set time range
    pub fn with_time_range(mut self, time_range: TimeRange) -> Self {
        self.time_range = Some(time_range);
        self
    }

    /// Set safe search
    pub fn with_safe_search(mut self, safe_search: bool) -> Self {
        self.safe_search = safe_search;
        self
    }

    /// Add engine-specific parameter
    pub fn with_engine_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.engine_params.insert(key.into(), value.into());
        self
    }
}

/// Query builder
pub struct QueryBuilder {
    query: Option<String>,
    result_type: ResultType,
    count: usize,
    offset: usize,
    timeout_seconds: u64,
    language: Option<String>,
    time_range: Option<TimeRange>,
    safe_search: bool,
    engine_params: HashMap<String, String>,
}

impl QueryBuilder {
    pub fn new() -> Self {
        QueryBuilder {
            query: None,
            result_type: ResultType::Web,
            count: 10,
            offset: 0,
            timeout_seconds: 5,
            language: None,
            time_range: None,
            safe_search: false,
            engine_params: HashMap::new(),
        }
    }

    pub fn query(mut self, query: impl Into<String>) -> Self {
        self.query = Some(query.into());
        self
    }

    pub fn result_type(mut self, result_type: ResultType) -> Self {
        self.result_type = result_type;
        self
    }

    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }

    pub fn offset(mut self, offset: usize) -> Self {
        self.offset = offset;
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.timeout_seconds = timeout;
        self
    }

    pub fn language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    pub fn time_range(mut self, time_range: TimeRange) -> Self {
        self.time_range = Some(time_range);
        self
    }

    pub fn safe_search(mut self, safe_search: bool) -> Self {
        self.safe_search = safe_search;
        self
    }

    pub fn engine_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.engine_params.insert(key.into(), value.into());
        self
    }

    pub fn build(self) -> Result<SearchQuery, crate::Error> {
        let query = self.query.ok_or_else(|| {
            crate::Error::QueryError("Query string is required".to_string())
        })?;

        Ok(SearchQuery {
            query,
            result_type: self.result_type,
            count: self.count,
            offset: self.offset,
            timeout_seconds: self.timeout_seconds,
            language: self.language,
            time_range: self.time_range,
            safe_search: self.safe_search,
            engine_params: self.engine_params,
        })
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Time range for search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeRange {
    Day,
    Week,
    Month,
    Year,
}

impl TimeRange {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimeRange::Day => "day",
            TimeRange::Week => "week",
            TimeRange::Month => "month",
            TimeRange::Year => "year",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "day" => Some(TimeRange::Day),
            "week" => Some(TimeRange::Week),
            "month" => Some(TimeRange::Month),
            "year" => Some(TimeRange::Year),
            _ => None,
        }
    }
}