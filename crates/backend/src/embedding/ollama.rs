use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, trace, warn};

use super::{EmbeddingError, EmbeddingMode, EmbeddingProvider};
use crate::config::EmbeddingConfig;

/// this really should be configurable but let's be real the gpu is the bottleneck
const OLLAMA_MAX_CONCURRENT_REQUESTS: usize = 4;

/// Calculate max batch size based on context length
/// Formula: clamp(context_length / avg_chunk_tokens, 1, 64)
fn calculate_max_batch_size(context_length: usize) -> usize {
  let calculated = context_length / 512;
  calculated.clamp(1, 64)
}

#[derive(Debug, Clone)]
pub struct OllamaProvider {
  client: reqwest::Client,
  base_url: String,
  model: String,
  dimensions: usize,
  /// Maximum batch size (auto-calculated from context_length if not set)
  max_batch_size: usize,
  /// Maximum concurrent requests to avoid overwhelming GPU
  max_concurrent: usize,
  /// Optional task instruction to prepend to queries.
  /// When Some and non-empty, queries are formatted as `Instruct: {instruction}\nQuery:{query}`.
  /// When None or empty, queries are embedded as-is.
  query_instruction: Option<String>,
}

impl OllamaProvider {
  pub fn new(config: &EmbeddingConfig) -> Result<Self, EmbeddingError> {
    let base_url = config.ollama_url.clone();
    let model = config.model.clone();
    let dimensions = config.dimensions;
    let max_batch_size = config
      .max_batch_size
      .unwrap_or_else(|| calculate_max_batch_size(config.context_length));
    let query_instruction = config.query_instruction.clone();

    let has_instruction = query_instruction.as_ref().is_some_and(|s| !s.is_empty());
    info!(
      base_url,
      model,
      dimensions,
      max_batch_size,
      has_query_instruction = has_instruction,
      "Ollama provider initialized"
    );
    Ok(Self {
      client: reqwest::Client::new(),
      base_url,
      model,
      dimensions,
      max_batch_size,
      max_concurrent: OLLAMA_MAX_CONCURRENT_REQUESTS,
      query_instruction,
    })
  }

  /// Single embedding endpoint (legacy)
  fn embeddings_url(&self) -> String {
    format!("{}/api/embeddings", self.base_url)
  }

  /// Batch embedding endpoint (new)
  fn embed_url(&self) -> String {
    format!("{}/api/embed", self.base_url)
  }

  /// Format text for embedding based on mode.
  ///
  /// For queries (if query_instruction is set): `Instruct: {instruction}\nQuery:{text}`
  /// For documents or if no instruction: text as-is
  fn format_for_embedding(&self, text: &str, mode: EmbeddingMode) -> String {
    match mode {
      EmbeddingMode::Query => {
        if let Some(ref instruction) = self.query_instruction
          && !instruction.is_empty()
        {
          return format!("Instruct: {}\nQuery:{}", instruction, text);
        }
        text.to_string()
      }
      EmbeddingMode::Document => text.to_string(),
    }
  }

  /// Native batch embedding using /api/embed endpoint.
  ///
  /// Processes sub-batches concurrently using semaphore-limited parallelism.
  /// This significantly improves throughput when multiple sub-batches are needed,
  /// while respecting the server's capacity limits.
  #[tracing::instrument(level = "trace", skip(self, texts), fields(batch_size = texts.len()))]
  async fn embed_batch_native(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    use std::sync::Arc;

    use tokio::sync::Semaphore;

    let num_batches = texts.len().div_ceil(self.max_batch_size);
    let start = Instant::now();

    // For single batch, no concurrency overhead needed
    if num_batches <= 1 {
      return self.embed_single_batch(texts).await;
    }

    debug!(
      batch_size = texts.len(),
      sub_batches = num_batches,
      max_batch_size = self.max_batch_size,
      model = %self.model,
      "Processing batch with concurrent sub-batches"
    );

    // Limit concurrent requests to avoid overwhelming local GPU
    let semaphore = Arc::new(Semaphore::new(self.max_concurrent));

    // Create indexed sub-batch tasks
    let futures: Vec<_> = texts
      .chunks(self.max_batch_size)
      .enumerate()
      .map(|(batch_idx, chunk)| {
        let permit = semaphore.clone();
        let provider = self.clone();
        let chunk_owned: Vec<String> = chunk.iter().map(|s| s.to_string()).collect();
        async move {
          let _permit = match permit.acquire().await {
            Ok(permit) => permit,
            Err(_) => {
              return Err(EmbeddingError::ProviderError("semaphore closed".to_string()));
            }
          };
          let chunk_refs: Vec<&str> = chunk_owned.iter().map(|s| s.as_str()).collect();
          let embeddings = provider.embed_single_batch(&chunk_refs).await?;
          Ok((batch_idx, embeddings))
        }
      })
      .collect();

    // Wait for all batches concurrently
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

    debug!(
      batch_size = texts.len(),
      sub_batches = num_batches,
      elapsed_ms = start.elapsed().as_millis(),
      "Batch embedding complete"
    );

    Ok(all_embeddings)
  }

