use crate::rate_limit::{RateLimitConfig, SlidingWindowLimiter};
use crate::{EmbeddingError, EmbeddingProvider};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/embeddings";
const DEFAULT_MODEL: &str = "openai/text-embedding-3-small";
const DEFAULT_DIMENSIONS: usize = 1536;
/// Default max batch size for OpenRouter.
/// OpenRouter accepts multiple texts per request, but very large batches
/// may hit token limits or timeout. 64 is a reasonable default.
const DEFAULT_MAX_BATCH_SIZE: usize = 64;

#[derive(Debug, Clone)]
pub struct OpenRouterProvider {
  client: reqwest::Client,
  api_key: String,
  model: String,
  dimensions: usize,
  /// Maximum texts per batch request
  max_batch_size: usize,
  /// Rate limiter for HTTP requests (shared across clones)
  rate_limiter: Arc<Mutex<SlidingWindowLimiter>>,
}

impl OpenRouterProvider {
  pub fn new(api_key: impl Into<String>) -> Self {
    Self {
      client: reqwest::Client::new(),
      api_key: api_key.into(),
      model: DEFAULT_MODEL.to_string(),
      dimensions: DEFAULT_DIMENSIONS,
      max_batch_size: DEFAULT_MAX_BATCH_SIZE,
      rate_limiter: Arc::new(Mutex::new(SlidingWindowLimiter::new(RateLimitConfig::for_openrouter()))),
    }
  }

  pub fn with_model(mut self, model: impl Into<String>, dimensions: usize) -> Self {
    self.model = model.into();
    self.dimensions = dimensions;
    self
  }

  /// Set the maximum batch size for embedding requests
  pub fn with_max_batch_size(mut self, max_batch_size: usize) -> Self {
    self.max_batch_size = max_batch_size.max(1); // At least 1
    self
  }

  /// Set a custom rate limit configuration
  pub fn with_rate_limit(mut self, config: RateLimitConfig) -> Self {
    self.rate_limiter = Arc::new(Mutex::new(SlidingWindowLimiter::new(config)));
    self
  }

  /// Get the current max batch size
  pub fn max_batch_size(&self) -> usize {
    self.max_batch_size
  }

  pub fn from_env() -> Option<Self> {
    std::env::var("OPENROUTER_API_KEY").ok().map(Self::new)
  }

  /// Acquire a rate limit slot, waiting if necessary
  async fn acquire_rate_limit_slot(&self) -> Result<(), EmbeddingError> {
    use std::time::Instant;
    use tokio::time::sleep;

    let config = RateLimitConfig::for_openrouter();
    let start = Instant::now();

    loop {
      let wait_time = {
        let mut limiter = self.rate_limiter.lock().await;
        limiter.check_and_record()
      };

      match wait_time {
        None => {
          // Slot acquired
          return Ok(());
        }
        Some(wait) => {
          // Check if we've exceeded max wait time
          if start.elapsed() + wait > config.max_wait {
            warn!("Rate limiter: max wait time exceeded ({:?})", config.max_wait);
            return Err(EmbeddingError::ProviderError(format!(
              "Rate limit wait time exceeded ({:?})",
              config.max_wait
            )));
          }

          debug!("Rate limiter: waiting {:?} for slot", wait);
          sleep(wait).await;
        }
      }
    }
  }

  /// Embed a single batch of texts (internal helper)
  /// Rate limiting is applied here at the HTTP request level.
  async fn embed_single_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    if texts.is_empty() {
      return Ok(Vec::new());
    }

    // Acquire rate limit slot before making HTTP request
    self.acquire_rate_limit_slot().await?;

    let request = EmbeddingRequest {
      model: &self.model,
      input: EmbeddingInput::Batch(texts.to_vec()),
    };

    debug!(
      "Embedding batch of {} texts with OpenRouter (model: {})",
      texts.len(),
      self.model
    );

    let response = self
      .client
      .post(OPENROUTER_URL)
      .header("Authorization", format!("Bearer {}", self.api_key))
      .header("Content-Type", "application/json")
      .json(&request)
      .send()
      .await?;

    if !response.status().is_success() {
      let status = response.status();
      let body = response.text().await.unwrap_or_default();
      warn!("OpenRouter batch embedding failed: {} - {}", status, body);
      return Err(EmbeddingError::ProviderError(format!(
        "OpenRouter returned {}: {}",
        status, body
      )));
    }

