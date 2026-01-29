//! Utility functions for the explore service.

/// Truncate content to a preview length.
pub fn truncate_preview(content: &str, max_len: usize) -> String {
  let content = content.trim();
  if content.len() <= max_len {
    content.to_string()
  } else {
    format!("{}...", &content[..max_len.saturating_sub(3)])
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_truncate_preview() {
    assert_eq!(truncate_preview("short", 10), "short");
    assert_eq!(truncate_preview("this is a longer string", 10), "this is...");
  }

  #[test]
  fn test_truncate_preview_exact_length() {
    assert_eq!(truncate_preview("exactly10!", 10), "exactly10!");
  }

  #[test]
  fn test_truncate_preview_whitespace() {
    assert_eq!(truncate_preview("  trimmed  ", 20), "trimmed");
  }

  #[test]
  fn test_truncate_preview_empty() {
    assert_eq!(truncate_preview("", 10), "");
  }
}
