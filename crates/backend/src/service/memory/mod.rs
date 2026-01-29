//! Memory service layer.
//!
//! This module provides the business logic for memory operations,
//! cleanly separated from handler request/response handling.
//!
//! ## Design Principles
//!
//! - **No side effects in reads**: Search operations do NOT auto-reinforce.
//!   If you want to track access, call `reinforce` explicitly.
//! - **Static methods**: Services are stateless; all dependencies passed as parameters.
//! - **Service errors**: Operations return `Result<T, ServiceError>` for clean error handling.
//!
//! ## Available Operations
//!
//! - [`search`] - Search memories with vector/text fallback and ranking
//! - [`add`] - Add a memory with duplicate detection
//! - [`get`] - Get a memory by ID or prefix
//! - [`list`] - List memories with filters
//! - [`delete`] - Soft or hard delete a memory
//! - [`restore`] - Restore a soft-deleted memory
//! - [`lifecycle`] - Reinforce, deemphasize, and supersede operations
//! - [`relationship`] - Add, delete, and list memory relationships

mod dedup;
mod lifecycle;
mod ranking;
pub mod search;

pub mod relationship;

use std::collections::HashSet;

use chrono::Utc;
use tracing::debug;
use uuid::Uuid;

pub use self::{
  dedup::check_duplicate,
  lifecycle::{deemphasize, reinforce, set_salience, supersede},
  ranking::RankingConfig,
  search::search,
};
use super::util::{FilterBuilder, Resolver};
pub use crate::context::memory::extract::decay::{DecayStats, MemoryDecay};
use crate::{
  context::memory::extract::{
    classifier::{extract_concepts, extract_files},
    dedup::compute_hashes,
  },
  db::ProjectDb,
  domain::memory::{Memory, MemoryType, Sector},
  embedding::EmbeddingProvider,
  ipc::types::memory::{
    MemoryAddParams, MemoryAddResult, MemoryFullDetail, MemoryGetParams, MemoryItem, MemoryListParams,
    MemoryRelatedItem, MemoryRelatedParams, MemoryRelatedResult, MemoryRelationshipItem, MemoryTimelineItem,
    MemoryTimelineResult,
  },
  service::util::ServiceError,
};

/// Context for memory service operations.
///
/// Contains all dependencies needed for memory operations.
pub struct MemoryContext<'a> {
  /// Project database connection
  pub db: &'a ProjectDb,
  /// Optional embedding provider for vector search
  pub embedding: &'a dyn EmbeddingProvider,
  /// Project ID for new memories
  pub project_id: Uuid,
}

impl<'a> MemoryContext<'a> {
  /// Create a new memory context
  pub fn new(db: &'a ProjectDb, embedding: &'a dyn EmbeddingProvider, project_id: Uuid) -> Self {
    Self {
      db,
      embedding,
      project_id,
    }
  }

  /// Get an embedding for the given text, if a provider is available
  async fn get_embedding(&self, text: &str) -> Result<Vec<f32>, ServiceError> {
    // Query mode - this is used for memory search queries
    Ok(
      self
        .embedding
        .embed(text, crate::embedding::EmbeddingMode::Query)
        .await?,
    )
  }
}

// ============================================================================
// Core Operations
// ============================================================================

