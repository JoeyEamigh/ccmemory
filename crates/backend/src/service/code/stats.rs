//! Code statistics service.
//!
//! Provides statistics about indexed code in a project.

use std::collections::HashMap;

use crate::{db::ProjectDb, ipc::types::code::CodeStatsResult, service::util::ServiceError};

/// Get comprehensive code statistics.
///
/// # Arguments
/// * `db` - Project database
///
/// # Returns
/// Code statistics including counts, breakdowns, and health score
pub async fn get_stats(db: &ProjectDb) -> Result<CodeStatsResult, ServiceError> {
  // Get all chunks for analysis
  let chunks = db.list_code_chunks(None, None).await?;

  let total_chunks = chunks.len();

  // Track unique files
  let mut files: std::collections::HashSet<String> = std::collections::HashSet::new();
  let mut language_counts: HashMap<String, usize> = HashMap::new();
  let mut type_counts: HashMap<String, usize> = HashMap::new();
  let mut total_tokens: u64 = 0;
  let mut total_lines: u64 = 0;

  for chunk in &chunks {
    files.insert(chunk.file_path.clone());

    let lang = format!("{:?}", chunk.language).to_lowercase();
    *language_counts.entry(lang).or_insert(0) += 1;

    let chunk_type = format!("{:?}", chunk.chunk_type).to_lowercase();
    *type_counts.entry(chunk_type).or_insert(0) += 1;

    total_tokens += chunk.tokens_estimate as u64;
    total_lines += (chunk.end_line - chunk.start_line + 1) as u64;
  }

  let total_files = files.len();
  let average_chunks_per_file = if total_files > 0 {
    total_chunks as f32 / total_files as f32
  } else {
    0.0
  };

  // Calculate index health score (0-100)
  // Factors:
  // - Having chunks (base requirement)
  // - Reasonable chunks per file (2-20 is good)
  // - Diverse chunk types (not all blocks)
  // - Multiple languages supported
  let health_score = calculate_health_score(total_chunks, total_files, average_chunks_per_file, &type_counts);

  Ok(CodeStatsResult {
    total_chunks,
    total_files,
    total_tokens_estimate: total_tokens,
    total_lines,
    average_chunks_per_file,
    language_breakdown: language_counts,
    chunk_type_breakdown: type_counts,
    index_health_score: health_score,
  })
}

/// Calculate a health score for the index (0-100).
fn calculate_health_score(
  total_chunks: usize,
  total_files: usize,
  avg_chunks: f32,
  type_counts: &HashMap<String, usize>,
) -> u32 {
  let mut score: f32 = 0.0;

  // Has content (25 points)
  if total_chunks > 0 {
    score += 25.0;
  }

  // Reasonable file coverage (25 points)
  if total_files > 0 {
    // Good if we have at least some files
    let file_score = (total_files.min(100) as f32 / 100.0) * 25.0;
    score += file_score;
  }

  // Reasonable chunks per file (25 points)
  // Optimal range: 2-20 chunks per file
  if (2.0..=20.0).contains(&avg_chunks) {
    score += 25.0;
  } else if avg_chunks > 0.0 && avg_chunks < 2.0 {
    score += 15.0; // Too few chunks per file
  } else if avg_chunks > 20.0 && avg_chunks <= 50.0 {
    score += 15.0; // Too many chunks per file
  } else if avg_chunks > 50.0 {
    score += 5.0; // Way too many
  }

  // Chunk type diversity (25 points)
  // Having functions and classes is good
  let has_functions = type_counts.get("function").copied().unwrap_or(0) > 0;
  let has_classes = type_counts.get("class").copied().unwrap_or(0) > 0;
  let has_modules = type_counts.get("module").copied().unwrap_or(0) > 0;

  if has_functions {
    score += 10.0;
  }
  if has_classes || has_modules {
    score += 10.0;
  }
  if type_counts.len() > 2 {
    score += 5.0; // Good variety
  }

  score.min(100.0) as u32
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_health_score_empty() {
    let type_counts = HashMap::new();
    let score = calculate_health_score(0, 0, 0.0, &type_counts);
    assert_eq!(score, 0);
  }

  #[test]
  fn test_health_score_basic() {
    let mut type_counts = HashMap::new();
    type_counts.insert("function".to_string(), 50);
    type_counts.insert("class".to_string(), 10);

    let score = calculate_health_score(60, 10, 6.0, &type_counts);
    assert!(score >= 50, "Expected >= 50, got {}", score);
  }

  #[test]
  fn test_health_score_max() {
    let mut type_counts = HashMap::new();
    type_counts.insert("function".to_string(), 500);
    type_counts.insert("class".to_string(), 100);
    type_counts.insert("module".to_string(), 50);

    // 100+ files, optimal chunks per file, diverse types
    let score = calculate_health_score(1000, 100, 10.0, &type_counts);
    assert!(score >= 90, "Expected >= 90, got {}", score);
  }

  #[test]
  fn test_health_score_too_many_chunks() {
    let mut type_counts = HashMap::new();
    type_counts.insert("block".to_string(), 1000);

    // Way too many chunks per file
    let score = calculate_health_score(1000, 10, 100.0, &type_counts);
    assert!(score < 50, "Expected < 50 for too many chunks, got {}", score);
  }
}
