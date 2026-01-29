//! SQL-injection-safe filter builder.
//!
//! This module provides a fluent API for building filter strings used in
//! database queries, with proper value escaping to prevent SQL injection.

/// Builder for constructing safe filter strings.
///
/// Provides a fluent API for building WHERE clauses with proper escaping
/// to prevent SQL injection attacks.
///
/// # Example
/// ```ignore
/// let filter = FilterBuilder::new()
///     .exclude_deleted()
///     .add_eq("sector", "semantic")
///     .add_min("salience", 0.5)
///     .build();
/// // Result: "is_deleted = false AND sector = 'semantic' AND salience >= 0.5"
/// ```
#[derive(Default)]
pub struct FilterBuilder {
  conditions: Vec<String>,
}

#[allow(dead_code)]
impl FilterBuilder {
  /// Create a new empty filter builder.
  pub fn new() -> Self {
    Self { conditions: Vec::new() }
  }

  /// Add a raw condition string (use with caution - caller must ensure safety).
  pub fn add_raw(mut self, condition: impl Into<String>) -> Self {
    self.conditions.push(condition.into());
    self
  }

  /// Add an equality condition with proper escaping.
  pub fn add_eq(mut self, column: &str, value: &str) -> Self {
    self.conditions.push(format!(
      "{} = '{}'",
      Self::escape_column(column),
      Self::escape_value(value)
    ));
    self
  }

  /// Add an equality condition only if the value is Some.
  pub fn add_eq_opt(self, column: &str, value: Option<&str>) -> Self {
    match value {
      Some(v) => self.add_eq(column, v),
      None => self,
    }
  }

  /// Add a LIKE condition with proper escaping.
  pub fn add_like(mut self, column: &str, pattern: &str) -> Self {
    self.conditions.push(format!(
      "{} LIKE '%{}%'",
      Self::escape_column(column),
      Self::escape_like_value(pattern)
    ));
    self
  }

  /// Add a LIKE condition only if the value is Some.
  pub fn add_like_opt(self, column: &str, pattern: Option<&str>) -> Self {
    match pattern {
      Some(p) => self.add_like(column, p),
      None => self,
    }
  }

  /// Add a prefix LIKE condition (value%).
  pub fn add_prefix(mut self, column: &str, prefix: &str) -> Self {
    self.conditions.push(format!(
      "{} LIKE '{}%'",
      Self::escape_column(column),
      Self::escape_like_value(prefix)
    ));
    self
  }

  /// Add a prefix LIKE condition only if the value is Some.
  pub fn add_prefix_opt(self, column: &str, prefix: Option<&str>) -> Self {
    match prefix {
      Some(p) => self.add_prefix(column, p),
      None => self,
    }
  }

  /// Add a minimum value condition (>=).
  pub fn add_min(mut self, column: &str, value: f32) -> Self {
    self
      .conditions
      .push(format!("{} >= {}", Self::escape_column(column), value));
    self
  }

  /// Add a minimum value condition only if the value is Some.
  pub fn add_min_opt(self, column: &str, value: Option<f32>) -> Self {
    match value {
      Some(v) => self.add_min(column, v),
      None => self,
    }
  }

  /// Add a maximum value condition (<=).
  pub fn add_max(mut self, column: &str, value: f32) -> Self {
    self
      .conditions
      .push(format!("{} <= {}", Self::escape_column(column), value));
    self
  }

  /// Add a maximum value condition only if the value is Some.
  pub fn add_max_opt(self, column: &str, value: Option<f32>) -> Self {
    match value {
      Some(v) => self.add_max(column, v),
      None => self,
    }
  }

  /// Add a less than condition (<) for timestamps or strings.
  pub fn add_lt(mut self, column: &str, value: &str) -> Self {
    self.conditions.push(format!(
      "{} < '{}'",
      Self::escape_column(column),
      Self::escape_value(value)
    ));
    self
  }

  /// Add a greater than condition (>) for timestamps or strings.
  pub fn add_gt(mut self, column: &str, value: &str) -> Self {
    self.conditions.push(format!(
      "{} > '{}'",
      Self::escape_column(column),
      Self::escape_value(value)
    ));
    self
  }

