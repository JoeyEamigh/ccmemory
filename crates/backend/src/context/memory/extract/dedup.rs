use std::collections::HashSet;

use tracing::{debug, trace};

use crate::domain::memory::Memory;

const FNV_PRIME: u64 = 0x100000001b3;
const FNV_OFFSET: u64 = 0xcbf29ce484222325;

/// Compute 64-bit SimHash for locality-sensitive hashing
pub fn simhash(text: &str) -> u64 {
  let tokens = tokenize(text);
  let token_count = tokens.len();
  let mut vector = [0i32; 64];

  for token in tokens {
    let hash = fnv1a_hash(token);
    for (i, v) in vector.iter_mut().enumerate() {
      if (hash >> i) & 1 == 1 {
        *v += 1;
      } else {
        *v -= 1;
      }
    }
  }

  let mut result = 0u64;
  for (i, &v) in vector.iter().enumerate() {
    if v > 0 {
      result |= 1 << i;
    }
  }

  trace!(
    text_len = text.len(),
    token_count = token_count,
    simhash = format!("{:016x}", result),
    "SimHash computed"
  );

  result
}

/// Compute Hamming distance between two SimHashes
pub fn hamming_distance(a: u64, b: u64) -> u32 {
  (a ^ b).count_ones()
}

/// FNV-1a hash for individual tokens
fn fnv1a_hash(s: &str) -> u64 {
  let mut hash = FNV_OFFSET;
  for byte in s.bytes() {
    hash ^= byte as u64;
    hash = hash.wrapping_mul(FNV_PRIME);
  }
  hash
}

/// Tokenize text for SimHash
fn tokenize(text: &str) -> Vec<&str> {
  text
    .split(|c: char| !c.is_alphanumeric() && c != '_')
    .filter(|s| s.len() >= 3)
    .collect()
}

/// Compute Jaccard similarity between two texts
pub fn jaccard_similarity(a: &str, b: &str) -> f32 {
  let tokens_a: HashSet<&str> = tokenize(a).into_iter().collect();
  let tokens_b: HashSet<&str> = tokenize(b).into_iter().collect();

  if tokens_a.is_empty() && tokens_b.is_empty() {
    return 1.0;
  }

  let intersection = tokens_a.intersection(&tokens_b).count();
  let union = tokens_a.union(&tokens_b).count();

  if union == 0 {
    return 1.0;
  }

  intersection as f32 / union as f32
}

/// Adaptive threshold based on content length
pub fn adaptive_threshold(content_len: usize) -> u32 {
  match content_len {
    0..=50 => 2,
    51..=200 => 3,
    201..=500 => 4,
    _ => 5,
  }
}

/// Compute SHA-256 hash of content
pub fn content_hash(content: &str) -> String {
  use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
  };

  let mut hasher = DefaultHasher::new();
  content.hash(&mut hasher);
  format!("{:016x}", hasher.finish())
}

/// Result of duplicate check
#[derive(Debug, Clone)]
pub enum DuplicateMatch {
  /// Exact content hash match
  Exact,
  /// SimHash similarity (with distance and Jaccard confirmation)
  Simhash { distance: u32, jaccard: f32 },
  /// No match
  None,
}

/// Check if two memories are duplicates
pub struct DuplicateChecker {
  jaccard_threshold: f32,
}

impl DuplicateChecker {
  pub fn new(jaccard_threshold: f32) -> Self {
    Self { jaccard_threshold }
  }

