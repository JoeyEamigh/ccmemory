//! TreeSitterParser implementation

use std::collections::{HashMap, HashSet};
use tree_sitter::{InputEdit, Language as TsLanguage, Parser, Point, Query, QueryCursor, StreamingIterator, Tree};

use crate::queries;
use engram_core::Language;

/// Holds the queries for a specific language
pub struct LanguageQueries {
  pub imports: Option<Query>,
  pub calls: Option<Query>,
  pub definitions: Option<Query>,
}

/// Cached parse tree for a file
struct CachedTree {
  content_hash: u64,
  tree: Tree,
  /// Store content for incremental parsing diff computation
  content: String,
}

/// Edit information from file watcher or editor
#[derive(Debug, Clone)]
pub struct TextEdit {
  pub start_offset: usize,
  pub end_offset: usize,
  pub new_text: String,
}

/// A definition extracted from code
#[derive(Debug, Clone)]
pub struct Definition {
  pub name: String,
  pub kind: DefinitionKind,
  pub start_line: u32,
  pub end_line: u32,
}

/// The kind of definition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionKind {
  Function,
  Method,
  Class,
  Struct,
  Interface,
  Trait,
  Enum,
  Const,
  Type,
  Module,
}

/// Tree-sitter based code parser
///
/// Lazily loads parsers and queries for each language as needed.
/// Supports caching parsed trees to avoid redundant parsing when
/// processing multiple chunks from the same file.
///
/// Reuses a single QueryCursor instance to avoid allocation overhead
/// when running multiple queries.
pub struct TreeSitterParser {
  parsers: HashMap<Language, Parser>,
  queries: HashMap<Language, LanguageQueries>,
  /// Cached trees per language (for single-file-at-a-time processing)
  tree_cache: HashMap<Language, CachedTree>,
  /// Reusable query cursor (avoids allocation per query)
  query_cursor: QueryCursor,
}

impl TreeSitterParser {
  /// Create a new TreeSitterParser
  pub fn new() -> Self {
    Self {
      parsers: HashMap::new(),
      queries: HashMap::new(),
      tree_cache: HashMap::new(),
      query_cursor: QueryCursor::new(),
    }
  }

  /// Simple hash for content (for cache invalidation)
  fn hash_content(content: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    hasher.finish()
  }

  /// Parse and cache a file's tree for subsequent queries.
  /// Returns true if parsing was successful.
  pub fn parse_file(&mut self, content: &str, lang: Language) -> bool {
    self.ensure_loaded(lang);

    let content_hash = Self::hash_content(content);

    // Check if we already have this exact content cached
    if let Some(cached) = self.tree_cache.get(&lang)
      && cached.content_hash == content_hash
    {
      return true;
    }

    let Some(parser) = self.parsers.get_mut(&lang) else {
      return false;
    };

    if let Some(tree) = parser.parse(content, None) {
      self.tree_cache.insert(
        lang,
        CachedTree {
          content_hash,
          tree,
          content: content.to_string(),
        },
      );
      true
    } else {
      false
    }
  }

  /// Parse with incremental update if old tree available.
  ///
  /// This method attempts to reuse the previous parse tree when possible:
  /// 1. If content is unchanged (hash match), returns immediately
  /// 2. If explicit edit info is provided, applies it directly
  /// 3. Otherwise, computes diff between old/new content
  /// 4. Falls back to full reparse if changes are too large (>50% of file)
  ///
  /// Returns true if parsing was successful.
  pub fn parse_file_incremental(&mut self, content: &str, lang: Language, edit: Option<&TextEdit>) -> bool {
    self.ensure_loaded(lang);

    let content_hash = Self::hash_content(content);

    // Exact match - no reparse needed
    if let Some(cached) = self.tree_cache.get(&lang)
      && cached.content_hash == content_hash
    {
      return true;
    }

    let Some(parser) = self.parsers.get_mut(&lang) else {
      return false;
    };

    // Try incremental parse if we have old tree
    let old_tree = if let Some(cached) = self.tree_cache.get_mut(&lang) {
      if let Some(edit) = edit {
        // Apply explicit edit to old tree
        let input_edit = Self::compute_input_edit(edit, &cached.content);
        cached.tree.edit(&input_edit);
        Some(&cached.tree)
      } else if let Some(input_edit) = Self::diff_content(&cached.content, content) {
        // Compute diff and apply
        cached.tree.edit(&input_edit);
        Some(&cached.tree)
      } else {
        None // Content too different, full reparse
      }
    } else {
      None
    };

    // Parse with old tree if available (incremental), otherwise full parse
    if let Some(tree) = parser.parse(content, old_tree.map(|t| t as &Tree)) {
      self.tree_cache.insert(
        lang,
        CachedTree {
          tree,
          content_hash,
          content: content.to_string(),
        },
      );
      return true;
    }

    false
  }

  /// Compute InputEdit from TextEdit
  fn compute_input_edit(edit: &TextEdit, old_content: &str) -> InputEdit {
    let start_byte = edit.start_offset;
    let old_end_byte = edit.end_offset;
    let new_end_byte = start_byte + edit.new_text.len();

    let start_position = Self::offset_to_point(old_content, start_byte);
    let old_end_position = Self::offset_to_point(old_content, old_end_byte);

    // For new end position, compute based on the edit
    let new_end_position = Self::compute_new_end_position(old_content, edit, start_position);

    InputEdit {
      start_byte,
      old_end_byte,
      new_end_byte,
      start_position,
      old_end_position,
      new_end_position,
    }
  }

  /// Convert byte offset to tree-sitter Point (row, column)
  fn offset_to_point(content: &str, offset: usize) -> Point {
    let prefix = &content[..offset.min(content.len())];
    let row = prefix.matches('\n').count();
    let col = prefix.rfind('\n').map_or(offset, |pos| offset - pos - 1);
    Point::new(row, col)
  }

