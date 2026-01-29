//! Memory search service.
//!
//! Provides memory search with vector/text fallback and post-search ranking.
//!
//! ## Design Note
//!
//! This search implementation **does NOT auto-reinforce** top results.
//! The previous behavior of automatically reinforcing memories during search
//! was a side effect in a read operation. If you want to track memory access,
//! call `lifecycle::reinforce` explicitly after search.

use tracing::debug;

use super::{MemoryContext, RankingConfig, ranking};
use crate::{
  domain::config::Config,
  ipc::types::{
    code::SearchQuality,
    memory::{MemoryItem, MemorySearchParams},
  },
  service::util::{FilterBuilder, ServiceError},
};

/// Result of a memory search operation.
pub struct SearchResult {
  /// The search results
  pub items: Vec<MemoryItem>,
  /// Search quality metadata
  pub search_quality: SearchQuality,
}

/// Extended search parameters with internal config.
pub struct SearchParams {
  /// Base parameters from the request
  pub base: MemorySearchParams,
  /// Optional ranking configuration override
  pub ranking_config: Option<RankingConfig>,
}

impl From<MemorySearchParams> for SearchParams {
  fn from(params: MemorySearchParams) -> Self {
    Self {
      base: params,
      ranking_config: None,
    }
  }
}

/// Search memories with vector/text fallback and ranking.
///
/// # Arguments
/// * `ctx` - Memory context with database and embedding provider
/// * `params` - Search parameters
/// * `config` - Project configuration for defaults
///
/// # Returns
/// * `Ok(SearchResult)` - Search results with metadata
/// * `Err(ServiceError)` - If search fails
///
/// # Note
/// This function does NOT auto-reinforce results. If you need to track access,
/// call `lifecycle::reinforce` explicitly for the memories you want to boost.
pub async fn search(
  ctx: &MemoryContext<'_>,
  params: impl Into<SearchParams>,
  config: &Config,
) -> Result<SearchResult, ServiceError> {
  let params = params.into();
  let base = params.base;

  // Build filter from parameters
  let filter = FilterBuilder::new()
    .exclude_inactive(base.include_superseded)
    .add_eq_opt("sector", base.sector.as_deref())
    .add_eq_opt("tier", base.tier.as_deref())
    .add_eq_opt("memory_type", base.memory_type.as_deref())
    .add_min_opt("salience", base.min_salience)
    .add_prefix_opt("scope_path", base.scope_path.as_deref())
    .add_eq_opt("scope_module", base.scope_module.as_deref())
    .add_eq_opt("session_id", base.session_id.as_deref())
    .build();

  let limit = base.limit.unwrap_or(config.search.default_limit);
  // Oversample by 2x for post-search ranking
  let fetch_limit = limit * 2;

  // Get ranking config
  let ranking_config = params
    .ranking_config
    .unwrap_or_else(|| RankingConfig::from(&config.search));

  // Try vector search first
  let query_vec = ctx.get_embedding(&base.query).await?;
  debug!("Using vector search for query: {}", base.query);

  let results = ctx
    .db
    .search_memories(&query_vec, fetch_limit, filter.as_deref())
    .await?;

  // Apply post-search ranking
  let ranked = ranking::rank_memories(results, limit, Some(&ranking_config));

  // Collect distances for search quality calculation
  let distances: Vec<f32> = ranked.iter().map(|(_, distance, _)| *distance).collect();
  let search_quality = SearchQuality::from_distances(&distances);

  let items = ranked
    .into_iter()
    .map(|(m, distance, rank_score)| {
      let similarity = 1.0 - distance.min(1.0);
      MemoryItem::from_search(&m, similarity, rank_score)
    })
    .collect();

  Ok(SearchResult { items, search_quality })
}

/// Search memories using a pre-computed embedding vector.
///
/// This is useful for cross-domain searches where you already have an embedding
/// (e.g., from a code chunk) and want to find semantically related memories
/// without recomputing the embedding.
///
/// Automatically filters out deleted memories.
///
/// # Arguments
/// * `db` - Project database connection
/// * `embedding` - Pre-computed embedding vector
/// * `limit` - Maximum number of results
/// * `filter` - Optional additional SQL filter (is_deleted=false is always applied)
///
/// # Returns
/// * `Ok(Vec<(Memory, f32)>)` - Memories with their distance scores
pub async fn search_by_embedding(
  db: &crate::db::ProjectDb,
  embedding: &[f32],
  limit: usize,
  filter: Option<&str>,
) -> Result<Vec<(crate::domain::memory::Memory, f32)>, ServiceError> {
  // Combine user filter with is_deleted check
  let full_filter = match filter {
    Some(f) => Some(format!("is_deleted = false AND {}", f)),
    None => Some("is_deleted = false".to_string()),
  };

  let results = db.search_memories(embedding, limit, full_filter.as_deref()).await?;

  Ok(results)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_filter_building() {
    let filter = FilterBuilder::new()
      .exclude_inactive(false)
      .add_eq_opt("sector", Some("semantic"))
      .add_prefix_opt("scope_path", Some("src/"))
      .build();

    let filter_str = filter.unwrap();
    assert!(filter_str.contains("sector = 'semantic'"));
    assert!(filter_str.contains("scope_path LIKE 'src/%'"));
  }
}