  /// Check for duplicate using multi-level strategy
  pub fn is_duplicate(&self, new_content: &str, new_hash: &str, new_simhash: u64, existing: &Memory) -> DuplicateMatch {
    // Level 1: Exact content hash match
    if new_hash == existing.content_hash {
      debug!(
        existing_id = %existing.id,
        "Exact duplicate found (content hash match)"
      );
      return DuplicateMatch::Exact;
    }

    // Level 2: SimHash similarity
    let distance = hamming_distance(new_simhash, existing.simhash);
    let threshold = adaptive_threshold(new_content.len());

    trace!(
      existing_id = %existing.id,
      hamming_distance = distance,
      threshold = threshold,
      "SimHash distance check"
    );

    if distance <= threshold {
      // Level 3: Always verify with Jaccard to confirm similarity
      // Even with low hamming distance, verify to prevent false positives
      let jaccard = jaccard_similarity(new_content, &existing.content);

      trace!(
        existing_id = %existing.id,
        jaccard = jaccard,
        jaccard_threshold = self.jaccard_threshold,
        "Jaccard verification"
      );

      if jaccard >= self.jaccard_threshold {
        debug!(
          existing_id = %existing.id,
          hamming_distance = distance,
          jaccard = jaccard,
          "Near-duplicate found (SimHash + Jaccard match)"
        );
        return DuplicateMatch::Simhash { distance, jaccard };
      }

      trace!(
        existing_id = %existing.id,
        jaccard = jaccard,
        "SimHash match rejected - Jaccard below threshold"
      );
    }

    DuplicateMatch::None
  }
}

/// Compute both hashes for a piece of content
pub fn compute_hashes(content: &str) -> (String, u64) {
  (content_hash(content), simhash(content))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::domain::memory::Sector;

  #[test]
  fn test_simhash_identical() {
    let text = "The quick brown fox jumps over the lazy dog";
    let hash1 = simhash(text);
    let hash2 = simhash(text);
    assert_eq!(hash1, hash2);
  }

  #[test]
  fn test_simhash_similar() {
    let text1 = "The quick brown fox jumps over the lazy dog";
    let text2 = "The quick brown fox jumps over a lazy dog";
    let hash1 = simhash(text1);
    let hash2 = simhash(text2);
    let distance = hamming_distance(hash1, hash2);
    // Similar texts should have small Hamming distance
    assert!(distance < 10, "Distance was {}", distance);
  }

  #[test]
  fn test_simhash_different() {
    let text1 = "The quick brown fox jumps over the lazy dog";
    let text2 = "Completely unrelated content about programming";
    let hash1 = simhash(text1);
    let hash2 = simhash(text2);
    let distance = hamming_distance(hash1, hash2);
    // Different texts should have larger Hamming distance
    assert!(distance > 10, "Distance was {}", distance);
  }

  #[test]
  fn test_jaccard_identical() {
    let text = "hello world foo bar";
    assert_eq!(jaccard_similarity(text, text), 1.0);
  }

  #[test]
  fn test_jaccard_similar() {
    let text1 = "hello world foo bar";
    let text2 = "hello world foo baz";
    let sim = jaccard_similarity(text1, text2);
    assert!(sim > 0.5);
    assert!(sim < 1.0);
  }

  #[test]
  fn test_jaccard_empty() {
    assert_eq!(jaccard_similarity("", ""), 1.0);
    // "a b" has no tokens >= 3 chars, so it's effectively empty
    assert_eq!(jaccard_similarity("hello world", ""), 0.0);
  }

  #[test]
  fn test_duplicate_checker_exact() {
    use uuid::Uuid;

    let content = "test content for deduplication";
    let (hash, sh) = compute_hashes(content);

    let mut memory = Memory::new(Uuid::new_v4(), content.to_string(), Sector::Semantic);
    memory.content_hash = hash.clone();
    memory.simhash = sh;

    let checker = DuplicateChecker::new(0.8);
    let result = checker.is_duplicate(content, &hash, sh, &memory);

    assert!(matches!(result, DuplicateMatch::Exact));
  }

  #[test]
  fn test_duplicate_checker_different() {
    use uuid::Uuid;

    let content1 = "The user prefers using TypeScript over JavaScript";
    let content2 = "Database connection pooling configuration settings";

    let (hash1, sh1) = compute_hashes(content1);
    let (hash2, sh2) = compute_hashes(content2);

    let mut memory = Memory::new(Uuid::new_v4(), content1.to_string(), Sector::Semantic);
    memory.content_hash = hash1;
    memory.simhash = sh1;

    let checker = DuplicateChecker::new(0.8);
    let result = checker.is_duplicate(content2, &hash2, sh2, &memory);

    assert!(matches!(result, DuplicateMatch::None));
  }
}
