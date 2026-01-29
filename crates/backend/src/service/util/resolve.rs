//! Generic ID/prefix resolution for all entity types.
//!
//! This module provides a unified pattern for resolving entities by ID or prefix,
//! replacing the duplicated resolution logic across handlers.

use std::fmt;

use crate::{
  db::{DbError, ProjectDb},
  domain::{code::CodeChunk, document::DocumentChunk, memory::Memory},
};

/// Error type for resolution operations.
#[derive(Debug)]
pub enum ResolveError {
  /// Item was not found.
  NotFound { item_type: &'static str, id: String },
  /// ID prefix is ambiguous (matches multiple items).
  Ambiguous { prefix: String, count: usize },
  /// Input validation failed.
  InvalidInput(String),
  /// Database error.
  Database(String),
}

impl fmt::Display for ResolveError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::NotFound { item_type, id } => write!(f, "{} not found: {}", item_type, id),
      Self::Ambiguous { prefix, count } => {
        write!(
          f,
          "Ambiguous prefix '{}' matches {} items. Use more characters.",
          prefix, count
        )
      }
      Self::InvalidInput(msg) => write!(f, "{}", msg),
      Self::Database(msg) => write!(f, "Database error: {}", msg),
    }
  }
}

impl std::error::Error for ResolveError {}

impl From<DbError> for ResolveError {
  fn from(e: DbError) -> Self {
    match e {
      DbError::AmbiguousPrefix { prefix, count } => Self::Ambiguous { prefix, count },
      DbError::InvalidInput(msg) => Self::InvalidInput(msg),
      other => Self::Database(other.to_string()),
    }
  }
}

/// Resolver for looking up entities by ID or prefix.
///
/// Provides a consistent interface for resolving memories, code chunks,
/// document chunks, and other entities by their full ID or a unique prefix.
pub struct Resolver;

impl Resolver {
  /// Resolve a memory by ID or prefix.
  ///
  /// Tries exact match first, then falls back to prefix matching.
  ///
  /// # Arguments
  /// * `db` - The project database
  /// * `id_or_prefix` - Full ID or unique prefix (minimum 6 characters)
  ///
  /// # Returns
  /// * `Ok(Memory)` - The resolved memory
  /// * `Err(ResolveError)` - Resolution failed
  pub async fn memory(db: &ProjectDb, id_or_prefix: &str) -> Result<Memory, ResolveError> {
    match db.get_memory_by_id_or_prefix(id_or_prefix).await {
      Ok(Some(memory)) => Ok(memory),
      Ok(None) => Err(ResolveError::NotFound {
        item_type: "Memory",
        id: id_or_prefix.to_string(),
      }),
      Err(e) => Err(e.into()),
    }
  }

  /// Resolve a code chunk by ID or prefix.
  ///
  /// Tries exact match first, then falls back to prefix matching.
  ///
  /// # Arguments
  /// * `db` - The project database
  /// * `id_or_prefix` - Full ID or unique prefix (minimum 6 characters)
  ///
  /// # Returns
  /// * `Ok(CodeChunk)` - The resolved code chunk
  /// * `Err(ResolveError)` - Resolution failed
  pub async fn code_chunk(db: &ProjectDb, id_or_prefix: &str) -> Result<CodeChunk, ResolveError> {
    match db.get_code_chunk_by_id_or_prefix(id_or_prefix).await {
      Ok(Some(chunk)) => Ok(chunk),
      Ok(None) => Err(ResolveError::NotFound {
        item_type: "Code chunk",
        id: id_or_prefix.to_string(),
      }),
      Err(e) => Err(e.into()),
    }
  }

  /// Resolve a document chunk by ID or prefix.
  ///
  /// Tries exact match first, then falls back to prefix matching.
  ///
  /// # Arguments
  /// * `db` - The project database
  /// * `id_or_prefix` - Full ID or unique prefix (minimum 6 characters)
  ///
  /// # Returns
  /// * `Ok(DocumentChunk)` - The resolved document chunk
  /// * `Err(ResolveError)` - Resolution failed
  pub async fn document_chunk(db: &ProjectDb, id_or_prefix: &str) -> Result<DocumentChunk, ResolveError> {
    match db.get_document_chunk_by_id_or_prefix(id_or_prefix).await {
      Ok(Some(chunk)) => Ok(chunk),
      Ok(None) => Err(ResolveError::NotFound {
        item_type: "Document chunk",
        id: id_or_prefix.to_string(),
      }),
      Err(e) => Err(e.into()),
    }
  }

  /// Try to resolve any entity type by ID or prefix.
  ///
  /// Attempts resolution in order: code chunk, memory, document chunk, entity.
  /// Returns the first successful match.
  ///
  /// # Arguments
  /// * `db` - The project database
  /// * `id_or_prefix` - Full ID or unique prefix
  ///
  /// # Returns
  /// * `Ok(ResolvedEntity)` - The resolved entity with its type
  /// * `Err(ResolveError)` - Resolution failed for all types
  pub async fn any(db: &ProjectDb, id_or_prefix: &str) -> Result<ResolvedEntity, ResolveError> {
    // Validate minimum length for prefix matching
    if id_or_prefix.len() < 6 {
      return Err(ResolveError::InvalidInput(
        "ID must be at least 6 characters".to_string(),
      ));
    }

    // Try code chunk first (most common case)
    if let Ok(chunk) = Self::code_chunk(db, id_or_prefix).await {
      return Ok(ResolvedEntity::Code(chunk));
    }

    // Try memory
    if let Ok(memory) = Self::memory(db, id_or_prefix).await {
      return Ok(ResolvedEntity::Memory(memory));
    }

    // Try document chunk
    if let Ok(chunk) = Self::document_chunk(db, id_or_prefix).await {
      return Ok(ResolvedEntity::Document(chunk));
    }

    Err(ResolveError::NotFound {
      item_type: "Item",
      id: id_or_prefix.to_string(),
    })
  }
}

/// Result of resolving an entity of unknown type.
pub enum ResolvedEntity {
  Memory(Memory),
  Code(CodeChunk),
  Document(DocumentChunk),
}

impl ResolvedEntity {
  /// Get the entity type as a string.
  pub fn entity_type(&self) -> &'static str {
    match self {
      Self::Memory(_) => "memory",
      Self::Code(_) => "code",
      Self::Document(_) => "document",
    }
  }

  /// Get the entity ID as a string.
  pub fn id(&self) -> String {
    match self {
      Self::Memory(m) => m.id.to_string(),
      Self::Code(c) => c.id.to_string(),
      Self::Document(d) => d.id.to_string(),
    }
  }
}
