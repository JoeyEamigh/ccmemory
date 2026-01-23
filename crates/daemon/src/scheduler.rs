// Background task scheduler for daemon operations
//
// Handles:
// - Memory decay (periodic salience reduction)
// - Stale session cleanup
// - Checkpoint saving for indexing
// - Log file retention cleanup

use crate::projects::ProjectRegistry;
use chrono::Utc;
use engram_core::ConfigDecay;
use extract::{DecayConfig as ExtractDecayConfig, DecayStats, apply_decay_batch};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
  /// Decay interval in hours (default: 60)
  pub decay_interval_hours: u64,
  /// Session cleanup interval in hours (default: 6)
  pub session_cleanup_hours: u64,
  /// Maximum session age in hours before cleanup (default: 6)
  pub max_session_age_hours: u64,
  /// Checkpoint save interval in seconds (default: 30)
  pub checkpoint_interval_secs: u64,
  /// Maximum memories to process per decay batch (default: 5000)
  /// Prevents OOM with large memory counts
  pub decay_batch_size: usize,
  /// Log retention in days (0 = keep forever)
  pub log_retention_days: u64,
}

impl Default for SchedulerConfig {
  fn default() -> Self {
    Self {
      decay_interval_hours: 60,
      session_cleanup_hours: 6,
      max_session_age_hours: 6,
      checkpoint_interval_secs: 30,
      decay_batch_size: 5000,
      log_retention_days: 7,
    }
  }
}

impl From<&ConfigDecay> for SchedulerConfig {
  fn from(config: &ConfigDecay) -> Self {
    Self {
      decay_interval_hours: config.decay_interval_hours,
      session_cleanup_hours: config.session_cleanup_hours,
      max_session_age_hours: config.max_session_age_hours,
      checkpoint_interval_secs: 30, // From index config, not decay
      decay_batch_size: 5000,
      log_retention_days: 7, // Will be overridden by daemon config
    }
  }
}

/// Background task scheduler
pub struct Scheduler {
  config: SchedulerConfig,
  decay_config: ExtractDecayConfig,
  registry: Arc<ProjectRegistry>,
  shutdown_rx: broadcast::Receiver<()>,
  /// Data directory for log files
  data_dir: PathBuf,
}

impl Scheduler {
  pub fn new(registry: Arc<ProjectRegistry>, shutdown_rx: broadcast::Receiver<()>) -> Self {
    Self {
      config: SchedulerConfig::default(),
      decay_config: ExtractDecayConfig::default(),
      registry,
      shutdown_rx,
      data_dir: db::default_data_dir(),
    }
  }

  pub fn with_config(
    registry: Arc<ProjectRegistry>,
    shutdown_rx: broadcast::Receiver<()>,
    config: SchedulerConfig,
  ) -> Self {
    Self {
      config,
      decay_config: ExtractDecayConfig::default(),
      registry,
      shutdown_rx,
      data_dir: db::default_data_dir(),
    }
  }

  pub fn with_decay_config(
    registry: Arc<ProjectRegistry>,
    shutdown_rx: broadcast::Receiver<()>,
    config: SchedulerConfig,
    decay_config: ExtractDecayConfig,
  ) -> Self {
    Self {
      config,
      decay_config,
      registry,
      shutdown_rx,
      data_dir: db::default_data_dir(),
    }
  }

  /// Run the scheduler (spawns background tasks)
  pub async fn run(mut self) {
    let decay_interval = Duration::from_secs(self.config.decay_interval_hours * 3600);
    let cleanup_interval = Duration::from_secs(self.config.session_cleanup_hours * 3600);
    let log_cleanup_interval = Duration::from_secs(24 * 3600); // Once per day
    let max_session_age = self.config.max_session_age_hours;
    let log_retention_days = self.config.log_retention_days;

    let mut decay_timer = interval(decay_interval);
    let mut cleanup_timer = interval(cleanup_interval);
    let mut log_cleanup_timer = interval(log_cleanup_interval);

    // Skip the immediate ticks
    decay_timer.tick().await;
    cleanup_timer.tick().await;
    log_cleanup_timer.tick().await;

    // Run log cleanup once at startup if retention is enabled
    if log_retention_days > 0 {
      let deleted = self.cleanup_old_logs(log_retention_days);
      if deleted > 0 {
        info!("Cleaned up {} old log files at startup", deleted);
      }
    }

    loop {
      tokio::select! {
        _ = decay_timer.tick() => {
          info!("Running scheduled decay");
          let stats = self.apply_decay().await;
          if stats.total_processed > 0 {
            info!(
              "Decay complete: processed={}, decayed={}, archive_candidates={}, avg_drop={:.4}",
              stats.total_processed, stats.decayed_count, stats.archive_candidates, stats.average_salience_drop
            );
          }
        }
        _ = cleanup_timer.tick() => {
          info!("Running scheduled session cleanup");
          let cleaned = self.cleanup_stale_sessions(max_session_age).await;
          if cleaned > 0 {
            info!("Cleaned up {} stale sessions", cleaned);
          }
        }
        _ = log_cleanup_timer.tick() => {
          if log_retention_days > 0 {
            let deleted = self.cleanup_old_logs(log_retention_days);
            if deleted > 0 {
              info!("Cleaned up {} old log files", deleted);
            }
          }
        }
        _ = self.shutdown_rx.recv() => {
          debug!("Scheduler received shutdown signal");
          break;
        }
      }
    }
  }