  /// Embed a single batch of texts (used internally by embed_batch_native)
  #[tracing::instrument(level = "trace", skip(self, texts), fields(batch_size = texts.len()))]
  async fn embed_single_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    let request = BatchEmbeddingRequest {
      model: &self.model,
      input: texts.to_vec(),
    };

    trace!(batch_size = texts.len(), model = %self.model, "Sending batch embedding request");
    let start = Instant::now();

    let response = self.client.post(self.embed_url()).json(&request).send().await?;

    trace!(
      status = %response.status(),
      elapsed_ms = start.elapsed().as_millis(),
      "Received batch embedding response"
    );

    if !response.status().is_success() {
      let status = response.status();
      let body = response.text().await.unwrap_or_default();
      warn!(
        status = %status,
        batch_size = texts.len(),
        model = %self.model,
        "Ollama batch embedding failed"
      );
      return Err(EmbeddingError::ProviderError(format!(
        "Ollama returned {}: {}",
        status, body
      )));
    }

    let result: BatchEmbeddingResponse = response.json().await?;
    trace!(
      embeddings_count = result.embeddings.len(),
      elapsed_ms = start.elapsed().as_millis(),
      "Parsed batch embedding response"
    );

    if result.embeddings.len() != texts.len() {
      error!(
        expected = texts.len(),
        got = result.embeddings.len(),
        model = %self.model,
        "Batch size mismatch in embedding response"
      );
      return Err(EmbeddingError::ProviderError(format!(
        "Batch size mismatch: got {} embeddings for {} inputs",
        result.embeddings.len(),
        texts.len()
      )));
    }

    for (i, embedding) in result.embeddings.iter().enumerate() {
      if embedding.len() != self.dimensions {
        warn!(
          index = i,
          expected = self.dimensions,
          got = embedding.len(),
          model = %self.model,
          "Unexpected embedding dimensions"
        );
      }
    }

    Ok(result.embeddings)
  }

  /// Parallel batch embedding (fallback) - uses semaphore to limit concurrency.
  /// NOTE: This expects pre-formatted texts (already has instruction prefix if needed).
  #[tracing::instrument(level = "trace", skip(self, texts), fields(batch_size = texts.len()))]
  async fn embed_batch_parallel(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    use std::sync::Arc;

    use tokio::sync::Semaphore;

    debug!(
      batch_size = texts.len(),
      max_concurrent = self.max_concurrent,
      "Using parallel fallback for batch embedding"
    );
    let start = Instant::now();

    let semaphore = Arc::new(Semaphore::new(self.max_concurrent));

    let futures: Vec<_> = texts
      .iter()
      .map(|text| {
        let permit = semaphore.clone();
        let text = text.to_string();
        let provider = self.clone();
        async move {
          let _permit = match permit.acquire().await {
            Ok(permit) => permit,
            Err(_) => {
              return Err(EmbeddingError::ProviderError("semaphore closed".to_string()));
            }
          };
          // Use raw embed - text is already formatted
          provider.embed_raw(&text).await
        }
      })
      .collect();

    let results: Vec<Result<Vec<f32>, EmbeddingError>> = futures::future::join_all(futures).await;

    // Collect results, propagating first error
    let collected: Result<Vec<Vec<f32>>, EmbeddingError> = results.into_iter().collect();

    match &collected {
      Ok(_) => {
        debug!(
          batch_size = texts.len(),
          elapsed_ms = start.elapsed().as_millis(),
          "Parallel fallback embedding complete"
        );
      }
      Err(e) => {
        warn!(
          batch_size = texts.len(),
          elapsed_ms = start.elapsed().as_millis(),
          err = %e,
          "Parallel fallback embedding failed"
        );
      }
    }

    collected
  }

  /// Raw embed without formatting - for internal use when text is already formatted.
  async fn embed_raw(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
    let request = EmbeddingRequest {
      model: &self.model,
      prompt: text,
    };

    trace!(text_len = text.len(), model = %self.model, "Sending raw embedding request");
    let start = Instant::now();

    let response = self.client.post(self.embeddings_url()).json(&request).send().await?;

    trace!(
      status = %response.status(),
      elapsed_ms = start.elapsed().as_millis(),
      "Received raw embedding response"
    );

    if !response.status().is_success() {
      let status = response.status();
      let body = response.text().await.unwrap_or_default();
      warn!(
        status = %status,
        text_len = text.len(),
        model = %self.model,
        "Ollama raw embedding failed"
      );
      return Err(EmbeddingError::ProviderError(format!(
        "Ollama returned {}: {}",
        status, body
      )));
    }

    let result: EmbeddingResponse = response.json().await?;

    if result.embedding.len() != self.dimensions {
      warn!(
        expected = self.dimensions,
        got = result.embedding.len(),
        model = %self.model,
        "Unexpected embedding dimensions"
      );
    }

    Ok(result.embedding)
  }
}

