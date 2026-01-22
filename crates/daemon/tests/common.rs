//! Common test utilities for daemon integration tests
//!
//! These tests verify end-to-end functionality of the daemon, tools, and database.
//!
//! Note: These tests expect Ollama to be running locally with the nomic-embed-text model.
//! Run: ollama pull nomic-embed-text

use daemon::{ProjectRegistry, Router};
use embedding::{EmbeddingProvider, OllamaProvider};
use std::sync::Arc;
use tempfile::TempDir;

/// Create a router with Ollama embedding and isolated temp directories
#[allow(dead_code)]
pub fn create_test_router() -> (TempDir, TempDir, Router) {
  let data_dir = TempDir::new().expect("Failed to create data temp dir");
  let project_dir = TempDir::new().expect("Failed to create project temp dir");

  let registry = Arc::new(ProjectRegistry::with_data_dir(data_dir.path().to_path_buf()));
  let embedding: Arc<dyn EmbeddingProvider> = Arc::new(OllamaProvider::new());
  let router = Router::with_embedding(registry, embedding);

  (data_dir, project_dir, router)
}
