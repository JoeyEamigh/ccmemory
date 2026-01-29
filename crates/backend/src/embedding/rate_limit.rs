// Rate limiter for API providers with sliding window rate limiting
//
// Implements a sliding window rate limiter that tracks requests over a
// configurable time window and delays requests when the limit is reached.
//
// The limiter supports a token-based refund mechanism for failed requests
// that didn't actually consume API rate limit capacity (network errors,
// server errors, etc.).

use std::{
  collections::VecDeque,
  time::{Duration, Instant},
};

use tracing::trace;

/// Token returned when recording a request, used for potential refunds.
///
/// When a request fails due to network errors or server errors (5xx),
/// the request likely didn't count against the API provider's rate limit.
/// Use this token to refund the slot and keep our local limiter accurate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitToken {
  timestamp: Instant,
  /// Unique identifier to distinguish tokens with same timestamp
  id: u64,
}

impl RateLimitToken {
  fn new(timestamp: Instant, id: u64) -> Self {
    Self { timestamp, id }
  }
}

/// Configuration for rate limiting
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
  /// Maximum requests allowed in the window
  pub max_requests: usize,
  /// Time window duration
  pub window: Duration,
  /// Maximum time to wait for a slot before failing
  pub max_wait: Duration,
}

impl Default for RateLimitConfig {
  fn default() -> Self {
    Self {
      max_requests: 70,
      window: Duration::from_secs(10),
      max_wait: Duration::from_secs(30),
    }
  }
}

impl RateLimitConfig {
  /// Create a config for OpenRouter (65 requests per 10s sliding window)
  /// OpenRouter's actual limit is 70/10s, but we use 65 for safety margin.
  pub fn for_openrouter() -> Self {
    Self {
      max_requests: 65,
      window: Duration::from_secs(10),
      max_wait: Duration::from_secs(60),
    }
  }

  #[allow(dead_code)]
  /// Create a config with custom limits
  pub fn new(max_requests: usize, window: Duration) -> Self {
    Self {
      max_requests,
      window,
      max_wait: Duration::from_secs(30),
    }
  }
}

/// Sliding window rate limiter with refund support.
///
/// Tracks requests using timestamps and supports refunding slots when
/// requests fail in ways that don't consume the API's rate limit capacity.
#[derive(Debug)]
pub struct SlidingWindowLimiter {
  config: RateLimitConfig,
  /// Request records (timestamp, token_id) within the window
  request_records: VecDeque<(Instant, u64)>,
  /// Counter for generating unique token IDs
  next_token_id: u64,
}

impl SlidingWindowLimiter {
  pub fn new(config: RateLimitConfig) -> Self {
    let capacity = config.max_requests + 1;
    Self {
      config,
      request_records: VecDeque::with_capacity(capacity),
      next_token_id: 0,
    }
  }

  /// Remove expired timestamps from the window
  fn prune_expired(&mut self) {
    let cutoff = Instant::now() - self.config.window;
    let before_count = self.request_records.len();
    while let Some(&(oldest_ts, _)) = self.request_records.front() {
      if oldest_ts < cutoff {
        self.request_records.pop_front();
      } else {
        break;
      }
    }
    let pruned = before_count - self.request_records.len();
    if pruned > 0 {
      trace!(
        pruned = pruned,
        remaining = self.request_records.len(),
        window_ms = self.config.window.as_millis(),
        "Rate limit window reset, pruned expired timestamps"
      );
    }
  }

  /// Check if we can make a request now, and if not, how long to wait
  fn check_and_wait_time(&mut self) -> Option<Duration> {
    self.prune_expired();

    if self.request_records.len() < self.config.max_requests {
      // Under the limit, can proceed immediately
      None
    } else {
      // At the limit - calculate when the oldest request will expire
      if let Some(&(oldest_ts, _)) = self.request_records.front() {
        let expires_at = oldest_ts + self.config.window;
        let now = Instant::now();
        if expires_at > now { Some(expires_at - now) } else { None }
      } else {
        None
      }
    }
  }

  /// Record a request and return a token that can be used to refund the slot.
  ///
  /// Use this when you need the ability to refund a slot if the request
  /// fails in a way that didn't actually consume API rate limit capacity.
  pub fn record_request_with_token(&mut self) -> RateLimitToken {
    let ts = Instant::now();
    let id = self.next_token_id;
    self.next_token_id = self.next_token_id.wrapping_add(1);
    self.request_records.push_back((ts, id));
    RateLimitToken::new(ts, id)
  }

