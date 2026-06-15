//! Demo search engine for testing purposes

use async_trait::async_trait;
use std::collections::HashMap;

use digse_core::{
    Engine, EngineCategory, EngineMetadata, Result,
    SearchQuery, SearchResult, ResultType,
};

/// Demo search engine that returns mock results
pub struct DemoEngine {
    metadata: EngineMetadata,
}

impl DemoEngine {
    pub fn new() -> Self {
        let metadata = EngineMetadata {
            name: "demo".to_string(),
            category: EngineCategory::General,
            enabled: true,
            requires_auth: false,
            timeout_seconds: 1,
            description: "Demo engine that returns mock results for testing".to_string(),
            website: Some("https://example.com".to_string()),
        };

        DemoEngine { metadata }
    }
}

#[async_trait]
impl Engine for DemoEngine {
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
        // Simulate network delay
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Generate mock results based on the query
        let mut results = Vec::new();

        let sample_results = vec![
            ("Rust Programming Language", "https://www.rust-lang.org/", "Systems programming language that runs blazingly fast, prevents segfaults, and threadsafety."),
            ("Rust - Wikipedia", "https://en.wikipedia.org/wiki/Rust_(programming_language)", "Rust is a multi-paradigm programming language designed for performance and safety."),
            ("The Rust Book", "https://doc.rust-lang.org/book/", "The Rust Programming Language book - comprehensive guide to Rust."),
            ("Rust by Example", "https://doc.rust-lang.org/rust-by-example/", "Learn Rust with examples - collection of runnable examples."),
            ("Cargo - Rust Package Manager", "https://doc.rust-lang.org/cargo/", "Cargo manages Rust projects and their dependencies."),
        ];

        for (i, (title, url, snippet)) in sample_results.iter().enumerate() {
            if query.count > i {
                results.push(
                    SearchResult::new(*title, *url)
                        .with_snippet(*snippet)
                        .with_engine("demo")
                        .with_rank(query.offset + i + 1)
                        .with_score(1.0 - (i as f64 * 0.1))
                );
            }
        }

        Ok(results)
    }

    fn supports_result_type(&self, result_type: &ResultType) -> bool {
        *result_type == ResultType::Web || *result_type == ResultType::All
    }

    fn settings(&self) -> HashMap<String, String> {
        let mut settings = HashMap::new();
        settings.insert("type".to_string(), "demo".to_string());
        settings.insert("purpose".to_string(), "testing".to_string());
        settings
    }
}
