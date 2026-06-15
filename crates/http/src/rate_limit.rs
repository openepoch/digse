//! Rate limiting using a token bucket algorithm.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

/// Rate limiter using a token bucket algorithm.
///
/// `acquire` blocks until a concurrency slot is free (semaphore) and then
/// enforces a minimum spacing between successive requests. `check` is a
/// non-blocking probe that consumes a token from a time-refilling bucket,
/// returning `false` when the bucket is empty.
#[derive(Clone)]
pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    min_interval: Duration,
    bucket: Arc<Mutex<TokenBucket>>,
}

/// Refilling token-bucket state backing [`RateLimiter::check`].
struct TokenBucket {
    capacity: f64,
    tokens: f64,
    refill_per_sec: f64,
    last: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// # Arguments
    /// * `requests_per_second` - Maximum number of requests per second
    pub fn new(requests_per_second: u32) -> Self {
        let rps = requests_per_second.max(1) as usize;
        let semaphore = Arc::new(Semaphore::new(rps));
        let min_interval = Duration::from_secs(1) / requests_per_second.max(1);

        RateLimiter {
            semaphore,
            min_interval,
            bucket: Arc::new(Mutex::new(TokenBucket {
                capacity: rps as f64,
                tokens: rps as f64,
                refill_per_sec: rps as f64,
                last: Instant::now(),
            })),
        }
    }

    /// Create a rate limiter with custom burst capacity.
    ///
    /// # Arguments
    /// * `requests_per_second` - Refill rate (tokens added per second)
    /// * `burst_size` - Maximum burst size (bucket capacity)
    pub fn with_burst(requests_per_second: u32, burst_size: u32) -> Self {
        let burst = burst_size.max(1) as usize;
        let rps = requests_per_second.max(1) as f64;
        let semaphore = Arc::new(Semaphore::new(burst));
        let min_interval = Duration::from_secs(1) / requests_per_second.max(1);

        RateLimiter {
            semaphore,
            min_interval,
            bucket: Arc::new(Mutex::new(TokenBucket {
                capacity: burst as f64,
                tokens: burst as f64,
                refill_per_sec: rps,
                last: Instant::now(),
            })),
        }
    }

    /// Acquire permission to make a request (blocking).
    pub async fn acquire(&self) {
        let _permit = self.semaphore.acquire().await.unwrap();
        tokio::time::sleep(self.min_interval).await;
    }

    /// Non-blocking probe: consume a token if one is available.
    ///
    /// Returns `true` when the request is allowed (a token was consumed) and
    /// `false` when the bucket is empty and the caller is rate limited. Tokens
    /// refill over time at `requests_per_second` up to the bucket capacity.
    pub fn check(&self) -> bool {
        let mut b = self
            .bucket
            .lock()
            .expect("rate limiter bucket mutex poisoned");
        let now = Instant::now();
        let elapsed = now.duration_since(b.last).as_secs_f64();
        b.tokens = (b.tokens + elapsed * b.refill_per_sec).min(b.capacity);
        b.last = now;
        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Rate limiter builder
pub struct RateLimiterBuilder {
    requests_per_second: u32,
    burst_size: Option<u32>,
}

impl RateLimiterBuilder {
    pub fn new() -> Self {
        RateLimiterBuilder {
            requests_per_second: 10,
            burst_size: None,
        }
    }

    pub fn requests_per_second(mut self, rps: u32) -> Self {
        self.requests_per_second = rps;
        self
    }

    pub fn burst_size(mut self, size: u32) -> Self {
        self.burst_size = Some(size);
        self
    }

    pub fn build(self) -> RateLimiter {
        match self.burst_size {
            Some(burst) => RateLimiter::with_burst(self.requests_per_second, burst),
            None => RateLimiter::new(self.requests_per_second),
        }
    }
}

impl Default for RateLimiterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter() {
        let limiter = RateLimiter::new(5); // 5 requests per second

        // Should allow 5 requests quickly (the bucket starts full)
        for _ in 0..5 {
            assert!(limiter.check());
        }

        // The 6th request should be rate limited
        assert!(!limiter.check());

        // Wait for at least one token to refill (interval is 200ms at 5 rps)
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert!(limiter.check());
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire() {
        let limiter = RateLimiter::new(10); // 10 requests per second

        // Should allow quick acquisition
        limiter.acquire().await;
        limiter.acquire().await;
        limiter.acquire().await;
    }
}