  /// Compute new end position after applying edit
  fn compute_new_end_position(_old_content: &str, edit: &TextEdit, start_position: Point) -> Point {
    // Count newlines in the new text
    let new_text_newlines = edit.new_text.matches('\n').count();

    if new_text_newlines == 0 {
      // Single line edit - column moves by new text length
      Point::new(start_position.row, start_position.column + edit.new_text.len())
    } else {
      // Multi-line edit - find column after last newline
      let last_newline_pos = edit.new_text.rfind('\n').unwrap();
      let col_after_newline = edit.new_text.len() - last_newline_pos - 1;
      Point::new(start_position.row + new_text_newlines, col_after_newline)
    }
  }

  /// Compute InputEdit by diffing old and new content.
  /// Returns None if changes are too large (>50% of file changed).
  fn diff_content(old: &str, new: &str) -> Option<InputEdit> {
    // Find common prefix
    let prefix_len = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();

    // Find common suffix (excluding prefix)
    let old_suffix = &old[prefix_len..];
    let new_suffix = &new[prefix_len..];
    let suffix_len = old_suffix
      .bytes()
      .rev()
      .zip(new_suffix.bytes().rev())
      .take_while(|(a, b)| a == b)
      .count();

    let start_byte = prefix_len;
    let old_end_byte = old.len().saturating_sub(suffix_len);
    let new_end_byte = new.len().saturating_sub(suffix_len);

    // Only use incremental if edit is localized (< 50% of file changed)
    if !old.is_empty() {
      let changed_ratio = (old_end_byte.saturating_sub(start_byte)) as f64 / old.len() as f64;
      if changed_ratio > 0.5 {
        return None; // Too much changed, full reparse likely faster
      }
    }

    let start_position = Self::offset_to_point(old, start_byte);
    let old_end_position = Self::offset_to_point(old, old_end_byte);
    let new_end_position = Self::offset_to_point(new, new_end_byte);

    Some(InputEdit {
      start_byte,
      old_end_byte,
      new_end_byte,
      start_position,
      old_end_position,
      new_end_position,
    })
  }

  /// Extract definitions using cached tree (parses if needed).
  /// More efficient when processing multiple operations on the same file.
  pub fn extract_definitions_cached(&mut self, content: &str, lang: Language) -> Vec<Definition> {
    if !self.parse_file(content, lang) {
      return Vec::new();
    }

    let Some(cached) = self.tree_cache.get(&lang) else {
      return Vec::new();
    };

    let Some(queries) = self.queries.get(&lang) else {
      return Vec::new();
    };

    let Some(query) = &queries.definitions else {
      return Vec::new();
    };

    let mut definitions = Vec::new();

    let mut matches = self
      .query_cursor
      .matches(query, cached.tree.root_node(), content.as_bytes());

    while let Some(match_) = matches.next() {
      let mut name: Option<String> = None;
      let mut start_line: Option<u32> = None;
      let mut end_line: Option<u32> = None;
      let mut kind = DefinitionKind::Function;

      for cap in match_.captures {
        let cap_name = &query.capture_names()[cap.index as usize];
        let node = cap.node;

        match *cap_name {
          "name" => {
            if let Ok(text) = node.utf8_text(content.as_bytes()) {
              name = Some(text.to_string());
            }
          }
          "definition.function" | "function" => {
            kind = DefinitionKind::Function;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.method" | "method" => {
            kind = DefinitionKind::Method;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.class" | "class" => {
            kind = DefinitionKind::Class;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.struct" | "struct" => {
            kind = DefinitionKind::Struct;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.interface" | "interface" => {
            kind = DefinitionKind::Interface;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.trait" | "trait" => {
            kind = DefinitionKind::Trait;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.enum" | "enum" => {
            kind = DefinitionKind::Enum;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.module" | "module" => {
            kind = DefinitionKind::Module;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.const" | "const" => {
            kind = DefinitionKind::Const;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          "definition.type" | "type" => {
            kind = DefinitionKind::Type;
            start_line = Some(node.start_position().row as u32 + 1);
            end_line = Some(node.end_position().row as u32 + 1);
          }
          _ => {}
        }
      }

      if let (Some(n), Some(sl), Some(el)) = (name, start_line, end_line) {
        definitions.push(Definition {
          name: n,
          kind,
          start_line: sl,
          end_line: el,
        });
      }
    }

    definitions
  }

  /// Convert 1-based line numbers to byte range in content.
  /// Returns (start_byte, end_byte) where end_byte is exclusive.
  fn line_range_to_byte_range(content: &str, start_line: u32, end_line: u32) -> (usize, usize) {
    let mut current_line = 1u32;
    let mut start_byte = 0usize;
    let mut end_byte = content.len();
    let mut found_start = false;

    for (i, c) in content.char_indices() {
      if !found_start && current_line >= start_line {
        start_byte = i;
        found_start = true;
      }
      if c == '\n' {
        current_line += 1;
        if current_line > end_line {
          end_byte = i + 1; // Include the newline
          break;
        }
      }
    }

    (start_byte, end_byte)
  }

  /// Extract imports and calls from a specific line range using cached tree.
  /// Uses byte range filtering for efficient query execution (avoids full tree traversal).
  pub fn extract_imports_and_calls_in_range(
    &mut self,
    content: &str,
    lang: Language,
    start_line: u32,
    end_line: u32,
  ) -> (Vec<String>, Vec<String>) {
    if !self.parse_file(content, lang) {
      return (Vec::new(), Vec::new());
    }

    let Some(cached) = self.tree_cache.get(&lang) else {
      return (Vec::new(), Vec::new());
    };

    let Some(queries) = self.queries.get(&lang) else {
      return (Vec::new(), Vec::new());
    };

    let mut imports = Vec::new();
    let mut calls = Vec::new();

    // Convert line range to byte range for efficient query filtering
    let (start_byte, end_byte) = Self::line_range_to_byte_range(content, start_line, end_line);

    // Get references to queries and tree before using cursor
    let imports_query = queries.imports.as_ref();
    let calls_query = queries.calls.as_ref();
    let root = cached.tree.root_node();

    // Run imports query with byte range filtering
    if let Some(query) = imports_query {
      // Set byte range to limit query to relevant portion of tree
      self.query_cursor.set_byte_range(start_byte..end_byte);
      let mut matches = self.query_cursor.matches(query, root, content.as_bytes());
      while let Some(match_) = matches.next() {
        for cap in match_.captures {
          if let Ok(text) = cap.node.utf8_text(content.as_bytes()) {
            let cleaned = text.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == '<' || c == '>');
            if !cleaned.is_empty() {
              imports.push(cleaned.to_string());
            }
          }
        }
      }
    }

    // Run calls query with byte range filtering
    if let Some(query) = calls_query {
      self.query_cursor.set_byte_range(start_byte..end_byte);
      let mut matches = self.query_cursor.matches(query, root, content.as_bytes());
      while let Some(match_) = matches.next() {
        for cap in match_.captures {
          if let Ok(text) = cap.node.utf8_text(content.as_bytes()) {
            let cleaned = text.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == '<' || c == '>');
            if !cleaned.is_empty() {
              calls.push(cleaned.to_string());
            }
          }
        }
      }
    }

