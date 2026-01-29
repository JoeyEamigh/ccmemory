use chrono::{DateTime, Utc};
use tracing::{debug, trace};

use crate::domain::memory::Memory;

/// Configuration for decay processing
#[derive(Debug, Clone)]
pub struct MemoryDecay {
  /// Salience threshold below which memories should be archived
  pub archive_threshold: f32,
  /// Maximum days without access before forced decay consideration
  pub max_idle_days: i64,
}

impl Default for MemoryDecay {
  fn default() -> Self {
    Self {
      archive_threshold: 0.1,
      max_idle_days: 90,
    }
  }
}

/// Result of applying decay to a memory
#[derive(Debug, Clone)]
pub struct DecayResult {
  pub previous_salience: f32,
  pub new_salience: f32,
  pub should_archive: bool,
}

/// Apply decay to a single memory
pub fn apply_decay(memory: &mut Memory, now: DateTime<Utc>, config: &MemoryDecay) -> DecayResult {
  let previous_salience = memory.salience;

  // Calculate days since last access
  let days_since_access = (now - memory.last_accessed).num_days() as f32;

  // Apply the memory's built-in decay
  memory.apply_decay(now);

  // Check if should be archived
  let should_archive = memory.salience < config.archive_threshold || days_since_access > config.max_idle_days as f32;

  trace!(
    memory_id = %memory.id,
    old_salience = previous_salience,
    new_salience = memory.salience,
    days_idle = days_since_access,
    should_archive = should_archive,
    "Salience decayed"
  );

  DecayResult {
    previous_salience,
    new_salience: memory.salience,
    should_archive,
  }
}

/// Apply decay to a batch of memories
pub fn apply_decay_batch(memories: &mut [Memory], now: DateTime<Utc>, config: &MemoryDecay) -> Vec<DecayResult> {
  debug!(memory_count = memories.len(), "Starting decay batch");

  let results: Vec<DecayResult> = memories.iter_mut().map(|m| apply_decay(m, now, config)).collect();

  let stats = DecayStats::from_results(&results);
  debug!(
    processed = stats.total_processed,
    decayed = stats.decayed_count,
    archive_candidates = stats.archive_candidates,
    avg_salience_drop = stats.average_salience_drop,
    "Decay batch complete"
  );

  if stats.archive_candidates > 0 {
    debug!(count = stats.archive_candidates, "Archive candidates identified");
  }

  results
}

/// Batch statistics for decay processing
#[derive(Debug, Default)]
pub struct DecayStats {
  pub total_processed: usize,
  pub decayed_count: usize,
  pub archive_candidates: usize,
  pub average_salience_drop: f32,
}

impl DecayStats {
  pub fn from_results(results: &[DecayResult]) -> Self {
    if results.is_empty() {
      return Self::default();
    }

    let total_processed = results.len();
    let decayed_count = results.iter().filter(|r| r.new_salience < r.previous_salience).count();
    let archive_candidates = results.iter().filter(|r| r.should_archive).count();
    let average_salience_drop = results
      .iter()
      .map(|r| r.previous_salience - r.new_salience)
      .sum::<f32>()
      / total_processed as f32;

    Self {
      total_processed,
      decayed_count,
      archive_candidates,
      average_salience_drop,
    }
  }
}

#[cfg(test)]
mod tests {
  use uuid::Uuid;

  use super::*;
  use crate::domain::memory::Sector;

  #[test]
  fn test_apply_decay() {
    let mut memory = Memory::new(Uuid::new_v4(), "test".into(), Sector::Episodic);
    memory.salience = 1.0;
    memory.importance = 0.5;

    let config = MemoryDecay::default();
    let future = Utc::now() + chrono::Duration::days(30);

    let result = apply_decay(&mut memory, future, &config);

    assert!(result.new_salience < result.previous_salience);
    assert_eq!(result.previous_salience, 1.0);
  }

  #[test]
  fn test_decay_varies_by_sector() {
    let config = MemoryDecay::default();
    let future = Utc::now() + chrono::Duration::days(30);

    // Episodic should decay faster than Emotional
    let mut episodic = Memory::new(Uuid::new_v4(), "test".into(), Sector::Episodic);
    episodic.salience = 1.0;
    episodic.importance = 0.5;

    let mut emotional = Memory::new(Uuid::new_v4(), "test".into(), Sector::Emotional);
    emotional.salience = 1.0;
    emotional.importance = 0.5;

    apply_decay(&mut episodic, future, &config);
    apply_decay(&mut emotional, future, &config);

    assert!(
      episodic.salience < emotional.salience,
      "Episodic {} should be less than Emotional {}",
      episodic.salience,
      emotional.salience
    );
  }

  #[test]
  fn test_importance_slows_decay() {
    let config = MemoryDecay::default();
    let future = Utc::now() + chrono::Duration::days(30);

    let mut low_importance = Memory::new(Uuid::new_v4(), "test".into(), Sector::Semantic);
    low_importance.salience = 1.0;
    low_importance.importance = 0.2;

    let mut high_importance = Memory::new(Uuid::new_v4(), "test".into(), Sector::Semantic);
    high_importance.salience = 1.0;
    high_importance.importance = 0.9;

    apply_decay(&mut low_importance, future, &config);
    apply_decay(&mut high_importance, future, &config);

    assert!(
      low_importance.salience < high_importance.salience,
      "Low importance {} should decay more than high importance {}",
      low_importance.salience,
      high_importance.salience
    );
  }

  #[test]
  fn test_access_count_provides_protection() {
    let config = MemoryDecay::default();
    let future = Utc::now() + chrono::Duration::days(60);

    let mut rarely_accessed = Memory::new(Uuid::new_v4(), "test".into(), Sector::Semantic);
    rarely_accessed.salience = 1.0;
    rarely_accessed.importance = 0.5;
    rarely_accessed.access_count = 0;

    let mut frequently_accessed = Memory::new(Uuid::new_v4(), "test".into(), Sector::Semantic);
    frequently_accessed.salience = 1.0;
    frequently_accessed.importance = 0.5;
    frequently_accessed.access_count = 100;

    apply_decay(&mut rarely_accessed, future, &config);
    apply_decay(&mut frequently_accessed, future, &config);

    assert!(
      rarely_accessed.salience < frequently_accessed.salience,
      "Rarely accessed {} should decay more than frequently accessed {}",
      rarely_accessed.salience,
      frequently_accessed.salience
    );
  }
}
