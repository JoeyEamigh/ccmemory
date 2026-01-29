//! Actor-based concurrency primitives
//!
//! This module provides the message types and handle patterns for the
//! actor-based daemon architecture. Instead of shared-state concurrency
//! with `Arc<Mutex<...>>`, components communicate via message passing.
//!
//! # Architecture
//!
//! - Each logical component runs as a long-lived task with its own event loop
//! - Components communicate via `mpsc` channels
//! - State is owned, not shared
//! - Response channels are `mpsc` (not oneshot) to support streaming
//!
//! # Actors
//!
//! - [`ProjectActor`]: Per-project coordinator that owns database, indexer, and watcher
//! - [`IndexerActor`]: Handles all file indexing operations (single file, batch, rename, delete)
//! - [`WatcherTask`]: Watches filesystem for changes and feeds jobs to IndexerActor
//! - [`ProjectRouter`]: Routes requests to ProjectActors, spawning them on demand
//!
//! # Streaming Pipeline
//!
//! The indexer uses a streaming pipeline for file indexing with backpressure:
//!
//! ```text
//! Scanner → Reader → Parser → Embedder → Writer
//!   256      128      256       64       flush
//! ```
//!
//! See [`PipelineConfig`] for configuration and [`message`] for pipeline message types.

pub mod handle;
pub mod indexer;
pub mod pipeline;
mod project;
mod router;
mod scheduler;
mod watcher;

pub mod lifecycle;
pub mod message;

#[cfg(test)]
mod __tests__;

pub use router::ProjectRouter;
pub use scheduler::{IdleShutdownConfig, Scheduler, SchedulerConfig};