    // Reset byte range for future queries (0..MAX means no filtering)
    self.query_cursor.set_byte_range(0..usize::MAX);

    // Deduplicate while preserving order
    let mut seen: HashSet<String> = HashSet::new();
    imports.retain(|s| seen.insert(s.clone()));

    seen.clear();
    calls.retain(|s| seen.insert(s.clone()));

    (imports, calls)
  }

  /// Clear the tree cache (call when switching to a different file)
  pub fn clear_cache(&mut self) {
    self.tree_cache.clear();
  }

  /// Check if a language is supported for parsing
  pub fn supports_language(&self, lang: Language) -> bool {
    self.get_grammar(lang).is_some()
  }

  /// Extract import statements from code
  pub fn extract_imports(&mut self, content: &str, lang: Language) -> Vec<String> {
    self.run_query(content, lang, |q| &q.imports)
  }

  /// Extract function/method calls from code
  pub fn extract_calls(&mut self, content: &str, lang: Language) -> Vec<String> {
    self.run_query(content, lang, |q| &q.calls)
  }

  /// Extract imports and calls using cached tree (parses if needed).
  /// Uses the tree cache for efficiency when processing multiple operations on the same file.
  pub fn extract_imports_and_calls(&mut self, content: &str, lang: Language) -> (Vec<String>, Vec<String>) {
    // Use cache - parse_file will use cached tree if content unchanged
    if !self.parse_file(content, lang) {
      return (Vec::new(), Vec::new());
    }

    let Some(cached) = self.tree_cache.get(&lang) else {
      return (Vec::new(), Vec::new());
    };

    let Some(queries) = self.queries.get(&lang) else {
      return (Vec::new(), Vec::new());
    };

    let mut imports = Vec::new();
    let mut calls = Vec::new();

    // Get query references and tree root before using cursor
    let imports_query = queries.imports.as_ref();
    let calls_query = queries.calls.as_ref();
    let root = cached.tree.root_node();

    // Run imports query (reusing cursor)
    if let Some(query) = imports_query {
      let mut matches = self.query_cursor.matches(query, root, content.as_bytes());
      while let Some(match_) = matches.next() {
        for cap in match_.captures {
          if let Ok(text) = cap.node.utf8_text(content.as_bytes()) {
            let cleaned = text.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == '<' || c == '>');
            if !cleaned.is_empty() {
              imports.push(cleaned.to_string());
            }
          }
        }
      }
    }

    // Run calls query (cursor is reset when matches() is called again)
    if let Some(query) = calls_query {
      let mut matches = self.query_cursor.matches(query, root, content.as_bytes());
      while let Some(match_) = matches.next() {
        for cap in match_.captures {
          if let Ok(text) = cap.node.utf8_text(content.as_bytes()) {
            let cleaned = text.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == '<' || c == '>');
            if !cleaned.is_empty() {
              calls.push(cleaned.to_string());
            }
          }
        }
      }
    }

    // Deduplicate while preserving order
    let mut seen: HashSet<String> = HashSet::new();
    imports.retain(|s| seen.insert(s.clone()));

    seen.clear();
    calls.retain(|s| seen.insert(s.clone()));

    (imports, calls)
  }

  /// Extract symbol definitions from code using cached tree (parses if needed).
  /// This is equivalent to extract_definitions_cached but with a shorter name for convenience.
  pub fn extract_definitions(&mut self, content: &str, lang: Language) -> Vec<Definition> {
    // Use cache - just delegate to the cached version
    self.extract_definitions_cached(content, lang)
  }

  fn run_query<F>(&mut self, content: &str, lang: Language, get_query: F) -> Vec<String>
  where
    F: Fn(&LanguageQueries) -> &Option<Query>,
  {
    // Use cache - parse_file will use cached tree if content unchanged
    if !self.parse_file(content, lang) {
      return Vec::new();
    }

    let Some(cached) = self.tree_cache.get(&lang) else {
      return Vec::new();
    };

    let Some(queries) = self.queries.get(&lang) else {
      return Vec::new();
    };

    let Some(query) = get_query(queries) else {
      return Vec::new();
    };

    let mut results: Vec<String> = Vec::new();
    let root = cached.tree.root_node();

    // Use reusable cursor
    let mut matches = self.query_cursor.matches(query, root, content.as_bytes());

    while let Some(match_) = matches.next() {
      for cap in match_.captures {
        if let Ok(text) = cap.node.utf8_text(content.as_bytes()) {
          // Clean up the string (remove quotes and angle brackets for imports, etc.)
          let cleaned = text.trim_matches(|c: char| c == '"' || c == '\'' || c == '`' || c == '<' || c == '>');
          if !cleaned.is_empty() {
            results.push(cleaned.to_string());
          }
        }
      }
    }

    // Deduplicate while preserving order
    let mut seen: HashSet<String> = HashSet::new();
    results.retain(|s| seen.insert(s.clone()));

    results
  }

  fn ensure_loaded(&mut self, lang: Language) {
    if self.parsers.contains_key(&lang) {
      return;
    }

    if let Some(grammar) = self.get_grammar(lang) {
      let mut parser = Parser::new();
      if parser.set_language(&grammar).is_ok() {
        self.parsers.insert(lang, parser);
        self.queries.insert(lang, queries::load_queries(lang, &grammar));
      }
    }
  }

  fn get_grammar(&self, lang: Language) -> Option<TsLanguage> {
    match lang {
      Language::Rust => Some(tree_sitter_rust::LANGUAGE.into()),
      Language::Python => Some(tree_sitter_python::LANGUAGE.into()),
      Language::JavaScript | Language::Jsx => Some(tree_sitter_javascript::LANGUAGE.into()),
      Language::TypeScript => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
      Language::Tsx => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
      Language::Go => Some(tree_sitter_go::LANGUAGE.into()),
      Language::Java => Some(tree_sitter_java::LANGUAGE.into()),
      Language::C => Some(tree_sitter_c::LANGUAGE.into()),
      Language::Cpp => Some(tree_sitter_cpp::LANGUAGE.into()),

      // Tier 2 (feature-gated)
      #[cfg(feature = "tier2")]
      Language::Ruby => Some(tree_sitter_ruby::LANGUAGE.into()),
      #[cfg(feature = "tier2")]
      Language::Php => Some(tree_sitter_php::LANGUAGE_PHP.into()),
      #[cfg(feature = "tier2")]
      Language::CSharp => Some(tree_sitter_c_sharp::LANGUAGE.into()),
      #[cfg(feature = "tier2")]
      Language::Kotlin => Some(tree_sitter_kotlin::LANGUAGE.into()),
      #[cfg(feature = "tier2")]
      Language::Shell => Some(tree_sitter_bash::LANGUAGE.into()),

      // Unsupported or not compiled
      _ => None,
    }
  }
}

