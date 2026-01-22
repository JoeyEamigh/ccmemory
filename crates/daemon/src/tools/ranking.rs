//! Ranking utilities for post-search scoring

use chrono::Utc;
use engram_core::{Memory, SearchConfig};

/// Ranking weights for post-search scoring
pub struct RankingWeights {
  pub semantic: f32, // Weight for vector similarity
  pub salience: f32, // Weight for salience score
  pub recency: f32,  // Weight for recency
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

/// Rank memories by combining vector similarity with salience, recency, and sector boosts
pub fn rank_memories(
  results: Vec<(Memory, f32)>,
  limit: usize,
  weights: Option<&RankingWeights>,
) -> Vec<(Memory, f32, f32)> {
  let default_weights = RankingWeights::default();
  let weights = weights.unwrap_or(&default_weights);
  let now = Utc::now();

  let mut scored: Vec<_> = results
    .into_iter()
    .map(|(m, distance)| {
      // Convert distance to similarity (1.0 - distance for cosine)
      let similarity = 1.0 - distance.min(1.0);

      // Recency score: decay based on days since last access
      let days_since_access = (now - m.last_accessed).num_days().max(0) as f32;
      let recency_score = (-0.02 * days_since_access).exp(); // Exponential decay

      // Sector boost
      let sector_boost = m.sector.search_boost();

      // Supersession penalty
      let supersession_penalty = if m.superseded_by.is_some() { 0.7 } else { 1.0 };

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