/// Add a new memory with duplicate detection.
///
/// # Arguments
/// * `ctx` - Memory context with database and embedding provider
/// * `params` - Parameters for the new memory
///
/// # Returns
/// * `Ok(MemoryAddResult)` - Result with the new or existing (if duplicate) memory ID
/// * `Err(ServiceError)` - If validation or database operation fails
pub async fn add(ctx: &MemoryContext<'_>, params: MemoryAddParams) -> Result<MemoryAddResult, ServiceError> {
  // Validate content length
  if params.content.len() < 5 {
    return Err(ServiceError::validation("Content too short (min 5 chars)"));
  }
  if params.content.len() > 32000 {
    return Err(ServiceError::validation("Content too long (max 32000 chars)"));
  }

  // Parse sector
  let sector = params
    .sector
    .as_deref()
    .and_then(|s| s.parse::<Sector>().ok())
    .unwrap_or(Sector::Semantic);

  // Parse memory type
  let memory_type = params.memory_type.as_deref().and_then(|t| t.parse::<MemoryType>().ok());

  // Compute hashes for deduplication
  let (content_hash, simhash) = compute_hashes(&params.content);

  // Check for duplicates
  if let Some(duplicate) = check_duplicate(ctx, &params.content, &content_hash, simhash).await? {
    return Ok(MemoryAddResult {
      id: duplicate.id,
      message: format!("Duplicate detected: {}", duplicate.reason),
      is_duplicate: true,
    });
  }

  // Create the new memory
  let mut memory = Memory::new(ctx.project_id, params.content.clone(), sector);

  // Set hashes
  memory.content_hash = content_hash;
  memory.simhash = simhash;

  // Extract concepts and files from content
  memory.concepts = extract_concepts(&params.content);
  memory.files = extract_files(&params.content);

  // Apply optional fields
  memory.memory_type = memory_type;
  if let Some(ctx_str) = params.context {
    memory.context = Some(ctx_str);
  }
  if let Some(tags) = params.tags {
    memory.tags = tags;
  }
  if let Some(categories) = params.categories {
    memory.categories = categories;
  }
  if let Some(scope_path) = params.scope_path {
    memory.scope_path = Some(scope_path);
  }
  if let Some(scope_module) = params.scope_module {
    memory.scope_module = Some(scope_module);
  }
  if let Some(imp) = params.importance {
    memory.importance = imp.clamp(0.0, 1.0);
  }

  // Generate embedding
  let vector = ctx.get_embedding(&params.content).await?;

  // Store in database
  ctx.db.add_memory(&memory, &vector).await?;

  Ok(MemoryAddResult {
    id: memory.id.to_string(),
    message: "Memory created successfully".to_string(),
    is_duplicate: false,
  })
}

/// Get a memory by ID or prefix with optional related memories.
///
/// # Arguments
/// * `ctx` - Memory context with database
/// * `params` - Parameters including memory_id and include_related flag
///
/// # Returns
/// * `Ok(MemoryFullDetail)` - Full memory details with optional relationships
/// * `Err(ServiceError)` - If memory not found or database error
pub async fn get(ctx: &MemoryContext<'_>, params: MemoryGetParams) -> Result<MemoryFullDetail, ServiceError> {
  let memory = Resolver::memory(ctx.db, &params.memory_id).await?;

  let mut detail = MemoryFullDetail::from(&memory);

  // Include relationships if requested
  if params.include_related.unwrap_or(false) {
    let relationships = ctx
      .db
      .get_all_relationships(&memory.id)
      .await
      .map(|rels| {
        rels
          .iter()
          .map(|r| MemoryRelationshipItem {
            relationship_type: r.relationship_type.as_str().to_string(),
            from_id: r.from_memory_id.to_string(),
            to_id: r.to_memory_id.to_string(),
            target_id: if r.from_memory_id == memory.id {
              r.to_memory_id.to_string()
            } else {
              r.from_memory_id.to_string()
            },
            confidence: r.confidence,
          })
          .collect()
      })
      .unwrap_or_default();
    detail = detail.with_relationships(relationships);
  }

  Ok(detail)
}

/// List memories with optional filters.
///
/// # Arguments
/// * `ctx` - Memory context with database
/// * `params` - List parameters (sector, limit, offset)
///
/// # Returns
/// * `Ok(Vec<MemoryItem>)` - List of memory items
/// * `Err(ServiceError)` - If database error
pub async fn list(ctx: &MemoryContext<'_>, params: MemoryListParams) -> Result<Vec<MemoryItem>, ServiceError> {
  let filter = FilterBuilder::new()
    .exclude_deleted()
    .add_eq_opt("sector", params.sector.as_deref())
    .build();

  let memories = ctx.db.list_memories(filter.as_deref(), params.limit).await?;

  Ok(memories.into_iter().map(|m| MemoryItem::from_list(&m)).collect())
}

/// List soft-deleted memories.
///
/// # Arguments
/// * `ctx` - Memory context with database
/// * `limit` - Maximum number of results
///
/// # Returns
/// * `Ok(Vec<MemoryItem>)` - List of deleted memory items
/// * `Err(ServiceError)` - If database error
pub async fn list_deleted(ctx: &MemoryContext<'_>, limit: Option<usize>) -> Result<Vec<MemoryItem>, ServiceError> {
  let memories = ctx
    .db
    .list_memories(Some("is_deleted = true"), limit.or(Some(20)))
    .await?;

  Ok(memories.into_iter().map(|m| MemoryItem::from_list(&m)).collect())
}