impl Default for TreeSitterParser {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  // ============================================================================
  // LANGUAGE SUPPORT TESTS
  // ============================================================================

  #[test]
  fn test_supports_tier1_languages() {
    let parser = TreeSitterParser::new();

    assert!(parser.supports_language(Language::Rust));
    assert!(parser.supports_language(Language::Python));
    assert!(parser.supports_language(Language::JavaScript));
    assert!(parser.supports_language(Language::TypeScript));
    assert!(parser.supports_language(Language::Tsx));
    assert!(parser.supports_language(Language::Jsx));
    assert!(parser.supports_language(Language::Go));
    assert!(parser.supports_language(Language::Java));
    assert!(parser.supports_language(Language::C));
    assert!(parser.supports_language(Language::Cpp));
  }

  #[test]
  fn test_unsupported_language_returns_empty() {
    let mut parser = TreeSitterParser::new();

    // Markdown has no import/call queries
    let imports = parser.extract_imports("# Header", Language::Markdown);
    assert!(imports.is_empty());

    let calls = parser.extract_calls("# Header", Language::Markdown);
    assert!(calls.is_empty());

    let defs = parser.extract_definitions("# Header", Language::Markdown);
    assert!(defs.is_empty());
  }

  // ============================================================================
  // ERROR HANDLING TESTS
  // ============================================================================

  #[test]
  fn test_invalid_syntax_returns_partial_results() {
    let mut parser = TreeSitterParser::new();

    // Invalid Rust syntax - parser should still work with partial results
    let content = r#"
use std::collections::HashMap;
fn broken( { // syntax error
    let x = helper_fn();
}
use chrono::Utc;
"#;
    let imports = parser.extract_imports(content, Language::Rust);
    // Should still extract valid imports despite syntax errors
    assert!(
      imports.contains(&"std::collections::HashMap".to_string()),
      "imports: {:?}",
      imports
    );
    assert!(imports.contains(&"chrono::Utc".to_string()), "imports: {:?}", imports);
  }

  #[test]
  fn test_empty_content() {
    let mut parser = TreeSitterParser::new();

    let imports = parser.extract_imports("", Language::Rust);
    assert!(imports.is_empty());

    let calls = parser.extract_calls("", Language::Rust);
    assert!(calls.is_empty());

    let defs = parser.extract_definitions("", Language::Rust);
    assert!(defs.is_empty());
  }

  #[test]
  fn test_whitespace_only_content() {
    let mut parser = TreeSitterParser::new();

    let content = "   \n\t\n   ";
    let imports = parser.extract_imports(content, Language::Rust);
    assert!(imports.is_empty());

    let calls = parser.extract_calls(content, Language::Python);
    assert!(calls.is_empty());
  }

  // ============================================================================
  // EDGE CASES: RUST
  // ============================================================================

  #[test]
  fn test_rust_nested_use_lists() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
use std::collections::{HashMap, HashSet, BTreeMap};
use std::{
    io::{self, Read, Write},
    fs::File,
};
use crate::foo::bar::{baz, qux as quux};
"#;
    let imports = parser.extract_imports(content, Language::Rust);

    // The parser should extract individual items from use lists
    assert!(imports.contains(&"HashMap".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"HashSet".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"BTreeMap".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"baz".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"qux".to_string()), "imports: {:?}", imports);
  }

