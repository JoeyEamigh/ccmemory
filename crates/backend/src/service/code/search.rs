//! Code search with symbol boosting and ranking.
//!
//! This module provides the business logic for code search operations,
//! including vector search with text fallback and multi-signal ranking.

use std::cmp::Ordering;

use tracing::debug;

use crate::{
  db::ProjectDb,
  domain::code::CodeChunk,
  embedding::EmbeddingProvider,
  ipc::types::code::{CodeItem, SearchQuality},
  service::util::{FilterBuilder, ServiceError},
};

// ============================================================================
// Context
// ============================================================================

/// Context for code service operations.
///
/// Contains all dependencies needed for code operations.
pub struct CodeContext<'a> {
  /// Project database connection
  pub db: &'a ProjectDb,
  /// Optional embedding provider for vector search
  pub embedding: &'a dyn EmbeddingProvider,
}

impl<'a> CodeContext<'a> {
  /// Create a new code context
  pub fn new(db: &'a ProjectDb, embedding: &'a dyn EmbeddingProvider) -> Self {
    Self { db, embedding }
  }

  /// Get an embedding for the given text, if a provider is available
  pub async fn get_embedding(&self, text: &str) -> Result<Vec<f32>, ServiceError> {
    // Query mode - this is used for search queries
    Ok(
      self
        .embedding
        .embed(text, crate::embedding::EmbeddingMode::Query)
        .await?,
    )
  }
}

// ============================================================================
// Search Parameters and Results
// ============================================================================

/// Parameters for code search.
#[derive(Debug, Clone, Default)]
pub struct SearchParams {
  /// The search query
  pub query: String,
  /// Filter by language
  pub language: Option<String>,
  /// Maximum number of results
  pub limit: Option<usize>,
  /// Whether to include file context (imports, siblings)
  pub include_context: bool,

  // === Metadata filters (Phase 3) ===
  /// Filter by visibility (e.g., ["pub", "pub(crate)"] for Rust).
  /// Applied as pre-filter before vector search for efficiency.
  pub visibility: Vec<String>,

  /// Filter by chunk type (e.g., ["function", "class"]).
  /// Valid types: function, class, module, block, import.
  pub chunk_type: Vec<String>,

  /// Minimum caller count filter. Only returns code that is called
  /// by at least this many other code chunks.
  pub min_caller_count: Option<u32>,

  // === Confidence-based features (Phase 5) ===
  /// Enable adaptive result limiting. When true:
  /// - If top results are very confident (distance < 0.2), limits to confident results only
  /// - Reduces noise from low-relevance matches
  ///
  /// Default: false (returns up to `limit` results regardless of confidence)
  pub adaptive_limit: bool,
}

/// Configuration for code search ranking.
#[derive(Debug, Clone)]
pub struct RankingConfig {
  /// Weight for vector similarity (0.0-1.0)
  pub semantic_weight: f32,
  /// Weight for symbol boost (0.0-1.0)
  pub symbol_weight: f32,
  /// Weight for visibility/importance (0.0-1.0)
  pub importance_weight: f32,
  /// Oversample factor for ranking (fetch more, rank, trim)
  pub oversample_factor: usize,
}

impl Default for RankingConfig {
  fn default() -> Self {
    // With pure semantic search (no hardcoded query expansion), we rely more
    // heavily on the embedding model's understanding of semantic relationships.
    // The semantic weight is increased since it's now the primary source of
    // concept matching (e.g., "auth" → authentication-related code).
    Self {
      semantic_weight: 0.55,
      symbol_weight: 0.30,
      importance_weight: 0.15,
      oversample_factor: 3,
    }
  }
}

/// Result of a code search operation.
#[derive(Debug, Clone)]
pub struct SearchResult {
  /// The search results
  pub results: Vec<CodeItem>,
  /// Query information for debugging
  pub query: String,
  /// Search quality metadata
  pub search_quality: SearchQuality,
}

