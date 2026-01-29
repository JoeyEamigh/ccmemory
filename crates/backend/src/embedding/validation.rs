// Text validation utilities for embedding providers
//
// Provides validation and truncation for text inputs before embedding,
// protecting against oversized chunks that exceed model context limits.

use tracing::warn;

use crate::config::CHARS_PER_TOKEN;

/// Configuration for text validation.
#[derive(Debug, Clone)]
pub struct TextValidationConfig {
  /// Maximum tokens allowed for embedding (model-specific).
  pub max_tokens: usize,
  /// Estimated characters per token for size calculation.
  pub chars_per_token: usize,
}

impl TextValidationConfig {
  /// Create config for a specific model's context length.
  pub fn for_context_length(context_length: usize) -> Self {
    Self {
      max_tokens: context_length,
      chars_per_token: CHARS_PER_TOKEN,
    }
  }

  /// Maximum characters allowed based on token estimate.
  pub fn max_chars(&self) -> usize {
    self.max_tokens * self.chars_per_token
  }

  /// Estimate token count for a text string.
  pub fn estimate_tokens(&self, text: &str) -> usize {
    text.len() / self.chars_per_token
  }
}

/// Result of validating text for embedding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
  /// Text is within limits, use as-is.
  Valid,
  /// Text exceeds limits and was truncated.
  Truncated {
    original_len: usize,
    truncated_len: usize,
    estimated_original_tokens: usize,
  },
}

/// Validate and optionally truncate text for embedding.
///
/// If the text exceeds the configured token limit, it will be truncated
/// to fit within the limit. Truncation happens at character boundaries
/// to avoid splitting multi-byte characters.
///
/// Returns the (possibly truncated) text and a validation result indicating
/// what happened.
///
/// # Example
///
/// ```
/// use ccengram_backend::embedding::validation::{validate_and_truncate, TextValidationConfig};
///
/// let config = TextValidationConfig::for_context_length(8192);
/// let (text, result) = validate_and_truncate("Hello, world!", &config);
/// ```
pub fn validate_and_truncate(text: &str, config: &TextValidationConfig) -> (String, ValidationResult) {
  let estimated_tokens = config.estimate_tokens(text);

  if estimated_tokens <= config.max_tokens {
    return (text.to_string(), ValidationResult::Valid);
  }

  // Need to truncate
  let max_chars = config.max_chars();
  let truncated: String = text.chars().take(max_chars).collect();
  let truncated_len = truncated.len();

  warn!(
    original_len = text.len(),
    truncated_len = truncated_len,
    estimated_tokens = estimated_tokens,
    max_tokens = config.max_tokens,
    "Text exceeds embedding model context limit, truncating"
  );

  (
    truncated,
    ValidationResult::Truncated {
      original_len: text.len(),
      truncated_len,
      estimated_original_tokens: estimated_tokens,
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_for_context_length() {
    let config = TextValidationConfig::for_context_length(4096);
    assert_eq!(config.max_tokens, 4096);
    assert_eq!(config.max_chars(), 4096 * 4);
  }

  #[test]
  fn test_estimate_tokens() {
    let config = TextValidationConfig::for_context_length(4096);
    // 100 chars / 4 chars per token = 25 tokens
    let text = "a".repeat(100);
    assert_eq!(config.estimate_tokens(&text), 25);
  }

  #[test]
  fn test_valid_text() {
    let config = TextValidationConfig::for_context_length(100);
    let text = "Hello, world!"; // 13 chars = ~3 tokens

    let (result, validation) = validate_and_truncate(text, &config);
    assert_eq!(result, text);
    assert_eq!(validation, ValidationResult::Valid);
  }

  #[test]
  fn test_truncated_text() {
    // Very small limit for testing
    let config = TextValidationConfig {
      max_tokens: 2,
      chars_per_token: 4,
    };
    // max_chars = 8

    let text = "Hello, wonderful world!"; // 23 chars
    let (result, validation) = validate_and_truncate(text, &config);

    assert_eq!(result, "Hello, w"); // First 8 chars
    match validation {
      ValidationResult::Truncated {
        original_len,
        truncated_len,
        ..
      } => {
        assert_eq!(original_len, 23);
        assert_eq!(truncated_len, 8);
      }
      _ => panic!("Expected Truncated result"),
    }
  }

  #[test]
  fn test_unicode_truncation() {
    // Ensure we don't split multi-byte characters
    let config = TextValidationConfig {
      max_tokens: 1,
      chars_per_token: 4,
    };
    // max_chars = 4

    // Unicode text with multi-byte chars
    let text = "Hello 世界!"; // Mix of ASCII and CJK
    let (result, _) = validate_and_truncate(text, &config);

    // Should truncate at character boundary, not byte boundary
    assert_eq!(result.chars().count(), 4);
    // Verify it's valid UTF-8 (would panic if not)
    let _ = result.as_str();
  }
}