/// Request for single embedding (legacy /api/embeddings endpoint)
#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
  model: &'a str,
  prompt: &'a str,
}

/// Response from single embedding (legacy /api/embeddings endpoint)
#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
  embedding: Vec<f32>,
}

/// Request for batch embedding (/api/embed endpoint)
#[derive(Debug, Serialize)]
struct BatchEmbeddingRequest<'a> {
  model: &'a str,
  input: Vec<&'a str>,
}

/// Response from batch embedding (/api/embed endpoint)
#[derive(Debug, Deserialize)]
struct BatchEmbeddingResponse {
  embeddings: Vec<Vec<f32>>,
}

#[async_trait]
impl EmbeddingProvider for OllamaProvider {
  fn name(&self) -> &str {
    "ollama"
  }

  fn model_id(&self) -> &str {
    &self.model
  }

  fn dimensions(&self) -> usize {
    self.dimensions
  }

  async fn embed(&self, text: &str, mode: EmbeddingMode) -> Result<Vec<f32>, EmbeddingError> {
    // Format text based on mode (query vs document)
    let formatted = self.format_for_embedding(text, mode);

    let request = EmbeddingRequest {
      model: &self.model,
      prompt: &formatted,
    };

    trace!(text_len = text.len(), mode = ?mode, model = %self.model, "Sending single embedding request");
    let start = Instant::now();

    let response = self.client.post(self.embeddings_url()).json(&request).send().await?;

    trace!(
      status = %response.status(),
      elapsed_ms = start.elapsed().as_millis(),
      "Received single embedding response"
    );

    if !response.status().is_success() {
      let status = response.status();
      let body = response.text().await.unwrap_or_default();
      warn!(
        status = %status,
        text_len = text.len(),
        model = %self.model,
        "Ollama single embedding failed"
      );
      return Err(EmbeddingError::ProviderError(format!(
        "Ollama returned {}: {}",
        status, body
      )));
    }

    let result: EmbeddingResponse = response.json().await?;

    if result.embedding.len() != self.dimensions {
      warn!(
        expected = self.dimensions,
        got = result.embedding.len(),
        model = %self.model,
        "Unexpected embedding dimensions"
      );
    }

    trace!(
      dimensions = result.embedding.len(),
      elapsed_ms = start.elapsed().as_millis(),
      "Single embedding complete"
    );

    Ok(result.embedding)
  }