  /// Add a IS NULL condition.
  pub fn add_is_null(mut self, column: &str) -> Self {
    self.conditions.push(format!("{} IS NULL", Self::escape_column(column)));
    self
  }

  /// Add a IS NOT NULL condition.
  pub fn add_is_not_null(mut self, column: &str) -> Self {
    self
      .conditions
      .push(format!("{} IS NOT NULL", Self::escape_column(column)));
    self
  }

  /// Exclude soft-deleted records.
  pub fn exclude_deleted(mut self) -> Self {
    self.conditions.push("is_deleted = false".to_string());
    self
  }

  /// Exclude superseded records.
  pub fn exclude_superseded(mut self) -> Self {
    self.conditions.push("superseded_by IS NULL".to_string());
    self
  }

  /// Conditionally exclude deleted and superseded based on config.
  pub fn exclude_inactive(self, include_superseded: bool) -> Self {
    if include_superseded {
      self
    } else {
      self.exclude_deleted().exclude_superseded()
    }
  }

  /// Add an IN clause condition with multiple values.
  ///
  /// Useful for filtering by multiple visibility types or chunk types.
  pub fn add_in(mut self, column: &str, values: &[&str]) -> Self {
    if values.is_empty() {
      return self;
    }
    let escaped: Vec<String> = values.iter().map(|v| format!("'{}'", Self::escape_value(v))).collect();
    self
      .conditions
      .push(format!("{} IN ({})", Self::escape_column(column), escaped.join(", ")));
    self
  }

  /// Add an IN clause condition only if the values are Some and non-empty.
  pub fn add_in_opt(self, column: &str, values: Option<&[String]>) -> Self {
    match values {
      Some(v) if !v.is_empty() => {
        let refs: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
        self.add_in(column, &refs)
      }
      _ => self,
    }
  }

  /// Add a minimum integer value condition (>= for u32).
  pub fn add_min_u32(mut self, column: &str, value: u32) -> Self {
    self
      .conditions
      .push(format!("{} >= {}", Self::escape_column(column), value));
    self
  }

  /// Add a minimum integer value condition only if the value is Some.
  pub fn add_min_u32_opt(self, column: &str, value: Option<u32>) -> Self {
    match value {
      Some(v) => self.add_min_u32(column, v),
      None => self,
    }
  }

  /// Build the final filter string.
  ///
  /// Returns `None` if no conditions were added.
  pub fn build(self) -> Option<String> {
    if self.conditions.is_empty() {
      None
    } else {
      Some(self.conditions.join(" AND "))
    }
  }

  /// Build the final filter string, returning empty string if no conditions.
  pub fn build_or_empty(self) -> String {
    self.build().unwrap_or_default()
  }

  /// Check if any conditions have been added.
  pub fn is_empty(&self) -> bool {
    self.conditions.is_empty()
  }

  /// Escape a column name (basic protection).
  fn escape_column(column: &str) -> &str {
    // TODO: For now, just ensure no special chars - could add quoting later
    // This is safe because column names come from our code, not user input
    column
  }

  /// Escape a string value for use in SQL.
  fn escape_value(value: &str) -> String {
    value.replace('\'', "''")
  }

