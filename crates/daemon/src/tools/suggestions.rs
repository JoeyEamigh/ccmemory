//! Suggestion generation for explore results.
//!
//! Generates related search suggestions based on:
//! 1. Query term expansion (synonyms, related concepts)
//! 2. Frequent symbols/concepts from results
//! 3. Entity names from the project

use std::collections::{HashMap, HashSet};

/// Query expansion mappings for common programming concepts.
/// These are bidirectional - if you search for "auth", you might want "authentication",
/// and vice versa.
fn get_expansion_map() -> HashMap<&'static str, Vec<&'static str>> {
  let mut map = HashMap::new();

  // Authentication & authorization
  map.insert("auth", vec!["authentication", "authorization", "login", "session"]);
  map.insert("authentication", vec!["auth", "login", "credentials", "oauth"]);
  map.insert("authorization", vec!["auth", "permissions", "roles", "access"]);
  map.insert("login", vec!["auth", "signin", "authentication", "session"]);
  map.insert("session", vec!["auth", "token", "jwt", "cookie"]);
  map.insert("oauth", vec!["authentication", "sso", "token", "openid"]);
  map.insert("jwt", vec!["token", "auth", "session", "bearer"]);

  // Database
  map.insert("database", vec!["db", "sql", "query", "orm", "repository"]);
  map.insert("db", vec!["database", "sql", "storage", "persistence"]);
  map.insert("sql", vec!["database", "query", "orm", "migration"]);
  map.insert("orm", vec!["database", "model", "entity", "repository"]);
  map.insert("query", vec!["database", "sql", "search", "filter"]);
  map.insert("migration", vec!["database", "schema", "sql", "upgrade"]);
  map.insert("repository", vec!["database", "dao", "store", "persistence"]);

  // API & HTTP
  map.insert("api", vec!["endpoint", "rest", "http", "route", "handler"]);
  map.insert("endpoint", vec!["api", "route", "handler", "controller"]);
  map.insert("rest", vec!["api", "http", "endpoint", "crud"]);
  map.insert("http", vec!["api", "request", "response", "client"]);
  map.insert("route", vec!["api", "endpoint", "handler", "path"]);
  map.insert("handler", vec!["api", "controller", "endpoint", "route"]);
  map.insert("middleware", vec!["api", "handler", "interceptor", "filter"]);

  // Error handling
  map.insert("error", vec!["exception", "failure", "result", "handling"]);
  map.insert("exception", vec!["error", "throw", "catch", "try"]);
  map.insert("result", vec!["error", "option", "maybe", "either"]);

  // Testing
  map.insert("test", vec!["testing", "unit", "integration", "mock"]);
  map.insert("testing", vec!["test", "spec", "assertion", "fixture"]);
  map.insert("mock", vec!["test", "stub", "fake", "double"]);
  map.insert("unit", vec!["test", "testing", "isolated", "function"]);
  map.insert("integration", vec!["test", "e2e", "end-to-end", "system"]);

  // Configuration
  map.insert("config", vec!["configuration", "settings", "options", "env"]);
  map.insert("configuration", vec!["config", "settings", "setup", "options"]);
  map.insert("settings", vec!["config", "options", "preferences", "parameters"]);
  map.insert("env", vec!["config", "environment", "variables", "dotenv"]);

  // Async & concurrency
  map.insert("async", vec!["await", "future", "promise", "concurrent"]);
  map.insert("concurrent", vec!["async", "parallel", "thread", "sync"]);
  map.insert("thread", vec!["concurrent", "parallel", "spawn", "worker"]);
  map.insert("sync", vec!["concurrent", "mutex", "lock", "atomic"]);

  // Data structures
  map.insert("list", vec!["array", "vector", "collection", "slice"]);
  map.insert("map", vec!["dict", "hashmap", "object", "record"]);
  map.insert("set", vec!["hashset", "collection", "unique"]);
  map.insert("tree", vec!["node", "graph", "hierarchy", "structure"]);

  // Patterns
  map.insert("factory", vec!["builder", "create", "construct", "pattern"]);
  map.insert("builder", vec!["factory", "construct", "fluent", "pattern"]);
  map.insert("singleton", vec!["instance", "global", "pattern"]);
  map.insert("observer", vec!["event", "listener", "subscribe", "pattern"]);
  map.insert("strategy", vec!["policy", "behavior", "pattern"]);

  // Frontend
  map.insert("component", vec!["widget", "view", "ui", "render"]);
  map.insert("state", vec!["store", "redux", "context", "management"]);
  map.insert("render", vec!["display", "draw", "view", "ui"]);
  map.insert("style", vec!["css", "styling", "theme", "layout"]);

  // File operations
  map.insert("file", vec!["io", "read", "write", "filesystem"]);
  map.insert("io", vec!["file", "stream", "read", "write"]);
  map.insert("stream", vec!["io", "buffer", "read", "write"]);

  // Network
  map.insert("network", vec!["socket", "connection", "tcp", "http"]);
  map.insert("socket", vec!["network", "tcp", "connection", "websocket"]);
  map.insert("websocket", vec!["socket", "realtime", "ws", "connection"]);

  // Security
  map.insert("security", vec!["encryption", "hash", "password", "auth"]);
  map.insert("encryption", vec!["security", "crypto", "decrypt", "cipher"]);
  map.insert("hash", vec!["security", "digest", "sha", "md5"]);
  map.insert("password", vec!["security", "hash", "auth", "credential"]);

  // Logging & monitoring
  map.insert("log", vec!["logging", "trace", "debug", "monitor"]);
  map.insert("logging", vec!["log", "logger", "trace", "output"]);
  map.insert("trace", vec!["log", "debug", "span", "telemetry"]);
  map.insert("metric", vec!["monitor", "measure", "stats", "telemetry"]);

  map
}

