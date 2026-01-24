//! Tool handlers for MCP requests
//!
//! This module provides the ToolHandler struct and all tool methods organized
//! by domain area.

mod code;
mod documents;
mod entities;
mod explore;
mod format;
mod memory;
mod ranking;
mod suggestions;
mod system;
mod watch;

pub use format::{format_context_response, format_explore_response};

use crate::projects::ProjectRegistry;
use embedding::EmbeddingProvider;
use engram_core::EmbeddingConfig;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

pub use ranking::{RankingWeights, rank_memories};

/// Default cache size for query embeddings (number of entries)
const EMBEDDING_CACHE_SIZE: u64 = 1000;
/// TTL for cached embeddings (5 minutes)
const EMBEDDING_CACHE_TTL_SECS: u64 = 300;

/// Handler for MCP tool calls
pub struct ToolHandler {
  pub(crate) registry: Arc<ProjectRegistry>,
  pub(crate) embedding: Option<Arc<dyn EmbeddingProvider>>,
  /// Embedding configuration for health check context_length comparison
  pub(crate) embedding_config: Option<EmbeddingConfig>,
  /// Cache for query embeddings to avoid redundant API calls
  /// Key: query text (String), Value: embedding vector (Vec<f32>)
  embedding_cache: Cache<String, Vec<f32>>,
}

/// Create the embedding cache with configured size and TTL
fn create_embedding_cache() -> Cache<String, Vec<f32>> {
  Cache::builder()
    .max_capacity(EMBEDDING_CACHE_SIZE)
    .time_to_live(Duration::from_secs(EMBEDDING_CACHE_TTL_SECS))
    .build()
}

impl ToolHandler {
  pub fn new(registry: Arc<ProjectRegistry>) -> Self {
    Self {
      registry,
      embedding: None,
      embedding_config: None,
      embedding_cache: create_embedding_cache(),
    }
  }

  pub fn with_embedding(registry: Arc<ProjectRegistry>, embedding: Arc<dyn EmbeddingProvider>) -> Self {
    Self {
      registry,
      embedding: Some(embedding),
      embedding_config: None,
      embedding_cache: create_embedding_cache(),
    }
  }

  pub fn with_embedding_and_config(
    registry: Arc<ProjectRegistry>,
    embedding: Arc<dyn EmbeddingProvider>,
    config: EmbeddingConfig,
  ) -> Self {
    Self {
      registry,
      embedding: Some(embedding),
      embedding_config: Some(config),
      embedding_cache: create_embedding_cache(),
    }
  }

  /// Get embedding for a query, with caching and fallback to None if provider unavailable
  ///
  /// Uses an LRU cache with 5-minute TTL to avoid redundant embedding API calls
  /// for repeated queries (common in interactive exploration workflows).
  pub(crate) async fn get_embedding(&self, text: &str) -> Option<Vec<f32>> {
    // Check cache first
    if let Some(cached) = self.embedding_cache.get(text).await {
      debug!("Embedding cache hit for query");
      return Some(cached);
    }

    // Cache miss - generate embedding
    if let Some(ref provider) = self.embedding {
      match provider.embed(text).await {
        Ok(vec) => {
          // Cache the result
          self.embedding_cache.insert(text.to_string(), vec.clone()).await;
          Some(vec)
        }
        Err(e) => {
          warn!("Embedding failed: {}", e);
          None
        }
      }
    } else {
      None
    }
  }

  /// Get embeddings for multiple texts in a batch (more efficient for bulk operations)
  ///
  /// Note: Batch embeddings are NOT cached as they're typically used for indexing
  /// (where each chunk is unique) rather than repeated queries.
  pub(crate) async fn get_embeddings_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
    if texts.is_empty() {
      return vec![];
    }
    if let Some(ref provider) = self.embedding {
      match provider.embed_batch(texts).await {
        Ok(vecs) => vecs.into_iter().map(Some).collect(),
        Err(e) => {
          warn!("Batch embedding failed: {}", e);
          vec![None; texts.len()]
        }
      }
    } else {
      vec![None; texts.len()]
    }
  }

  /// Get cache statistics for monitoring
  pub fn embedding_cache_stats(&self) -> (u64, u64) {
    (self.embedding_cache.entry_count(), EMBEDDING_CACHE_SIZE)
  }
}

#[cfg(test)]
pub(crate) fn create_test_handler() -> (tempfile::TempDir, ToolHandler) {
  let data_dir = tempfile::TempDir::new().expect("Failed to create temp dir");
  let registry = Arc::new(ProjectRegistry::with_data_dir(data_dir.path().to_path_buf()));
  let handler = ToolHandler::new(registry);
  (data_dir, handler)
}
