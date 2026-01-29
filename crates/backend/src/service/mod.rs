//! Business logic services.
//!
//! This module contains the service layer that encapsulates business logic,
//! providing a clean separation between handlers (request/response) and
//! core operations.
//!
//! ## Available Services
//!
//! - [`code`] - Code search, expansion, context retrieval, and statistics
//! - [`docs`] - Document search and context retrieval
//! - [`memory`] - Memory search, ranking, deduplication, lifecycle
//! - [`explore`] - Unified cross-domain search and context retrieval
//! - [`project`] - Project info, stats, and cleanup

pub mod code;
pub mod docs;
pub mod explore;
pub mod hooks;
pub mod memory;
pub mod project;
pub mod util;

#[cfg(test)]
mod __tests__;