/// Tokenize a query into individual terms.
fn tokenize_query(query: &str) -> Vec<String> {
  query
    .to_lowercase()
    .split(|c: char| !c.is_alphanumeric() && c != '_')
    .filter(|s| !s.is_empty() && s.len() >= 2)
    .map(String::from)
    .collect()
}

/// Calculate similarity between two strings using Levenshtein distance.
fn string_similarity(a: &str, b: &str) -> f32 {
  let a_lower = a.to_lowercase();
  let b_lower = b.to_lowercase();

  if a_lower == b_lower {
    return 1.0;
  }

  // Check if one is a substring of the other
  if a_lower.contains(&b_lower) || b_lower.contains(&a_lower) {
    return 0.8;
  }

  // Check prefix match
  let min_len = a_lower.len().min(b_lower.len());
  let common_prefix = a_lower
    .chars()
    .zip(b_lower.chars())
    .take_while(|(c1, c2)| c1 == c2)
    .count();

  if common_prefix >= min_len / 2 {
    return 0.6;
  }

  0.0
}

/// Generate suggestions based on query and results.
///
/// # Arguments
/// * `query` - The original search query
/// * `result_symbols` - Symbols extracted from search results
/// * `result_content_words` - Frequent words from result content
/// * `max_suggestions` - Maximum number of suggestions to return
///
/// # Returns
/// A list of suggested search terms, ordered by relevance.
pub fn generate_suggestions(
  query: &str,
  result_symbols: &[String],
  result_content_words: &[String],
  max_suggestions: usize,
) -> Vec<String> {
  let query_terms: HashSet<String> = tokenize_query(query).into_iter().collect();
  let expansion_map = get_expansion_map();

  let mut suggestions: Vec<(String, f32)> = Vec::new();
  let mut seen: HashSet<String> = HashSet::new();

  // Add query terms to seen set to avoid suggesting them back
  for term in &query_terms {
    seen.insert(term.clone());
  }

  // 1. Add expanded terms from the query
  for term in &query_terms {
    if let Some(expansions) = expansion_map.get(term.as_str()) {
      for expansion in expansions {
        let exp_string = expansion.to_string();
        if !seen.contains(&exp_string) && !is_too_similar(&exp_string, &query_terms) {
          seen.insert(exp_string.clone());
          suggestions.push((exp_string, 0.9)); // High priority for direct expansions
        }
      }
    }
  }

  // 2. Add frequent symbols from results (cleaned up)
  for symbol in result_symbols {
    let clean_symbol = clean_symbol(symbol);
    if !clean_symbol.is_empty() && !seen.contains(&clean_symbol) && !is_too_similar(&clean_symbol, &query_terms) {
      seen.insert(clean_symbol.clone());
      suggestions.push((clean_symbol, 0.7)); // Medium priority for result symbols
    }
  }

  // 3. Add frequent content words (if they're programming-relevant)
  for word in result_content_words {
    let word_lower = word.to_lowercase();
    if !seen.contains(&word_lower) && !is_too_similar(&word_lower, &query_terms) && is_programming_term(&word_lower) {
      seen.insert(word_lower.clone());
      suggestions.push((word_lower, 0.5)); // Lower priority for content words
    }
  }

  // 4. Look for reverse expansions (if query matches an expansion, suggest the key)
  for (key, expansions) in expansion_map.iter() {
    for term in &query_terms {
      if expansions.contains(&term.as_str()) {
        let key_string = key.to_string();
        if !seen.contains(&key_string) && !is_too_similar(&key_string, &query_terms) {
          seen.insert(key_string.clone());
          suggestions.push((key_string, 0.85)); // High priority for reverse expansions
        }
      }
    }
  }

  // Sort by priority (descending) and take top N
  suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

  suggestions.into_iter().take(max_suggestions).map(|(s, _)| s).collect()
}