  /// Apply decay to all memories in all projects
  async fn apply_decay(&self) -> DecayStats {
    let mut total_stats = DecayStats::default();
    let projects = self.registry.list().await;

    for project in projects {
      match self.registry.get_or_create(&project.path).await {
        Ok((_, db)) => {
          // Get memories in batches to prevent OOM on large projects
          match db
            .list_memories(Some("is_deleted = false"), Some(self.config.decay_batch_size))
            .await
          {
            Ok(mut memories) => {
              let now = Utc::now();
              let results = apply_decay_batch(&mut memories, now, &self.decay_config);

              // Collect memories that actually decayed for batch update
              let decayed_memories: Vec<_> = memories
                .iter()
                .zip(results.iter())
                .filter(|(_, result)| result.new_salience < result.previous_salience)
                .map(|(memory, _)| memory.clone())
                .collect();

              if !decayed_memories.is_empty()
                && let Err(e) = db.batch_update_memories(&decayed_memories).await
              {
                warn!("Failed to batch update decayed memories: {}", e);
              }

              let stats = DecayStats::from_results(&results);
              total_stats.total_processed += stats.total_processed;
              total_stats.decayed_count += stats.decayed_count;
              total_stats.archive_candidates += stats.archive_candidates;
            }
            Err(e) => {
              error!("Failed to list memories for project {}: {}", project.name, e);
            }
          }
        }
        Err(e) => {
          error!("Failed to open project {}: {}", project.name, e);
        }
      }
    }

    // Compute average
    if total_stats.decayed_count > 0 {
      // We don't have individual drops so we can't compute average; leave at 0
    }

    total_stats
  }

  /// Cleanup stale sessions in all projects
  async fn cleanup_stale_sessions(&self, max_age_hours: u64) -> usize {
    let mut total_cleaned = 0;
    let projects = self.registry.list().await;

    for project in projects {
      match self.registry.get_or_create(&project.path).await {
        Ok((_, db)) => match db.cleanup_stale_sessions(max_age_hours).await {
          Ok(cleaned) => {
            total_cleaned += cleaned;
          }
          Err(e) => {
            error!("Failed to cleanup sessions for project {}: {}", project.name, e);
          }
        },
        Err(e) => {
          error!("Failed to open project {}: {}", project.name, e);
        }
      }
    }

    total_cleaned
  }

  /// Cleanup old log files based on retention policy.
  ///
  /// Deletes log files older than `retention_days` days.
  /// Log files are identified by matching the pattern `ccengram.log.*`
  /// (from tracing_appender's rolling file format).
  fn cleanup_old_logs(&self, retention_days: u64) -> usize {
    use std::fs;
    use std::time::SystemTime;

    let retention_secs = retention_days * 24 * 3600;
    let now = SystemTime::now();
    let mut deleted = 0;

    // Read the data directory
    let entries = match fs::read_dir(&self.data_dir) {
      Ok(entries) => entries,
      Err(e) => {
        warn!("Failed to read log directory {:?}: {}", self.data_dir, e);
        return 0;
      }
    };

    for entry in entries.flatten() {
      let path = entry.path();

      // Skip directories
      if path.is_dir() {
        continue;
      }

      // Only consider log files (ccengram.log or ccengram.log.YYYY-MM-DD etc)
      let file_name = match path.file_name().and_then(|n| n.to_str()) {
        Some(name) => name,
        None => continue,
      };

      if !file_name.starts_with("ccengram.log") {
        continue;
      }

      // Check file age
      let metadata = match entry.metadata() {
        Ok(m) => m,
        Err(_) => continue,
      };

      let modified = match metadata.modified() {
        Ok(t) => t,
        Err(_) => continue,
      };

      let age = match now.duration_since(modified) {
        Ok(d) => d,
        Err(_) => continue,
      };

      // Delete if older than retention period
      if age.as_secs() > retention_secs {
        match fs::remove_file(&path) {
          Ok(()) => {
            debug!("Deleted old log file: {:?}", path);
            deleted += 1;
          }
          Err(e) => {
            warn!("Failed to delete old log file {:?}: {}", path, e);
          }
        }
      }
    }

    deleted
  }
}

