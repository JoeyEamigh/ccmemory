//! Memory ranking algorithms.
//!
//! Provides post-search ranking that combines:
//! - Semantic similarity (from vector search)
//! - Salience score (memory importance over time)
//! - Recency (time since last access)
//! - Sector boost (different sectors have different importance)

use chrono::Utc;

use crate::domain::{config::SearchConfig, memory::Memory};

/// Ranking weights for post-search scoring.
///
/// These weights determine how different factors contribute to the final rank score.
/// They should sum to approximately 1.0 for consistent scoring.
#[derive(Debug, Clone)]
pub struct RankingWeights {
  /// Weight for vector similarity score (0.0 to 1.0)
  pub semantic: f32,
  /// Weight for salience score (0.0 to 1.0)
  pub salience: f32,
  /// Weight for recency score (0.0 to 1.0)
  pub recency: f32,
}

impl Default for RankingWeights {
  fn default() -> Self {
    Self {
      semantic: 0.5,
      salience: 0.3,
      recency: 0.2,
    }
  }
}

impl From<&SearchConfig> for RankingWeights {
  fn from(config: &SearchConfig) -> Self {
    Self {
      semantic: config.semantic_weight as f32,
      salience: config.salience_weight as f32,
      recency: config.recency_weight as f32,
    }
  }
}

/// Full ranking configuration.
#[derive(Debug, Clone)]
pub struct RankingConfig {
  /// Base weights for scoring
  pub weights: RankingWeights,
  /// Penalty multiplier for superseded memories (0.0 to 1.0)
  pub supersession_penalty: f32,
  /// Recency decay factor (higher = faster decay)
  pub recency_decay_factor: f32,
}

impl Default for RankingConfig {
  fn default() -> Self {
    Self {
      weights: RankingWeights::default(),
      supersession_penalty: 0.7,
      recency_decay_factor: 0.02,
    }
  }
}

impl From<&SearchConfig> for RankingConfig {
  fn from(config: &SearchConfig) -> Self {
    Self {
      weights: RankingWeights::from(config),
      ..Default::default()
    }
  }
}

/// Rank memories by combining vector similarity with salience, recency, and sector boosts.
///
/// # Arguments
/// * `results` - Vector search results as (Memory, distance) tuples
/// * `limit` - Maximum number of results to return
/// * `config` - Optional ranking configuration (uses defaults if None)
///
/// # Returns
/// Vector of (Memory, distance, rank_score) tuples, sorted by rank_score descending.
///
/// # Scoring Algorithm
///
/// For each memory, the rank score is computed as:
/// ```text
/// similarity = 1.0 - min(distance, 1.0)
/// recency = exp(-decay_factor * days_since_last_access)
/// base_score = (semantic_weight * similarity) + (salience_weight * salience) + (recency_weight * recency)
/// rank_score = base_score * sector_boost * supersession_penalty
/// ```
///
/// The sector boost is determined by the memory's sector (e.g., Reflective gets 1.2x, Episodic gets 0.8x).
/// The supersession penalty (default 0.7) is applied if the memory has been superseded.
pub fn rank_memories(
  results: Vec<(Memory, f32)>,
  limit: usize,
  config: Option<&RankingConfig>,
) -> Vec<(Memory, f32, f32)> {
  let default_config = RankingConfig::default();
  let config = config.unwrap_or(&default_config);
  let weights = &config.weights;
  let now = Utc::now();

  let mut scored: Vec<_> = results
    .into_iter()
    .map(|(m, distance)| {
      // Convert distance to similarity (1.0 - distance for cosine)
      let similarity = 1.0 - distance.min(1.0);

      // Recency score: exponential decay based on days since last access
      let days_since_access = (now - m.last_accessed).num_days().max(0) as f32;
      let recency_score = (-config.recency_decay_factor * days_since_access).exp();

      // Sector-specific boost
      let sector_boost = m.sector.search_boost();

      // Supersession penalty
      let supersession_penalty = if m.superseded_by.is_some() {
        config.supersession_penalty
      } else {
        1.0
      };

      // Combined rank score
      let rank_score =
        (weights.semantic * similarity + weights.salience * m.salience + weights.recency * recency_score)
          * sector_boost
          * supersession_penalty;

      (m, distance, rank_score)
    })
    .collect();

  // Sort by rank score descending
  scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

  // Return top N
  scored.into_iter().take(limit).collect()
}

#[cfg(test)]
mod tests {
  use uuid::Uuid;

  use super::*;
  use crate::domain::memory::Sector;

  fn create_test_memory(sector: Sector, salience: f32, superseded: bool) -> Memory {
    let mut m = Memory::new(Uuid::new_v4(), "test content".to_string(), sector);
    m.salience = salience;
    if superseded {
      m.superseded_by = Some(crate::domain::memory::MemoryId::new());
    }
    m
  }

  #[test]
  fn test_rank_memories_ordering() {
    let m1 = create_test_memory(Sector::Semantic, 0.9, false);
    let m2 = create_test_memory(Sector::Semantic, 0.3, false);
    let m3 = create_test_memory(Sector::Semantic, 0.6, false);

    let results = vec![(m1, 0.1), (m2, 0.1), (m3, 0.1)];
    let ranked = rank_memories(results, 3, None);

    // Higher salience should rank higher (same distance)
    assert!(ranked[0].0.salience > ranked[1].0.salience);
    assert!(ranked[1].0.salience > ranked[2].0.salience);
  }

  #[test]
  fn test_rank_memories_supersession_penalty() {
    let m1 = create_test_memory(Sector::Semantic, 0.8, false);
    let m2 = create_test_memory(Sector::Semantic, 0.8, true); // Superseded

    let results = vec![(m1.clone(), 0.1), (m2.clone(), 0.1)];
    let ranked = rank_memories(results, 2, None);

    // Non-superseded should rank higher
    assert!(ranked[0].0.superseded_by.is_none());
    assert!(ranked[1].0.superseded_by.is_some());
  }

  #[test]
  fn test_rank_memories_sector_boost() {
    let m1 = create_test_memory(Sector::Reflective, 0.5, false); // 1.2x boost
    let m2 = create_test_memory(Sector::Episodic, 0.5, false); // 0.8x boost

    let results = vec![(m1.clone(), 0.1), (m2.clone(), 0.1)];
    let ranked = rank_memories(results, 2, None);

    // Reflective should rank higher due to boost
    assert_eq!(ranked[0].0.sector, Sector::Reflective);
    assert_eq!(ranked[1].0.sector, Sector::Episodic);
  }

  #[test]
  fn test_rank_memories_limit() {
    let memories: Vec<_> = (0..10)
      .map(|i| {
        let m = create_test_memory(Sector::Semantic, i as f32 / 10.0, false);
        (m, 0.1)
      })
      .collect();

    let ranked = rank_memories(memories, 3, None);
    assert_eq!(ranked.len(), 3);
  }
}