/// Check if a suggestion is too similar to any query term.
fn is_too_similar(suggestion: &str, query_terms: &HashSet<String>) -> bool {
  for term in query_terms {
    if string_similarity(suggestion, term) >= 0.7 {
      return true;
    }
  }
  false
}

/// Clean up a symbol name for use as a suggestion.
fn clean_symbol(symbol: &str) -> String {
  // Remove common prefixes/suffixes
  let cleaned = symbol.trim_start_matches('_').trim_end_matches('_');

  // Split camelCase/PascalCase and snake_case
  let mut words: Vec<String> = Vec::new();
  let mut current_word = String::new();

  for c in cleaned.chars() {
    if c == '_' {
      if !current_word.is_empty() {
        words.push(current_word.to_lowercase());
        current_word = String::new();
      }
    } else if c.is_uppercase() && !current_word.is_empty() {
      words.push(current_word.to_lowercase());
      current_word = c.to_lowercase().to_string();
    } else {
      current_word.push(c.to_ascii_lowercase());
    }
  }
  if !current_word.is_empty() {
    words.push(current_word.to_lowercase());
  }

  // Filter to words with at least 2 chars
  let words: Vec<String> = words.into_iter().filter(|w| w.len() >= 2).collect();

  // If it's a compound name, return the longest meaningful word
  if words.len() > 1 {
    words.into_iter().max_by_key(|w| w.len()).unwrap_or_default()
  } else {
    words.into_iter().next().unwrap_or_default()
  }
}

