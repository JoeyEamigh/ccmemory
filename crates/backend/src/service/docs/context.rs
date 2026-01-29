//! Document context service.
//!
//! Provides context retrieval for document chunks (adjacent chunks).

use crate::{
  db::ProjectDb,
  ipc::types::docs::{DocContextChunk, DocContextResult, DocContextSections},
  service::util::{Resolver, ServiceError},
};

// ============================================================================
// Context
// ============================================================================

/// Parameters for document context retrieval.
#[derive(Debug, Clone)]
pub struct ContextParams {
  /// Document chunk ID
  pub doc_id: String,
  /// Number of chunks before to include
  pub before: Option<usize>,
  /// Number of chunks after to include
  pub after: Option<usize>,
}

/// Get document context (adjacent chunks around a target chunk).
///
/// # Arguments
/// * `db` - Project database
/// * `params` - Context parameters
///
/// # Returns
/// * `Ok(DocContextResult)` - Context with before, target, and after chunks
/// * `Err(ServiceError)` - If chunk not found or database error
pub async fn get_context(db: &ProjectDb, params: ContextParams) -> Result<DocContextResult, ServiceError> {
  let chunk = Resolver::document_chunk(db, &params.doc_id).await?;

  let before_count = params.before.unwrap_or(2);
  let after_count = params.after.unwrap_or(2);

  // Get adjacent chunks
  let adjacent = db
    .get_adjacent_document_chunks(&chunk.document_id, chunk.chunk_index, before_count, after_count)
    .await
    .unwrap_or_default();

  let mut before_chunks: Vec<DocContextChunk> = Vec::new();
  let mut after_chunks: Vec<DocContextChunk> = Vec::new();

  for adj in adjacent {
    if adj.chunk_index < chunk.chunk_index {
      before_chunks.push(DocContextChunk {
        chunk_index: adj.chunk_index,
        content: adj.content,
      });
    } else if adj.chunk_index > chunk.chunk_index {
      after_chunks.push(DocContextChunk {
        chunk_index: adj.chunk_index,
        content: adj.content,
      });
    }
  }

  Ok(DocContextResult {
    chunk_id: chunk.id.to_string(),
    document_id: chunk.document_id.to_string(),
    title: chunk.title,
    source: chunk.source,
    context: DocContextSections {
      before: before_chunks,
      target: DocContextChunk {
        chunk_index: chunk.chunk_index,
        content: chunk.content,
      },
      after: after_chunks,
    },
    total_chunks: chunk.total_chunks,
  })
}
