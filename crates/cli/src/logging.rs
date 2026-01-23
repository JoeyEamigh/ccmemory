//! Logging utilities for CLI commands and daemon

use engram_core::Config;
use std::path::PathBuf;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;

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

/// Parse log level from config string
fn parse_log_level(level: &str) -> tracing::Level {
  match level.to_lowercase().as_str() {
    "off" | "error" => tracing::Level::ERROR,
    "warn" => tracing::Level::WARN,
    "info" => tracing::Level::INFO,
    "debug" => tracing::Level::DEBUG,
    "trace" => tracing::Level::TRACE,
    _ => tracing::Level::INFO,
  }
}

/// Initialize logging for daemon with config-driven settings.
///
/// In foreground mode: Logs to console only with colors
/// In background mode: Logs to file only (no ANSI)
///
/// Returns the guard that must be kept alive for the duration of the program
pub fn init_daemon_logging_with_config(foreground: bool) -> Option<WorkerGuard> {
  // Load config from current directory (or defaults)
  let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
  let config = Config::load_for_project(&cwd);
  let daemon_config = &config.daemon;

  // Parse log level from config
  let level = parse_log_level(&daemon_config.log_level);

  // Build env filter (allows RUST_LOG override)
  let env_filter = EnvFilter::builder()
    .with_default_directive(level.into())
    .from_env_lossy();

  if foreground {
    // Foreground mode: console only with colors
    tracing_subscriber::fmt()
      .with_env_filter(env_filter)
      .with_target(true)
      .with_ansi(true)
      .init();
    None
  } else {
    // Background mode: file logging only
    let log_dir = data_dir();
    if std::fs::create_dir_all(&log_dir).is_err() {
      // Fall back to console-only logging
      init_cli_logging();
      return None;
    }

    // Create rolling file appender based on config
    let file_appender = match daemon_config.log_rotation.as_str() {
      "hourly" => tracing_appender::rolling::hourly(&log_dir, "ccengram.log"),
      "never" => tracing_appender::rolling::never(&log_dir, "ccengram.log"),
      _ => tracing_appender::rolling::daily(&log_dir, "ccengram.log"),
    };

    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
      .with_env_filter(env_filter)
      .with_target(true)
      .with_ansi(false)
      .with_writer(file_writer)
      .init();

    Some(guard)
  }
}
