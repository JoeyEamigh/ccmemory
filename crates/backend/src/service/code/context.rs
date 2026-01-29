//! Code context retrieval: callers, callees, siblings, and full context.
//!
//! This module provides services for retrieving context around code chunks,
//! including call graph navigation and related memories.

use std::{collections::HashSet, path::Path};

use super::search::CodeContext;
use crate::{
  db::ProjectDb,
  domain::code::CodeChunk,
  ipc::types::{
    code::{
      CodeCalleeItem, CodeCalleesResponse, CodeCallersResponse, CodeContextFullResponse, CodeContextResponse,
      CodeContextSection, CodeContextSections, CodeFullDoc, CodeItem, CodeRelatedItem, CodeRelatedResponse,
    },
    memory::MemoryItem,
  },
  service::util::{Resolver, ServiceError},
};

// ============================================================================
// Call Graph Navigation
// ============================================================================

/// Get chunks that call symbols defined in a given chunk.
///
/// Uses LIKE queries to find chunks that have symbols in their calls list.
///
/// # Arguments
/// * `db` - Project database
/// * `symbols` - Symbols to find callers for
/// * `exclude_id` - Optional chunk ID to exclude (usually the source chunk)
/// * `limit` - Maximum number of results
///
/// # Returns
/// List of caller chunks
pub async fn get_callers(
  db: &ProjectDb,
  symbols: &[String],
  exclude_id: Option<uuid::Uuid>,
  limit: usize,
) -> Result<Vec<CodeChunk>, ServiceError> {
  let mut callers = Vec::new();
  let mut seen_ids: HashSet<uuid::Uuid> = HashSet::new();

  if let Some(id) = exclude_id {
    seen_ids.insert(id);
  }

  // Find chunks that call any of these symbols
  for symbol in symbols {
    if callers.len() >= limit {
      break;
    }

    let filter = format!("calls LIKE '%\"{}%'", symbol.replace('\'', "''"));
    if let Ok(chunks) = db.list_code_chunks(Some(&filter), Some(limit)).await {
      for caller in chunks {
        if seen_ids.insert(caller.id) {
          callers.push(caller);
          if callers.len() >= limit {
            break;
          }
        }
      }
    }
  }

  Ok(callers)
}

/// Get the definitions of functions/methods that a chunk calls.
///
/// Uses symbol lookup to find definitions of called symbols.
///
/// # Arguments
/// * `db` - Project database
/// * `chunk_id` - ID of the chunk to find callees for
/// * `exclude_id` - Optional chunk ID to exclude
/// * `limit` - Maximum number of results
///
/// # Returns
/// Tuple of (resolved callees, unresolved call names)
pub async fn get_callees(
  db: &ProjectDb,
  chunk_id: &str,
  exclude_id: Option<uuid::Uuid>,
  limit: usize,
) -> Result<(Vec<(String, CodeChunk)>, Vec<String>), ServiceError> {
  let mut callees = Vec::new();
  let mut unresolved = Vec::new();
  let mut seen_symbols: HashSet<String> = HashSet::new();

  // Get the chunk to access its calls list
  let chunk = match db.get_code_chunk_by_id_or_prefix(chunk_id).await? {
    Some(c) => c,
    None => return Ok((callees, unresolved)),
  };

  // Look up definitions for each called symbol
  for target_symbol in &chunk.calls {
    if callees.len() >= limit {
      break;
    }

    if seen_symbols.contains(target_symbol) {
      continue;
    }
    seen_symbols.insert(target_symbol.clone());

    // Find chunk that defines this symbol
    let filter = format!("symbols LIKE '%\"{}%'", target_symbol.replace('\'', "''"));
    if let Ok(chunks) = db.list_code_chunks(Some(&filter), Some(1)).await
      && let Some(callee) = chunks.into_iter().next()
      && callee.symbols.iter().any(|s| s == target_symbol)
    {
      if exclude_id != Some(callee.id) {
        callees.push((target_symbol.clone(), callee));
      }
    } else {
      unresolved.push(target_symbol.clone());
    }
  }

  Ok((callees, unresolved))
}

