//! Digse - Dig Search Engines
//!
//! This library provides a flexible search interface that aggregates results
//! from multiple search engines. It can be used both as a library and as a CLI tool.
//!
//! # Example
//!
//! ```rust,no_run
//! use digse::{DigseSearch, SearchBuilder, SearchQuery};
//! use digse_core::ResultType;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let search = SearchBuilder::new()
//!     .engines(vec!["duckduckgo".to_string()])
//!     .concurrent_engines(3)
//!     .build();
//!
//! let query = SearchQuery::new("rust programming")
//!     .with_result_type(ResultType::Web)
//!     .with_count(10);
//!
//! let response = search.search(&query).await?;
//! println!("{}", serde_json::to_string_pretty(&response)?);
//! # Ok(())
//! # }
//! ```

pub mod search;
pub mod config;
pub mod output;

// Re-export core types for convenience
pub use digse_core::{
    Engine, EngineCategory, EngineMetadata,
    SearchQuery, SearchResult, ResultType, Error, Result,
    SearchResponse, EngineStat, EngineStatus,
    TimeRange
};

// Re-export search functionality
pub use search::{DigseSearch, SearchBuilder};

// Re-export configuration
pub use config::{
    SearchConfig, EngineSelection, OutputFormat,
    DigseConfig, SearchDefaults, ServeConfig, ConfigError,
};

// Re-export output formatting
pub use output::{build_response_envelope, render_json, Formatter, JsonFormatter, PrettyFormatter};

/// Current version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// User agent string
pub const USER_AGENT: &str = concat!("digse/", env!("CARGO_PKG_VERSION"));
