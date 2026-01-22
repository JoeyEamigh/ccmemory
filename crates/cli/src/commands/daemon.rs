//! Daemon command

use anyhow::{Context, Result};
use daemon::{Daemon, DaemonConfig};
use tracing::info;

/// Start the daemon
pub async fn cmd_daemon() -> Result<()> {
  let config = DaemonConfig::default();
  let mut daemon = Daemon::new(config);

  info!("Starting CCEngram daemon");
  daemon.run().await.context("Failed to run daemon")?;

  Ok(())
}
