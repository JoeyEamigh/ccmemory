//! Memory deduplication service.
//!
//! Provides duplicate detection using a multi-level strategy:
//! 1. Exact content hash match (fastest, catches identical content)
//! 2. SimHash similarity (catches near-duplicates)
//! 3. Jaccard verification (confirms semantic similarity)

use tracing::debug;

use super::MemoryContext;
use crate::{
  context::memory::extract::dedup::{DuplicateChecker, DuplicateMatch},
  service::util::ServiceError,
};

/// Result of a duplicate check.
#[derive(Debug, Clone)]
pub struct DuplicateResult {
  /// ID of the existing duplicate memory
  pub id: String,
  /// Reason for the duplicate detection
  pub reason: &'static str,
}

/// Check if content already exists as a memory.
///
/// This function searches for potential duplicates using vector similarity
/// and then applies hash-based deduplication checks.
///
/// # Arguments
/// * `ctx` - Memory context with database and embedding provider
/// * `content` - The new content to check
/// * `content_hash` - Pre-computed content hash (SHA-256)
/// * `simhash` - Pre-computed SimHash for locality-sensitive matching
///
/// # Returns
/// * `Ok(Some(DuplicateResult))` - If a duplicate is found
/// * `Ok(None)` - If no duplicate exists
/// * `Err(ServiceError)` - If the check fails
///
/// # Deduplication Strategy
///
/// 1. Use vector search to find semantically similar candidates (top 10)
/// 2. For each candidate, check for exact hash match
/// 3. If no exact match, check SimHash distance
/// 4. If SimHash indicates similarity, verify with Jaccard similarity
///
/// This multi-level approach balances speed (hash checks are O(1))
/// with accuracy (Jaccard catches edge cases).
pub async fn check_duplicate(
  ctx: &MemoryContext<'_>,
  content: &str,
  content_hash: &str,
  simhash: u64,
) -> Result<Option<DuplicateResult>, ServiceError> {
  // Get embedding for similarity search
  let query_vec = ctx.get_embedding(content).await?;

  // Search for similar memories
  let candidates = match ctx.db.search_memories(&query_vec, 10, Some("is_deleted = false")).await {
    Ok(c) => c,
    Err(e) => {
      debug!("Vector search for dedup failed: {}", e);
      return Ok(None);
    }
  };

  if candidates.is_empty() {
    return Ok(None);
  }

  // Create checker and check each candidate
  let checker = DuplicateChecker::new(0.8);

  for (memory, _distance) in &candidates {
    let match_result = checker.is_duplicate(content, content_hash, simhash, memory);

    match match_result {
      DuplicateMatch::Exact => {
        debug!("Duplicate memory detected (exact match): {}", memory.id);
        return Ok(Some(DuplicateResult {
          id: memory.id.to_string(),
          reason: "Exact content match",
        }));
      }
      DuplicateMatch::Simhash { distance, jaccard } if jaccard > 0.85 => {
        debug!(
          "Duplicate memory detected (similar): {} (distance={}, jaccard={})",
          memory.id, distance, jaccard
        );
        return Ok(Some(DuplicateResult {
          id: memory.id.to_string(),
          reason: "Highly similar content",
        }));
      }
      _ => {}
    }
  }

  Ok(None)
}