/// Check if a word is likely a programming-related term.
fn is_programming_term(word: &str) -> bool {
  let programming_terms: HashSet<&str> = [
    // Common programming concepts
    "function",
    "method",
    "class",
    "struct",
    "interface",
    "trait",
    "enum",
    "type",
    "module",
    "package",
    "import",
    "export",
    "async",
    "await",
    "promise",
    "future",
    "thread",
    "process",
    "memory",
    "pointer",
    "reference",
    "variable",
    "constant",
    "parameter",
    "argument",
    "return",
    "yield",
    "error",
    "exception",
    "result",
    "option",
    "null",
    "none",
    "some",
    "true",
    "false",
    "boolean",
    "string",
    "number",
    "integer",
    "float",
    "array",
    "vector",
    "list",
    "map",
    "set",
    "hash",
    "queue",
    "stack",
    "tree",
    "node",
    "graph",
    "loop",
    "iterator",
    "range",
    "filter",
    "reduce",
    "collect",
    "stream",
    "buffer",
    "reader",
    "writer",
    "file",
    "path",
    "directory",
    "socket",
    "connection",
    "request",
    "response",
    "client",
    "server",
    "handler",
    "controller",
    "service",
    "repository",
    "model",
    "view",
    "component",
    "template",
    "render",
    "state",
    "props",
    "context",
    "hook",
    "callback",
    "listener",
    "event",
    "signal",
    "channel",
    "mutex",
    "lock",
    "atomic",
    "sync",
    "cache",
    "store",
    "database",
    "query",
    "schema",
    "migration",
    "index",
    "key",
    "value",
    "record",
    "field",
    "column",
    "table",
    "transaction",
    "commit",
    "rollback",
    "test",
    "mock",
    "stub",
    "fixture",
    "assertion",
    "expect",
    "config",
    "setting",
    "option",
    "flag",
    "feature",
    "plugin",
    "extension",
    "middleware",
    "decorator",
    "annotation",
    "attribute",
    "macro",
    "generic",
    "template",
    "trait",
    "protocol",
    "delegate",
    "factory",
    "builder",
    "singleton",
    "observer",
    "strategy",
    "adapter",
    "proxy",
    "wrapper",
    "util",
    "helper",
    "common",
    "shared",
    "internal",
    "public",
    "private",
    "protected",
    "static",
    "final",
    "abstract",
    "virtual",
    "override",
    "implement",
    "extend",
    "inherit",
    "compose",
    "inject",
    "provide",
    "consume",
    "produce",
    "publish",
    "subscribe",
    "dispatch",
    "emit",
    "trigger",
    "parse",
    "serialize",
    "deserialize",
    "encode",
    "decode",
    "encrypt",
    "decrypt",
    "compress",
    "decompress",
    "validate",
    "sanitize",
    "normalize",
    "transform",
    "convert",
    "format",
    "render",
    "display",
    "print",
    "debug",
    "trace",
    "info",
    "warn",
    "fatal",
    "panic",
    "abort",
    "exit",
    "init",
    "setup",
    "teardown",
    "cleanup",
    "dispose",
    "destroy",
    "create",
    "update",
    "delete",
    "remove",
    "insert",
    "append",
    "push",
    "pop",
    "shift",
    "unshift",
    "slice",
    "splice",
    "concat",
    "merge",
    "split",
    "join",
    "sort",
    "reverse",
    "search",
    "find",
    "lookup",
    "match",
    "replace",
    "substitute",
  ]
  .into_iter()
  .collect();

  programming_terms.contains(word)
    || word.len() >= 4 && (word.ends_with("er") || word.ends_with("or") || word.ends_with("ion"))
}