  #[test]
  fn test_rust_reexports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
pub use crate::module::Type;
pub use self::internal::Helper;
pub(crate) use super::parent::Item;
"#;
    let imports = parser.extract_imports(content, Language::Rust);

    assert!(
      imports.contains(&"crate::module::Type".to_string()),
      "imports: {:?}",
      imports
    );
    assert!(
      imports.contains(&"self::internal::Helper".to_string()),
      "imports: {:?}",
      imports
    );
    assert!(
      imports.contains(&"super::parent::Item".to_string()),
      "imports: {:?}",
      imports
    );
  }

  #[test]
  fn test_rust_async_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
async fn example() {
    let result = fetch_data().await;
    let processed = process(result).await?;
    tokio::spawn(async move {
        task_fn().await;
    });
}
"#;
    let calls = parser.extract_calls(content, Language::Rust);

    assert!(calls.contains(&"fetch_data".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"process".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"spawn".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"task_fn".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_rust_closure_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
fn example() {
    let items = vec![1, 2, 3];
    items.iter().map(|x| transform(x)).filter(|y| validate(y)).collect();
    let closure = |a, b| compute(a, b);
    closure(1, 2);
}
"#;
    let calls = parser.extract_calls(content, Language::Rust);

    assert!(calls.contains(&"iter".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"map".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"transform".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"filter".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"validate".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"collect".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"compute".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"closure".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_rust_generic_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
fn example() {
    let map: HashMap<String, i32> = HashMap::new();
    let result = parse::<MyType>(data);
    Vec::<u8>::with_capacity(100);
}
"#;
    let calls = parser.extract_calls(content, Language::Rust);

    // Parser should handle turbofish syntax for generic calls
    assert!(calls.contains(&"new".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"parse".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"with_capacity".to_string()), "calls: {:?}", calls);
  }

  // ============================================================================
  // EDGE CASES: PYTHON
  // ============================================================================

  #[test]
  fn test_python_relative_imports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
from . import sibling_module
from .. import parent_module
from ...grandparent import specific
from .utils import helper, another
"#;
    let imports = parser.extract_imports(content, Language::Python);

    // Check we capture relative import patterns
    assert!(
      imports.iter().any(|i| i.starts_with('.')),
      "should have relative imports: {:?}",
      imports
    );
  }

  #[test]
  fn test_python_star_import() {
    let mut parser = TreeSitterParser::new();
    let content = "from module import *";
    let imports = parser.extract_imports(content, Language::Python);

    assert!(imports.contains(&"module".to_string()), "imports: {:?}", imports);
  }

  #[test]
  fn test_python_decorated_function_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
@decorator
@another_decorator(arg)
def my_function():
    helper()

@property
def computed(self):
    return calculate()
"#;
    let calls = parser.extract_calls(content, Language::Python);

    // Parser should extract decorators as calls (they're invocations)
    assert!(calls.contains(&"decorator".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"another_decorator".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"helper".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"property".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"calculate".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_python_comprehension_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
result = [transform(x) for x in items if validate(x)]
gen = (process(i) for i in range(10))
dict_comp = {key(k): value(v) for k, v in pairs}
"#;
    let calls = parser.extract_calls(content, Language::Python);

    assert!(calls.contains(&"transform".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"validate".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"process".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"range".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"key".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"value".to_string()), "calls: {:?}", calls);
  }

  // ============================================================================
  // EDGE CASES: TYPESCRIPT/JAVASCRIPT
  // ============================================================================

  #[test]
  fn test_typescript_type_only_imports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
// Type-only imports
import type { User, Post } from './types';
import type * as Types from './all-types';

// Mixed: type-only and regular imports in same statement
import { type Config, useConfig } from './config';
import { useState, type Dispatch, type SetStateAction } from 'react';

// Regular imports for comparison
import axios from 'axios';
import { helper } from './utils';
"#;
    let imports = parser.extract_imports(content, Language::TypeScript);

    // Type-only imports should be captured
    assert!(imports.contains(&"./types".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"./all-types".to_string()), "imports: {:?}", imports);

    // Mixed imports should be captured
    assert!(imports.contains(&"./config".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"react".to_string()), "imports: {:?}", imports);

    // Regular imports
    assert!(imports.contains(&"axios".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"./utils".to_string()), "imports: {:?}", imports);
  }

  #[test]
  fn test_typescript_dynamic_imports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
const module = await import('./dynamic');
const { default: Component } = await import(`./components/${name}`);
"#;
    let imports = parser.extract_imports(content, Language::TypeScript);

    assert!(imports.contains(&"./dynamic".to_string()), "imports: {:?}", imports);
  }

  #[test]
  fn test_jsx_fragment_and_components() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
function App() {
    return (
        <>
            <Header />
            <Main>
                <Sidebar items={items} />
                <Content />
            </Main>
            <Footer />
        </>
    );
}
"#;
    let calls = parser.extract_calls(content, Language::Jsx);

    assert!(calls.contains(&"Header".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"Main".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"Sidebar".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"Content".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"Footer".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_typescript_optional_chaining_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
const result = obj?.method();
const nested = a?.b?.c();
const arr = items?.[0]?.transform();
"#;
    let calls = parser.extract_calls(content, Language::TypeScript);

    assert!(calls.contains(&"method".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"c".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"transform".to_string()), "calls: {:?}", calls);
  }

  /// Test NodeNext/Node16 module resolution style imports
  /// Uses .js extension for TypeScript files, explicit extensions required
  #[test]
  fn test_typescript_nodenext_imports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
// NodeNext/Node16: Must use .js extension for .ts files
import { helper } from './utils.js';           // Actual file: ./utils.ts
import { Button } from './components/Button.js'; // Actual file: ./components/Button.tsx
import type { Config } from './config.js';

// ESM extensions
import { logger } from './logging.mjs';
import { data } from './data.cjs';

// JSON imports with assertion (Node16+)
import config from './config.json' assert { type: 'json' };

// External packages (no extension needed)
import express from 'express';
import { z } from 'zod';
"#;
    let imports = parser.extract_imports(content, Language::TypeScript);

    // NodeNext style with .js extension
    assert!(imports.contains(&"./utils.js".to_string()), "imports: {:?}", imports);
    assert!(
      imports.contains(&"./components/Button.js".to_string()),
      "imports: {:?}",
      imports
    );
    assert!(imports.contains(&"./config.js".to_string()), "imports: {:?}", imports);

    // ESM extensions
    assert!(imports.contains(&"./logging.mjs".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"./data.cjs".to_string()), "imports: {:?}", imports);

    // JSON import
    assert!(imports.contains(&"./config.json".to_string()), "imports: {:?}", imports);

    // External packages
    assert!(imports.contains(&"express".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"zod".to_string()), "imports: {:?}", imports);
  }

  /// Test Bundler module resolution style imports
  /// No extensions needed, bundler handles resolution
  #[test]
  fn test_typescript_bundler_imports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
// Bundler style: No extensions needed
import { helper } from './utils';
import { Button } from './components/Button';
import type { Config } from './config';

// Index file imports (bundler resolves)
import { routes } from './routes';         // Resolves to ./routes/index.ts
import * as models from './models';        // Resolves to ./models/index.ts

// Alias imports (bundler resolves via paths config)
import { api } from '@/api';
import { useAuth } from '@hooks/useAuth';
import { Button } from '~/components/Button';

// External packages
import React, { useState, useEffect } from 'react';
import { clsx } from 'clsx';
"#;
    let imports = parser.extract_imports(content, Language::TypeScript);

    // Bundler style without extensions
    assert!(imports.contains(&"./utils".to_string()), "imports: {:?}", imports);
    assert!(
      imports.contains(&"./components/Button".to_string()),
      "imports: {:?}",
      imports
    );
    assert!(imports.contains(&"./config".to_string()), "imports: {:?}", imports);

    // Index file imports
    assert!(imports.contains(&"./routes".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"./models".to_string()), "imports: {:?}", imports);

    // Alias imports
    assert!(imports.contains(&"@/api".to_string()), "imports: {:?}", imports);
    assert!(
      imports.contains(&"@hooks/useAuth".to_string()),
      "imports: {:?}",
      imports
    );
    assert!(
      imports.contains(&"~/components/Button".to_string()),
      "imports: {:?}",
      imports
    );

    // External packages
    assert!(imports.contains(&"react".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"clsx".to_string()), "imports: {:?}", imports);
  }

  /// Test Classic/Node10 module resolution style imports
  /// .ts extension allowed, node_modules resolution
  #[test]
  fn test_typescript_classic_imports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
