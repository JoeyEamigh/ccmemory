//! Document search service.
//!
//! Provides search functionality for documents with vector/text fallback.

use crate::{
  db::ProjectDb,
  embedding::EmbeddingProvider,
  ipc::types::docs::{DocSearchItem, DocsSearchParams},
  service::util::ServiceError,
};

// ============================================================================
// Service Context
// ============================================================================

/// Context for docs service operations.
pub struct DocsContext<'a> {
  /// Project database connection
  pub db: &'a ProjectDb,
  /// Optional embedding provider for vector search
  pub embedding: &'a dyn EmbeddingProvider,
}

impl<'a> DocsContext<'a> {
  /// Create a new docs context
  pub fn new(db: &'a ProjectDb, embedding: &'a dyn EmbeddingProvider) -> Self {
    Self { db, embedding }
  }

  /// Get an embedding for the given text, if a provider is available
  pub async fn get_embedding(&self, text: &str) -> Result<Vec<f32>, ServiceError> {
    // Query mode - this is used for docs search queries
    Ok(
      self
        .embedding
        .embed(text, crate::embedding::EmbeddingMode::Query)
        .await?,
    )
  }
}

// ============================================================================
// Search
// ============================================================================

/// Search parameters for documents.
#[derive(Debug, Clone)]
pub struct SearchParams {
  /// The search query
  pub query: String,
  /// Maximum number of results
  pub limit: Option<usize>,
}

impl From<DocsSearchParams> for SearchParams {
  fn from(p: DocsSearchParams) -> Self {
    Self {
      query: p.query,
      limit: p.limit,
    }
  }
}

/// Search documents with vector search and text fallback.
///
/// # Arguments
/// * `ctx` - Docs context with database and embedding provider
/// * `params` - Search parameters
///
/// # Returns
/// * `Ok(Vec<DocSearchItem>)` - Search results
/// * `Err(ServiceError)` - If search fails
pub async fn search(ctx: &DocsContext<'_>, params: SearchParams) -> Result<Vec<DocSearchItem>, ServiceError> {
  let limit = params.limit.unwrap_or(10);

  // Try vector search first
  let query_vec = ctx.get_embedding(&params.query).await?;
  let results = ctx.db.search_documents(&query_vec, limit, None).await?;
  let items: Vec<DocSearchItem> = results
    .into_iter()
    .map(|(doc, distance)| {
      let similarity = 1.0 - distance.min(1.0);
      DocSearchItem::from_search(&doc, similarity)
    })
    .collect();
  Ok(items)
}