/// Extract significant words from content for suggestion generation.
pub fn extract_content_words(content: &str, limit: usize) -> Vec<String> {
  let mut word_counts: HashMap<String, usize> = HashMap::new();

  // Common stop words to filter out
  let stop_words: HashSet<&str> = [
    "the",
    "a",
    "an",
    "and",
    "or",
    "but",
    "in",
    "on",
    "at",
    "to",
    "for",
    "of",
    "with",
    "by",
    "from",
    "as",
    "is",
    "was",
    "are",
    "were",
    "been",
    "be",
    "have",
    "has",
    "had",
    "do",
    "does",
    "did",
    "will",
    "would",
    "could",
    "should",
    "may",
    "might",
    "must",
    "shall",
    "can",
    "need",
    "this",
    "that",
    "these",
    "those",
    "it",
    "its",
    "if",
    "then",
    "else",
    "when",
    "where",
    "while",
    "how",
    "what",
    "which",
    "who",
    "whom",
    "why",
    "not",
    "no",
    "yes",
    "all",
    "each",
    "every",
    "both",
    "few",
    "more",
    "most",
    "other",
    "some",
    "such",
    "only",
    "own",
    "same",
    "so",
    "than",
    "too",
    "very",
    "just",
    "also",
    "now",
    "here",
    "there",
    "new",
    "old",
    "first",
    "last",
    "long",
    "great",
    "little",
    "own",
    "other",
    "old",
    "right",
    "big",
    "high",
    "small",
    "next",
    "early",
    "young",
    "important",
    "public",
    "bad",
    "true",
    "false",
  ]
  .into_iter()
  .collect();

  for word in content.split(|c: char| !c.is_alphanumeric() && c != '_') {
    let word_lower = word.to_lowercase();
    if word_lower.len() >= 3
      && word_lower.len() <= 30
      && !stop_words.contains(word_lower.as_str())
      && word_lower.chars().all(|c| c.is_alphanumeric() || c == '_')
    {
      *word_counts.entry(word_lower).or_insert(0) += 1;
    }
  }

  // Sort by count and take top N
  let mut sorted: Vec<_> = word_counts.into_iter().collect();
  sorted.sort_by(|a, b| b.1.cmp(&a.1));

  sorted.into_iter().take(limit).map(|(word, _)| word).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_tokenize_query() {
    let tokens = tokenize_query("user authentication flow");
    assert_eq!(tokens, vec!["user", "authentication", "flow"]);

    let tokens = tokenize_query("UserService.authenticate");
    assert_eq!(tokens, vec!["userservice", "authenticate"]);
  }

  #[test]
  fn test_generate_suggestions_from_query() {
    let suggestions = generate_suggestions("auth", &[], &[], 5);

    assert!(!suggestions.is_empty());
    // Should suggest related terms
    let has_related = suggestions
      .iter()
      .any(|s| s == "authentication" || s == "authorization" || s == "login" || s == "session");
    assert!(has_related, "Expected related terms, got: {:?}", suggestions);
    // Should not contain the original query term
    assert!(!suggestions.contains(&"auth".to_string()));
  }

  #[test]
  fn test_generate_suggestions_with_symbols() {
    let symbols = vec![
      "UserService".to_string(),
      "authenticate".to_string(),
      "SessionManager".to_string(),
    ];

    let suggestions = generate_suggestions("login", &symbols, &[], 10);

    // Should include expansions of "login"
    assert!(
      suggestions
        .iter()
        .any(|s| s == "auth" || s == "session" || s == "authentication")
    );
    // Should include cleaned symbols
    assert!(
      suggestions
        .iter()
        .any(|s| s == "user" || s == "session" || s == "authenticate")
    );
  }

  #[test]
  fn test_suggestions_no_duplicates() {
    let suggestions = generate_suggestions(
      "database query",
      &["QueryBuilder".to_string(), "DatabaseConnection".to_string()],
      &["sql".to_string(), "query".to_string()],
      10,
    );

    // Check for uniqueness
    let unique: HashSet<_> = suggestions.iter().collect();
    assert_eq!(suggestions.len(), unique.len());
  }

  #[test]
  fn test_suggestions_limit() {
    let suggestions = generate_suggestions("api endpoint handler", &[], &[], 3);
    assert!(suggestions.len() <= 3);
  }

  #[test]
  fn test_suggestions_not_similar_to_query() {
    let suggestions = generate_suggestions("authentication", &[], &[], 5);

    // Should not suggest things too similar to "authentication"
    for suggestion in &suggestions {
      assert!(string_similarity(suggestion, "authentication") < 0.7);
    }
  }

  #[test]
  fn test_extract_content_words() {
    let content =
      "The user authentication service handles login requests and validates credentials against the database.";
    let words = extract_content_words(content, 5);

    // Should extract programming-relevant words
    assert!(words.contains(&"authentication".to_string()) || words.contains(&"service".to_string()));
    // Should not include stop words
    assert!(!words.contains(&"the".to_string()));
    assert!(!words.contains(&"and".to_string()));
  }

  #[test]
  fn test_clean_symbol() {
    assert_eq!(clean_symbol("UserService"), "service"); // Longest word
    assert_eq!(clean_symbol("_internal_helper"), "internal"); // Longest word
    assert_eq!(clean_symbol("HTTPClient"), "client"); // Longest word
    assert_eq!(clean_symbol("auth"), "auth"); // Single word
    assert_eq!(clean_symbol("authenticate_user"), "authenticate"); // Longest word
  }

  #[test]
  fn test_is_programming_term() {
    assert!(is_programming_term("function"));
    assert!(is_programming_term("handler"));
    assert!(is_programming_term("repository"));
    assert!(!is_programming_term("cat"));
    assert!(!is_programming_term("hello"));
  }

  #[test]
  fn test_string_similarity() {
    assert_eq!(string_similarity("auth", "auth"), 1.0);
    assert!(string_similarity("authentication", "auth") > 0.5);
    assert!(string_similarity("login", "logout") > 0.5);
    assert!(string_similarity("database", "testing") < 0.5);
  }
}