// Classic/Node10: Can use .ts extension directly (less common)
import { helper } from './utils.ts';
import { Button } from './components/Button.tsx';

// Also supports extensionless
import { config } from './config';

// Package imports
import lodash from 'lodash';
import { map, filter } from 'lodash/fp';

// Namespace import
import * as fs from 'fs';
import * as path from 'path';

// Side effect imports
import 'reflect-metadata';
import './polyfills';
"#;
    let imports = parser.extract_imports(content, Language::TypeScript);

    // Classic style with .ts extension
    assert!(imports.contains(&"./utils.ts".to_string()), "imports: {:?}", imports);
    assert!(
      imports.contains(&"./components/Button.tsx".to_string()),
      "imports: {:?}",
      imports
    );

    // Extensionless
    assert!(imports.contains(&"./config".to_string()), "imports: {:?}", imports);

    // Package imports
    assert!(imports.contains(&"lodash".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"lodash/fp".to_string()), "imports: {:?}", imports);

    // Built-in modules
    assert!(imports.contains(&"fs".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"path".to_string()), "imports: {:?}", imports);

    // Side effect imports
    assert!(
      imports.contains(&"reflect-metadata".to_string()),
      "imports: {:?}",
      imports
    );
    assert!(imports.contains(&"./polyfills".to_string()), "imports: {:?}", imports);
  }

  /// Test require() and dynamic imports for CommonJS interop
  #[test]
  fn test_typescript_require_imports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
// CommonJS require (for CJS modules)
const fs = require('fs');
const { join } = require('path');
const config = require('./config.json');

// Dynamic import (ESM)
const module = await import('./dynamic.js');
const { default: Component } = await import('./Component');

// Re-exports
export { helper } from './utils';
export * from './constants';
export * as utils from './utils';
"#;
    let imports = parser.extract_imports(content, Language::TypeScript);

    // require() calls
    assert!(imports.contains(&"fs".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"path".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"./config.json".to_string()), "imports: {:?}", imports);

    // Dynamic imports
    assert!(imports.contains(&"./dynamic.js".to_string()), "imports: {:?}", imports);

    // Re-exports count as imports (source dependency)
    assert!(imports.contains(&"./utils".to_string()), "imports: {:?}", imports);
    assert!(imports.contains(&"./constants".to_string()), "imports: {:?}", imports);
  }

  // ============================================================================
  // EDGE CASES: GO
  // ============================================================================

  #[test]
  fn test_go_blank_identifier_import() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
import (
    _ "github.com/lib/pq"        // side-effect import
    . "fmt"                       // dot import
    alias "long/package/name"
)
"#;
    let imports = parser.extract_imports(content, Language::Go);

    assert!(
      imports.contains(&"github.com/lib/pq".to_string()),
      "imports: {:?}",
      imports
    );
    assert!(imports.contains(&"fmt".to_string()), "imports: {:?}", imports);
    assert!(
      imports.contains(&"long/package/name".to_string()),
      "imports: {:?}",
      imports
    );
  }

  #[test]
  fn test_go_deferred_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