  /// Refund a rate limit slot using the token from `record_request_with_token`.
  ///
  /// Call this when a request fails due to:
  /// - Network errors (request never reached the API)
  /// - Timeouts (request may not have been processed)
  /// - 5xx server errors (API-side failure before rate limiting)
  ///
  /// Do NOT refund for:
  /// - 429 (rate limited) - the API counted this request
  /// - 4xx errors - the request was processed
  /// - Successful responses - obviously
  ///
  /// Returns true if the token was found and removed, false otherwise.
  pub fn refund(&mut self, token: RateLimitToken) -> bool {
    // Find and remove the matching record
    if let Some(pos) = self
      .request_records
      .iter()
      .position(|&(ts, id)| ts == token.timestamp && id == token.id)
    {
      self.request_records.remove(pos);
      trace!(
        token_id = token.id,
        remaining = self.request_records.len(),
        "Rate limit slot refunded"
      );
      true
    } else {
      // Token may have already expired and been pruned, that's fine
      trace!(
        token_id = token.id,
        "Rate limit refund: token not found (may have expired)"
      );
      false
    }
  }

  /// Check if a slot is available and record with token if so.
  /// Returns Ok(token) if slot was acquired, or Err(wait_time) if we need to wait.
  pub fn check_and_record_with_token(&mut self) -> Result<RateLimitToken, Duration> {
    let wait = self.check_and_wait_time();
    match wait {
      None => Ok(self.record_request_with_token()),
      Some(duration) => Err(duration),
    }
  }
}

#[cfg(test)]
mod tests {

  use super::*;

  #[test]
  fn test_sliding_window_under_limit() {
    let config = RateLimitConfig::new(5, Duration::from_secs(1));
    let mut limiter = SlidingWindowLimiter::new(config);

    // First 5 requests should go through immediately
    for _ in 0..5 {
      assert!(limiter.check_and_wait_time().is_none());
      limiter.record_request_with_token();
    }
  }

  #[test]
  fn test_sliding_window_at_limit() {
    let config = RateLimitConfig::new(5, Duration::from_secs(10));
    let mut limiter = SlidingWindowLimiter::new(config);

    // Fill up the window
    for _ in 0..5 {
      limiter.record_request_with_token();
    }

    // 6th request should need to wait
    let wait = limiter.check_and_wait_time();
    assert!(wait.is_some());
    assert!(wait.unwrap() <= Duration::from_secs(10));
  }

  #[test]
  fn test_refund_nonexistent_token() {
    let config = RateLimitConfig::new(5, Duration::from_secs(10));
    let mut limiter = SlidingWindowLimiter::new(config);

    // Create a fake token
    let fake_token = RateLimitToken::new(Instant::now(), 99999);

    // Refund should return false
    assert!(!limiter.refund(fake_token));
  }

  #[test]
  fn test_refund_already_expired() {
    let config = RateLimitConfig::new(5, Duration::from_millis(10));
    let mut limiter = SlidingWindowLimiter::new(config);

    let token = limiter.record_request_with_token();

    // Wait for window to expire
    std::thread::sleep(Duration::from_millis(15));

    // Token should be gone after prune
    limiter.prune_expired();
    assert!(!limiter.refund(token));
  }

  #[test]
  fn test_check_and_record_with_token() {
    let config = RateLimitConfig::new(2, Duration::from_secs(10));
    let mut limiter = SlidingWindowLimiter::new(config);

    // First two should succeed
    let result1 = limiter.check_and_record_with_token();
    assert!(result1.is_ok());

    let result2 = limiter.check_and_record_with_token();
    assert!(result2.is_ok());

    // Third should fail with wait duration
    let result3 = limiter.check_and_record_with_token();
    assert!(result3.is_err());
    assert!(result3.unwrap_err() <= Duration::from_secs(10));
  }

  #[test]
  fn test_refund_restores_capacity() {
    let config = RateLimitConfig::new(2, Duration::from_secs(10));
    let mut limiter = SlidingWindowLimiter::new(config);

    // Fill to capacity
    let token1 = limiter.check_and_record_with_token().unwrap();
    let _token2 = limiter.check_and_record_with_token().unwrap();

    // Can't proceed
    assert!(limiter.check_and_record_with_token().is_err());

    // Refund first token
    limiter.refund(token1);

    // Now can proceed again
    assert!(limiter.check_and_record_with_token().is_ok());
  }
}