/// Get sibling chunks in the same file.
///
/// # Arguments
/// * `db` - Project database
/// * `file_path` - Path of the file
/// * `exclude_id` - Optional chunk ID to exclude
/// * `limit` - Maximum number of results
///
/// # Returns
/// List of sibling chunks
pub async fn get_siblings(
  db: &ProjectDb,
  file_path: &str,
  exclude_id: Option<uuid::Uuid>,
  limit: usize,
) -> Result<Vec<CodeChunk>, ServiceError> {
  let filter = format!("file_path = '{}'", file_path.replace('\'', "''"));
  let chunks = db.list_code_chunks(Some(&filter), None).await?;

  let siblings: Vec<CodeChunk> = chunks
    .into_iter()
    .filter(|c| exclude_id != Some(c.id))
    .take(limit)
    .collect();

  Ok(siblings)
}

/// Get memories related to a code chunk (by file path or symbol mentions).
///
/// # Arguments
/// * `db` - Project database
/// * `file_path` - Path of the file
/// * `symbols` - Symbols defined in the chunk
/// * `limit` - Maximum number of results
///
/// # Returns
/// List of related memories
pub async fn get_related_memories(
  db: &ProjectDb,
  file_path: &str,
  symbols: &[String],
  limit: usize,
) -> Result<Vec<crate::domain::memory::Memory>, ServiceError> {
  let mut memories = Vec::new();
  let mut seen_ids = HashSet::new();

  // Search by file name
  let file_name = std::path::Path::new(file_path)
    .file_name()
    .map(|s| s.to_string_lossy().to_string())
    .unwrap_or_default();

  if !file_name.is_empty() {
    let filter = format!(
      "is_deleted = false AND content LIKE '%{}%'",
      file_name.replace('\'', "''")
    );
    if let Ok(found) = db.list_memories(Some(&filter), Some(limit)).await {
      for m in found {
        if seen_ids.insert(m.id) {
          memories.push(m);
        }
      }
    }
  }

  // Search by symbol names
  for symbol in symbols {
    if memories.len() >= limit {
      break;
    }

    let filter = format!("is_deleted = false AND content LIKE '%{}%'", symbol.replace('\'', "''"));
    if let Ok(found) = db.list_memories(Some(&filter), Some(limit - memories.len())).await {
      for m in found {
        if seen_ids.insert(m.id) {
          memories.push(m);
        }
      }
    }
  }

  memories.truncate(limit);
  Ok(memories)
}

// ============================================================================
// Full Context
// ============================================================================

/// Parameters for full context retrieval.
#[derive(Debug, Clone, Default)]
pub struct ContextFullParams {
  /// Chunk ID to get context for
  pub chunk_id: String,
  /// Depth for each context section
  pub depth: Option<usize>,
}

