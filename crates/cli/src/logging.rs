//! Logging utilities for CLI commands and daemon

use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Get the CCEngram data directory (respects env vars)
pub fn data_dir() -> PathBuf {
  daemon::default_data_dir()
}

/// Get the log file path
#[allow(dead_code)]
pub fn log_file_path() -> PathBuf {
  data_dir().join("ccengram.log")
}

/// Initialize logging for CLI commands (console only)
pub fn init_cli_logging() {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
    .init();
}

/// Initialize logging for daemon with file appender
/// Returns the guard that must be kept alive for the duration of the program
pub fn init_daemon_logging() -> Option<WorkerGuard> {
  let log_dir = data_dir();
  if std::fs::create_dir_all(&log_dir).is_err() {
    // Fall back to console-only logging
    init_cli_logging();
    return None;
  }

  // Create a rolling file appender (daily rotation)
  let file_appender = tracing_appender::rolling::daily(&log_dir, "ccengram.log");
  let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

  // Create layers for both console and file
  let env_filter = tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into());

  let console_layer = tracing_subscriber::fmt::layer().with_target(true).with_ansi(true);

  let file_layer = tracing_subscriber::fmt::layer()
    .with_target(true)
    .with_ansi(false)
    .with_writer(file_writer);

  tracing_subscriber::registry()
    .with(env_filter)
    .with(console_layer)
    .with(file_layer)
    .init();

  Some(guard)
}
