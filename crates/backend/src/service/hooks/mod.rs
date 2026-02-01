//! Hook service layer.
//!
//! This module provides the business logic for processing Claude Code hook events,
//! cleanly separated from transport concerns (IPC/CLI).
//!
//! ## Architecture
//!
//! The hook system follows the service pattern with these components:
//!
//! - **Context** (`HookContext`) - Bundles dependencies for hook processing
//! - **State** (`HookState`) - Mutable state for session tracking and deduplication
//! - **Handlers** - Thin adapters that call service functions
//! - **Services** - Business logic (extraction)
//!
//! ## Module Structure
//!
//! ```text
//! hooks/
//! ├── mod.rs          # Re-exports and public API
//! ├── event.rs        # HookEvent enum and parsing
//! ├── context.rs      # SegmentContext for session accumulation
//! ├── extraction.rs   # Memory extraction service
//! └── handler.rs      # Event dispatch and handling
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use crate::service::hooks::{HookContext, HookState, dispatch, HookEvent};
//!
//! // Create context with dependencies
//! let ctx = HookContext::new(db, embedding, llm, project_id, &config);
//! let mut state = HookState::new();
//!
//! // Dispatch hook event
//! let result = dispatch(&ctx, &mut state, event, &params, None).await?;
//! ```
//!
//! ## Design Principles
//!
//! - **Handlers are thin** - No business logic, just request/response transformation
//! - **Services are testable** - Pure functions with injected dependencies
//! - **State is explicit** - HookState passed through handlers, not hidden

mod context;
mod event;
mod extraction;
mod handler;

// Re-export public types
pub use event::HookEvent;
pub use handler::{HookContext, HookState, SessionStartInfo, dispatch};