/// Get comprehensive context for a code chunk in one call.
///
/// Returns callers, callees, siblings, related memories, and documentation.
///
/// # Arguments
/// * `ctx` - Code context with database and embedding provider
/// * `params` - Context parameters
///
/// # Returns
/// Full context response with all sections
pub async fn get_full_context(
  ctx: &CodeContext<'_>,
  params: ContextFullParams,
) -> Result<CodeContextFullResponse, ServiceError> {
  let depth = params.depth.unwrap_or(5);

  // Resolve the chunk
  let chunk = Resolver::code_chunk(ctx.db, &params.chunk_id).await?;
  let chunk_id = chunk.id;
  let chunk_id_string = chunk_id.to_string();

  // Fetch all context in parallel
  let (callers_result, callees_result, siblings_result, memories_result, docs_result) = tokio::join!(
    get_callers(ctx.db, &chunk.symbols, Some(chunk_id), depth),
    get_callees(ctx.db, &chunk_id_string, Some(chunk_id), depth),
    get_siblings(ctx.db, &chunk.file_path, Some(chunk_id), depth),
    get_related_memories(ctx.db, &chunk.file_path, &chunk.symbols, depth),
    get_related_docs(ctx, &chunk.content, depth)
  );

  // Convert callers
  let callers: Vec<CodeItem> = callers_result
    .unwrap_or_default()
    .into_iter()
    .map(|c| CodeItem::from_caller(&c))
    .collect();

  // Convert callees
  let (callees_chunks, unresolved_calls) = callees_result.unwrap_or_default();
  let callees: Vec<CodeCalleeItem> = callees_chunks
    .into_iter()
    .map(|(call, c)| CodeCalleeItem::from_chunk_with_call(&c, &call))
    .collect();

  // Convert siblings
  let same_file: Vec<CodeItem> = siblings_result
    .unwrap_or_default()
    .into_iter()
    .map(|c| {
      let mut item = CodeItem::from_caller(&c);
      item.chunk_type = Some(format!("{:?}", c.chunk_type).to_lowercase());
      item
    })
    .collect();

  // Convert memories
  let memories: Vec<MemoryItem> = memories_result
    .unwrap_or_default()
    .into_iter()
    .map(|m| MemoryItem::from_list(&m))
    .collect();

  // Documentation
  let documentation = docs_result.unwrap_or_default();

  // Build the chunk item with full details
  let chunk_item = CodeItem::from_detail(&chunk);

  Ok(CodeContextFullResponse {
    chunk: chunk_item,
    callers,
    callees,
    unresolved_calls,
    same_file,
    memories,
    documentation,
  })
}

/// Get documentation related to code content via semantic search.
async fn get_related_docs(
  ctx: &CodeContext<'_>,
  content: &str,
  limit: usize,
) -> Result<Vec<CodeFullDoc>, ServiceError> {
  let mut docs = Vec::new();

  let query_vec = ctx.get_embedding(content).await?;
  if let Ok(results) = ctx.db.search_documents(&query_vec, limit, None).await {
    for (doc, distance) in results {
      docs.push(CodeFullDoc {
        id: doc.id.to_string(),
        title: doc.title,
        content: doc.content,
        similarity: 1.0 - distance.min(1.0),
      });
    }
  }

  Ok(docs)
}

// ============================================================================
// Callers/Callees Handlers
// ============================================================================

/// Parameters for callers query.
#[derive(Debug, Clone)]
pub struct CallersParams {
  /// Chunk ID or symbol name
  pub chunk_id: Option<String>,
  /// Direct symbol name
  pub symbol: Option<String>,
  /// Maximum results
  pub limit: Option<usize>,
}

/// Get callers for a chunk or symbol.
pub async fn get_callers_response(db: &ProjectDb, params: CallersParams) -> Result<CodeCallersResponse, ServiceError> {
  let limit = params.limit.unwrap_or(20);

  // Resolve the symbol
  let symbol = if let Some(ref chunk_id) = params.chunk_id {
    let chunk = Resolver::code_chunk(db, chunk_id).await?;
    chunk
      .symbols
      .first()
      .cloned()
      .ok_or_else(|| ServiceError::validation("Chunk has no symbols"))?
  } else if let Some(ref sym) = params.symbol {
    sym.clone()
  } else {
    return Err(ServiceError::validation("Must provide chunk_id or symbol"));
  };

  // Find chunks that call this symbol
  let filter = format!("calls LIKE '%\"{}%'", symbol.replace('\'', "''"));
  let callers = db.list_code_chunks(Some(&filter), Some(limit)).await?;

  let items: Vec<CodeItem> = callers.into_iter().map(|c| CodeItem::from_caller(&c)).collect();

  let count = items.len();
  Ok(CodeCallersResponse {
    symbol,
    callers: items,
    count,
  })
}