/// Soft-delete a memory.
///
/// # Arguments
/// * `ctx` - Memory context with database
/// * `memory_id` - ID or prefix of the memory to delete
///
/// # Returns
/// * `Ok(Memory)` - The deleted memory
/// * `Err(ServiceError)` - If memory not found or database error
pub async fn delete(ctx: &MemoryContext<'_>, memory_id: &str) -> Result<Memory, ServiceError> {
  let mut memory = Resolver::memory(ctx.db, memory_id).await?;
  memory.delete(Utc::now());
  ctx.db.update_memory(&memory, None).await?;

  Ok(memory)
}

/// Hard-delete a memory permanently.
///
/// # Arguments
/// * `ctx` - Memory context with database
/// * `memory_id` - ID or prefix of the memory to delete
///
/// # Returns
/// * `Ok(String)` - The deleted memory ID
/// * `Err(ServiceError)` - If memory not found or database error
pub async fn hard_delete(ctx: &MemoryContext<'_>, memory_id: &str) -> Result<String, ServiceError> {
  let memory = Resolver::memory(ctx.db, memory_id).await?;
  ctx.db.delete_memory(&memory.id).await?;

  Ok(memory.id.to_string())
}

/// Restore a soft-deleted memory.
///
/// # Arguments
/// * `ctx` - Memory context with database
/// * `memory_id` - ID or prefix of the memory to restore
///
/// # Returns
/// * `Ok(Memory)` - The restored memory
/// * `Err(ServiceError)` - If memory not found, not deleted, or database error
pub async fn restore(ctx: &MemoryContext<'_>, memory_id: &str) -> Result<Memory, ServiceError> {
  let mut memory = Resolver::memory(ctx.db, memory_id).await?;

  if !memory.is_deleted {
    return Err(ServiceError::validation("Memory is not deleted"));
  }

  memory.restore(Utc::now());
  ctx.db.update_memory(&memory, None).await?;

  Ok(memory)
}

/// Find memories related to a given memory.
///
/// Uses multiple strategies:
/// 1. Explicit relationships (from memory_relationships table)
/// 2. Shared entities (co-occurrence via concepts)
/// 3. Semantic similarity (vector search)
///
/// # Arguments
/// * `ctx` - Memory context with database and embedding provider
/// * `params` - Parameters including memory_id, methods, and limit
///
/// # Returns
/// * `Ok(MemoryRelatedResult)` - Related memories with scores and relationship types
/// * `Err(ServiceError)` - If memory not found or database error
pub async fn related(
  ctx: &MemoryContext<'_>,
  params: MemoryRelatedParams,
) -> Result<MemoryRelatedResult, ServiceError> {
  let memory = Resolver::memory(ctx.db, &params.memory_id).await?;
  let limit = params.limit.unwrap_or(10);

  let mut related: Vec<(Memory, f32, String)> = Vec::new();
  let mut seen_ids = HashSet::new();
  seen_ids.insert(memory.id);

  // Method 1: Explicit relationships
  if let Ok(relationships) = ctx.db.get_all_relationships(&memory.id).await {
    for rel in relationships {
      let related_id = if rel.from_memory_id == memory.id {
        rel.to_memory_id
      } else {
        rel.from_memory_id
      };

      if seen_ids.insert(related_id)
        && let Ok(Some(related_mem)) = ctx.db.get_memory(&related_id).await
      {
        related.push((
          related_mem,
          rel.confidence,
          format!("relationship:{}", rel.relationship_type.as_str()),
        ));
      }
    }
  }

  // Method 2: Shared concepts
  for concept in &memory.concepts {
    let filter = format!(
      "is_deleted = false AND concepts LIKE '%{}%'",
      concept.replace('\'', "''")
    );
    if let Ok(matches) = ctx.db.list_memories(Some(&filter), Some(5)).await {
      for m in matches {
        if seen_ids.insert(m.id) {
          related.push((m, 0.6, format!("entity:{}", concept)));
        }
      }
    }
  }

  // Method 3: Semantic similarity
  let query_vec = ctx.get_embedding(&memory.content).await?;
  if let Ok(similar) = search::search_by_embedding(ctx.db, &query_vec, limit, None).await {
    for (m, distance) in similar {
      if seen_ids.insert(m.id) {
        let similarity = 1.0 - distance.min(1.0);
        related.push((m, similarity, "similar".to_string()));
      }
    }
  }

  // Method 4: Supersession chain
  if let Some(superseded_by) = memory.superseded_by
    && seen_ids.insert(superseded_by)
    && let Ok(Some(superseding)) = ctx.db.get_memory(&superseded_by).await
  {
    related.push((superseding, 1.0, "superseded_by".to_string()));
  }

  // Find memories this one supersedes
  let filter = format!("superseded_by = '{}'", memory.id);
  if let Ok(superseded) = ctx.db.list_memories(Some(&filter), Some(5)).await {
    for m in superseded {
      if seen_ids.insert(m.id) {
        related.push((m, 0.9, "supersedes".to_string()));
      }
    }
  }

  // Sort by score and truncate
  related.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
  related.truncate(limit);

  let results: Vec<MemoryRelatedItem> = related
    .into_iter()
    .map(|(m, score, relationship)| MemoryRelatedItem {
      id: m.id.to_string(),
      content: m.content,
      summary: m.summary,
      memory_type: m.memory_type.map(|t| t.as_str().to_string()),
      sector: m.sector.as_str().to_string(),
      salience: m.salience,
      score,
      relationship,
      created_at: m.created_at.to_rfc3339(),
    })
    .collect();

  let count = results.len();

  Ok(MemoryRelatedResult {
    memory_id: memory.id.to_string(),
    content: memory.content,
    related: results,
    count,
  })
}

