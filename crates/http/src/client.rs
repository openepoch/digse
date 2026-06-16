//! HTTP client with connection pooling and timeout management

use reqwest::Client;
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::rate_limit::RateLimiter;

/// HTTP client with advanced features
#[derive(Clone)]
pub struct HttpClient {
    client: Client,
    rate_limiter: Arc<Option<RateLimiter>>,
    semaphore: Arc<Semaphore>,
    #[allow(dead_code)]
    default_timeout: Duration,
}

impl HttpClient {
    /// Create a new HTTP client with default settings
    pub fn new() -> Self {
        let client = Client::builder()
            .gzip(true)
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        HttpClient {
            client,
            rate_limiter: Arc::new(None),
            semaphore: Arc::new(Semaphore::new(100)), // Max 100 concurrent requests
            default_timeout: Duration::from_secs(5),
        }
    }

    /// Create a new HTTP client with custom settings
    pub fn with_timeout(timeout_secs: u64) -> Self {
        let client = Client::builder()
            .gzip(true)
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        HttpClient {
            client,
            rate_limiter: Arc::new(None),
            semaphore: Arc::new(Semaphore::new(100)),
            default_timeout: Duration::from_secs(timeout_secs),
        }
    }

    /// Set rate limiter
    pub fn with_rate_limiter(mut self, rate_limiter: RateLimiter) -> Self {
        self.rate_limiter = Arc::new(Some(rate_limiter));
        self
    }

    /// Set max concurrent requests
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.semaphore = Arc::new(Semaphore::new(max));
        self
    }

    /// Execute GET request
    pub async fn get(&self, url: &str) -> Result<reqwest::Response, reqwest::Error> {
        self.execute(reqwest::Client::get(&self.client, url)).await
    }

    /// Execute GET request with headers
    pub async fn get_with_headers(
        &self,
        url: &str,
        headers: reqwest::header::HeaderMap,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let request = reqwest::Client::get(&self.client, url).headers(headers);
        self.execute(request).await
    }

    /// Execute POST request
    pub async fn post(&self, url: &str, body: impl Into<reqwest::Body>) -> Result<reqwest::Response, reqwest::Error> {
        self.execute(reqwest::Client::post(&self.client, url).body(body)).await
    }

    /// Execute request with rate limiting and concurrency control
    async fn execute(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, reqwest::Error> {
        // Acquire semaphore for concurrency control
        let _permit = self.semaphore.acquire().await.unwrap();

        // Apply rate limiting if configured
        if let Some(rate_limiter) = self.rate_limiter.as_ref() {
            rate_limiter.acquire().await;
        }

        // Execute the request
        let response = request.send().await?;
        Ok(response)
    }

    /// Get the underlying reqwest client
    pub fn client(&self) -> &Client {
        &self.client
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP client builder
pub struct HttpClientBuilder {
    timeout: Duration,
    max_concurrent: usize,
    rate_limiter: Option<RateLimiter>,
    user_agent: Option<String>,
}

impl HttpClientBuilder {
    pub fn new() -> Self {
        HttpClientBuilder {
            timeout: Duration::from_secs(5),
            max_concurrent: 100,
            rate_limiter: None,
            user_agent: None,
        }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent = max;
        self
    }

    pub fn rate_limiter(mut self, rate_limiter: RateLimiter) -> Self {
        self.rate_limiter = Some(rate_limiter);
        self
    }

    pub fn user_agent(mut self, user_agent: String) -> Self {
        self.user_agent = Some(user_agent);
        self
    }

    pub fn build(self) -> HttpClient {
        let client = Client::builder()
            .gzip(true)
            .timeout(self.timeout)
            .build()
            .expect("Failed to create HTTP client");

        HttpClient {
            client,
            rate_limiter: Arc::new(self.rate_limiter),
            semaphore: Arc::new(Semaphore::new(self.max_concurrent)),
            default_timeout: self.timeout,
        }
    }
}

impl Default for HttpClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}