// ============================================================================
// Core Search Implementation
// ============================================================================

/// Search for code chunks with vector search and ranking.
///
/// # Arguments
/// * `ctx` - Code context with database and embedding provider
/// * `params` - Search parameters
/// * `config` - Ranking configuration
///
/// # Returns
/// * `Ok(SearchResult)` - Search results with query info
/// * `Err(ServiceError)` - If search fails
pub async fn search(
  ctx: &CodeContext<'_>,
  params: SearchParams,
  config: &RankingConfig,
) -> Result<SearchResult, ServiceError> {
  let limit = params.limit.unwrap_or(10);
  let oversample = (limit * config.oversample_factor).min(50);

  // Build filter using FilterBuilder for all metadata filters
  let filter = FilterBuilder::new()
    .add_eq_opt(
      "language",
      params.language.as_ref().map(|l| l.to_lowercase()).as_deref(),
    )
    .add_in_opt(
      "visibility",
      if params.visibility.is_empty() {
        None
      } else {
        Some(&params.visibility)
      },
    )
    .add_in_opt(
      "chunk_type",
      if params.chunk_type.is_empty() {
        None
      } else {
        Some(&params.chunk_type)
      },
    )
    .add_min_u32_opt("caller_count", params.min_caller_count)
    .build();

  debug!("Code search: query='{}'", params.query);

  // Embed the original query - the model understands natural language
  let query_vec = ctx.get_embedding(&params.query).await?;
  debug!("Using vector search with ranking for code query");
  let results = ctx
    .db
    .search_code_chunks(&query_vec, oversample, filter.as_deref())
    .await?;

  // Apply ranking
  let ranked = rank_results(results, &params.query, config);

  // Collect distances for search quality calculation (before limiting)
  let distances: Vec<f32> = ranked.iter().map(|r| r.distance).collect();
  let search_quality = SearchQuality::from_distances(&distances);

  // Calculate effective limit based on adaptive_limit setting
  let effective_limit = if params.adaptive_limit {
    calculate_adaptive_limit(&ranked, limit)
  } else {
    limit
  };

  // Take top results and convert to CodeItem
  let items: Vec<CodeItem> = ranked
    .into_iter()
    .take(effective_limit)
    .map(|r| {
      let mut item = CodeItem::from_search_with_confidence(&r.chunk, r.rank_score, r.confidence);
      // Include context if requested
      if params.include_context {
        item.imports = r.chunk.imports.clone();
        item.calls = r.chunk.calls.clone();
      }
      item
    })
    .collect();

  Ok(SearchResult {
    results: items,
    query: params.query,
    search_quality,
  })
}

// ============================================================================
// Ranking
// ============================================================================

/// Result of ranking a code chunk, including both scores.
#[derive(Debug, Clone)]
pub struct RankedResult {
  /// The code chunk
  pub chunk: CodeChunk,
  /// Final weighted score (semantic + symbol + importance)
  pub rank_score: f32,
  /// Raw distance from vector search (lower = better)
  pub distance: f32,
  /// Raw confidence derived from distance (1.0 - distance)
  pub confidence: f32,
}

/// Rank code search results by combining multiple signals.
///
/// Signals:
/// - Vector similarity (semantic relevance)
/// - Symbol boost (exact/partial matches on symbols, definition names, calls)
/// - Importance (visibility: public > private)
///
/// Returns `RankedResult` with both the weighted rank score and raw confidence.
pub fn rank_results(results: Vec<(CodeChunk, f32)>, query: &str, config: &RankingConfig) -> Vec<RankedResult> {
  let query_terms: Vec<&str> = query.split_whitespace().collect();

  let mut scored: Vec<RankedResult> = results
    .into_iter()
    .map(|(chunk, distance)| {
      let confidence = 1.0 - distance.min(1.0);
      let symbol_boost = calculate_symbol_boost(&chunk, &query_terms);
      let importance = calculate_importance(&chunk);

      // Weighted combination
      let rank_score = confidence * config.semantic_weight
        + symbol_boost * config.symbol_weight
        + importance * config.importance_weight;

      RankedResult {
        chunk,
        rank_score,
        distance,
        confidence,
      }
    })
    .collect();

  // Sort by rank score descending
  scored.sort_by(|a, b| b.rank_score.partial_cmp(&a.rank_score).unwrap_or(Ordering::Equal));

  scored
}