func example() {
    defer file.Close()
    defer func() { cleanup() }()
    go processAsync()
}
"#;
    let calls = parser.extract_calls(content, Language::Go);

    assert!(calls.contains(&"Close".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"cleanup".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"processAsync".to_string()), "calls: {:?}", calls);
  }

  // ============================================================================
  // EDGE CASES: JAVA
  // ============================================================================

  #[test]
  fn test_java_static_imports() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
import static java.lang.Math.PI;
import static java.util.Collections.*;
import static org.junit.Assert.assertEquals;
"#;
    let imports = parser.extract_imports(content, Language::Java);

    // Static imports should be captured
    assert!(imports.iter().any(|i| i.contains("Math")), "imports: {:?}", imports);
  }

  #[test]
  fn test_java_anonymous_class_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
public class Example {
    public void test() {
        Runnable r = new Runnable() {
            @Override
            public void run() {
                execute();
            }
        };
        list.forEach(item -> process(item));
    }
}
"#;
    let calls = parser.extract_calls(content, Language::Java);

    assert!(calls.contains(&"execute".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"forEach".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"process".to_string()), "calls: {:?}", calls);
  }

  // ============================================================================
  // EDGE CASES: C/C++
  // ============================================================================

  #[test]
  fn test_c_macro_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
#include <assert.h>
#define MAX(a, b) ((a) > (b) ? (a) : (b))