/// Get a memory's timeline context (memories before and after).
///
/// # Arguments
/// * `ctx` - Memory context with database
/// * `memory_id` - ID or prefix of the anchor memory
/// * `before_count` - Number of memories before to retrieve
/// * `after_count` - Number of memories after to retrieve
///
/// # Returns
/// * `Ok(MemoryTimelineResult)` - Timeline with anchor, before, and after memories
/// * `Err(ServiceError)` - If memory not found or database error
pub async fn timeline(
  ctx: &MemoryContext<'_>,
  memory_id: &str,
  before_count: usize,
  after_count: usize,
) -> Result<MemoryTimelineResult, ServiceError> {
  let memory = Resolver::memory(ctx.db, memory_id).await?;

  // Build timeline item for anchor
  let anchor = MemoryTimelineItem::from(&memory);

  // Get memories before
  let before_filter = format!(
    "is_deleted = false AND created_at < '{}' ORDER BY created_at DESC",
    memory.created_at.to_rfc3339()
  );
  let before: Vec<MemoryTimelineItem> = ctx
    .db
    .list_memories(Some(&before_filter), Some(before_count))
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|m| MemoryTimelineItem::from(&m))
    .collect();

  // Get memories after
  let after_filter = format!(
    "is_deleted = false AND created_at > '{}' ORDER BY created_at ASC",
    memory.created_at.to_rfc3339()
  );
  let after: Vec<MemoryTimelineItem> = ctx
    .db
    .list_memories(Some(&after_filter), Some(after_count))
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|m| MemoryTimelineItem::from(&m))
    .collect();

  Ok(MemoryTimelineResult { anchor, before, after })
}

/// Apply decay to all memories in the database.
///
/// This is called periodically by the scheduler to gradually reduce
/// the salience of memories based on time since last access, sector,
/// and importance.
///
/// # Arguments
/// * `ctx` - Memory context with database
/// * `config` - Decay configuration
///
/// # Returns
/// * `Ok(DecayStats)` - Statistics about the decay operation
/// * `Err(ServiceError)` - If database error
pub async fn apply_decay(ctx: &MemoryContext<'_>, config: &MemoryDecay) -> Result<DecayStats, ServiceError> {
  use chrono::Utc;

  use crate::context::memory::extract::decay::apply_decay_batch;

  let now = Utc::now();

  // Load all non-deleted memories
  let mut memories = ctx.db.list_memories(Some("is_deleted = false"), None).await?;

  if memories.is_empty() {
    return Ok(DecayStats::default());
  }

  debug!(memory_count = memories.len(), "Applying decay to memories");

  // Apply decay
  let results = apply_decay_batch(&mut memories, now, config);
  let stats = DecayStats::from_results(&results);

  // Find memories that actually changed (salience decreased)
  let changed: Vec<_> = memories
    .into_iter()
    .zip(results.iter())
    .filter(|(_, r)| r.new_salience < r.previous_salience)
    .map(|(m, _)| m)
    .collect();

  // Batch update changed memories
  if !changed.is_empty() {
    debug!(changed_count = changed.len(), "Updating decayed memories");
    ctx.db.batch_update_memories(&changed).await?;
  }

  Ok(stats)
}