/// Calculate adaptive result limit based on confidence distribution.
///
/// When enabled, this reduces the result count when top results are very confident,
/// avoiding noise from low-relevance matches. Rules:
/// - If top 3 results all have distance < 0.2 (90%+ confidence), return at most 5
/// - If top result has distance < 0.15 (85%+ confidence), return at most 8
/// - If best distance > 0.5 (low confidence), return all up to limit
///
/// This helps focus the user's attention on the most relevant results when the
/// embedding model is confident about the matches.
fn calculate_adaptive_limit(ranked: &[RankedResult], max_limit: usize) -> usize {
  if ranked.is_empty() {
    return 0;
  }

  let best_distance = ranked.first().map(|r| r.distance).unwrap_or(1.0);

  // Low confidence search - return all results
  if best_distance > 0.5 {
    return max_limit;
  }

  // Check if top 3 are all very confident (distance < 0.2)
  let top_3_confident = ranked.iter().take(3).all(|r| r.distance < 0.2);

  if top_3_confident {
    // Very confident search - limit to 5 results
    return max_limit.min(5);
  }

  // Single very confident result
  if best_distance < 0.15 {
    return max_limit.min(8);
  }

  // Default: return up to max_limit
  max_limit
}

/// Calculate boost factor based on symbol/metadata matches.
pub fn calculate_symbol_boost(chunk: &CodeChunk, query_terms: &[&str]) -> f32 {
  let mut boost = 0.0f32;

  for term in query_terms {
    let term_lower = term.to_lowercase();

    // Symbol match (highest boost)
    for symbol in &chunk.symbols {
      if symbol.to_lowercase() == term_lower {
        boost += 0.4; // Exact match
      } else if symbol.to_lowercase().contains(&term_lower) {
        boost += 0.2; // Partial match
      }
    }

    // Definition name match
    if let Some(ref name) = chunk.definition_name {
      if name.to_lowercase() == term_lower {
        boost += 0.35;
      } else if name.to_lowercase().contains(&term_lower) {
        boost += 0.15;
      }
    }

    // Imports match (medium boost)
    for import in &chunk.imports {
      if import.to_lowercase().contains(&term_lower) {
        boost += 0.1;
        break; // Only count once per term
      }
    }

    // Calls match (medium boost)
    for call in &chunk.calls {
      if call.to_lowercase() == term_lower {
        boost += 0.15;
        break;
      }
    }

    // File path match (low boost)
    if chunk.file_path.to_lowercase().contains(&term_lower) {
      boost += 0.05;
    }
  }

  // Cap the boost to avoid runaway scores
  boost.min(1.0)
}