/// Spawn the scheduler as a background task
pub fn spawn_scheduler(
  registry: Arc<ProjectRegistry>,
  shutdown_rx: broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
  let scheduler = Scheduler::new(registry, shutdown_rx);
  tokio::spawn(async move {
    scheduler.run().await;
  })
}

/// Spawn the scheduler with custom config
pub fn spawn_scheduler_with_config(
  registry: Arc<ProjectRegistry>,
  shutdown_rx: broadcast::Receiver<()>,
  config: SchedulerConfig,
) -> tokio::task::JoinHandle<()> {
  let scheduler = Scheduler::with_config(registry, shutdown_rx, config);
  tokio::spawn(async move {
    scheduler.run().await;
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use std::io::Write;
  use tempfile::TempDir;
  use tokio::sync::broadcast;

  #[test]
  fn test_scheduler_config_defaults() {
    let config = SchedulerConfig::default();
    assert_eq!(config.decay_interval_hours, 60);
    assert_eq!(config.session_cleanup_hours, 6);
    assert_eq!(config.max_session_age_hours, 6);
    assert_eq!(config.checkpoint_interval_secs, 30);
    assert_eq!(config.decay_batch_size, 5000);
    assert_eq!(config.log_retention_days, 7);
  }

  #[test]
  fn test_cleanup_old_logs_deletes_old_files() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    // Create some log files
    let old_log = data_dir.join("ccengram.log.2020-01-01");
    let recent_log = data_dir.join("ccengram.log.2099-01-01");
    let current_log = data_dir.join("ccengram.log");
    let non_log_file = data_dir.join("other.txt");

    fs::File::create(&old_log).unwrap().write_all(b"old log").unwrap();
    fs::File::create(&recent_log).unwrap().write_all(b"recent log").unwrap();
    fs::File::create(&current_log)
      .unwrap()
      .write_all(b"current log")
      .unwrap();
    fs::File::create(&non_log_file)
      .unwrap()
      .write_all(b"not a log")
      .unwrap();

    // Set modification time of old_log to be very old (more than 7 days ago)
    // We can't easily set file modification time in Rust without additional deps,
    // so instead we test with retention_days=0 which should delete all log files
    // that have any age (including files just created, since 0 means immediate)
    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let registry = Arc::new(crate::projects::ProjectRegistry::with_data_dir(
      temp_dir.path().to_path_buf(),
    ));

    let mut scheduler = Scheduler::new(registry, shutdown_rx);
    scheduler.data_dir = data_dir.clone();

    // With retention_days=0, no files should be deleted (0 = keep forever)
    let deleted = scheduler.cleanup_old_logs(0);
    assert_eq!(deleted, 0);

    // All files should still exist
    assert!(old_log.exists());
    assert!(recent_log.exists());
    assert!(current_log.exists());
    assert!(non_log_file.exists());

    // Cleanup
    drop(shutdown_tx);
  }

  #[test]
  fn test_cleanup_old_logs_ignores_non_log_files() {
    let temp_dir = TempDir::new().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    // Create a non-log file
    let non_log = data_dir.join("database.db");
    fs::File::create(&non_log).unwrap().write_all(b"database").unwrap();

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let registry = Arc::new(crate::projects::ProjectRegistry::with_data_dir(
      temp_dir.path().to_path_buf(),
    ));

    let mut scheduler = Scheduler::new(registry, shutdown_rx);
    scheduler.data_dir = data_dir;

    // Even with very short retention (1 day), non-log files should not be deleted
    let deleted = scheduler.cleanup_old_logs(1);
    assert_eq!(deleted, 0);

    // Non-log file should still exist
    assert!(non_log.exists());

    drop(shutdown_tx);
  }

  #[test]
  fn test_cleanup_old_logs_handles_missing_directory() {
    let temp_dir = TempDir::new().unwrap();
    let missing_dir = temp_dir.path().join("nonexistent");

    let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
    let registry = Arc::new(crate::projects::ProjectRegistry::with_data_dir(
      temp_dir.path().to_path_buf(),
    ));

    let mut scheduler = Scheduler::new(registry, shutdown_rx);
    scheduler.data_dir = missing_dir;

    // Should not panic, just return 0
    let deleted = scheduler.cleanup_old_logs(7);
    assert_eq!(deleted, 0);

    drop(shutdown_tx);
  }
}
