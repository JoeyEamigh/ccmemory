//! Daemon command

use anyhow::{Context, Result, bail};
use daemon::{Daemon, DaemonConfig, default_socket_path, is_running};
use tracing::info;

/// Start the daemon
///
/// # Arguments
/// * `foreground` - Run in foreground mode (disables auto-shutdown, logs to console)
/// * `background` - Run in background mode (enables auto-shutdown, for auto-start)
pub async fn cmd_daemon(foreground: bool, background: bool) -> Result<()> {
  // --foreground and --background are mutually exclusive
  if foreground && background {
    bail!("Cannot specify both --foreground and --background");
  }

  // If --background, we're being spawned by auto-start
  // Check if already running and exit silently
  let socket_path = default_socket_path();
  if is_running(&socket_path) {
    if background {
      // Another instance is handling it, exit silently
      return Ok(());
    }
    bail!("Daemon is already running at {:?}", socket_path);
  }

  // Create config based on mode
  let config = if foreground {
    DaemonConfig::foreground()
  } else {
    DaemonConfig::background()
  };

  let mut daemon = Daemon::new(config);

  info!("Starting CCEngram daemon");
  daemon.run().await.context("Failed to run daemon")?;

  Ok(())
}