/// Parameters for callees query.
#[derive(Debug, Clone)]
pub struct CalleesParams {
  /// Chunk ID
  pub chunk_id: String,
  /// Maximum results per call
  pub limit: Option<usize>,
}

/// Get callees for a chunk.
pub async fn get_callees_response(db: &ProjectDb, params: CalleesParams) -> Result<CodeCalleesResponse, ServiceError> {
  let limit_per_call = params.limit.unwrap_or(3);

  let chunk = Resolver::code_chunk(db, &params.chunk_id).await?;

  if chunk.calls.is_empty() {
    return Ok(CodeCalleesResponse {
      chunk_id: chunk.id.to_string(),
      calls: chunk.calls,
      callees: vec![],
      unresolved: vec![],
    });
  }

  let mut callees: Vec<CodeCalleeItem> = Vec::new();
  let mut unresolved = Vec::new();
  let mut seen_ids = HashSet::new();

  for call in &chunk.calls {
    let filter = format!("symbols LIKE '%\"{}%'", call.replace('\'', "''"));
    match db.list_code_chunks(Some(&filter), Some(limit_per_call)).await {
      Ok(matches) => {
        if matches.is_empty() {
          unresolved.push(call.clone());
        } else {
          for m in matches {
            if seen_ids.insert(m.id) {
              callees.push(CodeCalleeItem::from_chunk_with_call(&m, call));
            }
          }
        }
      }
      Err(_) => {
        unresolved.push(call.clone());
      }
    }
  }

  Ok(CodeCalleesResponse {
    chunk_id: chunk.id.to_string(),
    calls: chunk.calls,
    callees,
    unresolved,
  })
}

// ============================================================================
// Related Code
// ============================================================================

/// Parameters for related code query.
#[derive(Debug, Clone)]
pub struct RelatedParams {
  /// Chunk ID
  pub chunk_id: String,
  /// Relationship methods to use
  pub methods: Option<Vec<String>>,
  /// Maximum results
  pub limit: Option<usize>,
}

/// Get code related to a chunk via multiple methods.
///
/// Methods: same_file, shared_imports, similar, callers, callees
pub async fn get_related(ctx: &CodeContext<'_>, params: RelatedParams) -> Result<CodeRelatedResponse, ServiceError> {
  let limit = params.limit.unwrap_or(20);

  let chunk = Resolver::code_chunk(ctx.db, &params.chunk_id).await?;

  let methods: Vec<&str> = params
    .methods
    .as_ref()
    .map(|m| m.iter().map(|s| s.as_str()).collect())
    .unwrap_or_else(|| vec!["same_file", "shared_imports", "similar"]);

  let mut related: Vec<(CodeChunk, f32, String)> = Vec::new();
  let mut seen_ids = HashSet::new();
  seen_ids.insert(chunk.id);

  for method in methods {
    match method {
      "same_file" => {
        if let Ok(siblings) = get_siblings(ctx.db, &chunk.file_path, Some(chunk.id), limit).await {
          for s in siblings {
            if seen_ids.insert(s.id) {
              related.push((s, 0.9, "same_file".to_string()));
            }
          }
        }
      }
      "shared_imports" => {
        for import in &chunk.imports {
          let filter = format!("imports LIKE '%{}%'", import.replace('\'', "''"));
          if let Ok(matches) = ctx.db.list_code_chunks(Some(&filter), Some(10)).await {
            for m in matches {
              if seen_ids.insert(m.id) {
                related.push((m, 0.7, format!("imports:{}", import)));
              }
            }
          }
        }
      }
      "similar" => {
        let query_vec = ctx.get_embedding(&chunk.content).await?;
        if let Ok(similar) = ctx.db.search_code_chunks(&query_vec, 10, None).await {
          for (c, distance) in similar {
            if seen_ids.insert(c.id) {
              let similarity = 1.0 - distance.min(1.0);
              related.push((c, similarity, "similar".to_string()));
            }
          }
        }
      }
      "callers" => {
        if let Some(symbol) = chunk.symbols.first() {
          let filter = format!("calls LIKE '%\"{}%'", symbol.replace('\'', "''"));
          if let Ok(callers) = ctx.db.list_code_chunks(Some(&filter), Some(10)).await {
            for c in callers {
              if seen_ids.insert(c.id) {
                related.push((c, 0.8, "caller".to_string()));
              }
            }
          }
        }
      }
      "callees" => {
        for call in &chunk.calls {
          let filter = format!("symbols LIKE '%\"{}%'", call.replace('\'', "''"));
          if let Ok(matches) = ctx.db.list_code_chunks(Some(&filter), Some(5)).await {
            for m in matches {
              if seen_ids.insert(m.id) {
                related.push((m, 0.8, format!("callee:{}", call)));
              }
            }
          }
        }
      }
      _ => {}
    }
  }

  // Sort by score descending and truncate
  related.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
  related.truncate(limit);

  let results: Vec<CodeRelatedItem> = related
    .into_iter()
    .map(|(c, score, relationship)| CodeRelatedItem::from_chunk_with_score(&c, score, &relationship))
    .collect();

  let count = results.len();
  Ok(CodeRelatedResponse {
    chunk_id: chunk.id.to_string(),
    file_path: chunk.file_path,
    symbols: chunk.symbols,
    related: results,
    count,
  })
}

