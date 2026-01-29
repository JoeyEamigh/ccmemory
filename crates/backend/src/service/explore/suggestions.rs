//! Suggestion generation for explore results.
//!
//! Generates related search suggestions using vector similarity:
//! 1. Symbols from semantically similar code chunks
//! 2. Entity names from related memories
//! 3. Already-found result symbols (cleaned and deduplicated)

use std::collections::HashSet;

use crate::db::ProjectDb;

/// Generate suggestions based on vector search results.
///
/// Uses the embedding to find semantically similar items and extracts
/// their symbols as suggestions. This replaces hardcoded synonym maps
/// with actual semantic similarity from the vector database.
///
/// # Arguments
/// * `db` - Project database for vector search
/// * `query_embedding` - The query's embedding vector (if available)
/// * `query` - The original search query (for deduplication)
/// * `result_symbols` - Symbols already found in search results
/// * `max_suggestions` - Maximum number of suggestions to return
pub async fn generate_suggestions(
  db: &ProjectDb,
  query_embedding: &[f32],
  query: &str,
  result_symbols: &[String],
  max_suggestions: usize,
) -> Vec<String> {
  let query_terms: HashSet<String> = tokenize_query(query).into_iter().collect();
  let mut seen: HashSet<String> = HashSet::new();
  let mut suggestions: Vec<String> = Vec::new();

  // Add query terms to seen set to avoid suggesting them back
  for term in &query_terms {
    seen.insert(term.clone());
  }

  // 1. Extract cleaned symbols from existing results (already semantically relevant)
  for symbol in result_symbols {
    let cleaned = clean_symbol(symbol);
    if !cleaned.is_empty() && !seen.contains(&cleaned) && !is_query_term(&cleaned, &query_terms) {
      seen.insert(cleaned.clone());
      suggestions.push(cleaned);
    }
  }

  // 2. If we have an embedding, search for additional related items
  // Search for additional code chunks (beyond what was in results)
  if let Ok(similar_code) = db.search_code_chunks(query_embedding, max_suggestions * 2, None).await {
    for (chunk, _distance) in similar_code {
      for symbol in &chunk.symbols {
        let cleaned = clean_symbol(symbol);
        if !cleaned.is_empty() && !seen.contains(&cleaned) && !is_query_term(&cleaned, &query_terms) {
          seen.insert(cleaned.clone());
          suggestions.push(cleaned);
        }
      }
    }
  }

  // Search for related memories to extract entity-like suggestions
  // Use the service layer function which properly filters out deleted memories
  if let Ok(similar_memories) =
    crate::service::memory::search::search_by_embedding(db, query_embedding, max_suggestions, None).await
  {
    for (memory, _distance) in similar_memories {
      // Extract potential entity names from memory content
      for word in extract_significant_terms(&memory.content) {
        if !seen.contains(&word) && !is_query_term(&word, &query_terms) {
          seen.insert(word.clone());
          suggestions.push(word);
        }
      }
    }
  }

  suggestions.truncate(max_suggestions);
  suggestions
}

/// Check if a term matches any query term (case-insensitive).
fn is_query_term(term: &str, query_terms: &HashSet<String>) -> bool {
  let term_lower = term.to_lowercase();
  query_terms.iter().any(|q| q.to_lowercase() == term_lower)
}

/// Tokenize a query into lowercase terms.
fn tokenize_query(query: &str) -> Vec<String> {
  query
    .to_lowercase()
    .split(|c: char| !c.is_alphanumeric() && c != '_')
    .filter(|s| s.len() >= 2)
    .map(String::from)
    .collect()
}

/// Clean up a symbol name for use as a suggestion.
///
/// Splits camelCase/PascalCase/snake_case and returns the longest
/// meaningful word component.
fn clean_symbol(symbol: &str) -> String {
  let cleaned = symbol.trim_start_matches('_').trim_end_matches('_');

  let mut words: Vec<String> = Vec::new();
  let mut current = String::new();

  for c in cleaned.chars() {
    if c == '_' {
      if !current.is_empty() {
        words.push(current.to_lowercase());
        current.clear();
      }
    } else if c.is_uppercase() && !current.is_empty() {
      words.push(current.to_lowercase());
      current = c.to_lowercase().to_string();
    } else {
      current.push(c.to_ascii_lowercase());
    }
  }
  if !current.is_empty() {
    words.push(current);
  }

  // Filter short words and return longest
  words
    .into_iter()
    .filter(|w| w.len() >= 3)
    .max_by_key(|w| w.len())
    .unwrap_or_default()
}

/// Extract significant terms from content (for memory-based suggestions).
///
/// Returns lowercase terms that look like identifiers or technical terms.
fn extract_significant_terms(content: &str) -> Vec<String> {
  // Common stop words
  const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for", "of", "with", "by", "from", "as", "is", "was",
    "are", "were", "been", "be", "have", "has", "had", "do", "does", "did", "will", "would", "could", "should", "this",
    "that", "these", "those", "it", "its", "if", "then", "else", "when", "where", "while", "how", "what", "which",
    "who", "not", "no", "yes", "all", "each", "every", "both", "few", "more", "most", "other", "some", "only", "own",
    "same", "so", "than", "too", "very", "just", "also", "now", "here", "there", "new", "use", "used", "using",
  ];

  content
    .split(|c: char| !c.is_alphanumeric() && c != '_')
    .filter(|w| {
      let lower = w.to_lowercase();
      w.len() >= 3 && w.len() <= 30 && !STOP_WORDS.contains(&lower.as_str())
    })
    .map(|w| w.to_lowercase())
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_clean_symbol_camel_case() {
    assert_eq!(clean_symbol("UserService"), "service");
    assert_eq!(clean_symbol("HTTPClient"), "client");
  }

  #[test]
  fn test_clean_symbol_snake_case() {
    assert_eq!(clean_symbol("_internal_helper"), "internal");
    assert_eq!(clean_symbol("authenticate_user"), "authenticate");
  }

  #[test]
  fn test_clean_symbol_single_word() {
    assert_eq!(clean_symbol("auth"), "auth");
    assert_eq!(clean_symbol("ab"), ""); // Too short
  }

  #[test]
  fn test_extract_significant_terms() {
    let terms = extract_significant_terms("The user authentication service handles requests");
    assert!(terms.contains(&"user".to_string()));
    assert!(terms.contains(&"authentication".to_string()));
    assert!(terms.contains(&"service".to_string()));
    // Stop words filtered
    assert!(!terms.contains(&"the".to_string()));
  }

  #[test]
  fn test_extract_significant_terms_filters_short() {
    let terms = extract_significant_terms("a b cd abc");
    assert!(!terms.contains(&"a".to_string()));
    assert!(!terms.contains(&"b".to_string()));
    assert!(!terms.contains(&"cd".to_string()));
    assert!(terms.contains(&"abc".to_string()));
  }

  #[test]
  fn test_tokenize_query() {
    let tokens = tokenize_query("user authentication flow");
    assert_eq!(tokens, vec!["user", "authentication", "flow"]);
  }

  #[test]
  fn test_tokenize_query_filters_short() {
    let tokens = tokenize_query("a b cd");
    assert_eq!(tokens, vec!["cd"]);
  }

  #[test]
  fn test_is_query_term() {
    let terms: HashSet<String> = ["auth", "login"].iter().map(|s| s.to_string()).collect();
    assert!(is_query_term("auth", &terms));
    assert!(is_query_term("AUTH", &terms)); // Case insensitive
    assert!(!is_query_term("database", &terms));
  }
}
