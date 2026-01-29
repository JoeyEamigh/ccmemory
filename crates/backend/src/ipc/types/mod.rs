//! IPC types - unified request/response types with conversions
//!
//! Each domain concept has its own module containing:
//! - Request types (input parameters)
//! - Response types (output data)
//! - Conversion traits from domain types

pub mod code;
pub mod docs;
pub mod hook;
pub mod memory;
pub mod project;
pub mod relationship;
pub mod search;
pub mod system;
pub mod watch;