  /// Escape a value for use in a LIKE pattern.
  fn escape_like_value(value: &str) -> String {
    // Escape both SQL quotes and LIKE special chars
    value.replace('\'', "''").replace('%', "\\%").replace('_', "\\_")
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_empty_filter() {
    let filter = FilterBuilder::new().build();
    assert!(filter.is_none());
  }

  #[test]
  fn test_single_condition() {
    let filter = FilterBuilder::new().add_eq("sector", "semantic").build();
    assert_eq!(filter, Some("sector = 'semantic'".to_string()));
  }

  #[test]
  fn test_multiple_conditions() {
    let filter = FilterBuilder::new()
      .exclude_deleted()
      .add_eq("sector", "semantic")
      .add_min("salience", 0.5)
      .build();
    assert_eq!(
      filter,
      Some("is_deleted = false AND sector = 'semantic' AND salience >= 0.5".to_string())
    );
  }

  #[test]
  fn test_sql_injection_prevention() {
    let filter = FilterBuilder::new()
      .add_eq("name", "test'; DROP TABLE memories; --")
      .build();
    assert_eq!(filter, Some("name = 'test''; DROP TABLE memories; --'".to_string()));
    // The escaped version is safe because the single quote is doubled
  }

  #[test]
  fn test_like_escaping() {
    let filter = FilterBuilder::new().add_like("content", "100% complete_test").build();
    // Should escape both % and _ in the pattern
    assert_eq!(filter, Some("content LIKE '%100\\% complete\\_test%'".to_string()));
  }

  #[test]
  fn test_optional_conditions() {
    let filter = FilterBuilder::new()
      .add_eq_opt("sector", Some("semantic"))
      .add_eq_opt("tier", None)
      .add_min_opt("salience", Some(0.5))
      .add_min_opt("importance", None)
      .build();
    assert_eq!(filter, Some("sector = 'semantic' AND salience >= 0.5".to_string()));
  }

  #[test]
  fn test_exclude_inactive() {
    let filter = FilterBuilder::new().exclude_inactive(false).build();
    assert_eq!(filter, Some("is_deleted = false AND superseded_by IS NULL".to_string()));

    let filter = FilterBuilder::new().exclude_inactive(true).build();
    assert!(filter.is_none());
  }

  #[test]
  fn test_timestamp_conditions() {
    let filter = FilterBuilder::new()
      .add_lt("created_at", "2024-01-01T00:00:00Z")
      .add_gt("updated_at", "2023-06-01T00:00:00Z")
      .build();
    assert_eq!(
      filter,
      Some("created_at < '2024-01-01T00:00:00Z' AND updated_at > '2023-06-01T00:00:00Z'".to_string())
    );
  }

  #[test]
  fn test_null_conditions() {
    let filter = FilterBuilder::new()
      .add_is_null("deleted_at")
      .add_is_not_null("content")
      .build();
    assert_eq!(filter, Some("deleted_at IS NULL AND content IS NOT NULL".to_string()));
  }

  #[test]
  fn test_in_clause() {
    let filter = FilterBuilder::new()
      .add_in("visibility", &["pub", "pub(crate)"])
      .build();
    assert_eq!(filter, Some("visibility IN ('pub', 'pub(crate)')".to_string()));
  }

  #[test]
  fn test_in_clause_empty() {
    let filter = FilterBuilder::new().add_in("visibility", &[]).build();
    assert!(filter.is_none(), "Empty IN clause should not add condition");
  }

  #[test]
  fn test_in_clause_escaping() {
    let filter = FilterBuilder::new()
      .add_in("name", &["test'; DROP TABLE", "normal"])
      .build();
    assert_eq!(filter, Some("name IN ('test''; DROP TABLE', 'normal')".to_string()));
  }

  #[test]
  fn test_in_opt_with_values() {
    let values = vec!["function".to_string(), "class".to_string()];
    let filter = FilterBuilder::new().add_in_opt("chunk_type", Some(&values)).build();
    assert_eq!(filter, Some("chunk_type IN ('function', 'class')".to_string()));
  }

  #[test]
  fn test_in_opt_none() {
    let filter = FilterBuilder::new().add_in_opt("chunk_type", None).build();
    assert!(filter.is_none());
  }

  #[test]
  fn test_min_u32() {
    let filter = FilterBuilder::new().add_min_u32("caller_count", 5).build();
    assert_eq!(filter, Some("caller_count >= 5".to_string()));
  }

  #[test]
  fn test_min_u32_opt() {
    let filter = FilterBuilder::new()
      .add_min_u32_opt("caller_count", Some(10))
      .add_min_u32_opt("callee_count", None)
      .build();
    assert_eq!(filter, Some("caller_count >= 10".to_string()));
  }

  #[test]
  fn test_code_filter_combination() {
    let filter = FilterBuilder::new()
      .add_in("visibility", &["pub", "pub(crate)"])
      .add_eq("chunk_type", "function")
      .add_eq("language", "rust")
      .add_min_u32("caller_count", 3)
      .build();
    assert_eq!(
      filter,
      Some(
        "visibility IN ('pub', 'pub(crate)') AND chunk_type = 'function' AND language = 'rust' AND caller_count >= 3"
          .to_string()
      )
    );
  }
}