  async fn embed_batch(&self, texts: &[&str], mode: EmbeddingMode) -> Result<Vec<Vec<f32>>, EmbeddingError> {
    if texts.is_empty() {
      trace!("Empty batch, returning immediately");
      return Ok(Vec::new());
    }

    // Format all texts based on mode
    let formatted: Vec<String> = texts.iter().map(|t| self.format_for_embedding(t, mode)).collect();
    let formatted_refs: Vec<&str> = formatted.iter().map(|s| s.as_str()).collect();

    debug!(batch_size = texts.len(), mode = ?mode, model = %self.model, "Embedding batch");

    // Try native batch API first, fall back to parallel on error
    match self.embed_batch_native(&formatted_refs).await {
      Ok(embeddings) => Ok(embeddings),
      Err(e) => {
        warn!(
          batch_size = texts.len(),
          err = %e,
          "Native batch embedding failed, falling back to parallel"
        );
        self.embed_batch_parallel(&formatted_refs).await
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config::{Config, EmbeddingConfig};

  #[test]
  fn test_provider_customization() {
    let config = Config {
      embedding: EmbeddingConfig {
        ollama_url: "http://custom:8080".to_string(),
        model: "custom-model".to_string(),
        dimensions: 1024,
        ..Default::default()
      },
      ..Default::default()
    };
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");

    assert_eq!(provider.base_url, "http://custom:8080");
    assert_eq!(provider.model_id(), "custom-model");
    assert_eq!(provider.dimensions(), 1024);
  }

  // Integration tests require a running Ollama instance
  #[tokio::test]
  #[ignore = "Requires running Ollama instance"]
  async fn test_embed_text() {
    let config = Config::default();
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");

    let embedding = provider.embed("Hello, world!", EmbeddingMode::Document).await.unwrap();
    assert_eq!(embedding.len(), provider.dimensions());
  }

  #[tokio::test]
  #[ignore = "Requires running Ollama instance"]
  async fn test_embed_batch() {
    let config = Config::default();
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");

    let texts = vec!["Hello", "World", "Test"];
    let embeddings = provider.embed_batch(&texts, EmbeddingMode::Document).await.unwrap();

    assert_eq!(embeddings.len(), 3);
    for embedding in &embeddings {
      assert_eq!(embedding.len(), provider.dimensions());
    }
  }

  #[tokio::test]
  #[ignore = "Requires running Ollama instance"]
  async fn test_embed_batch_empty_input() {
    let config = Config::default();
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");
    // Empty input should return empty vec, not error (no network call needed)
    let result = provider.embed_batch(&[], EmbeddingMode::Document).await;
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
  }

  #[test]
  fn test_max_batch_size_calculation() {
    // 32K context / 512 tokens per chunk = 64 (capped at max)
    assert_eq!(calculate_max_batch_size(32768), 64);

    // 16K context / 512 tokens per chunk = 32
    assert_eq!(calculate_max_batch_size(16384), 32);

    // 8K context / 512 tokens per chunk = 16
    assert_eq!(calculate_max_batch_size(8192), 16);

    // 4K context / 512 tokens per chunk = 8
    assert_eq!(calculate_max_batch_size(4096), 8);

    // Very small context should still return at least 1
    assert_eq!(calculate_max_batch_size(256), 1);
  }

  #[test]
  fn test_context_length_configuration() {
    let config = Config {
      embedding: EmbeddingConfig {
        context_length: 8192,
        ..Default::default()
      },
      ..Default::default()
    };
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");
    assert_eq!(provider.max_batch_size, 16); // 8192 / 512 = 16
  }

  #[test]
  fn test_explicit_max_batch_size() {
    let config = Config {
      embedding: EmbeddingConfig {
        context_length: 32768,
        max_batch_size: Some(10),
        ..Default::default()
      },
      ..Default::default()
    };
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");
    assert_eq!(provider.max_batch_size, 10);
  }

  #[tokio::test]
  #[ignore = "Requires running Ollama instance"]
  async fn test_embed_batch_native_success() {
    // Integration test: verify native batch API works when Ollama is available
    let config = Config::default();
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");

    // Test with 5 texts as specified in acceptance criteria
    let texts = vec![
      "First sentence to embed",
      "Second sentence to embed",
      "Third sentence to embed",
      "Fourth sentence to embed",
      "Fifth sentence to embed",
    ];

    let embeddings = provider.embed_batch_native(&texts).await.unwrap();

    // Verify we got 5 embeddings
    assert_eq!(embeddings.len(), 5);

    // Verify each has correct dimensions
    for embedding in &embeddings {
      assert_eq!(embedding.len(), provider.dimensions());
    }
  }

  #[tokio::test]
  async fn test_embed_batch_with_fallback() {
    // Integration test: verify fallback works
    // This test uses a non-existent URL to force fallback
    let config = Config {
      embedding: EmbeddingConfig {
        ollama_url: "http://localhost:99999".to_string(),
        ..Default::default()
      },
      ..Default::default()
    };
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");

    // Should fail (no server at that port)
    let result = provider.embed_batch(&["test"], EmbeddingMode::Document).await;
    assert!(result.is_err());
  }

  #[tokio::test]
  #[ignore = "Requires running Ollama instance"]
  async fn test_embed_batch_sub_batching() {
    // Integration test: verify large batches are split correctly
    let config = Config {
      embedding: EmbeddingConfig {
        max_batch_size: Some(3),
        ..Default::default()
      },
      ..Default::default()
    };
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");

    // 7 texts should be split into 3 sub-batches (3 + 3 + 1)
    let texts: Vec<&str> = (0..7)
      .map(|i| match i {
        0 => "Text zero",
        1 => "Text one",
        2 => "Text two",
        3 => "Text three",
        4 => "Text four",
        5 => "Text five",
        6 => "Text six",
        _ => unreachable!(),
      })
      .collect();

    let embeddings = provider.embed_batch(&texts, EmbeddingMode::Document).await.unwrap();
    assert_eq!(embeddings.len(), 7);
  }

  #[test]
  fn test_batch_split_calculation() {
    // Verify batch splitting logic
    // With max_batch_size=16 and context_length=8192:
    // 100 chunks should be split into ceil(100/16) = 7 sub-batches
    let config = Config {
      embedding: EmbeddingConfig {
        context_length: 8192,
        ..Default::default()
      },
      ..Default::default()
    };
    let provider = OllamaProvider::new(&config.embedding).expect("could not create provider");
    assert_eq!(provider.max_batch_size, 16);

    let num_batches = 100_usize.div_ceil(provider.max_batch_size);
    assert_eq!(num_batches, 7);
  }
}
