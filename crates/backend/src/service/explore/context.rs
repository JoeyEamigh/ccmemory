//! Context retrieval for the explore service.
//!
//! This module provides comprehensive context retrieval for explore results,
//! including code chunks, memories, and documents.

use std::collections::HashSet;

use tracing::debug;

use super::{
  types::{
    CallInfo, CodeContext, ContextResponse, ContextResult, DocChunkEntry, DocContext, ExploreContext, MemoryContext,
    RelatedCodeInfo, RelatedMemoryInfo, SiblingInfo, TimelineContext, TimelineEntry,
  },
  util::truncate_preview,
};
use crate::{
  db::{DbError, ProjectDb},
  domain::{
    code::{ChunkType, CodeChunk},
    document::DocumentChunk,
    memory::{Memory, MemoryId},
  },
  service::util::ServiceError,
};

// ============================================================================
// Full Context Retrieval
// ============================================================================

/// Get comprehensive context for an explore result by ID.
///
/// # Arguments
/// * `ctx` - Explore context with database
/// * `ids` - List of IDs to get context for
/// * `depth` - Context depth (for timeline, adjacent chunks)
///
/// # Returns
/// * `Ok(ContextResponse)` - Context for all IDs
/// * `Err(ServiceError)` - If all lookups fail
pub async fn get_context(
  ctx: &ExploreContext<'_>,
  ids: &[String],
  depth: usize,
) -> Result<ContextResponse, ServiceError> {
  let mut code_contexts: Vec<CodeContext> = Vec::new();
  let mut memory_contexts: Vec<MemoryContext> = Vec::new();
  let mut doc_contexts: Vec<DocContext> = Vec::new();
  let mut errors: Vec<String> = Vec::new();

  for id in ids {
    match fetch_context(ctx.db, id, depth).await {
      Ok(ContextResult::Code(c)) => code_contexts.push(c),
      Ok(ContextResult::Memory(m)) => memory_contexts.push(m),
      Ok(ContextResult::Doc(d)) => doc_contexts.push(d),
      Err(e) => errors.push(format!("{}: {}", id, e)),
    }
  }

  if !errors.is_empty() && code_contexts.is_empty() && memory_contexts.is_empty() && doc_contexts.is_empty() {
    return Err(ServiceError::not_found("items", errors.join("; ")));
  }

  // Build response based on what we found
  let response = if !code_contexts.is_empty() && memory_contexts.is_empty() && doc_contexts.is_empty() {
    ContextResponse::Code { items: code_contexts }
  } else if code_contexts.is_empty() && !memory_contexts.is_empty() && doc_contexts.is_empty() {
    ContextResponse::Memory { items: memory_contexts }
  } else if code_contexts.is_empty() && memory_contexts.is_empty() && !doc_contexts.is_empty() {
    ContextResponse::Doc { items: doc_contexts }
  } else {
    ContextResponse::Mixed {
      code: code_contexts,
      memories: memory_contexts,
      docs: doc_contexts,
    }
  };

  Ok(response)
}

/// Fetch full context for an ID (auto-detects type).
pub async fn fetch_context(db: &ProjectDb, id: &str, depth: usize) -> Result<ContextResult, String> {
  // Validate ID length for prefix matching
  if id.len() < 6 {
    return Err("ID must be at least 6 characters".to_string());
  }

  // Try code chunk first
  match db.get_code_chunk_by_id_or_prefix(id).await {
    Ok(Some(chunk)) => {
      return Ok(ContextResult::Code(build_code_context(db, chunk, depth).await));
    }
    Err(DbError::AmbiguousPrefix { prefix, count }) => {
      return Err(format!(
        "Ambiguous prefix '{}' matches {} items. Use more characters.",
        prefix, count
      ));
    }
    _ => {}
  }

  // Try memory
  if let Ok(memory_id) = id.parse::<MemoryId>()
    && let Ok(Some(memory)) = db.get_memory(&memory_id).await
  {
    return Ok(ContextResult::Memory(build_memory_context(db, memory, depth).await));
  }

  // Try document chunk
  match db.get_document_chunk_by_id_or_prefix(id).await {
    Ok(Some(chunk)) => {
      return Ok(ContextResult::Doc(build_doc_context(db, chunk, depth).await));
    }
    Err(DbError::AmbiguousPrefix { prefix, count }) => {
      return Err(format!(
        "Ambiguous prefix '{}' matches {} items. Use more characters.",
        prefix, count
      ));
    }
    _ => {}
  }

  Err(format!("Item not found: {}", id))
}

// ============================================================================
// Code Context Building
// ============================================================================