    let result: EmbeddingResponse = response.json().await?;

    if result.data.len() != texts.len() {
      warn!(
        "Batch size mismatch: got {} embeddings for {} inputs",
        result.data.len(),
        texts.len()
      );
      return Err(EmbeddingError::ProviderError(format!(
        "Batch size mismatch: got {} embeddings for {} inputs",
        result.data.len(),
        texts.len()
      )));
    }

    Ok(result.data.into_iter().map(|d| d.embedding).collect())
  }

  /// Embed texts with sub-batching and full concurrent processing.
  ///
  /// Splits large batches into sub-batches of max_batch_size and processes
  /// them concurrently. Rate limiting is handled at the HTTP request level
  /// inside embed_single_batch(), so we can safely send all sub-batches
  /// concurrently - the rate limiter will naturally throttle them.
  async fn embed_batch_concurrent(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    let num_batches = texts.len().div_ceil(self.max_batch_size);

    // For single batch, no concurrency overhead needed
    if num_batches <= 1 {
      return self.embed_single_batch(texts).await;
    }

    debug!(
      "Processing {} texts in {} concurrent sub-batches (max batch size: {})",
      texts.len(),
      num_batches,
      self.max_batch_size
    );

    // Create indexed sub-batch tasks - NO semaphore limit, rate limiter handles throttling
    let futures: Vec<_> = texts
      .chunks(self.max_batch_size)
      .enumerate()
      .map(|(batch_idx, chunk)| {
        let provider = self.clone();
        let chunk_owned: Vec<String> = chunk.iter().map(|s| s.to_string()).collect();
        async move {
          let chunk_refs: Vec<&str> = chunk_owned.iter().map(|s| s.as_str()).collect();
          let embeddings = provider.embed_single_batch(&chunk_refs).await?;
          Ok::<_, EmbeddingError>((batch_idx, embeddings))
        }
      })
      .collect();

    // Wait for all batches concurrently - rate limiter inside embed_single_batch
    // will naturally throttle to stay within OpenRouter's 70 req/10s limit
    #[allow(clippy::type_complexity)]
    let results: Vec<Result<(usize, Vec<Vec<f32>>), EmbeddingError>> = futures::future::join_all(futures).await;

    // Collect and sort results by batch index to maintain order
    let mut indexed_results: Vec<(usize, Vec<Vec<f32>>)> = Vec::with_capacity(num_batches);
    for result in results {
      indexed_results.push(result?);
    }
    indexed_results.sort_by_key(|(idx, _)| *idx);

    // Flatten into final result
    let mut all_embeddings = Vec::with_capacity(texts.len());
    for (_, embeddings) in indexed_results {
      all_embeddings.extend(embeddings);
    }

    info!(
      "Batch embedded {} texts in {} concurrent sub-batches",
      texts.len(),
      num_batches
    );

    Ok(all_embeddings)
  }
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
  model: &'a str,
  input: EmbeddingInput<'a>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum EmbeddingInput<'a> {
  Single(&'a str),
  Batch(Vec<&'a str>),
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
  data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
  embedding: Vec<f32>,
}

#[async_trait]
impl EmbeddingProvider for OpenRouterProvider {
  fn name(&self) -> &str {
    "openrouter"
  }

  fn model_id(&self) -> &str {
    &self.model
  }

  fn dimensions(&self) -> usize {
    self.dimensions
  }

  async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
    // Acquire rate limit slot before making HTTP request
    self.acquire_rate_limit_slot().await?;

    let request = EmbeddingRequest {
      model: &self.model,
      input: EmbeddingInput::Single(text),
    };

    debug!("Embedding text with OpenRouter: {} chars", text.len());

    let response = self
      .client
      .post(OPENROUTER_URL)
      .header("Authorization", format!("Bearer {}", self.api_key))
      .header("Content-Type", "application/json")
      .json(&request)
      .send()
      .await?;

    if !response.status().is_success() {
      let status = response.status();
      let body = response.text().await.unwrap_or_default();
      warn!("OpenRouter embedding failed: {} - {}", status, body);
      return Err(EmbeddingError::ProviderError(format!(
        "OpenRouter returned {}: {}",
        status, body
      )));
    }

    let result: EmbeddingResponse = response.json().await?;

    result
      .data
      .into_iter()
      .next()
      .map(|d| d.embedding)
      .ok_or_else(|| EmbeddingError::ProviderError("No embedding in response".into()))
  }

  async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    if texts.is_empty() {
      return Ok(Vec::new());
    }

    self.embed_batch_concurrent(texts).await
  }

  async fn is_available(&self) -> bool {
    // OpenRouter is a cloud service, just check we have an API key
    !self.api_key.is_empty()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_provider_new() {
    let provider = OpenRouterProvider::new("test-key");
    assert_eq!(provider.name(), "openrouter");
    assert_eq!(provider.model_id(), DEFAULT_MODEL);
    assert_eq!(provider.dimensions(), DEFAULT_DIMENSIONS);
    assert_eq!(provider.max_batch_size(), DEFAULT_MAX_BATCH_SIZE);
  }

  #[test]
  fn test_provider_customization() {
    let provider = OpenRouterProvider::new("test-key")
      .with_model("custom/model", 512)
      .with_max_batch_size(32);

    assert_eq!(provider.model_id(), "custom/model");
    assert_eq!(provider.dimensions(), 512);
    assert_eq!(provider.max_batch_size(), 32);
  }

  #[test]
  fn test_max_batch_size_minimum() {
    // Batch size should never be 0
    let provider = OpenRouterProvider::new("test-key").with_max_batch_size(0);
    assert_eq!(provider.max_batch_size(), 1);
  }

  #[test]
  fn test_from_env_missing() {
    // Clear any existing env var for this test
    unsafe {
      std::env::remove_var("OPENROUTER_API_KEY");
    }
    assert!(OpenRouterProvider::from_env().is_none());
  }

  #[tokio::test]
  async fn test_is_available_with_key() {
    let provider = OpenRouterProvider::new("test-key");
    assert!(provider.is_available().await);
  }

  #[tokio::test]
  async fn test_is_available_without_key() {
    let provider = OpenRouterProvider::new("");
    assert!(!provider.is_available().await);
  }

  #[test]
  fn test_batch_splitting_calculation() {
    let provider = OpenRouterProvider::new("test-key").with_max_batch_size(10);

    // 25 texts should be split into 3 batches (10 + 10 + 5)
    let num_batches = 25_usize.div_ceil(provider.max_batch_size());
    assert_eq!(num_batches, 3);

    // 10 texts should be 1 batch
    let num_batches = 10_usize.div_ceil(provider.max_batch_size());
    assert_eq!(num_batches, 1);

    // 11 texts should be 2 batches
    let num_batches = 11_usize.div_ceil(provider.max_batch_size());
    assert_eq!(num_batches, 2);
  }

  // Integration tests - run when OPENROUTER_API_KEY is set, skip otherwise
  #[tokio::test]
  async fn test_embed_text() {
    let Some(provider) = OpenRouterProvider::from_env() else {
      eprintln!("OPENROUTER_API_KEY not set, skipping test");
      return;
    };

    let embedding = provider.embed("Hello, world!").await.unwrap();
    assert_eq!(embedding.len(), provider.dimensions());
  }

  #[tokio::test]
  async fn test_embed_batch() {
    let Some(provider) = OpenRouterProvider::from_env() else {
      eprintln!("OPENROUTER_API_KEY not set, skipping test");
      return;
    };

    let texts = vec!["Hello", "World", "Test"];
    let embeddings = provider.embed_batch(&texts).await.unwrap();

    assert_eq!(embeddings.len(), 3);
    for embedding in &embeddings {
      assert_eq!(embedding.len(), provider.dimensions());
    }
  }

  #[tokio::test]
  async fn test_embed_batch_with_subbatching() {
    let Some(provider) = OpenRouterProvider::from_env() else {
      eprintln!("OPENROUTER_API_KEY not set, skipping test");
      return;
    };
    let provider = provider.with_max_batch_size(2); // Force sub-batching

    // 5 texts with batch size 2 = 3 sub-batches
    let texts = vec!["One", "Two", "Three", "Four", "Five"];
    let embeddings = provider.embed_batch(&texts).await.unwrap();

    assert_eq!(embeddings.len(), 5);
    for embedding in &embeddings {
      assert_eq!(embedding.len(), provider.dimensions());
    }
  }

  #[tokio::test]
  async fn test_embed_batch_empty() {
    let provider = OpenRouterProvider::new("test-key");
    let result = provider.embed_batch(&[]).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
  }
}
