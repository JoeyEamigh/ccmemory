//! Document-related services.
//!
//! This module provides business logic for document operations including:
//! - Document search with vector/text fallback
//! - Document context retrieval (adjacent chunks)
//! - Document ingestion from files with streaming progress
//!
//! ## Services
//!
//! - [`search`] - Document search with vector/text fallback
//! - [`context`] - Document context retrieval (adjacent chunks)
//! - [`ingest`] - Document ingestion with streaming progress support

pub mod context;
pub mod ingest;
pub mod search;

// Re-export commonly used items from search
// Re-export commonly used items from context
pub use context::{ContextParams, get_context};
// Re-export commonly used items from ingest
pub use ingest::{IngestContext, IngestParams, IngestProgress, ingest};
pub use search::{DocsContext, SearchParams, search};