/// Calculate importance factor based on visibility and caller count.
///
/// Combines two signals:
/// 1. **Visibility**: Public APIs are more likely to be relevant (pub > pub(crate) > private)
/// 2. **Caller count**: Code called by many other places is more central/important
///
/// The caller count uses a logarithmic scale to prevent functions with very high
/// caller counts from completely dominating results, while still giving a meaningful
/// boost to frequently-called code.
pub fn calculate_importance(chunk: &CodeChunk) -> f32 {
  // Base score from visibility (0.5-0.85 range to leave room for caller boost)
  let visibility_score = match chunk.visibility.as_deref() {
    Some("pub") | Some("export") | Some("export default") | Some("public") => 0.85,
    Some("pub(crate)") | Some("protected") => 0.70,
    Some("private") | Some("pub(super)") => 0.50,
    _ => 0.60, // Unknown visibility
  };

  // Caller count boost using logarithmic scale
  // log1p(0)=0, log1p(5)≈1.79, log1p(10)≈2.40, log1p(50)≈3.93
  // With 0.05 weight: 5 callers → +0.09, 50 callers → +0.20
  let caller_boost = (1.0 + chunk.caller_count as f32).ln() * 0.05;
  // Cap the caller boost to prevent runaway scores
  let caller_boost = caller_boost.min(0.15);

  // Combine and cap at 1.0
  (visibility_score + caller_boost).min(1.0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::code::{ChunkType, Language};

  fn create_test_chunk(
    symbols: Vec<&str>,
    imports: Vec<&str>,
    calls: Vec<&str>,
    file_path: &str,
    definition_name: Option<&str>,
    visibility: Option<&str>,
  ) -> CodeChunk {
    CodeChunk {
      id: uuid::Uuid::new_v4(),
      file_path: file_path.to_string(),
      content: "test content".to_string(),
      language: Language::Rust,
      chunk_type: ChunkType::Function,
      symbols: symbols.into_iter().map(String::from).collect(),
      imports: imports.into_iter().map(String::from).collect(),
      calls: calls.into_iter().map(String::from).collect(),
      start_line: 1,
      end_line: 10,
      file_hash: "hash123".to_string(),
      indexed_at: chrono::Utc::now(),
      tokens_estimate: 50,
      definition_kind: Some("function".to_string()),
      definition_name: definition_name.map(String::from),
      visibility: visibility.map(String::from),
      signature: None,
      docstring: None,
      parent_definition: None,
      embedding_text: None,
      content_hash: None,
      caller_count: 0,
      callee_count: 0,
    }
  }

  #[test]
  fn test_symbol_boost_exact_match() {
    let chunk = create_test_chunk(
      vec!["authenticate"],
      vec![],
      vec![],
      "auth.rs",
      Some("authenticate"),
      Some("pub"),
    );

    let boost = calculate_symbol_boost(&chunk, &["authenticate"]);
    assert!(boost >= 0.7, "Expected >= 0.7, got {}", boost);
  }

  #[test]
  fn test_symbol_boost_partial_match() {
    let chunk = create_test_chunk(
      vec!["authenticate_user"],
      vec![],
      vec![],
      "auth.rs",
      Some("authenticate_user"),
      Some("pub"),
    );

    let boost = calculate_symbol_boost(&chunk, &["auth"]);
    assert!(boost >= 0.35, "Expected >= 0.35, got {}", boost);
  }

  #[test]
  fn test_symbol_boost_capped() {
    let chunk = create_test_chunk(
      vec!["auth", "authenticate", "authorization"],
      vec!["auth_lib"],
      vec!["auth_check"],
      "auth/auth.rs",
      Some("auth"),
      Some("pub"),
    );

    let boost = calculate_symbol_boost(&chunk, &["auth"]);
    assert!(boost <= 1.0, "Boost should be capped at 1.0, got {}", boost);
  }

  #[test]
  fn test_importance_pub_no_callers() {
    let chunk = create_test_chunk(vec![], vec![], vec![], "test.rs", None, Some("pub"));
    // Base visibility 0.85 + log1p(0)*0.05 = 0.85 + 0 = 0.85
    assert!(
      (calculate_importance(&chunk) - 0.85).abs() < 0.01,
      "Public with no callers should be ~0.85"
    );
  }

  #[test]
  fn test_importance_private_no_callers() {
    let chunk = create_test_chunk(vec![], vec![], vec![], "test.rs", None, Some("private"));
    // Base visibility 0.50 + log1p(0)*0.05 = 0.50 + 0 = 0.50
    assert!(
      (calculate_importance(&chunk) - 0.50).abs() < 0.01,
      "Private with no callers should be ~0.50"
    );
  }

  #[test]
  fn test_importance_unknown_no_callers() {
    let chunk = create_test_chunk(vec![], vec![], vec![], "test.rs", None, None);
    // Base visibility 0.60 + log1p(0)*0.05 = 0.60 + 0 = 0.60
    assert!(
      (calculate_importance(&chunk) - 0.60).abs() < 0.01,
      "Unknown visibility with no callers should be ~0.60"
    );
  }

  #[test]
  fn test_importance_with_callers() {
    let mut chunk = create_test_chunk(vec![], vec![], vec![], "test.rs", None, Some("pub"));
    chunk.caller_count = 10;
    // Base visibility 0.85 + log1p(10)*0.05 ≈ 0.85 + 2.40*0.05 = 0.85 + 0.12 = 0.97
    let importance = calculate_importance(&chunk);
    assert!(
      importance > 0.90,
      "Public with 10 callers should be > 0.90, got {}",
      importance
    );
    assert!(
      importance < 1.0,
      "Importance should be capped below 1.0, got {}",
      importance
    );
  }

  #[test]
  fn test_importance_caller_boost_capped() {
    let mut chunk = create_test_chunk(vec![], vec![], vec![], "test.rs", None, Some("pub"));
    chunk.caller_count = 1000; // Very high caller count
    let importance = calculate_importance(&chunk);
    // Even with 1000 callers, should be capped at 1.0
    assert!(
      importance <= 1.0,
      "Importance should be capped at 1.0, got {}",
      importance
    );
  }

  #[test]
  fn test_importance_callers_boost_private_function() {
    let private_no_callers = create_test_chunk(vec![], vec![], vec![], "test.rs", None, Some("private"));
    let mut private_many_callers = create_test_chunk(vec![], vec![], vec![], "test.rs", None, Some("private"));
    private_many_callers.caller_count = 50;

    let no_callers_importance = calculate_importance(&private_no_callers);
    let many_callers_importance = calculate_importance(&private_many_callers);

    assert!(
      many_callers_importance > no_callers_importance,
      "Private with many callers ({}) should rank higher than private with no callers ({})",
      many_callers_importance,
      no_callers_importance
    );
  }

  #[test]
  fn test_rank_results_prefers_exact() {
    let config = RankingConfig::default();

    let chunk_exact = create_test_chunk(
      vec!["authenticate"],
      vec![],
      vec![],
      "auth.rs",
      Some("authenticate"),
      Some("pub"),
    );

    let chunk_partial = create_test_chunk(
      vec!["user_auth_helper"],
      vec![],
      vec![],
      "helpers.rs",
      Some("user_auth_helper"),
      Some("pub"),
    );

    let results = vec![
      (chunk_partial.clone(), 0.1), // Better vector similarity
      (chunk_exact.clone(), 0.3),   // Worse vector but exact match
    ];

    let ranked = rank_results(results, "authenticate", &config);
    assert_eq!(ranked[0].chunk.symbols[0], "authenticate");
  }

  #[test]
  fn test_ranked_result_includes_confidence() {
    let config = RankingConfig::default();

    let chunk = create_test_chunk(
      vec!["test_func"],
      vec![],
      vec![],
      "test.rs",
      Some("test_func"),
      Some("pub"),
    );

    let results = vec![(chunk.clone(), 0.25)]; // distance = 0.25

    let ranked = rank_results(results, "test", &config);
    assert_eq!(ranked.len(), 1);

    let result = &ranked[0];
    // Confidence = 1.0 - distance
    assert!(
      (result.confidence - 0.75).abs() < 0.01,
      "Confidence should be 0.75 (1.0 - 0.25), got {}",
      result.confidence
    );
    assert!(
      (result.distance - 0.25).abs() < 0.01,
      "Distance should be preserved as 0.25, got {}",
      result.distance
    );
  }
}
