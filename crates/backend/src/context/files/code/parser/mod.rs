//! Tree-sitter based code parsing for CCEngram
//!
//! This crate provides accurate extraction of:
//! - Import statements
//! - Function/method calls
//! - Symbol definitions
//!
//! # Example
//! ```ignore
//! use ccengram_parser::TreeSitterParser;
//! use crate::domain::Language;
//!
//! let mut parser = TreeSitterParser::new();
//! let imports = parser.extract_imports(code, Language::Rust);
//! let calls = parser.extract_calls(code, Language::Rust);
//! ```

mod languages;
mod sitter;

pub use sitter::*;