/// Build full code context.
async fn build_code_context(db: &ProjectDb, chunk: CodeChunk, depth: usize) -> CodeContext {
  // Fetch all context in parallel for better performance
  let (callers, callees, siblings, memories) = tokio::join!(
    get_callers(db, &chunk, depth),
    get_callees(db, &chunk, depth),
    get_siblings(db, &chunk, depth),
    get_related_memories_for_code(db, &chunk, depth)
  );

  // Extract signature (first line for functions)
  let signature = if chunk.chunk_type == ChunkType::Function {
    chunk.content.lines().next().map(String::from)
  } else {
    None
  };

  CodeContext {
    id: chunk.id.to_string(),
    file: chunk.file_path,
    content: chunk.content,
    language: format!("{:?}", chunk.language).to_lowercase(),
    lines: (chunk.start_line, chunk.end_line),
    symbols: chunk.symbols,
    imports: chunk.imports,
    signature,
    callers,
    callees,
    siblings,
    memories,
  }
}

/// Get callers for a code chunk.
///
/// Uses symbol-based search to find chunks that call symbols defined in this chunk.
/// This is a heuristic approach since we don't maintain a full call graph.
pub async fn get_callers(db: &ProjectDb, chunk: &CodeChunk, limit: usize) -> Vec<CallInfo> {
  let mut callers = Vec::new();
  let mut seen_ids = std::collections::HashSet::new();

  // Search for chunks that call symbols defined in this chunk
  for symbol in &chunk.symbols {
    if callers.len() >= limit {
      break;
    }

    // Find chunks that have this symbol in their calls list
    let filter = format!("calls LIKE '%\"{}%'", symbol.replace('\'', "''"));
    if let Ok(chunks) = db.list_code_chunks(Some(&filter), Some(limit)).await {
      for caller in chunks {
        // Skip self-references and duplicates
        if caller.id == chunk.id || seen_ids.contains(&caller.id.to_string()) {
          continue;
        }
        seen_ids.insert(caller.id.to_string());

        callers.push(CallInfo {
          id: caller.id.to_string(),
          file: caller.file_path.clone(),
          lines: (caller.start_line, caller.end_line),
          preview: truncate_preview(&caller.content, 100),
          symbols: Some(caller.symbols.clone()),
          signature: caller.signature.clone(),
        });

        if callers.len() >= limit {
          break;
        }
      }
    }
  }

  callers
}

/// Get callees for a code chunk.
///
/// Uses the chunk's calls list to find definitions of called symbols.
pub async fn get_callees(db: &ProjectDb, chunk: &CodeChunk, limit: usize) -> Vec<CallInfo> {
  let mut callees = Vec::new();
  let mut seen_symbols = std::collections::HashSet::new();

  // Look up definitions for each called symbol
  for target_symbol in &chunk.calls {
    if callees.len() >= limit {
      break;
    }

    // Skip duplicates
    if seen_symbols.contains(target_symbol) {
      continue;
    }
    seen_symbols.insert(target_symbol.clone());

    // Find chunk that defines this symbol
    let filter = format!("symbols LIKE '%\"{}%'", target_symbol.replace('\'', "''"));
    if let Ok(chunks) = db.list_code_chunks(Some(&filter), Some(1)).await
      && let Some(callee) = chunks.into_iter().next()
      && callee.id != chunk.id
      && callee.symbols.iter().any(|s| s == target_symbol)
    {
      callees.push(CallInfo {
        id: callee.id.to_string(),
        file: callee.file_path.clone(),
        lines: (callee.start_line, callee.end_line),
        preview: truncate_preview(&callee.content, 100),
        symbols: Some(callee.symbols.clone()),
        signature: callee.signature.clone(),
      });
    }
  }

  callees
}

/// Get sibling symbols in the same file.
pub async fn get_siblings(db: &ProjectDb, chunk: &CodeChunk, limit: usize) -> Vec<SiblingInfo> {
  let mut siblings = Vec::new();

  if let Ok(chunks) = db
    .list_code_chunks(
      Some(&format!("file_path = '{}'", chunk.file_path.replace('\'', "''"))),
      None,
    )
    .await
  {
    for sibling in chunks {
      if sibling.id != chunk.id {
        for symbol in &sibling.symbols {
          siblings.push(SiblingInfo {
            symbol: symbol.clone(),
            kind: format!("{:?}", sibling.chunk_type).to_lowercase(),
            line: sibling.start_line,
          });
        }
      }

      if siblings.len() >= limit {
        break;
      }
    }
  }

  siblings.truncate(limit);
  siblings
}

