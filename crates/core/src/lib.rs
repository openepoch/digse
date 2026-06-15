//! Digse core types and traits
//!
//! This crate provides the core abstractions for the digse metasearch engine,
//! including the Engine trait, search query types, result types, and error handling.

pub mod error;
pub mod engine;
pub mod query;
pub mod result;
pub mod filter;

pub use error::{Error, Result};
pub use engine::{Engine, EngineCategory, EngineMetadata};
pub use query::{SearchQuery, QueryBuilder, TimeRange};
pub use result::{SearchResult, ResultType, SearchResultBuilder, SearchResponse, EngineStat, EngineStatus};
pub use filter::{UrlFilter, FilterBuilder};