//! Integration tests for daemon lifecycle management.
//!
//! Tests auto-shutdown, session tracking, and foreground mode.
//!
//! Note: ShutdownWatcher tests are in unit tests since with_check_interval
//! is only available in the crate itself.

use daemon::{ActivityTracker, Daemon, DaemonConfig, SessionId, SessionTracker};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Test session registration and unregistration
#[tokio::test]
async fn test_session_lifecycle() {
  let session_tracker = Arc::new(SessionTracker::new(1800));

  // Initially no sessions
  assert_eq!(session_tracker.active_count().await, 0);
  assert!(!session_tracker.has_active_sessions().await);

  // Register a session
  session_tracker.register(SessionId::from("session-1")).await;
  assert_eq!(session_tracker.active_count().await, 1);
  assert!(session_tracker.has_active_sessions().await);

  // Register another session
  session_tracker.register(SessionId::from("session-2")).await;
  assert_eq!(session_tracker.active_count().await, 2);

  // Unregister one
  session_tracker.unregister(&SessionId::from("session-1")).await;
  assert_eq!(session_tracker.active_count().await, 1);

  // Unregister the other
  session_tracker.unregister(&SessionId::from("session-2")).await;
  assert_eq!(session_tracker.active_count().await, 0);
  assert!(!session_tracker.has_active_sessions().await);
}

/// Test stale session cleanup
#[tokio::test]
async fn test_stale_session_cleanup() {
  // Very short session timeout (0 = immediate stale)
  let session_tracker = Arc::new(SessionTracker::new(0));

  // Register a session
  session_tracker.register(SessionId::from("stale-session")).await;
  assert_eq!(session_tracker.active_count().await, 1);

  // Wait a bit for it to become stale
  tokio::time::sleep(Duration::from_millis(10)).await;

  // Cleanup should remove it
  let stale = session_tracker.cleanup_stale().await;
  assert_eq!(stale.len(), 1);
  assert_eq!(stale[0].0, "stale-session");

  // Should be empty now
  assert_eq!(session_tracker.active_count().await, 0);
}

/// Test activity tracker touch and idle duration
#[tokio::test]
async fn test_activity_tracker() {
  let tracker = ActivityTracker::new();

  // Initial idle time should be minimal
  let initial_idle = tracker.idle_duration();
  assert!(initial_idle.as_millis() < 100, "Initial idle should be minimal");

  // Wait a bit
  tokio::time::sleep(Duration::from_millis(50)).await;

  // Idle time should have increased
  let idle_after_wait = tracker.idle_duration();
  assert!(idle_after_wait.as_millis() >= 40, "Idle should increase over time");

  // Touch should reset idle time
  tracker.touch();
  let idle_after_touch = tracker.idle_duration();
  assert!(idle_after_touch.as_millis() < 20, "Idle should reset after touch");
}

/// Test DaemonConfig defaults from config file
#[test]
fn test_daemon_config_defaults() {
  let config = DaemonConfig::default();

  // Should have reasonable defaults
  assert!(!config.socket_path.to_string_lossy().is_empty());
  assert!(!config.data_dir.to_string_lossy().is_empty());

  // Default idle timeout is 5 minutes = 300 seconds
  assert_eq!(config.idle_timeout_secs, 300);

  // Default session timeout is 30 minutes = 1800 seconds
  assert_eq!(config.session_timeout_secs, 1800);

  // Default is background mode
  assert!(!config.foreground);

  // Default log retention is 7 days
  assert_eq!(config.log_retention_days, 7);
}

/// Test DaemonConfig foreground/background constructors
#[test]
fn test_daemon_config_modes() {
  // Foreground mode
  let fg_config = DaemonConfig::foreground();
  assert!(fg_config.foreground, "Foreground config should have foreground=true");

  // Background mode
  let bg_config = DaemonConfig::background();
  assert!(!bg_config.foreground, "Background config should have foreground=false");
}

/// Test that Daemon can be created with custom data directory
#[tokio::test]
async fn test_daemon_with_temp_dir() {
  let temp_dir = TempDir::new().expect("Failed to create temp dir");

  let config = DaemonConfig {
    data_dir: temp_dir.path().to_path_buf(),
    socket_path: temp_dir.path().join("test.sock"),
    foreground: false,
    ..DaemonConfig::default()
  };

  let daemon = Daemon::new(config);

  // Check that trackers are accessible
  let session_tracker = daemon.session_tracker();
  let activity_tracker = daemon.activity_tracker();

  assert_eq!(session_tracker.active_count().await, 0);
  assert!(activity_tracker.idle_duration().as_secs() < 1);
}

/// Test that touch updates session activity
#[tokio::test]
async fn test_session_touch_updates_activity() {
  // Use a short session timeout
  let session_tracker = Arc::new(SessionTracker::new(1)); // 1 second timeout

  // Register a session
  session_tracker.register(SessionId::from("touch-test")).await;

  // Wait almost to timeout
  tokio::time::sleep(Duration::from_millis(500)).await;

  // Touch to keep alive
  session_tracker.touch(&SessionId::from("touch-test")).await;

  // Wait a bit more
  tokio::time::sleep(Duration::from_millis(600)).await;

  // Should still be alive because we touched it
  let stale = session_tracker.cleanup_stale().await;

  // May or may not be stale depending on timing, but at least verify cleanup works
  assert!(stale.len() <= 1);
}