/// Get related memories for a code chunk.
///
/// Uses vector search with the chunk's embedding for semantic matching.
/// Falls back to file/symbol LIKE queries only if no embedding is available.
///
/// This is more efficient (one vector search vs N+1 LIKE queries) and finds
/// semantically related memories even when they don't contain exact symbol names.
pub async fn get_related_memories_for_code(db: &ProjectDb, chunk: &CodeChunk, limit: usize) -> Vec<RelatedMemoryInfo> {
  // Try vector search first using the chunk's stored embedding
  if let Ok(Some(embedding)) = db.get_code_chunk_embedding(&chunk.id).await {
    debug!(
      chunk_id = %chunk.id,
      symbols = ?chunk.symbols,
      "Using vector search to find related memories for code chunk"
    );

    // Single vector search instead of N+1 LIKE queries
    // Use the service layer function which properly filters out deleted memories
    if let Ok(results) = crate::service::memory::search::search_by_embedding(db, &embedding, limit, None).await {
      let memories: Vec<RelatedMemoryInfo> = results
        .into_iter()
        .map(|(memory, _distance)| RelatedMemoryInfo {
          id: memory.id.to_string(),
          content: truncate_preview(&memory.content, 150),
          memory_type: memory
            .memory_type
            .map_or_else(|| "none".to_string(), |t| format!("{:?}", t).to_lowercase()),
          sector: format!("{:?}", memory.sector).to_lowercase(),
        })
        .collect();

      if !memories.is_empty() {
        debug!(
          chunk_id = %chunk.id,
          found = memories.len(),
          "Found related memories via vector search"
        );
        return memories;
      }
    }
  }

  // Fallback: LIKE queries for file name and symbols (when no embedding available)
  debug!(
    chunk_id = %chunk.id,
    "No embedding available, falling back to LIKE queries for related memories"
  );

  let mut memories = Vec::new();
  let mut seen_ids: HashSet<String> = HashSet::new();

  // Search by file name
  let file_name = std::path::Path::new(&chunk.file_path)
    .file_name()
    .map(|s| s.to_string_lossy().to_string())
    .unwrap_or_default();

  if !file_name.is_empty()
    && let Ok(found) = db
      .list_memories(
        Some(&format!(
          "is_deleted = false AND content LIKE '%{}%'",
          file_name.replace('\'', "''")
        )),
        Some(limit),
      )
      .await
  {
    for memory in found {
      let id_str = memory.id.to_string();
      if seen_ids.insert(id_str.clone()) {
        memories.push(RelatedMemoryInfo {
          id: id_str,
          content: truncate_preview(&memory.content, 150),
          memory_type: memory
            .memory_type
            .map_or_else(|| "none".to_string(), |t| format!("{:?}", t).to_lowercase()),
          sector: format!("{:?}", memory.sector).to_lowercase(),
        });
      }
    }
  }

  // Search by primary symbol only (not all symbols to reduce queries)
  if memories.len() < limit
    && let Some(symbol) = chunk.symbols.first()
    && let Ok(found) = db
      .list_memories(
        Some(&format!(
          "is_deleted = false AND content LIKE '%{}%'",
          symbol.replace('\'', "''")
        )),
        Some(limit - memories.len()),
      )
      .await
  {
    for memory in found {
      let id_str = memory.id.to_string();
      if seen_ids.insert(id_str.clone()) {
        memories.push(RelatedMemoryInfo {
          id: id_str,
          content: truncate_preview(&memory.content, 150),
          memory_type: memory
            .memory_type
            .map_or_else(|| "none".to_string(), |t| format!("{:?}", t).to_lowercase()),
          sector: format!("{:?}", memory.sector).to_lowercase(),
        });
      }
    }
  }

  memories.truncate(limit);
  memories
}

// ============================================================================
// Memory Context Building
// ============================================================================

/// Build full memory context.
async fn build_memory_context(db: &ProjectDb, memory: Memory, depth: usize) -> MemoryContext {
  // Fetch timeline, related memories, and related code in parallel
  let (timeline, related, related_code) = tokio::join!(
    get_memory_timeline(db, &memory, depth),
    get_related_memories_for_memory(db, &memory, depth),
    get_related_code_for_memory(db, &memory, depth)
  );

  MemoryContext {
    id: memory.id.to_string(),
    content: memory.content,
    sector: format!("{:?}", memory.sector).to_lowercase(),
    memory_type: memory
      .memory_type
      .map_or_else(|| "none".to_string(), |t| format!("{:?}", t).to_lowercase()),
    salience: memory.salience,
    created_at: memory.created_at.to_rfc3339(),
    timeline,
    related,
    related_code,
  }
}