int main() {
    assert(condition);
    int x = MAX(1, 2);
    printf("result: %d\n", x);
}
"#;
    let calls = parser.extract_calls(content, Language::C);

    assert!(calls.contains(&"assert".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"MAX".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"printf".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_cpp_template_calls() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
#include <memory>
#include <vector>

void example() {
    auto ptr = std::make_shared<MyClass>(args);
    auto vec = std::vector<int>{1, 2, 3};
    std::sort(vec.begin(), vec.end());
}
"#;
    let calls = parser.extract_calls(content, Language::Cpp);

    assert!(calls.contains(&"sort".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"begin".to_string()), "calls: {:?}", calls);
    assert!(calls.contains(&"end".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_cpp_namespace_using() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
using namespace std;
using std::cout;
using std::endl;

namespace myns {
    void func() {}
}
"#;
    let imports = parser.extract_imports(content, Language::Cpp);

    // Check that using declarations are captured
    assert!(!imports.is_empty(), "should have using declarations: {:?}", imports);
  }

  // ============================================================================
  // DEFINITION EXTRACTION TESTS
  // ============================================================================

  #[test]
  fn test_definition_line_numbers() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
fn first_fn() {
    // line 2-4
}

fn second_fn() {
    // line 6-8
}
"#;
    let defs = parser.extract_definitions(content, Language::Rust);

    let first = defs.iter().find(|d| d.name == "first_fn");
    let second = defs.iter().find(|d| d.name == "second_fn");

    assert!(first.is_some(), "should find first_fn");
    assert!(second.is_some(), "should find second_fn");

    let first = first.unwrap();
    let second = second.unwrap();

    assert!(first.start_line < second.start_line, "first should be before second");
    assert_eq!(first.kind, DefinitionKind::Function);
    assert_eq!(second.kind, DefinitionKind::Function);
  }

  #[test]
  fn test_definition_kinds() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
fn my_function() {}
struct MyStruct {}
enum MyEnum { A, B }
trait MyTrait {}
mod my_module {}
const MY_CONST: i32 = 42;
type MyType = String;
"#;
    let defs = parser.extract_definitions(content, Language::Rust);

    let fn_def = defs.iter().find(|d| d.name == "my_function");
    let struct_def = defs.iter().find(|d| d.name == "MyStruct");
    let enum_def = defs.iter().find(|d| d.name == "MyEnum");
    let trait_def = defs.iter().find(|d| d.name == "MyTrait");
    let mod_def = defs.iter().find(|d| d.name == "my_module");

    assert!(
      matches!(fn_def.map(|d| d.kind), Some(DefinitionKind::Function)),
      "fn kind"
    );
    assert!(
      matches!(struct_def.map(|d| d.kind), Some(DefinitionKind::Struct)),
      "struct kind"
    );
    assert!(
      matches!(enum_def.map(|d| d.kind), Some(DefinitionKind::Enum)),
      "enum kind"
    );
    assert!(
      matches!(trait_def.map(|d| d.kind), Some(DefinitionKind::Trait)),
      "trait kind"
    );
    assert!(
      matches!(mod_def.map(|d| d.kind), Some(DefinitionKind::Module)),
      "module kind"
    );
  }

  // ============================================================================
  // DEDUPLICATION TESTS
  // ============================================================================

  #[test]
  fn test_deduplication() {
    let mut parser = TreeSitterParser::new();
    let content = r#"
fn example() {
    helper();
    helper();
    helper();
}
"#;
    let calls = parser.extract_calls(content, Language::Rust);

    // Should only have one entry for "helper"
    let helper_count = calls.iter().filter(|c| *c == "helper").count();
    assert_eq!(helper_count, 1, "helper should appear only once: {:?}", calls);
  }

  // ============================================================================
  // PARSER REUSE TESTS
  // ============================================================================

  #[test]
  fn test_parser_reuse_across_files() {
    let mut parser = TreeSitterParser::new();

    // Parse multiple files of the same language
    let rust1 = "use std::io; fn a() { helper1(); }";
    let rust2 = "use std::fs; fn b() { helper2(); }";

    let imports1 = parser.extract_imports(rust1, Language::Rust);
    let calls1 = parser.extract_calls(rust1, Language::Rust);

    let imports2 = parser.extract_imports(rust2, Language::Rust);
    let calls2 = parser.extract_calls(rust2, Language::Rust);

    assert!(imports1.contains(&"std::io".to_string()));
    assert!(calls1.contains(&"helper1".to_string()));
    assert!(imports2.contains(&"std::fs".to_string()));
    assert!(calls2.contains(&"helper2".to_string()));
  }

  #[test]
  fn test_parser_multiple_languages() {
    let mut parser = TreeSitterParser::new();

    let rust_code = "use std::io; fn main() { println!(\"hello\"); }";
    let python_code = "import os\nprint('hello')";
    let js_code = "import fs from 'fs'; console.log('hello');";

    let rust_imports = parser.extract_imports(rust_code, Language::Rust);
    let python_imports = parser.extract_imports(python_code, Language::Python);
    let js_imports = parser.extract_imports(js_code, Language::JavaScript);

    assert!(rust_imports.contains(&"std::io".to_string()));
    assert!(python_imports.contains(&"os".to_string()));
    assert!(js_imports.contains(&"fs".to_string()));
  }

  // ============================================================================
  // INCREMENTAL PARSING TESTS
  // ============================================================================

  #[test]
  fn test_incremental_parse_small_edit() {
    let mut parser = TreeSitterParser::new();

    // Parse original content
    let original = r#"
fn main() {
    println!("hello");
}

fn helper() {
    println!("original");
}
"#;

    assert!(parser.parse_file(original, Language::Rust));

    // Small edit: change "original" to "modified"
    let modified = r#"
fn main() {
    println!("hello");
}

fn helper() {
    println!("modified");
}
"#;

    // Incremental parse should succeed
    assert!(parser.parse_file_incremental(modified, Language::Rust, None));

    // Verify parsing still works correctly
    let defs = parser.extract_definitions_cached(modified, Language::Rust);
    assert!(defs.iter().any(|d| d.name == "main"), "should find main");
    assert!(defs.iter().any(|d| d.name == "helper"), "should find helper");
  }

  #[test]
  fn test_incremental_parse_with_explicit_edit() {
    let mut parser = TreeSitterParser::new();

    let original = "fn foo() { bar(); }";
    assert!(parser.parse_file(original, Language::Rust));

    // Edit: change "bar" to "baz"
    let modified = "fn foo() { baz(); }";
    let edit = super::TextEdit {
      start_offset: 11, // position of 'b' in 'bar'
      end_offset: 14,   // position after 'r' in 'bar'
      new_text: "baz".to_string(),
    };

    assert!(parser.parse_file_incremental(modified, Language::Rust, Some(&edit)));

    let calls = parser.extract_calls(modified, Language::Rust);
    assert!(calls.contains(&"baz".to_string()), "calls: {:?}", calls);
  }

  #[test]
  fn test_diff_content_finds_single_edit() {
    let old = "fn main() { hello(); }";
    let new = "fn main() { goodbye(); }";

    let edit = TreeSitterParser::diff_content(old, new);
    assert!(edit.is_some(), "should find edit");

    let edit = edit.unwrap();
    // The edit should be localized to the function name change
    assert!(
      edit.start_byte <= 12,
      "start should be before 'hello': {}",
      edit.start_byte
    );
    assert!(
      edit.old_end_byte >= 17,
      "old end should be after 'hello': {}",
      edit.old_end_byte
    );
  }

  #[test]
  fn test_diff_content_large_change_returns_none() {
    let old = "fn a() {} fn b() {} fn c() {}";
    let new = "completely different content here";

    // Large changes (>50% different) should return None
    let edit = TreeSitterParser::diff_content(old, new);
    assert!(edit.is_none(), "should return None for large changes");
  }

  #[test]
  fn test_offset_to_point_single_line() {
    let content = "hello world";
    let point = TreeSitterParser::offset_to_point(content, 6);
    assert_eq!(point.row, 0);
    assert_eq!(point.column, 6);
  }

  #[test]
  fn test_offset_to_point_multi_line() {
    let content = "line one\nline two\nline three";
    // 'l' in "line two" is at offset 9
    let point = TreeSitterParser::offset_to_point(content, 9);
    assert_eq!(point.row, 1);
    assert_eq!(point.column, 0);

    // 't' in "two" is at offset 14
    let point = TreeSitterParser::offset_to_point(content, 14);
    assert_eq!(point.row, 1);
    assert_eq!(point.column, 5);
  }

  #[test]
  fn test_incremental_parse_add_function() {
    let mut parser = TreeSitterParser::new();

    let original = r#"
fn main() {
    println!("hello");
}
"#;

    assert!(parser.parse_file(original, Language::Rust));

    // Add a new function
    let modified = r#"
fn main() {
    println!("hello");
}

fn new_function() {
    println!("new");
}
"#;

    assert!(parser.parse_file_incremental(modified, Language::Rust, None));

    let defs = parser.extract_definitions_cached(modified, Language::Rust);
    assert!(defs.iter().any(|d| d.name == "main"), "should find main");
    assert!(
      defs.iter().any(|d| d.name == "new_function"),
      "should find new_function"
    );
  }

  #[test]
  fn test_incremental_parse_delete_function() {
    let mut parser = TreeSitterParser::new();

    let original = r#"
fn main() {
    println!("hello");
}

fn to_delete() {
    println!("bye");
}
"#;

    assert!(parser.parse_file(original, Language::Rust));

    // Delete the second function
    let modified = r#"
fn main() {
    println!("hello");
}
"#;

    assert!(parser.parse_file_incremental(modified, Language::Rust, None));

    let defs = parser.extract_definitions_cached(modified, Language::Rust);
    assert!(defs.iter().any(|d| d.name == "main"), "should find main");
    assert!(
      !defs.iter().any(|d| d.name == "to_delete"),
      "should not find deleted function"
    );
  }
}