// ============================================================================
// File Context
// ============================================================================

/// Parameters for file context retrieval.
#[derive(Debug, Clone)]
pub struct FileContextParams {
  /// Chunk ID to get context for
  pub chunk_id: String,
  /// Number of lines before the chunk to include
  pub before: Option<usize>,
  /// Number of lines after the chunk to include
  pub after: Option<usize>,
}

/// Get file context around a code chunk (lines before and after).
///
/// Reads the actual source file from disk and extracts lines around
/// the specified chunk.
///
/// # Arguments
/// * `db` - Project database
/// * `root_path` - Project root directory for resolving file paths
/// * `params` - Parameters including chunk_id and line counts
///
/// # Returns
/// * `Ok(CodeContextResponse)` - File context with before, target, and after sections
/// * `Err(ServiceError)` - If chunk not found, file not readable, or database error
pub async fn get_file_context(
  db: &ProjectDb,
  root_path: &Path,
  params: FileContextParams,
) -> Result<CodeContextResponse, ServiceError> {
  let chunk = Resolver::code_chunk(db, &params.chunk_id).await?;

  let before_lines = params.before.unwrap_or(10);
  let after_lines = params.after.unwrap_or(10);

  // Read the file and extract context
  let file_path = root_path.join(&chunk.file_path);
  let content = tokio::fs::read_to_string(&file_path)
    .await
    .map_err(|e| ServiceError::project(format!("Failed to read file: {}", e)))?;

  let lines: Vec<&str> = content.lines().collect();
  let total_lines = lines.len();

  let start = (chunk.start_line as usize).saturating_sub(1);
  let end = (chunk.end_line as usize).min(total_lines);

  // Before section
  let before_start = start.saturating_sub(before_lines);
  let before_content = lines[before_start..start].join("\n");

  // Target section
  let target_content = lines[start..end].join("\n");

  // After section
  let after_end = (end + after_lines).min(total_lines);
  let after_content = lines[end..after_end].join("\n");

  Ok(CodeContextResponse {
    chunk_id: chunk.id.to_string(),
    file_path: chunk.file_path,
    language: format!("{:?}", chunk.language).to_lowercase(),
    context: CodeContextSections {
      before: CodeContextSection {
        content: before_content,
        start_line: before_start + 1,
        end_line: start,
      },
      target: CodeContextSection {
        content: target_content,
        start_line: start + 1,
        end_line: end,
      },
      after: CodeContextSection {
        content: after_content,
        start_line: end + 1,
        end_line: after_end,
      },
    },
    total_file_lines: total_lines,
    warning: None,
  })
}