/// Get timeline around a memory.
async fn get_memory_timeline(db: &ProjectDb, memory: &Memory, depth: usize) -> TimelineContext {
  let mut before = Vec::new();
  let mut after = Vec::new();

  // Get memories before this one
  let before_filter = format!(
    "is_deleted = false AND created_at < '{}' ORDER BY created_at DESC",
    memory.created_at.to_rfc3339()
  );
  if let Ok(memories) = db.list_memories(Some(&before_filter), Some(depth)).await {
    for m in memories {
      before.push(TimelineEntry {
        id: m.id.to_string(),
        content: truncate_preview(&m.content, 100),
        memory_type: m
          .memory_type
          .map_or_else(|| "none".to_string(), |t| format!("{:?}", t).to_lowercase()),
        created_at: m.created_at.to_rfc3339(),
      });
    }
  }

  // Get memories after this one
  let after_filter = format!(
    "is_deleted = false AND created_at > '{}' ORDER BY created_at ASC",
    memory.created_at.to_rfc3339()
  );
  if let Ok(memories) = db.list_memories(Some(&after_filter), Some(depth)).await {
    for m in memories {
      after.push(TimelineEntry {
        id: m.id.to_string(),
        content: truncate_preview(&m.content, 100),
        memory_type: m
          .memory_type
          .map_or_else(|| "none".to_string(), |t| format!("{:?}", t).to_lowercase()),
        created_at: m.created_at.to_rfc3339(),
      });
    }
  }

  TimelineContext { before, after }
}

/// Get related memories for a memory via relationships.
async fn get_related_memories_for_memory(db: &ProjectDb, memory: &Memory, limit: usize) -> Vec<RelatedMemoryInfo> {
  let mut related = Vec::new();

  if let Ok(relationships) = db.get_all_relationships(&memory.id).await {
    for rel in relationships.into_iter().take(limit) {
      let other_id = if rel.from_memory_id == memory.id {
        rel.to_memory_id
      } else {
        rel.from_memory_id
      };

      if let Ok(Some(other)) = db.get_memory(&other_id).await {
        related.push(RelatedMemoryInfo {
          id: other.id.to_string(),
          content: truncate_preview(&other.content, 100),
          memory_type: other
            .memory_type
            .map_or_else(|| "none".to_string(), |t| format!("{:?}", t).to_lowercase()),
          sector: format!("{:?}", other.sector).to_lowercase(),
        });
      }
    }
  }

  related
}

/// Get related code for a memory via cross-domain vector search.
///
/// Uses the memory's embedding to search code chunks. This enables finding
/// code that is semantically related to a memory even if the memory doesn't
/// mention specific symbols or file names.
///
/// Phase 4: Cross-domain search capability.
pub async fn get_related_code_for_memory(db: &ProjectDb, memory: &Memory, limit: usize) -> Vec<RelatedCodeInfo> {
  // Get the memory's embedding for cross-domain search
  let embedding = match db.get_memory_embedding(&memory.id).await {
    Ok(Some(emb)) => emb,
    Ok(None) => {
      debug!(
        memory_id = %memory.id,
        "Memory has no embedding, cannot find related code"
      );
      return Vec::new();
    }
    Err(e) => {
      debug!(
        memory_id = %memory.id,
        error = %e,
        "Failed to get memory embedding for cross-domain search"
      );
      return Vec::new();
    }
  };

  debug!(
    memory_id = %memory.id,
    "Using vector search to find related code for memory"
  );

  // Search code chunks using the memory's embedding
  match db.search_code_chunks(&embedding, limit, None).await {
    Ok(results) => {
      let related: Vec<RelatedCodeInfo> = results
        .into_iter()
        .map(|(chunk, _distance)| RelatedCodeInfo {
          id: chunk.id.to_string(),
          file: chunk.file_path.clone(),
          lines: (chunk.start_line, chunk.end_line),
          preview: truncate_preview(&chunk.content, 150),
          symbols: chunk.symbols.clone(),
          language: Some(format!("{:?}", chunk.language).to_lowercase()),
        })
        .collect();

      debug!(
        memory_id = %memory.id,
        found = related.len(),
        "Found related code via cross-domain vector search"
      );

      related
    }
    Err(e) => {
      debug!(
        memory_id = %memory.id,
        error = %e,
        "Failed to search code chunks for memory"
      );
      Vec::new()
    }
  }
}

// ============================================================================
// Document Context Building
// ============================================================================

/// Build full document context.
async fn build_doc_context(db: &ProjectDb, chunk: DocumentChunk, depth: usize) -> DocContext {
  let mut before = Vec::new();
  let mut after = Vec::new();

  // Get adjacent chunks
  if let Ok(chunks) = db
    .get_adjacent_document_chunks(&chunk.document_id, chunk.chunk_index, depth, depth)
    .await
  {
    for adj in chunks {
      if adj.chunk_index < chunk.chunk_index {
        before.push(DocChunkEntry {
          chunk_index: adj.chunk_index,
          content: adj.content,
        });
      } else if adj.chunk_index > chunk.chunk_index {
        after.push(DocChunkEntry {
          chunk_index: adj.chunk_index,
          content: adj.content,
        });
      }
    }
  }

  DocContext {
    id: chunk.id.to_string(),
    title: chunk.title,
    content: chunk.content,
    source: chunk.source,
    chunk_index: chunk.chunk_index,
    total_chunks: chunk.total_chunks,
    before,
    after,
  }
}
