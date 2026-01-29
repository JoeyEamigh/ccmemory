//! Code-related services.
//!
//! This module provides business logic for code operations including:
//! - Query expansion and intent detection
//! - Code search with ranking and symbol boosting
//! - Code context retrieval (callers, callees, siblings, related)
//! - Code statistics
//! - Code indexing (file scanning)
//! - Code chunk import
//!
//! ## Services
//!
//! - [`search`] - Code search with vector/text fallback and ranking
//! - [`context`] - Call graph navigation and context retrieval
//! - [`stats`] - Code index statistics
//! - [`index`] - File scanning for code indexing
//! - [`import`] - Direct chunk import

pub mod context;
pub mod index;
pub mod search;
pub mod startup_scan;
pub mod stats;

// Re-export commonly used items from context
pub use context::{
  CalleesParams, CallersParams, ContextFullParams, RelatedParams, get_callees_response, get_callers_response,
  get_full_context, get_related, get_related_memories,
};
// Re-export commonly used items from search
pub use search::{CodeContext, RankingConfig, SearchParams, search};
// Re-export commonly used items from stats
pub use stats::get_stats;
