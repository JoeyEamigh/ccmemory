//! Client utilities for connecting to the daemon with auto-start support.
//!
//! Provides `connect_or_start()` which transparently starts the daemon
//! if it's not running, then connects to it.

use crate::server::{Client, ServerError, default_socket_path};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{debug, info};

/// Timeout for waiting for daemon to start
const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Interval between connection attempts during startup
const STARTUP_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Connect to the daemon, starting it if necessary.
///
/// This function:
/// 1. Tries to connect to an existing daemon
/// 2. If that fails, spawns a new daemon in background mode
/// 3. Polls for the socket to become available (up to 5 seconds)
/// 4. Returns a connected client
///
/// # Errors
///
/// Returns an error if:
/// - The daemon fails to start within the timeout
/// - Connection to the daemon fails after startup
pub async fn connect_or_start() -> Result<Client, ServerError> {
  let socket_path = default_socket_path();
  connect_or_start_at(&socket_path).await
}

/// Connect to the daemon at a specific socket path, starting it if necessary.
pub async fn connect_or_start_at(socket_path: &Path) -> Result<Client, ServerError> {
  // Try to connect first
  match Client::connect_to(socket_path).await {
    Ok(client) => {
      debug!("Connected to existing daemon");
      return Ok(client);
    }
    Err(e) => {
      debug!("Daemon not running ({}), starting...", e);
    }
  }

  // Start daemon in background
  start_daemon_background()?;

  // Wait for socket to become available
  let deadline = tokio::time::Instant::now() + STARTUP_TIMEOUT;

  loop {
    if tokio::time::Instant::now() >= deadline {
      return Err(ServerError::Connection("Daemon failed to start within timeout".into()));
    }

    // Check if socket exists and is connectable
    if socket_path.exists() {
      match Client::connect_to(socket_path).await {
        Ok(client) => {
          info!("Connected to newly started daemon");
          return Ok(client);
        }
        Err(_) => {
          // Socket exists but not ready yet
          debug!("Socket exists but connection failed, retrying...");
        }
      }
    }

    tokio::time::sleep(STARTUP_POLL_INTERVAL).await;
  }
}

/// Start the daemon in background mode.
fn start_daemon_background() -> Result<(), ServerError> {
  // Get path to current executable
  let exe =
    std::env::current_exe().map_err(|e| ServerError::Connection(format!("Failed to get executable path: {}", e)))?;

  debug!("Starting daemon from {:?}", exe);

  // Spawn daemon with --background flag
  // This runs the daemon in background mode with auto-shutdown enabled
  let child = Command::new(&exe)
    .args(["daemon", "--background"])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|e| ServerError::Connection(format!("Failed to spawn daemon: {}", e)))?;

  debug!(pid = child.id(), "Spawned daemon process");

  // Don't wait for child - it's daemonized
  // The process is detached and will continue running
  Ok(())
}

/// Check if the daemon is running at the default socket path.
pub fn is_daemon_running() -> bool {
  let socket_path = default_socket_path();
  crate::lifecycle::is_running(&socket_path)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_is_daemon_running_no_socket() {
    // When no socket exists, daemon should not be running
    // This test just verifies the function doesn't panic
    let _ = is_daemon_running();
  }
}
