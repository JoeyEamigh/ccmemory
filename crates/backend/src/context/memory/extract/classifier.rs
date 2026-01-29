//! Content extraction utilities for memories.
//!
//! Extracts concepts, file references, and identifiers from memory content.

use tracing::trace;

/// Extract concepts from memory content
pub fn extract_concepts(content: &str) -> Vec<String> {
  trace!(text_len = content.len(), "Extracting concepts");

  let mut concepts = Vec::new();

  // Backtick strings (code references)
  let backtick_count_before = concepts.len();
  for cap in find_backtick_content(content) {
    if cap.len() >= 2 && cap.len() <= 100 {
      concepts.push(cap);
    }
  }
  let backtick_concepts = concepts.len() - backtick_count_before;

  // CamelCase identifiers
  let camel_count_before = concepts.len();
  for word in content.split_whitespace() {
    if is_camel_case(word) && word.len() >= 3 && word.len() <= 50 {
      concepts.push(word.to_string());
    }
  }
  let camel_concepts = concepts.len() - camel_count_before;

  // snake_case identifiers
  let snake_count_before = concepts.len();
  for word in content.split_whitespace() {
    let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
    if is_snake_case(cleaned) && cleaned.len() >= 3 && cleaned.len() <= 50 {
      concepts.push(cleaned.to_string());
    }
  }
  let snake_concepts = concepts.len() - snake_count_before;

  // File paths
  let path_count_before = concepts.len();
  for word in content.split_whitespace() {
    if looks_like_file_path(word) {
      concepts.push(
        word
          .trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-')
          .to_string(),
      );
    }
  }
  let path_concepts = concepts.len() - path_count_before;

  // Deduplicate and sort
  concepts.sort();
  concepts.dedup();

  trace!(
    backtick = backtick_concepts,
    camel_case = camel_concepts,
    snake_case = snake_concepts,
    file_paths = path_concepts,
    total = concepts.len(),
    "Concepts extracted"
  );

  concepts
}

/// Extract file references from content
pub fn extract_files(content: &str) -> Vec<String> {
  trace!(text_len = content.len(), "Extracting file references");

  let mut files = Vec::new();

  for word in content.split_whitespace() {
    let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-');
    if looks_like_file_path(cleaned) {
      files.push(cleaned.to_string());
    }
  }

  // Also check for backtick-wrapped paths
  for cap in find_backtick_content(content) {
    if looks_like_file_path(&cap) {
      files.push(cap);
    }
  }

  files.sort();
  files.dedup();

  trace!(
    count = files.len(),
    files = ?files,
    "File references extracted"
  );

  files
}

fn find_backtick_content(content: &str) -> Vec<String> {
  let mut results = Vec::new();
  let mut in_backtick = false;
  let mut current = String::new();

  for ch in content.chars() {
    if ch == '`' {
      if in_backtick {
        if !current.is_empty() {
          results.push(current.clone());
        }
        current.clear();
      }
      in_backtick = !in_backtick;
    } else if in_backtick {
      current.push(ch);
    }
  }

  results
}

fn is_camel_case(s: &str) -> bool {
  if s.is_empty() {
    return false;
  }

  let chars: Vec<char> = s.chars().collect();

  // Must start with uppercase
  if !chars[0].is_uppercase() {
    return false;
  }

  // Must have at least one lowercase and one more uppercase (after first)
  let has_lower = chars.iter().any(|c| c.is_lowercase());
  let has_internal_upper = chars.iter().skip(1).any(|c| c.is_uppercase());

  has_lower && has_internal_upper && s.chars().all(|c| c.is_alphanumeric())
}

fn is_snake_case(s: &str) -> bool {
  if s.is_empty() || !s.contains('_') {
    return false;
  }

  // All lowercase letters, digits, and underscores
  s.chars().all(|c| c.is_lowercase() || c.is_ascii_digit() || c == '_')
    && !s.starts_with('_')
    && !s.ends_with('_')
    && !s.contains("__")
}

fn looks_like_file_path(s: &str) -> bool {
  if s.is_empty() || s.len() < 3 {
    return false;
  }

  // Must contain a dot for extension or a slash for path
  let has_extension = s.contains('.') && {
    let parts: Vec<&str> = s.split('.').collect();
    if let Some(ext) = parts.last() {
      !ext.is_empty() && ext.len() <= 4 && ext.chars().all(|c| c.is_alphabetic())
    } else {
      false
    }
  };

  let has_path_sep = s.contains('/');

  (has_extension || has_path_sep)
    && s
      .chars()
      .all(|c| c.is_alphanumeric() || c == '.' || c == '/' || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_extract_concepts_backticks() {
    let content = "The `UserService` handles `user_authentication` in the `auth.ts` file";
    let concepts = extract_concepts(content);

    assert!(concepts.contains(&"UserService".to_string()));
    assert!(concepts.contains(&"user_authentication".to_string()));
    assert!(concepts.contains(&"auth.ts".to_string()));
  }

  #[test]
  fn test_extract_concepts_camel_case() {
    let content = "The AuthController uses the UserRepository to fetch data";
    let concepts = extract_concepts(content);

    assert!(concepts.contains(&"AuthController".to_string()));
    assert!(concepts.contains(&"UserRepository".to_string()));
  }

  #[test]
  fn test_extract_concepts_snake_case() {
    let content = "Call the get_user_by_id function to retrieve user_data";
    let concepts = extract_concepts(content);

    assert!(concepts.contains(&"get_user_by_id".to_string()));
    assert!(concepts.contains(&"user_data".to_string()));
  }

  #[test]
  fn test_extract_files() {
    let content = "Check the src/auth/login.ts file and the `config/app.json` configuration";
    let files = extract_files(content);

    assert!(files.contains(&"src/auth/login.ts".to_string()));
    assert!(files.contains(&"config/app.json".to_string()));
  }

  #[test]
  fn test_casing_and_parsing() {
    assert!(is_camel_case("UserService"));
    assert!(is_camel_case("HttpResponse"));
    assert!(!is_camel_case("userservice"));
    assert!(!is_camel_case("USERSERVICE"));
    assert!(!is_camel_case("user_service"));

    assert!(is_snake_case("user_service"));
    assert!(is_snake_case("get_user_by_id"));
    assert!(!is_snake_case("userService"));
    assert!(!is_snake_case("_private"));
    assert!(!is_snake_case("trailing_"));
    assert!(!is_snake_case("double__underscore"));

    assert!(looks_like_file_path("src/auth/login.ts"));
    assert!(looks_like_file_path("config.json"));
    assert!(looks_like_file_path("README.md"));
    assert!(!looks_like_file_path("hello"));
    assert!(!looks_like_file_path("a.toolongextension"));
  }
}
