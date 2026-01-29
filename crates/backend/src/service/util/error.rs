//! Unified error types for service operations.
//!
//! This module provides a standard error type that can be used across all
//! services and handlers, with proper conversion to IPC error codes.

use crate::{db::DbError, embedding::EmbeddingError};

/// Unified error type for service operations.
///
/// This enum provides a consistent error handling pattern across all services,
/// with automatic conversion to appropriate IPC error codes.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
  /// Item was not found in the database.
  #[error("{item_type} not found: {id}")]
  NotFound { item_type: &'static str, id: String },
  /// ID prefix matches multiple items.
  #[error("Ambiguous prefix '{prefix}' matches {count} items")]
  Ambiguous { prefix: String, count: usize },
  /// Input validation failed.
  #[error("Validation error: {0}")]
  Validation(String),
  /// Database operation failed.
  #[error("Database error: {0}")]
  Database(#[from] DbError),
  /// Embedding operation failed.
  #[error("Embedding error: {0}")]
  Embedding(#[from] EmbeddingError),
  /// Project initialization or access failed.
  #[error("Project error: {0}")]
  Project(String),
  #[error("Error using the LLM service: {0}")]
  Llm(#[from] llm::LlmError),
  /// Internal processing error.
  #[error("Internal error: {0}")]
  Internal(String),
}

impl ServiceError {
  /// Get the IPC error code for this error type.
  ///
  /// Error codes follow JSON-RPC conventions:
  /// - `-32602`: Invalid params (validation errors)
  /// - `-32000`: Server error (all other errors)
  pub fn code(&self) -> i32 {
    match self {
      Self::Validation(_) => -32602,
      _ => -32000,
    }
  }

  /// Create a not-found error.
  pub fn not_found(item_type: &'static str, id: impl Into<String>) -> Self {
    Self::NotFound {
      item_type,
      id: id.into(),
    }
  }

  /// Create a validation error.
  pub fn validation(msg: impl Into<String>) -> Self {
    Self::Validation(msg.into())
  }

  /// Create a project error.
  pub fn project(msg: impl Into<String>) -> Self {
    Self::Project(msg.into())
  }

  /// Create an internal error.
  pub fn internal(msg: impl Into<String>) -> Self {
    Self::Internal(msg.into())
  }
}

impl From<super::resolve::ResolveError> for ServiceError {
  fn from(e: super::resolve::ResolveError) -> Self {
    match e {
      super::resolve::ResolveError::NotFound { item_type, id } => Self::NotFound { item_type, id },
      super::resolve::ResolveError::Ambiguous { prefix, count } => Self::Ambiguous { prefix, count },
      super::resolve::ResolveError::InvalidInput(msg) => Self::Validation(msg),
      super::resolve::ResolveError::Database(msg) => Self::Database(DbError::Query(msg)),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_error_codes() {
    assert_eq!(ServiceError::validation("test").code(), -32602);
    assert_eq!(ServiceError::not_found("memory", "abc123").code(), -32000);
    assert_eq!(
      ServiceError::Ambiguous {
        prefix: "abc".to_string(),
        count: 5
      }
      .code(),
      -32000
    );
  }
}
