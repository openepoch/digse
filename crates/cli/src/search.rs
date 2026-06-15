//! Core search functionality
//!
//! This module contains the main search logic that can be used both
//! from the CLI and as a library.

use crate::{SearchConfig, EngineSelection};
use digse_core::{SearchQuery, SearchResult, ResultType, Engine, Error, Result, SearchResponse, EngineStatus, EngineStat};
use digse_engines::all_engines;
use std::time::Instant;
use std::time::Duration;

/// Main search engine
pub struct DigseSearch {
    config: SearchConfig,
}

impl DigseSearch {
    /// Create a new search instance with default configuration
    pub fn new() -> Self {
        DigseSearch {
            config: SearchConfig::default(),
        }
    }

    /// Create a new search instance with custom configuration
    pub fn with_config(config: SearchConfig) -> Self {
        DigseSearch { config }
    }

    /// Perform a search with the given query
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResponse> {
        let engines = self.get_engines(query.result_type)?;

        if engines.is_empty() {
            return Err(Error::EngineNotAvailable("No engines available".to_string()));
        }

        self.perform_search(query, engines).await
    }

    /// Get engines to use based on configuration
    fn get_engines(&self, result_type: ResultType) -> Result<Vec<Box<dyn Engine>>> {
        let mut engines: Vec<_> = all_engines()
            .into_iter()
            .filter(|e| e.supports_result_type(&result_type))
            .collect();

        // Apply filters based on config
        match &self.config.engine_selection {
            EngineSelection::All => {
                // Use all available engines
            }
            EngineSelection::Specific(names) => {
                engines.retain(|e| names.contains(&e.name().to_lowercase()));
            }
            EngineSelection::Exclude(names) => {
                engines.retain(|e| !names.contains(&e.name().to_lowercase()));
            }
            EngineSelection::Categories(categories) => {
                engines.retain(|e| categories.contains(&e.category()));
            }
        }

        Ok(engines)
    }

    /// Perform the actual search
    async fn perform_search(
        &self,
        query: &SearchQuery,
        engines: Vec<Box<dyn Engine>>,
    ) -> Result<SearchResponse> {
        let start = Instant::now();
        let mut all_results = Vec::new();
        let mut engine_stats = Vec::new();

        let semaphore = std::sync::Arc::new(
            tokio::sync::Semaphore::new(self.config.concurrent_engines)
        );

        // Explicit type: the async block returns Ok(...) in all arms, so the inner
        // Result's error type is unconstrained — annotate to resolve E0282.
        // NOTE: use fully-qualified std::result::Result; the `Result` in scope is
        // digse_core::Result<T> (a 1-generic alias), not the 2-generic std Result.
        let mut tasks: Vec<tokio::task::JoinHandle<
            std::result::Result<(Vec<SearchResult>, u64, String, EngineStatus), Error>,
        >> = Vec::new();

        for engine in engines {
            let semaphore = semaphore.clone();
            let query = query.clone();
            let engine_name = engine.name().to_string();
            let timeout = Duration::from_secs(self.config.timeout_seconds);

            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                let engine_start = Instant::now();

                let result = tokio::time::timeout(
                    timeout,
                    engine.search(&query)
                ).await;

                match result {
                    Ok(Ok(results)) => {
                        let duration = engine_start.elapsed().as_millis() as u64;
                        Ok((results, duration, engine_name, EngineStatus::Success))
                    }
                    Ok(Err(_)) => {
                        let duration = engine_start.elapsed().as_millis() as u64;
                        Ok((vec![], duration, engine_name, EngineStatus::Failed))
                    }
                    Err(_) => {
                        let duration = engine_start.elapsed().as_millis() as u64;
                        Ok((vec![], duration, engine_name, EngineStatus::Timeout))
                    }
                }
            });

            tasks.push(task);
        }

        // Collect results
        for task in tasks {
            match task.await {
                Ok(Ok((results, duration, engine_name, status))) => {
                    if self.config.show_engine_stats {
                        engine_stats.push(EngineStat {
                            engine: engine_name.clone(),
                            results_count: results.len(),
                            duration_ms: duration,
                            status,
                        });
                    }
                    all_results.extend(results);
                }
                Ok(Err(e)) => {
                    if log::log_enabled!(log::Level::Debug) {
                        eprintln!("Error in task: {}", e);
                    }
                }
                Err(e) => {
                    if log::log_enabled!(log::Level::Debug) {
                        eprintln!("Task join error: {}", e);
                    }
                }
            }
        }

        // Sort results by score and rank
        all_results.sort_by(|a, b| {
            b.score.partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.rank.cmp(&b.rank))
        });

        let search_duration = start.elapsed().as_millis() as u64;
        let engines_used: Vec<String> = engine_stats.iter().map(|s| s.engine.clone()).collect();

        Ok(SearchResponse {
            query: query.query.clone(),
            result_type: query.result_type,
            total_results: all_results.len(),
            engines_used,
            search_duration_ms: if self.config.show_engine_stats {
                Some(search_duration)
            } else {
                None
            },
            engine_stats,
            results: all_results,
        })
    }
}

impl Default for DigseSearch {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for search configuration
pub struct SearchBuilder {
    config: SearchConfig,
}

impl SearchBuilder {
    pub fn new() -> Self {
        SearchBuilder {
            config: SearchConfig::default(),
        }
    }

    pub fn engines(mut self, engines: Vec<String>) -> Self {
        self.config.engine_selection = EngineSelection::Specific(engines);
        self
    }

    pub fn exclude_engines(mut self, engines: Vec<String>) -> Self {
        self.config.engine_selection = EngineSelection::Exclude(engines);
        self
    }

    pub fn categories(mut self, categories: Vec<crate::EngineCategory>) -> Self {
        self.config.engine_selection = EngineSelection::Categories(categories);
        self
    }

    pub fn all_engines(mut self) -> Self {
        self.config.engine_selection = EngineSelection::All;
        self
    }

    pub fn concurrent_engines(mut self, count: usize) -> Self {
        self.config.concurrent_engines = count;
        self
    }

    pub fn timeout(mut self, timeout: u64) -> Self {
        self.config.timeout_seconds = timeout;
        self
    }

    pub fn show_stats(mut self, show: bool) -> Self {
        self.config.show_engine_stats = show;
        self
    }

    pub fn config(mut self, config: SearchConfig) -> Self {
        self.config = config;
        self
    }

    pub fn build(self) -> DigseSearch {
        DigseSearch::with_config(self.config)
    }
}

impl Default for SearchBuilder {
    fn default() -> Self {
        Self::new()
    }
}
