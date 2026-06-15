//! Error types for digse

use thiserror::Error;

/// Digse error type
#[derive(Error, Debug)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("JSON parsing failed: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Engine '{0}' is not available")]
    EngineNotAvailable(String),

    #[error("Engine '{0}' returned an error: {1}")]
    EngineError(String, String),

    #[error("Timeout reached after {0}s")]
    Timeout(u64),

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("No results found")]
    NoResults,

    #[error("Query parsing error: {0}")]
    QueryError(String),

    #[error("Configuration error: {0}")]
    ConfigError(String),

    #[error("Filter error: {0}")]
    FilterError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Other error: {0}")]
    Other(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;